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

- [ ] 桌面通知点击跳转到对应任务（`sendDesktopNotification` 的 projectId/taskId 参数已预留）
- [ ] Codex 支持（当前仅 Claude Code）
- [ ] 清理 `.nezha/hooks/` 中的旧 `.sh`/`.cjs` 文件
- [ ] Windows 兼容性长期验证（hooks 在不同 Claude Code 版本上的行为）
- [ ] 通知开关（用户可能不想每个 idle 都弹通知）
