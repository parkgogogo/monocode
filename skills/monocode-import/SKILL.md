---
name: monocode-import
description: Import a Claude Code, Codex or Pi session (including the current conversation) into the local MonoCode app, with its full history visible and the next message resuming the same provider session. Use when the user wants to move or migrate a session into MonoCode, continue a terminal conversation in MonoCode, or bring old Claude/Codex/Pi chats into it.
---

# 把会话导入 MonoCode

`scripts/monocode_import.py` 做三件事：

1. 把 Claude Code、Codex 或 Pi 的会话文件，转换成 MonoCode 自己存聊天记录的格式（`sessions.blocks_json`）。
2. 在 MonoCode 的数据库里写入一行会话记录，并把 `provider_session_id` 指向原来的会话。在 MonoCode 里发下一条消息时，就会用原会话继续，上下文完整：Claude 用 `claude --resume`，Codex 用 `thread/resume`，Pi 用 `pi --session`。
3. 如果会话目录还不在 MonoCode 的项目列表里，就把它加进去。

脚本只用 Python 标准库，系统自带的 `python3`（3.9 以上）就能跑。命令里用 `-I` 运行。

## 会话文件位置

| harness | 会话文件 | `provider_session_id` |
|---|---|---|
| `claude` | `~/.claude/projects/<目录>/<uuid>.jsonl` | 文件名里的 uuid |
| `codex` | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `session_meta.payload.id`（thread id） |
| `pi` | `~/.pi/agent/sessions/<目录>/<时间>_<uuid>.jsonl` | 文件头 `session.id` |

## 流程

下面的 `$S` 指本 skill 的 `scripts/` 目录。

1. **找到要导入的会话。**
   - 导入**当前这个 Claude Code 对话**：它就是当前目录对应文件夹里最新的那个 `.jsonl`。用下面的命令列出来确认，`--session` 传 uuid 就行。
     ```sh
     python3 -I $S/monocode_import.py list --harness claude --cwd "$PWD"
     ```
   - 导入其他会话：把 `--harness` 换成 `codex` 或 `pi`；不加 `--cwd` 会列出所有目录的会话。
   - `--session` 可以是完整路径、uuid，或文件名的一部分。匹配到多个时脚本会报错，并列出候选。
2. **先试转换，什么都不写**：
   ```sh
   python3 -I $S/monocode_import.py import --harness claude --session <id> --dry-run
   ```
   把输出里的项目目录、标题和记录条数告诉用户。标题不合适就用 `--title` 换一个，标题写英文或用户的语言都行。
3. **写入真实数据库前，先征得用户同意。** 这一步会改用户的真实数据：默认是 release 版的 `~/Library/Application Support/com.monocode.desktop/monocode.db`，以及它旁边的 `monocode.db.gpui.json`。
4. **让用户退出 MonoCode 原生版和 Tauri 桌面版。** 不要自己去杀用户正在用的进程。数据库被打开时，脚本会拒绝写入，并报出占用它的进程号。
5. **导入**：
   ```sh
   python3 -I $S/monocode_import.py import --harness claude --session <id> --title "<标题>" --select-project
   ```
   - 脚本会先备份 `monocode.db.bak-<时间>` 和 `monocode.db.gpui.json.bak-<时间>`。
   - 再写入会话，聊天里的图片存到数据库旁边的 `imported/<会话 id>/` 里。
   - `--select-project` 让 MonoCode 打开时直接停在这个项目上。
   - 同一个会话导入过一次，再导会被拒绝；`--replace` 会替换掉上次导入的那条。
6. **验证**：打开 MonoCode，确认三点：项目出现在侧栏，会话在项目下面，聊天记录显示正常。有 macos-harness 的话就截图确认。
7. **导入当前对话的情况**：导入应该是这次对话的最后一步。
   - 告诉用户退出这个终端会话，之后只在 MonoCode 里继续。两边同时写同一个会话文件会出问题。
   - 导入之后终端里新产生的消息不会出现在 MonoCode 的历史里，但会保留在上下文里。

## 其他参数

| 参数 | 作用 |
|---|---|
| `--dev` | 写 dev 数据库 `~/Library/Application Support/Monocode Development/monocode.db` |
| `--db <路径>` | 写指定的数据库 |
| `--cwd <目录>` | 换一个项目目录。会话文件里没记录目录时必须指定 |
| `--mode` | 权限模式：`read-only` / `supervised`（默认）/ `auto-accept-edits` / `auto` / `full-access` |
| `--model` | 模型 id。默认是 `<harness>:default`，Pi 用会话里记录的模型 |
| `--dump <文件>` | 配合 `--dry-run`，把转换结果写成 JSON 方便检查 |

**不碰真实数据的完整演练**：先复制一份数据库，导入到副本里，再让 app 打开副本。

```sh
mkdir -p /tmp/mc-test
python3 -I -c "import sqlite3,sys; s=sqlite3.connect(sys.argv[1]); d=sqlite3.connect(sys.argv[2]); s.backup(d)" \
  "$HOME/Library/Application Support/com.monocode.desktop/monocode.db" /tmp/mc-test/monocode.db
cp "$HOME/Library/Application Support/com.monocode.desktop/monocode.db.gpui.json" /tmp/mc-test/
python3 -I $S/monocode_import.py import --harness claude --session <id> --db /tmp/mc-test/monocode.db --select-project
"/Applications/Monocode Native.app/Contents/MacOS/monocode-gpui" --monocode-db /tmp/mc-test/monocode.db --check-data
"/Applications/Monocode Native.app/Contents/MacOS/monocode-gpui" --monocode-db /tmp/mc-test/monocode.db --cwd <项目目录>
```

## 回滚

1. 退出 MonoCode。
2. 用导入时生成的两个 `.bak-<时间>` 文件，覆盖回 `monocode.db` 和 `monocode.db.gpui.json`。
3. 删掉 `imported/<会话 id>/`。

## 转换规则

- **显示**：你的消息、回复、思考过程、工具调用和工具输出都会显示。
  - 工具输出和 MonoCode 实时运行时一样，只保留最后 64KB。
  - 没有执行完的工具调用，显示为已取消。
- **不显示**：Claude 的 `<system-reminder>`、meta 消息、后台任务通知、子 agent 内部消息，以及 MonoCode 自动加在提示词前的 visuals 说明。
- **上下文压缩**：显示成一条 “Context compacted” 系统提示。
- **Pi**：会话是树结构，只导入最新的那条分支。
- **Codex**：
  - 只支持带 `event_msg` / `item_completed` 记录的 rollout 格式（新版 Codex），旧格式会报错。
  - Codex 的思考内容多数是加密的，所以通常没有 reasoning 记录。
- **格式来源**：生成的记录格式照着 `crates/monocode-core/src/store/shared/blocks.rs`（`update` 函数）来写。作者改了格式，这里也要跟着改。

## 测试

```sh
python3 -I -m unittest discover -s skills/monocode-import/scripts
```
