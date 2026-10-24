---
name: monocode-handoff
description: Hand one piece of the current work to a new MonoCode session run by another agent (Claude, Codex, Pi, ...), with a focused brief instead of the whole conversation. Use when the user wants to split a task off, delegate part of the work, start a parallel session for something, or says "hand off" / "交给别的 agent" / "开个新会话做" inside MonoCode.
---

# 把一部分任务交给 MonoCode 里的新会话

MonoCode 自带的 handoff 只能由用户在回复菜单里触发，会把**整个会话**交给**另一家** agent，交接说明也是自动生成的。这个 skill 是主动 handoff：由我挑出其中**一件事**，写一份聚焦的交接说明，再通过 MonoCode 的 Operator 接口新建一个会话。

## 前提

- **这个会话已经开启了 Operator**：用户发过一条带 `/operator` 的消息，或者在输入框 “+” 菜单里勾选了 Operator。
  - 判断方法：运行 `sessions.list`。返回 `Operator is not enabled for this thread` 就是没开，这时请用户开启，不要绕过。
- **凭证已经在环境变量里**：`MONOCODE_APP_ENDPOINT`、`MONOCODE_APP_TOKEN` 等。**永远不要打印它们。**
- **CLI 位置**：`$MONOCODE_VISUAL_EXECUTABLE`。没有这个变量时，用 `/Applications/Monocode Native.app/Contents/MacOS/monocode-gpui`。

```sh
MC="${MONOCODE_VISUAL_EXECUTABLE:-/Applications/Monocode Native.app/Contents/MacOS/monocode-gpui}"
"$MC" app help                       # 所有动作和参数
"$MC" app sessions.list --json '{}'
"$MC" app models.list --json '{}'    # 可用的 harness、模型、权限模式
```

## 流程

1. **和用户对齐要交出去什么。**
   - 一次只交一件事，并且要说清楚完成标准。
   - 用户没说交给谁时，问一句。可选的 harness 有 `claude`、`codex`、`pi`、`cursor`、`grok`、`opencode`，以 `models.list` 的结果为准。
2. **写交接说明。** 按下面的模板写，用用户的语言（中文）。
3. **建会话。默认存成草稿、不发送。** 草稿模式下新会话不会开始跑，用户在 MonoCode 里看过、改过之后自己点发送。只有用户明确说 “直接发” 时，才去掉 `draft`。
   ```sh
   "$MC" app sessions.start --request-id "handoff-<短名>-<时间>" --input - <<'EOF'
   {
     "title": "<简短标题>",
     "harness": "codex",
     "draft": true,
     "reveal": true,
     "placement": "right",
     "prompt": "<交接说明全文>"
   }
   EOF
   ```
   - **怎么打开**：`placement: "right"` 或 `"down"` 在当前会话旁边分屏打开；`"tab"` 开成新标签页。`reveal: true` 会切过去，让用户直接看到。
   - **改不改代码**：要改代码的任务，用 `"workspaceMode": "worktree"` 加 `"worktreeBase": "<分支>"` 在独立的 worktree 里做，免得和当前会话改同一个工作区。只读的调查任务不用。
   - **其他可选参数**：`model`、`effort`、`runtimeMode`（`read-only` / `supervised` / `auto-accept-edits` / `auto` / `full-access`）。不确定就不传，用 MonoCode 的默认值。
   - **网络不稳时**：调用失败或结果不确定，就用**同一个 `--request-id`、同样的内容**重试，不会重复建会话。
4. **把结果告诉用户**：新会话的 id、标题、用了哪个 agent、是不是草稿，以及交接说明的要点（不要全文贴出）。
5. **之后跟进**（可选）：
   - 用户问进展时，用 `sessions.read {"sessionId": <id>}` 读最近的对话，再总结给用户。
   - 需要补充信息时，用 `sessions.send`。如果它还是草稿，用 `sessions.draft` 改草稿内容。
   - 这些操作也要用户同意；不要自己替那个会话做决定。

## 交接说明模板

接手的 agent 看不到这个会话，所以说明必须**单独读就能看懂**。但已经写在文件、提交、issue、CLAUDE.md 里的内容，只给路径，不要重复贴。

```markdown
# <任务名>

## 目标
<一两句话：要做成什么。>

## 完成标准
- <可以检查的结果，例如：测试通过、截图确认、提交到某个分支>

## 背景和已经确认的结论
- <只放这件事需要的。附上具体的文件路径和行号。>
- <已经排除的方向，免得它重复踩坑。>

## 约定
- 先读仓库根目录的 CLAUDE.md，按里面的分支流程做（例如从 `upstream/mc/migrate-monocode-to-gpui` 切 `fix/...` 分支）。
- 不要推送到 upstream，也不要开 PR。
- <其他限制，例如：不要动某些文件、不要碰真实数据。>

## 范围之外
- <明确不需要它做的事。>

## 做完之后
- 用中文汇报：改了什么、怎么验证的、还有什么没解决。
```

写说明的注意事项：

- **结论要带证据**：写 “`shell/settings.rs:389` 用了 `crate::data_dir()`”，不要只写 “背景图存错了”。
- **不确定的事要标出来**：猜测就写明是猜测，不要写成结论。
- **不放敏感信息**：不放凭证、token，也不放用户的私人信息。
- **控制长度**：通常一两屏就够了。接口上限是 240KB，但长不等于有用。
- **不能在提示词里写 `/operator`**：接口会拒绝。新会话要用 Operator，需要用户自己在那个会话里开启。

## 和 MonoCode 自带 handoff 的区别

| | 自带 handoff | 这个 skill |
|---|---|---|
| 谁触发 | 用户在回复菜单里点 | agent 按用户的要求发起 |
| 交出去什么 | 整个会话 | 其中一件事 |
| 交接说明 | 自动生成：最近两条用户消息、最后一次回复、改过的文件 | 按模板手写，只放这件事需要的 |
| 能交给谁 | 只能换一家 agent | 任何 harness，包括同一家 |
| 原会话 | 用户之后在新会话里继续 | 原会话继续做别的，两边并行 |
