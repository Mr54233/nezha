# Nezha 通知系统 — 技术实现文档

> 分支 `feat/task-notifications`

> 状态（2026-05-18）：Claude Code 通知适配阶段完成；点击跳转、idle 检测、原生通知、设置项和声音开关已接入。Codex 当前已有 done/failed 生命周期通知与 input_required 检测，idle 通知待基于 Codex session JSONL 适配；Codex 不走 Claude Code hooks。

---

## 一、功能概述

Nezha 通知系统在用户将应用放到后台运行 AI 编程任务时，提供以下能力：

- **桌面通知**：任务完成（done）、失败（failed）、输出完毕待回复（idle）时弹出系统通知
- **应用内 Toast**：窗口可见时在应用内弹出 Toast 弹窗
- **点击跳转**：点击桌面通知或 Toast 后自动切换到对应项目并选中任务
- **通知设置**：总开关、应用内/系统通知独立控制、通知声音、Toast 四角位置、按类型过滤
- **Attention Badge**：任务需要关注时在任务列表和项目导航栏显示未读指示器

---

## 二、数据流全景

```
Claude Code 进程 (PTY 子进程)
  ├─ Stop hook (.mjs) ──────→ .nezha/events/{session_id}.json
  ├─ Notification hook (.mjs) → .nezha/events/{session_id}.json
  └─ session JSONL ──────────→ stop_reason: "end_turn" (兜底)
        │
        ▼
Nezha 后端 (hooks.rs — notify crate 文件监听)
  │  读取 event JSON → 匹配 session_id → 查找 task_id
  │  emit("task-status", { task_id, status: "idle" })
  │
  ├─ session.rs 兜底：解析 JSONL end_turn → 同样 emit idle
  │
  ▼
前端 App.tsx
  ├─ shouldNotifyStatus(done/failed/idle) → 决定是否弹通知
  │
  ├─ 窗口不可见 或 失焦 + system 开关
  │   └─ sendDesktopNotification() — 两级策略：
  │       ├─ 1. invoke("send_native_notification") — user-notify crate (全平台)
  │       │     → 点击通过 "notification-clicked" Tauri 事件处理
  │       └─ 2. window.Notification — Web API 兜底
  │
  ├─ 窗口可见 + 获焦 + !isSelected + inApp 开关
  │   └─ showToast(body, type, onClick)
  │
  └─ 更新 task 状态: attentionRequestedAt / hasUnreadEvent
```

---

## 三、后端实现

### 3.1 Hooks 系统 (`src-tauri/src/hooks.rs`)

将 Claude Code 的 Stop / Notification hooks 事件转换为 `idle` 任务状态。

#### Hook 脚本

两个 Node.js ESM 脚本写入 `.nezha/hooks/`：

| 脚本 | 触发时机 | 行为 |
|------|---------|------|
| `nezha-hook-stop.mjs` | Claude Code 停止输出 | 读取 stdin JSON，提取 session_id，写 event 文件 |
| `nezha-hook-notification.mjs` | Claude Code 需要通知用户 | 同上 |

两者逻辑相同：从 stdin 读取 JSON payload，提取 `session_id`，写入 `.nezha/events/{session_id}.json`。

选择 Node.js 是因为 Claude Code 本身依赖 Node.js，跨平台必定可用（早期 bash + jq 方案在 Windows 上不可用）。

#### 配置注入

`inject_hooks_config()` 修改项目级 `.claude/settings.local.json`，注册 hooks：

```json
{
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "node ... nezha-hook-stop.mjs", "timeout": 5 }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "node ... nezha-hook-notification.mjs", "timeout": 5 }] }]
  }
}
```

在 `run_task` 和 `resume_task` 中调用 `ensure_hook_scripts()` + `inject_hooks_config()`。

#### 事件监听

`spawn_hooks_event_watcher()` 在独立线程中运行：

1. 使用 `notify` crate 监听 `.nezha/events/` 目录变化
2. 检测到新 `.json` 文件 → 解析 `session_id`
3. 通过 `claude_sessions` 映射匹配 `task_id`
4. emit `"task-status"` 事件，status = `"idle"`
5. 处理完后删除 event 文件
6. 任务结束后自动清理

同时有轮询兜底：如果 `notify` watcher 创建失败，每 500ms 扫描一次目录。

### 3.2 Session JSONL 兜底 (`src-tauri/src/session.rs`)

作为 hooks 的补充路径：

- 解析 Claude Code 的 JSONL 会话文件
- 检测 `stop_reason: "end_turn"` → emit `task-status: idle`
- 不依赖 hooks，直接从会话文件解析

### 3.3 原生通知 (`src-tauri/src/lib.rs`)

使用 `user-notify` crate（v0.4）统一全平台桌面通知 API。

#### 依赖

```toml
user-notify = "0.4"
```

内部实现：
- Windows：WinRT Toast（`windows` crate）
- macOS：`objc2-user-notifications`
- Linux：`notify-rust`（XDG Desktop Notification）

#### 初始化（`setup` 阶段）

```rust
let notif_manager = get_notification_manager("com.hanshutx.nezha".into(), None);
let app_handle = app.handle().clone();
notif_manager.register(
    Box::new(move |response| {
        if matches!(response.action, NotificationResponseAction::Default) {
            app_handle.emit("notification-clicked", response.user_info.clone());
            // 恢复窗口
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
    }),
    vec![],
).ok();
app.manage(NotificationState { manager: notif_manager });
```

- `get_notification_manager()` 根据平台返回对应的实现（失败时返回空 mock）
- `register()` 设置点击回调，点击时 emit `notification-clicked` 事件携带 `user_info`（含 `projectId`、`taskId`）
- `NotificationState` 通过 `app.manage()` 注入 Tauri 状态

#### `send_native_notification` 命令

```rust
#[tauri::command]
async fn send_native_notification(
    state: tauri::State<'_, NotificationState>,
    title: String, body: String,
    project_id: String, task_id: String,
) -> Result<(), String>
```

通过 `NotificationBuilder` 构造通知，`user_info` 嵌入 `projectId` 和 `taskId`，点击时原样返回给回调。

#### AUMID 设置（Windows）

```rust
#[cfg(target_os = "windows")]
SetCurrentProcessExplicitAppUserModelID("com.hanshutx.nezha")
```

确保通知来源正确显示为 NeZha（安装版生效，dev 模式仍显示 PowerShell）。

---

## 四、前端实现

### 4.1 状态判定 (`src/App.tsx`)

```typescript
// 需要 attention badge 的状态（排序优先 + 未读圆点）
function isAttentionStatus(status: TaskStatus): boolean {
  return status === "input_required" || status === "idle"
      || status === "detached" || status === "interrupted";
}

// 需要弹通知的状态（桌面通知 + Toast）
function shouldNotifyStatus(status: TaskStatus): boolean {
  return status === "done" || status === "failed" || status === "idle";
}
```

设计原则：中间状态（`input_required`、`detached`、`interrupted`）只显示 attention badge，不弹通知，避免重复弹窗。

### 4.2 通知分发流程

`task-status` 事件监听器内的核心逻辑：

```
task-status 事件到达
  │
  ├─ 非 shouldNotifyStatus? → 跳过（只更新 attention 状态）
  │
  ├─ 设置检查
  │   ├─ ns.enabled == false? → 跳过
  │   └─ ns.types[status] == false? → 跳过
  │
  ├─ 去重：同一任务同一状态 5 秒内不重复
  │
  ├─ 窗口不可见 或 失焦 + system 开关
  │   └─ sendDesktopNotification(title, body, projectId, taskId)
  │
  └─ 窗口可见 + 获焦 + !isSelected + inApp 开关
      └─ showToast(body, toastType, onClick)
```

### 4.3 `sendDesktopNotification()` — 两级策略

```typescript
async function sendDesktopNotification(title, body, projectId, taskId) {
  try {
    await invoke("send_native_notification", { title, body, projectId, taskId });
  } catch {
    // Web Notification API 兜底
    const n = new window.Notification(title, { body });
    n.onclick = () => { n.close(); getCurrentWindow().setFocus(); };
  }
}
```

- 第一级：`user-notify` 统一全平台（Windows/macOS/Linux）
- 第二级：Web Notification API 兜底（仅当 Rust 命令失败时触发）

### 4.4 点击跳转

#### user-notify 路径（全平台）

1. `user-notify` 检测到用户点击通知
2. Rust 回调：emit `"notification-clicked"` 携带 `{ projectId, taskId }` + 恢复窗口
3. 前端 `listen("notification-clicked")` → 直接用 payload 导航

```typescript
listen<{ projectId: string; taskId: string }>("notification-clicked", (event) => {
  const { projectId, taskId } = event.payload;
  if (projectId && taskId) {
    navigateToTaskRef.current(projectId, taskId);
  }
});
```

#### Toast 点击

- `showToast(body, type, () => navigateToTask(projectId, taskId))`
- 点击 Toast → 执行 onClick → 跳转 + dismiss

### 4.5 `navigateToTask()`

ref 包裹的跳转函数，每次渲染更新闭包保证 state 最新：

1. 查找任务所在项目
2. `setActiveProject()` — 切换到目标项目
3. `mountProject()` — 挂载项目视图
4. `updateProjectView()` — 设置 selectedTaskId
5. 清除 `hasUnreadEvent`

### 4.6 Toast 组件 (`src/components/Toast.tsx`)

| 特性 | 实现 |
|------|------|
| 位置 | 从 `localStorage("nezha:notificationSettings").toastPosition` 读取 |
| 四角布局 | `position: fixed` + top/bottom/left/right 组合 |
| 动画 | CSS `toast-{left|right}-{in|out}` 方向性滑入滑出 |
| 背景 | 实色背景 `var(--toast-{type}-bg)` + 前景色 `var(--toast-{type}-fg)` |
| 图标 | lucide-react：CheckCircle2 / AlertTriangle / AlertCircle / Info |
| 进度条 | 底部 2px 高度，4500ms 线性收缩 |
| 容量 | 最多 3 条（保留最后 2 条 + 新增 1 条） |
| 跨组件同步 | `CustomEvent("toast-position-changed")`，设置面板变更时触发 |

### 4.7 通知设置面板 (`src/components/app-settings/NotificationPanel.tsx`)

| 设置项 | 字段 | 说明 |
|--------|------|------|
| 总开关 | `enabled` | 关闭后所有通知静默，子项全部 disabled |
| 应用内通知 | `inApp` | 控制 Toast 弹窗 |
| 系统通知 | `system` | 控制桌面通知 |
| 通知声音 | `sound` | 控制应用内 Toast 声音；系统桌面通知使用系统自身音效 |
| Toast 位置 | `toastPosition` | 四角选择器（28x28px 按钮 + 8x8px 指示点） |
| 类型过滤 | `types.done / failed / idle` | 每种通知类型独立开关 |

设置持久化到 `localStorage("nezha:notificationSettings")`，通过 props 从 `App.tsx` 穿透到 `NotificationPanel`。

### 4.8 通知设置数据模型 (`src/types.ts`)

```typescript
export type ToastPosition = "bottom-right" | "bottom-left" | "top-right" | "top-left";

export interface NotificationSettings {
  enabled: boolean;
  inApp: boolean;
  system: boolean;
  sound: boolean;
  toastPosition: ToastPosition;
  types: { done: boolean; failed: boolean; idle: boolean };
}

export const DEFAULT_NOTIFICATION_SETTINGS: NotificationSettings = {
  enabled: true, inApp: true, system: true, sound: true,
  toastPosition: "bottom-right",
  types: { done: true, failed: true, idle: true },
};
```

---

## 五、样式系统

### 5.1 Toast 颜色变量 (`src/styles/themes.css`)

每个主题定义 8 个变量（4 类型 x 2 属性）：

| 变量 | 亮色主题 | 暗色主题 |
|------|---------|---------|
| `--toast-success-bg` | `#dcfce7` | `#14332a` |
| `--toast-success-fg` | `#15803d` | `#3dd68c` |
| `--toast-error-bg` | `#fee2e2` | `#331a1a` |
| `--toast-error-fg` | `#b91c1c` | `#ff7b7b` |
| `--toast-warning-bg` | `#fef3c7` | `#332b14` |
| `--toast-warning-fg` | `#a16207` | `#f5a623` |
| `--toast-info-bg` | `#dbeafe` | `#1a2340` |
| `--toast-info-fg` | `#1d4ed8` | `#7f9aff` |

### 5.2 Toast 动画 (`src/App.css`)

四个方向性动画：

```css
@keyframes toast-right-in  { from { transform: translateX(100%); opacity: 0; } to { transform: none; opacity: 1; } }
@keyframes toast-right-out { from { transform: none; opacity: 1; } to { transform: translateX(100%); opacity: 0; } }
@keyframes toast-left-in   { from { transform: translateX(-100%); opacity: 0; } to { transform: none; opacity: 1; } }
@keyframes toast-left-out  { from { transform: none; opacity: 1; } to { transform: translateX(-100%); opacity: 0; } }
@keyframes toast-progress  { from { width: 100%; } to { width: 0%; } }
```

根据 Toast 位置的左右方向选择对应的 in/out 动画。

---

## 六、i18n

### 通知文案

| Key | English | 中文 |
|-----|---------|------|
| `taskNotif.done` | Task completed | 任务完成 |
| `taskNotif.failed` | Task failed | 任务失败 |
| `taskNotif.idle` | Output complete, awaiting reply | 输出完毕，待回复 |

### 设置面板文案

| Key | English | 中文 |
|-----|---------|------|
| `notif.masterToggle` | Enable Notifications | 启用通知 |
| `notif.masterToggleDesc` | Turn off to suppress all task notifications | 关闭后所有任务通知将被静默 |
| `notif.inAppToggle` | In-App Notifications | 应用内通知 |
| `notif.inAppToggleDesc` | Show toast popups within the app | 在应用内显示 Toast 弹窗 |
| `notif.systemToggle` | System Notifications | 系统通知 |
| `notif.systemToggleDesc` | Show desktop notifications when the window is unfocused | 窗口失焦时显示桌面通知 |
| `notif.soundToggle` | Notification Sound | 通知声音 |
| `notif.soundToggleDesc` | Play a soft sound for in-app toast notifications | 应用内 Toast 弹出时播放提示音 |
| `notif.toastPosition` | Toast Position | Toast 位置 |
| `notif.typeFilterLabel` | Notification Types | 通知类型 |
| `notif.typeDone` | Task completed | 任务完成 |
| `notif.typeFailed` | Task failed | 任务失败 |
| `notif.typeIdle` | Waiting for input | 等待输入 |

---

## 七、跨平台架构

### Codex 适配状态

Codex 不提供 Claude Code hooks 等价能力，因此通知模块不能复用 `.nezha/hooks/` 方案。当前 Codex 路径已经具备以下基础：

- `pty.rs` 会为 Codex 启动 session watcher，但不会注入 hooks，也不会启动 hooks event watcher。
- `session.rs` 会发现 `<project>/.codex/sessions` 与 `~/.codex/sessions` 下的 `rollout-*.jsonl`。
- Codex watcher 已能识别 `request_user_input`、需要确认的 `exec_command` / `apply_patch`，并在需要用户处理时 emit `input_required`。
- 任务进程退出时仍由现有生命周期路径触发 `done` / `failed` 通知。

缺口是 Codex watcher 目前只在 `input_required` 与 `running` 之间切换，没有 emit `idle`。下一步应基于 Codex JSONL 增加一层状态 reducer：跟踪本轮用户消息后的函数调用、函数调用输出、assistant 文本消息和确认状态，只在“没有未完成工具调用、没有确认/用户输入请求、出现最终 assistant 文本”时 emit `idle`，并对同一轮对话去重。

### 桌面通知发送 — user-notify 统一

```
sendDesktopNotification()
  │
  ├─ 1. send_native_notification (Rust → user-notify crate)
  │     ✅ Windows: WinRT Toast
  │     ✅ macOS:   objc2-user-notifications
  │     ✅ Linux:   notify-rust (XDG)
  │     → 点击回调统一通过 "notification-clicked" Tauri 事件处理
  │
  └─ 2. window.Notification (Web API，仅兜底)
        ✅ 全平台: 发送 + onclick 回调
        ❌ 无最小化恢复，来源显示为 webview
```

### 跨平台状态总结

| 功能 | Windows | macOS | Linux |
|------|---------|-------|-------|
| Claude Code hooks 事件检测 | DONE | DONE | DONE |
| Claude Code Session JSONL 兜底 | DONE | DONE | DONE |
| Codex input_required 检测 | DONE | DONE | DONE |
| Codex idle 检测 | TODO | TODO | TODO |
| idle 状态 + attention badge | DONE | DONE | DONE |
| 应用内 Toast | DONE | DONE | DONE |
| 通知设置面板 | DONE | DONE | DONE |
| 通知声音 | DONE | DONE | DONE |
| 桌面通知发送 | DONE | DONE | DONE |
| 通知点击跳转 | DONE | DONE | DONE |
| 最小化恢复 | DONE | DONE | DONE |

---

## 八、已知限制

| 限制 | 影响范围 | 说明 |
|------|---------|------|
| dev 模式通知来源显示 PowerShell | Windows dev | 安装版正常，Windows 平台限制 |
| Codex 不支持 hooks | 全平台 | 不是缺陷；Codex 需要基于 session JSONL 适配 idle，而不是注入 Claude Code hooks |
| `user-notify` LGPL-3.0 许可证 | 全平台 | 静态链接需注意合规，或作为动态库加载 |

---

## 九、文件改动总览

| 文件 | 说明 |
|------|------|
| `src-tauri/src/hooks.rs` | Hook 脚本生成、配置注入、事件文件监听 |
| `src-tauri/src/lib.rs` | user-notify 初始化、send_native_notification、AUMID |
| `src-tauri/src/pty.rs` | run_task/resume_task 中注入 hooks + 启动监听 |
| `src-tauri/src/session.rs` | end_turn 兜底检测 |
| `src-tauri/Cargo.toml` | user-notify、windows、notify crate |
| `src-tauri/capabilities/default.json` | 权限配置 |
| `src/App.tsx` | 通知分发、点击跳转、去重、设置 |
| `src/App.css` | Toast 方向性动画 |
| `src/components/Toast.tsx` | 实色背景、方向动画、位置读取 |
| `src/components/app-settings/NotificationPanel.tsx` | 通知设置面板 |
| `src/components/AppSettingsDialog.tsx` | 通知导航项 |
| `src/types.ts` | TaskStatus、NotificationSettings、ToastPosition |
| `src/styles/themes.css` | Toast 背景/前景色变量 |
| `src/i18n.tsx` | 通知文案、idle 文案 |
| `src/components/StatusIcon.tsx` | idle 用 Clock 图标 |
| `src/components/task-panel/TaskList.tsx` | idle 排序优先级 |
| `src/components/task-panel/TaskListItem.tsx` | idle 的 badge 颜色 |
| `src/components/ProjectRail.tsx` | 项目导航栏 attention 指示器 |
| `src/components/TaskPanel.tsx` | attention 指示器、通知设置 props |
| `src/components/RunningView.tsx` | idle 视为活跃状态 |
| `src/components/SidebarFooterActions.tsx` | 通知设置 props 传递 |
| `src/components/ProjectPage.tsx` | 通知设置 props 传递 |
| `src/components/WelcomePage.tsx` | 通知设置 props 传递 |
