# 通知系统完善 (2026-05-18)

## 提交记录

| 提交 | 说明 |
|------|------|
| `31221d7` | feat: Windows native notification click-to-navigate via WinRT Toast |
| `054241d` | style: redesign toast notifications with theme-aware card style |
| `b3fe1fa` | feat: add notification settings panel |
| `2321001` | fix: restore window from minimized state on notification click |
| `c497048` | feat: toast position setting, solid color backgrounds, i18n cleanup |

## 一、Windows 原生通知点击跳转

之前的通知点击跳转依赖 `tauri-plugin-notification` 的 `onAction` API，无法在 Windows 上正常工作（dev 模式通知来源显示 PowerShell，点击无反应）。

**方案**：绕过 Tauri 插件的 JS 层，改用 Rust 侧直接调用 WinRT Toast Notification API。

- **进程 AUMID** — `setup` 中调用 `SetCurrentProcessExplicitAppUserModelID("com.hanshutx.nezha")`，安装版通知来源正确显示为 NeZha
- **WinRT Toast** — 用 `windows` crate 创建 `ToastNotification`，通过 `CreateToastNotifierWithId` 指定 AUMID
- **Activated 回调** — 点击通知时 emit `notification-clicked` Tauri 事件 + `window.set_focus()`，前端监听后执行 `navigateToTask` 跳转到对应任务
- **`std::mem::forget(toast)`** — 防止 Toast 对象被 drop 后回调失效
- **兜底路径** — `pendingNotificationNav` ref + `window focus` / `onFocusChanged` 事件，用户手动切回应用也能跳转

**最小化恢复**（`2321001`）— 初版只调了 `set_focus()`，无法从最小化状态恢复窗口。加 `unminimize()` 解决。

**已知限制**：
- dev 模式通知来源仍显示 PowerShell（Windows 平台限制，安装版正常）
- `std::mem::forget(toast)` 每次通知泄漏一个 COM 对象，量极小可接受
- 非 Windows 平台走 `window.Notification` fallback

## 二、Toast 样式重构

**`054241d`** — 重新设计 toast 通知外观：
- 卡片式设计，`var(--bg-card)` 背景 + `var(--border-medium)` 边框
- 左侧 3px 彩色边框标识类型（success/error/warning/info）
- 类型对应图标（CheckCircle2/AlertTriangle/AlertCircle/Info）
- 滑入/滑出 CSS 动画 + 底部进度条

**`c497048`** — 用户反馈背景透明太丑，改为实色背景：
- 亮色主题：浅绿/浅红/浅黄/浅蓝实色底
- 暗色主题：深色调实色底
- 文字颜色改为对应前景色，不再用 `text-primary`
- 新增 `--toast-*-bg` / `--toast-*-fg` 主题变量（8 个/主题）

## 三、通知设置面板

**`b3fe1fa`** — 在应用设置中新增"通知"菜单项：
- **总开关** — 关闭后所有通知静默
- **应用内通知** — 控制是否显示 toast 弹窗
- **系统通知** — 控制是否发送桌面通知
- **Toast 位置选择**（`c497048` 新增）— 四角可选，带方向滑入动画
- **类型过滤** — done/failed/idle 各类型独立开关
- 设置通过 props 从 App.tsx → ProjectPage/WelcomePage → TaskPanel → SidebarFooterActions → AppSettingsDialog → NotificationPanel 层层传递
- 持久化到 localStorage（key: `nezha:notificationSettings`）
- App.tsx 的通知发送逻辑根据设置决定是否发送

## 四、文案优化

- `idle` 通知文案改为 **"输出完毕，待回复"** / **"Output complete, awaiting reply"**（原："任务等待输入中"）
- Toast 消息去掉状态前缀拼接，只显示任务名（原 `"任务完成: xxx"` → `"xxx"`）
- 删除 3 个未使用的 i18n key（`inputRequired`、`detached`、`interrupted`）

## 文件改动汇总

| 文件 | 涉及提交 | 说明 |
|------|---------|------|
| `src-tauri/src/lib.rs` | `31221d7`, `2321001` | `send_native_notification`（WinRT Toast + Activated 回调）、AUMID 设置、`unminimize()` |
| `src-tauri/Cargo.toml` | `31221d7` | 新增 `windows` crate（Windows 目标） |
| `src-tauri/capabilities/default.json` | `31221d7` | 新增通知相关权限 |
| `src/App.tsx` | `31221d7`, `b3fe1fa`, `c497048` | 通知事件监听、去重、设置 state/props 穿透、toast 消息格式 |
| `src/components/Toast.tsx` | `054241d`, `c497048` | 实色背景、方向动画、位置读取、ToastPosition 类型 |
| `src/components/app-settings/NotificationPanel.tsx` | `b3fe1fa`, `c497048` | 通知设置面板（开关+位置选择+类型过滤） |
| `src/components/AppSettingsDialog.tsx` | `b3fe1fa` | 新增通知导航项 |
| `src/components/app-settings/types.ts` | `b3fe1fa` | NavKey 新增 "notifications" |
| `src/types.ts` | `b3fe1fa`, `c497048` | NotificationSettings 接口 + ToastPosition 类型 |
| `src/styles/themes.css` | `c497048` | toast 背景和前景色变量（亮/暗色各 8 个） |
| `src/App.css` | `054241d`, `c497048` | 方向性 toast 动画（left/right in/out） |
| `src/i18n.tsx` | `b3fe1fa`, `c497048` | 通知设置文案、idle 文案更新、删除无用 key |
| `src/components/SidebarFooterActions.tsx` | `b3fe1fa` | 通知设置 props 传递 |
| `src/components/TaskPanel.tsx` | `b3fe1fa` | 通知设置 props 传递 |
| `src/components/ProjectPage.tsx` | `b3fe1fa` | 通知设置 props 传递 |
| `src/components/WelcomePage.tsx` | `b3fe1fa` | 通知设置 props 传递 |

---

# 上游生命周期模型对齐 & 通知精简 (2026-05-16)

## 背景

上游 main 已合并至 v0.3.6，新增了 `detached`（终端断开）、`interrupted`（应用重启中断）、`attentionRequestedAt`（统一关注时间戳）等任务生命周期模型。需要将通知系统与上游对齐，同时解决通知重复弹出的问题。

## 改动

### 1. 通知触发精简

**问题**：任务完成时弹出 3 个通知（`input_required` → `idle` → `done` 各触发一次）。

**方案**：通知只在终态触发，中间态由 attention badge 承载。

- 新增 `isAttentionStatus()`（`input_required` / `idle` / `detached` / `interrupted`）— 驱动 attention badge、排序、未读圆点
- 新增 `shouldNotifyStatus()`（仅 `done` / `failed`）— 驱动桌面通知和 toast
- `input_required`、`idle`、`detached`、`interrupted` 不再弹通知，只显示 attention badge

### 2. `updateTaskStatus` 对齐

- `attentionRequestedAt` 改由 `isAttentionStatus()` 驱动，覆盖 `detached`/`interrupted`
- `hasUnreadEvent` 跟 attention 状态 + 终态同步

### 3. TaskList / ProjectRail 简化

- `isAttention` 判断从硬编码状态枚举改为 `task.attentionRequestedAt != null`
- ProjectRail attention badge 同步简化

### 4. i18n

- 新增 `taskNotif.detached`（终端断开连接）和 `taskNotif.interrupted`（任务被中断）翻译（虽然当前不触发通知，但保留以备后用）
- `unreadBadgeColor` 新增 `detached`/`interrupted` 映射到 warning 色

## 未解决问题

- **恢复任务后任务从列表消失**：`detached` 任务恢复后状态变为 `pending`，从 attention 组掉出。若 `createdAt` 超出展示窗口，任务不可见。已尝试让 `pending`/`running` 绕过窗口过滤，但仍有问题，需进一步排查。
- `TaskList.tsx` 中 `isAttention` 的 `attentionRequestedAt` 方案可能在某些边界情况下与上游状态不同步，可能需要回退为 status 检查。

## 文件改动

| 文件 | 说明 |
|------|------|
| `src/App.tsx` | 新增 `isAttentionStatus`/`shouldNotifyStatus`，通知触发改为仅 done/failed |
| `src/components/task-panel/TaskList.tsx` | `isAttention` 改为 `attentionRequestedAt != null`，pending/running 绕过窗口过滤 |
| `src/components/task-panel/TaskListItem.tsx` | `unreadBadgeColor` 新增 detached/interrupted |
| `src/components/ProjectRail.tsx` | attention badge 判断简化 |
| `src/i18n.tsx` | 新增 detached/interrupted 通知文案 |

---

# 通知点击跳转开发日志 (2026-05-12)

## 目标

让用户点击通知后直接跳转到对应任务，包括两条路径：
1. **桌面通知**（系统通知）点击 → 切换到对应项目并选中任务
2. **应用内 Toast** 点击 → 跳转到对应任务

## 完成情况

| 目标 | 状态 | 说明 |
|------|------|------|
| 应用内 Toast 点击跳转 | ✅ 已完成 | `showToast` 第三参数传 `onClick` 回调，点击后切换项目+选中任务+清未读 |
| `navigateToTask` 抽取 | ✅ 已完成 | ref 包裹的跳转函数，每次渲染更新闭包，统一供 Toast / 桌面通知共用 |
| 桌面通知点击跳转 | ❌ 未完成 | Windows 上通知回退到 PowerShell，点击无法激活 Tauri 窗口（见下方） |

## 已完成的改动

### `src/App.tsx`

1. **`navigateToTaskRef`** — ref 包裹的跳转函数，内容为：查找项目 → `setActiveProject` → `mountProject` → `updateProjectView`（选中任务）→ 清 `hasUnreadEvent`。每次渲染更新闭包以保证 state 是最新的。

2. **`pendingNotificationNav`** — 发送桌面通知前存储 `{ projectId, taskId }`，用于窗口获焦时的兜底导航。

3. **`onFocusChanged` 监听** — 通过 `getCurrentWindow().onFocusChanged` 监听 Tauri 窗口焦点变化。窗口获焦时检查 `pendingNotificationNav`，有值则自动跳转到对应任务（dev 模式兜底路径）。

4. **`onAction` 监听（生产模式）** — `@tauri-apps/plugin-notification` 的 `onAction` 回调，读取 `sendNotification` 传入的 `extra: { projectId, taskId }`，调用 `getCurrentWindow().setFocus()` + 跳转。预期在生产构建中生效。

5. **`sendDesktopNotification` 参数恢复** — 函数签名恢复 `(title, body, projectId, taskId, permissionRef)`，`sendNotification` 调用加 `extra: { projectId, taskId }` 供 `onAction` 读取。

6. **Toast onClick** — `showToast` 调用补传第三参数 `() => navigateToTaskRef.current(task.projectId, task_id)`。

### `src-tauri/tauri.conf.json`

- 打包目标从 `"all"` 改为 `["nsis"]`（WiX 下载超时，改用 NSIS 打包器）。

## 桌面通知点击跳转 — 未完成原因

### 根因

Windows 上 `tauri-plugin-notification` 使用 WinRT Toast Notification API。该 API 要求：

1. **AUMID（Application User Model ID）** 必须正确设置在进程上
2. **开始菜单快捷方式** 必须存在，且快捷方式的 AUMID 属性与进程 AUMID 一致

只有两个条件同时满足，Windows 才能将通知与正确的应用关联，点击通知时才能激活对应窗口并传递点击事件。

### 实际表现

- **dev 模式**（`pnpm tauri dev`）：通知来源显示 "PowerShell"，点击后无反应（既不激活窗口，也不触发 `onAction`）
- **release exe 直接运行**（未安装）：同上，通知仍回退到 PowerShell AUMID
- **NSIS 安装版**：已构建安装包并安装测试，但通知来源仍显示 "PowerShell"，点击仍无反应

### 已尝试的方案

| 方案 | 结果 |
|------|------|
| `onAction` + `extra` 数据 | `onAction` 回调未触发，通知与 Tauri 应用未关联 |
| `window.addEventListener("focus")` + pending 导航 | 窗口未被激活，focus 事件不触发 |
| `getCurrentWindow().onFocusChanged` + pending 导航 | 同上，Tauri 窗口焦点未变化 |
| NSIS 安装后测试 | 安装后通知仍显示 PowerShell，AUMID 可能未正确注册到快捷方式 |

### 可能的后续方向

1. **Rust 侧设置进程 AUMID** — 在 `setup` 中调用 `SetCurrentProcessExplicitAppUserModelID("com.hanshutx.nezha")`，强制设置进程级 AUMID
2. **安装时注册快捷方式 AUMID** — 通过 NSIS 自定义脚本在安装时创建带 AUMID 属性的快捷方式
3. **放弃 WinRT Toast** — 改用 `notify-rust` 或自定义通知机制（如系统托盘气泡通知），绕开 AUMID 限制
4. **窗口获焦兜底** — 目前 `onFocusChanged` 代码已就绪，当用户手动切回应用时会自动跳转到通知对应的任务。需要用户实际测试此路径是否工作

---

# Hooks 通知系统开发日志 (2026-05-09)

## 背景

`feat/task-notifications` 分支之前的通知触发机制仅依赖进程级事件（进程退出 → done/failed）和 session JSONL 解析（`stop_reason == "tool_use"` → input_required），无法检测 **agent 完成一轮输出后等待用户输入**的场景。这是通知系统最核心的需求。

## 方案选型

参考了 Warp 终端的开源实现（`warpdotdev/claude-code-warp`），他们利用 Claude Code 原生 hooks 系统解决了同样的问题。最终采用双路径方案：

1. **主路径**：Claude Code hooks（Stop / Notification 事件）→ hook 脚本写文件 → `notify` crate 监听 → emit idle
2. **兜底路径**：session JSONL 的 `end_turn` stop_reason → 直接触发 idle

## 架构

```
Claude Code 进程 (PTY 子进程)
  ├─ Stop hook (.mjs) ───→ .nezha/events/{session_id}.json
  ├─ Notification hook ──→ .nezha/events/{session_id}.json
  └─ session JSONL ───→ stop_reason: "end_turn" (兜底)
        │
        ▼
Nezha 后端 (notify crate 文件监听 / session watcher)
  │
  ├─ session_id 匹配 task_id → emit("task-status", {status: "idle"})
  │
  ▼
前端 App.tsx
  ├─ 窗口不可见/失焦 → sendDesktopNotification (原生桌面通知)
  ├─ 窗口可见+获得焦点 → showToast (应用内 toast)
  └─ 更新 hasUnreadEvent / attentionRequestedAt
```

## 开发历程与踩坑

### 坑 1: Hook 脚本用了 bash + jq — Windows 上不可用

初始方案用 bash 脚本 + `jq` 解析 JSON。部署后发现：
- Windows 系统没有 `jq`
- Claude Code 在 Windows 上可能不走 bash 执行 hook command

**解决**：改用 Node.js 脚本（Claude Code 本身依赖 Node.js，必定可用）。

### 坑 2: .js 扩展名 + require — 项目 "type": "module" 冲突

项目 `package.json` 声明了 `"type": "module"`，`.js` 文件被当作 ESM，`require()` 报错。

**解决**：先改 `.cjs`（允许 require），后改用 ESM `import` 语法 + `.mjs` 扩展名（最终方案）。

### 坑 3: 窗口被遮挡时 `visibilityState` 仍为 "visible"

`document.visibilityState` 只在窗口最小化时变为 "hidden"。窗口被其他窗口遮挡时仍是 "visible"，导致桌面通知不触发。

**解决**：在通知分发时加 `!document.hasFocus()` 检查。不维护 ref，直接在分发那一刻检查：
```typescript
if (!isWindowActive.current || !document.hasFocus()) {
  sendDesktopNotification(...);
} else if (...) {
  showToast(...);
}
```

### 坑 4: 应用内通知在打开设置等页面时不显示

原来只在 `!isSelected` 时弹 toast，但打开设置对话框时任务仍"选中"，toast 不弹。

**解决**：`idle` 和 `input_required` 状态始终弹 toast，不判断选中状态。

## 文件清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `src-tauri/src/hooks.rs` | 新建 | Hook 脚本生成、settings.local.json 注入、notify crate 事件文件监听 |
| `src-tauri/src/lib.rs` | 修改 | 注册 `hooks` 模块 |
| `src-tauri/src/pty.rs` | 修改 | `run_task`/`resume_task` 中注入 hooks + 启动事件监听 |
| `src-tauri/src/session.rs` | 修改 | `process_claude_session_line` 添加 `end_turn` → idle 兜底检测 |
| `src/types.ts` | 修改 | `TaskStatus` 添加 `"idle"`，`isActiveTaskStatus` 包含 idle |
| `src/App.tsx` | 修改 | idle 通知分发 + hasFocus() 检查 + 始终弹 idle toast |
| `src/components/StatusIcon.tsx` | 修改 | idle 用 Clock 图标 |
| `src/components/task-panel/TaskListItem.tsx` | 修改 | idle 的 label 和 badge 颜色 |
| `src/components/task-panel/TaskList.tsx` | 修改 | idle 排序优先级（和 input_required 同级） |
| `src/components/TaskPanel.tsx` | 修改 | attention 指示器包含 idle |
| `src/components/ProjectRail.tsx` | 修改 | 项目导航栏 attention 指示器包含 idle |
| `src/components/RunningView.tsx` | 修改 | idle 视为活跃状态 |
| `src/i18n.tsx` | 修改 | 英/中翻译：status.idle、taskNotif.idle |

## 后续 TODO

- [ ] 桌面通知点击跳转到对应任务（见「通知点击跳转」章节）
- [ ] Codex 支持（当前仅 Claude Code）
- [ ] 清理 `.nezha/hooks/` 中的旧 `.sh`/`.cjs` 文件
- [ ] Windows 兼容性长期验证（hooks 在不同 Claude Code 版本上的行为）
- [ ] 通知开关（用户可能不想每个 idle 都弹通知）
