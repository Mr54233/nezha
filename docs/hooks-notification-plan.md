# Claude Code Hooks 任务通知方案

## 问题

当前通知系统仅依赖进程级事件（进程退出 → done/failed）和 session JSONL 解析（`stop_reason == "tool_use"` → input_required），无法检测 agent 完成一轮输出后等待用户输入的场景。

## 方案

利用 Claude Code 原生 hooks 系统，在 agent 完成（`Stop` 事件）和空闲等待（`Notification` 事件）时，通过 hook 脚本将事件写入文件，Nezha 后端用 `notify` crate 监听并推送通知。

### 架构

```
Claude Code 进程 (Nezha 通过 PTY 启动)
  │
  ├─ Stop 事件 ──→ hook 脚本 ──→ 写入 .nezha/events/{session_id}.json
  ├─ Notification ──→ hook 脚本 ──→ 写入 .nezha/events/{session_id}.json
  │
  ▼
Nezha 后端 (notify crate file watcher)
  │
  ├─ 读取事件 → session_id 匹配 task_id → emit("task-status", {status: "idle"})
  │
  ▼
前端 App.tsx
  │
  ├─ idle 状态 → 桌面通知 / Toast + hasUnreadEvent
  └─ 用户切回 → 清除未读
```

### Hook 脚本

写入项目 `.nezha/hooks/`，使用 `$CLAUDE_PROJECT_DIR` 定位项目路径：

- `nezha-hook-stop.sh` — Stop 事件时写入 `{event, session_id, ts}`
- `nezha-hook-notification.sh` — Notification 事件时写入 `{event, session_id, message, ts}`

### Hooks 配置注入

写入项目 `.claude/settings.local.json`（不提交到 git），格式：

```json
{
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/.nezha/hooks/nezha-hook-stop.sh", "timeout": 5 }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/.nezha/hooks/nezha-hook-notification.sh", "timeout": 5 }] }]
  }
}
```

每次 `run_task` 前注入，确保配置存在。

### 新增状态

`TaskStatus` 新增 `"idle"` — agent 完成一轮输出，等待用户输入，进程仍存活。

### 涉及文件

| 文件 | 操作 |
|------|------|
| `src-tauri/src/hooks.rs` | 新建 — 脚本生成、配置注入、事件监听 |
| `src-tauri/src/lib.rs` | 修改 — 注册模块 |
| `src-tauri/src/pty.rs` | 修改 — run_task 中调用 |
| `src/types.ts` | 修改 — TaskStatus 添加 idle |
| `src/App.tsx` | 修改 — 处理 idle 状态通知 |
| `src/components/task-panel/TaskListItem.tsx` | 修改 — idle 状态 badge |
