# Nezha 任务通知系统

> 分支 `feat/task-notifications` · 状态：功能完成

---

## 一、数据流

```
Claude Code 进程 (PTY 子进程)
  │
  ├─ Stop hook ──→ notification-hook.mjs ─→ .nezha/events/{session_id}.json
  │   stdin: { session_id, hook_event_name: "Stop", last_assistant_message }
  │   → 后端 emit("task-status", { status: "idle", hook_event, hook_message })
  │
  ├─ Notification hook (matcher: "permission_prompt")
  │   ─→ notification-hook.mjs ─→ .nezha/events/{session_id}.json
  │   stdin: { session_id, hook_event_name: "Notification", message, notification_type }
  │   → 后端 emit("task-status", { status: "input_required", hook_event, hook_message })
  │
  └─ session JSONL ─→ stop_reason: "tool_use" → emit("task-status", { status: "input_required" })
      ※ 无 hook_event 字段，仅驱动 UI 状态，不触发通知
        │
        ▼
前端 App.tsx (task-status 监听器)
  │
  ├─ hook_event 存在 + shouldNotifyStatus(done/failed/idle/input_required)?
  │   ├─ title = 任务名（无名称则取提示词前 60 字符）
  │   ├─ body = hook_message（取第一行，Markdown 剥离，120 字符截断）
  │   │       无 hook_message 时兜底状态文案（任务完成/任务失败/需要授权等）
  │   ├─ 窗口不在前台 + ns.system → sendDesktopNotification()
  │   │   ├─ invoke("send_native_notification") → user-notify crate
  │   │   └─ 失败时 → window.Notification API 兜底
  │   │
  │   └─ 窗口在前台 + !isSelected + ns.inApp → showToast()
  │       └─ 可选 playSound → Web Audio 合成风铃音色
  │
  └─ isAttentionStatus(input_required/idle/detached/interrupted)?
      └─ 更新 attentionRequestedAt / hasUnreadEvent → 驱动 badge
```

桌面通知和 Toast **互斥**：窗口不在前台（最小化/被遮挡/失焦）走桌面通知，前台走 Toast。

---

## 二、Hooks 授权（Consent）

hooks 需要修改用户的 `.claude/settings.local.json`，因此需要用户明确授权。

### 2.1 授权流程

1. 用户首次在 Nezha 里启动 Claude Code 任务 → 检查 `.nezha/config.toml` 中的 `hooks_consent` 字段
2. 字段未设置（`None`）→ 弹持久 toast："任务通知需要注入 hooks，是否授权？"，带操作按钮
3. 用户点击"授权"→ 调用 `set_hooks_consent(true)` → 写入 `hooks_consent = true` + 注入 hooks + 生成脚本
4. 用户关闭 toast → 不注入，任务正常运行，不再弹。用户可去设置 → 通知面板手动开启
5. 已有 hooks 的项目自动视为已授权（兼容老用户迁移）

### 2.2 撤销授权

用户在设置 → 通知面板关闭"允许注入 Hooks"→ 调用 `set_hooks_consent(false)` → 从 `settings.local.json` 移除 Nezha 的 hooks 条目 + 清理空配置。

### 2.3 存储

- **授权状态**：`.nezha/config.toml` 的 `agent.hooks_consent`（`true` / `false` / 未设置）
- **Tauri 命令**：`set_hooks_consent(project_path, consent)` — 设置 consent 并注入/移除 hooks
- **读取**：`read_project_config` 读取 consent 状态

---

## 三、后端实现

### 3.1 Hooks 系统 (`src-tauri/src/hooks.rs`)

通过 Claude Code hooks 获取 agent 生命周期事件，转换为任务状态通知。

**Hook 脚本**：单个 Node.js ESM 脚本 `notification-hook.mjs`，写入 `.nezha/hooks/`（Claude Code 必定依赖 Node.js，跨平台可用）。

**两个 hook 事件，各自职责不同：**

| Hook 事件 | matcher | stdin 关键字段 | emit 状态 | 触发时机 |
|-----------|---------|---------------|----------|---------|
| `Stop` | 无 | `session_id`, `last_assistant_message` | `idle` | agent 完成一轮回复（用户中断时不触发） |
| `Notification` | `permission_prompt` | `session_id`, `message`, `notification_type` | `input_required` | agent 需要用户授权 |

Notification hook 只注册了 `permission_prompt` matcher，其他类型（`idle_prompt`、`auth_success` 等）不会触发。

脚本从 stdin 提取字段，`last_assistant_message`/`message` 截断至 500 字符，`hook_event_name` 和 `notification_type` 透传，写入 `.nezha/events/{session_id}.json`。

**配置注入**：`inject_hooks_config()` 修改 `.claude/settings.local.json` 注册 hooks。采用 **append 模式**：读取已有的 hooks 数组，移除 Nezha 之前的条目（按 command 匹配），再追加新条目。用户自己配置的其他 hooks 不受影响。

**配置移除**：`remove_hooks_config()` 从 `settings.local.json` 中移除所有 Nezha 的 hooks 条目，清理空数组。用于用户撤销授权时调用。

**Consent 检查**：`has_hooks_consent()` 检查 `config.toml` 中的 `hooks_consent` 字段。如果从未设置但 hooks 已存在，自动视为已授权（兼容老用户）。

**注入时机**：`run_task` / `resume_task` 中检查 consent 后才调用注入。无 consent 时跳过 hooks 注入和 hooks event watcher。

**事件监听**：`spawn_hooks_event_watcher()` 用 `notify` crate 监听 `.nezha/events/`，检测新文件 → 匹配 task_id → 根据 `notification_type` 决定 emit `idle` 或 `input_required` → 删除 event 文件。监听失败时有 500ms 轮询兜底。

**并发安全**：多个任务的 watcher 共享同一个 events 目录。匹配到当前任务的 event 文件立即删除；不匹配的文件保留给对应 watcher 处理；超过 30 秒未被任何 watcher 处理的孤儿文件兜底清理。

**去重**：按状态类型独立去重（`idle` 和 `input_required` 各维护时间戳），同一类型 10 秒内不重复 emit。

**安全**：event 文件大小限制 64KB，超过跳过。session 不匹配时直接忽略（`resolve_task_id` 过滤非 Nezha 实例）。

**旧文件清理**：`ensure_hook_scripts()` 自动删除历史版本的旧脚本（`nezha-hook-stop.mjs`、`nezha-hook-notification.mjs`、`nezha-hook.mjs`）。

### 3.2 Session JSONL (`src-tauri/src/session.rs`)

解析 JSONL 检测 `stop_reason: "tool_use"` → emit `task-status: { status: "input_required" }`（不带 `hook_event` 字段）。此路径仅驱动前端 UI 状态变更（任务面板显示"需要确认"等），**不触发桌面通知和 Toast**。idle 检测完全由 hooks 负责，session.rs 不再触发 idle。

### 3.3 桌面通知发送 (`src-tauri/src/lib.rs`)

使用 `user-notify` crate（v0.4）统一全平台桌面通知 API：

| 平台 | 实现 |
|------|------|
| Windows | WinRT Toast |
| macOS | `objc2-user-notifications` |
| Linux | `notify-rust`（XDG） |

**初始化**（`setup` 阶段）：
- `get_notification_manager("com.hanshutx.nezha", None)` 获取平台实现
- `register()` 设置点击回调：emit `notification-clicked` + `unminimize()` + `setFocus()`
- Windows 额外调用 `SetCurrentProcessExplicitAppUserModelID` 设置 AUMID

**命令**：`send_native_notification(title, body, project_id, task_id)` — `NotificationBuilder` 构造通知，`user_info` 嵌入 `projectId` 和 `taskId` 供点击回调返回。

---

## 四、前端实现

### 4.1 通知触发判定

```typescript
function isAttentionStatus(status) {
  return ["input_required", "idle", "detached", "interrupted"].includes(status);
}
function shouldNotifyStatus(status) {
  return ["done", "failed", "idle", "input_required"].includes(status);
}
```

- **shouldNotifyStatus** → 驱动桌面通知和 Toast
- **isAttentionStatus** → 驱动 attention badge（未读圆点、排序优先）

### 4.2 通知内容

```
task-status 事件 payload: { task_id, status, hook_event, hook_message }
  │
  ├─ title = task.name ?? task.prompt.slice(0, 60)
  │
  ├─ hook_message 存在
  │   → stripMarkdown() → 取第一个非空行 → 截断至 120 字符 → 通知 body
  │
  └─ hook_message 缺失
      → 状态文案（"任务完成" / "任务失败" / "需要授权" / "输出完毕，待回复"）
```

### 4.3 分发逻辑

```
task-status 事件
  ├─ 无 hook_event? → 仅更新 UI 状态和 attention badge
  ├─ hook_event 存在 + 非 shouldNotifyStatus? → 只更新 attention badge
  ├─ ns.enabled == false || ns.types[status] == false? → 跳过
  ├─ 同任务同状态 5 秒内去重
  ├─ (!windowActive || !hasFocus) && ns.system → 桌面通知
  └─ windowActive && hasFocus && !isSelected && ns.inApp → Toast
```

### 4.4 桌面通知（两级策略）

```typescript
async function sendDesktopNotification(title, body, projectId, taskId) {
  try {
    await invoke("send_native_notification", { title, body, projectId, taskId });
  } catch {
    const n = new window.Notification(title, { body });
    n.onclick = () => { n.close(); getCurrentWindow().setFocus(); };
  }
}
```

### 4.5 点击跳转

- **桌面通知**：Rust `register` 回调 → emit `notification-clicked` + 恢复窗口 → 前端 `navigateToTaskRef`
- **Toast**：`showToast(body, type, { onClick: () => navigateToTask(taskId) })`

`navigateToTask`：查找项目 → `setActiveProject` → `mountProject` → 选中任务 → 清除未读。

### 4.6 Toast 组件 (`src/components/Toast.tsx`)

| 特性 | 实现 |
|------|------|
| 位置 | 四角可选，从 localStorage 读取 |
| 动画 | CSS 方向性滑入/滑出 |
| 背景 | 实色背景，按类型着色 |
| 进度条 | 底部 2px，4500ms 线性收缩 |
| 悬停暂停 | 鼠标悬浮时倒计时暂停 + 进度条冻结，移出后恢复（2 秒） |
| 持久模式 | 不自动消失，无进度条，用于授权提示等场景 |
| 操作按钮 | 可选 actionLabel，点击执行 onClick 后自动关闭 |
| 容量 | 最多 3 条 |
| 声音 | 可选，Web Audio API 合成风铃音色（C5 → E5 + 泛音） |

### 4.7 首次授权提醒

用户首次启动 Claude Code 任务时，如果 `hooks_consent` 从未设置，弹出一个**持久 toast**（不自动消失、带关闭按钮和"允许注入 Hooks"操作按钮）。用户授权后 consent 写入 config，hooks 自动注入。关闭 toast 后不再弹出，用户可在设置 → 通知面板中手动开启。

### 4.8 通知设置 (`src/components/app-settings/NotificationPanel.tsx`)

| 设置项 | 字段 | 说明 |
|--------|------|------|
| 允许注入 Hooks | `config.toml: hooks_consent` | 控制 hooks 注入/移除，自管理状态 |
| 总开关 | `enabled` | 关闭后所有通知静默 |
| 应用内通知 | `inApp` | 控制 Toast |
| 系统通知 | `system` | 控制桌面通知 |
| 通知声音 | `sound` | Toast 音效 |
| Toast 位置 | `toastPosition` | 四角选择器 |
| 类型过滤 | `types.done/failed/idle/input_required` | 独立开关 |

通知设置持久化到 `localStorage("nezha:notificationSettings")`。Hooks consent 持久化到 `.nezha/config.toml`。

---

## 五、未完成项

| 项目 | 说明 |
|------|------|
| Codex idle 检测 | Codex 不支持 hooks，需基于 session JSONL 状态 reducer |
| macOS 通知权限 | 未调用 `requestAuthorization`，首次可能静默失败 |
| Linux 兼容性 | 未测试 |
| dev 模式通知来源 | Windows dev 模式显示 PowerShell，安装版正常 |

---

## 六、关键文件

| 文件 | 职责 |
|------|------|
| `src-tauri/src/hooks.rs` | Hook 脚本生成、配置注入（append 模式）、移除、consent 检查、事件监听、并发安全删除、去重 |
| `src-tauri/src/config.rs` | `hooks_consent` 字段、`set_hooks_consent` 命令 |
| `src-tauri/src/lib.rs` | user-notify 初始化、`send_native_notification`、AUMID |
| `src-tauri/src/pty.rs` | `run_task`/`resume_task` 中检查 consent 后注入 hooks |
| `src-tauri/src/session.rs` | `tool_use → input_required` 检测（仅 UI 状态，不触发通知） |
| `src/App.tsx` | 通知分发、去重、跳转、hook_event 过滤、首次 consent 提醒 |
| `src/components/Toast.tsx` | Toast 组件 + 音效 + 持久模式 + 悬停暂停 + 操作按钮 |
| `src/components/app-settings/NotificationPanel.tsx` | 设置面板 + consent 开关 |
| `src/types.ts` | `NotificationSettings`（含 `input_required`）、`ToastPosition` |
