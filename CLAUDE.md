# 本 Fork 的工作约定

这份文件只存在于我们自己的 build 分支 `build`（origin），属于 custom 提交，**永远不要提给作者**。

计划和说明用中文，代码、命令、标识符保持原样。后续迭代一律基于 GPUI 原生版（`crates/monocode-gpui`）。

## 仓库和分支

| remote | 地址 | 说明 |
|---|---|---|
| `origin` | `git@github.com:parkgogogo/monocode.git` | 我们的 Fork |
| `upstream` | `git@github.com:hardbeat920/monocode.git` | 作者（Nick）的仓库 |

**作者基线**：目前是 `upstream/mc/migrate-monocode-to-gpui`。

- GPUI 版还在实验阶段，没有合进 `upstream/main`；作者的 GPUI 开发和 GPUI 相关 PR 都在这个分支上。
- 如果作者以后把 GPUI 合进了 `main`，基线就改成 `upstream/main`。改的时候同步更新这份文件里所有提到基线的地方。

分支分三层：

```
upstream/mc/migrate-monocode-to-gpui   作者基线
   ├─ feat/xxx、fix/xxx                 开发分支：从作者基线最新代码切出，做完提 PR 给作者
   │     └─ 同时合进 ↓
   └─ build                             build 分支（跟踪 origin/build）
         = 作者基线 + 所有开发分支 + custom 提交
         → 日常使用的 release 版都从这里 build
```

build 分支 2026-10-24 以前叫 `mc/migrate-monocode-to-gpui`，和作者分支同名，容易弄混，所以改名为 `build`。本地和 origin 上都不要再建叫 `mc/migrate-monocode-to-gpui` 的分支，需要作者分支时一律写全 `upstream/mc/migrate-monocode-to-gpui`。

## 开发流程

1. **从作者基线切开发分支**，不要从 build 分支切。否则 PR 里会混进 custom 提交。
   ```sh
   git fetch upstream
   git worktree add ../monocode-<名字> -b feat/<名字> upstream/mc/migrate-monocode-to-gpui
   ```
   每个开发分支用独立的 worktree，主目录 `~/labs/monocode` 保持在 build 分支上。
2. **按作者的规范写提交**，参照作者近期的提交和 `CONTRIBUTING.md`：
   - 提交标题用英文祈使句，例如 `Add … in GPUI`。
   - PR 标题用 `feat(native): …` / `fix(native): …`。
   - 一个 PR 只做一件事。UI 改动附前后对比截图。
   - 设置搜索的 INDEX 关键词只写英文。
   - 改了 vendored GPUI 的，要同步更新 `NOTICE` 和 `docs/native/migration.md`。
3. **做完先合进 build 分支**，自己用一段时间。
   ```sh
   git merge --no-ff feat/<名字>
   ```
4. 依赖另一个开发分支的改动，就叠在那个分支上切出来，并在 PR 里注明依赖关系。

## 提 PR 给作者

- **不经过用户明确同意，不推送到 `upstream`，也不开 PR。** 推送到 `origin` 的开发分支没有问题。
- PR 的目标分支是作者基线，内容只能有这个开发分支自己的提交。
- 提之前先 rebase 到作者最新代码，然后确认 diff 里没有 `CLAUDE.md` 和下表的 custom 改动：
  ```sh
  git fetch upstream
  git rebase upstream/mc/migrate-monocode-to-gpui
  git diff --name-only upstream/mc/migrate-monocode-to-gpui...HEAD
  ```
- 大一点的改动，按 `CONTRIBUTING.md` 的要求先开 issue 和作者商量范围。作者是唯一的维护者，小而聚焦的 PR 合得最快。

## Custom 提交清单

下面这些只留在 build 分支上，不提给作者。新增 custom 功能时，在这张表里加一行。

| 提交 / 文件 | 内容 | 为什么不提 |
|---|---|---|
| `CLAUDE.md` | 本文件 | 只是我们自己的工作约定 |
| `410317ae` / `crates/monocode-gpui/src/frame_log.rs` | `MONOCODE_FRAME_LOG` 重绘帧数读数 | 只用于本地性能排查 |

已经提给作者、或者准备提的开发分支（合并状态以实际为准）：

| 分支 | 内容 | 状态 |
|---|---|---|
| `feat/gpui-font-settings` | UI、正文、代码字体的字族、字重和字号设置 | 未提交 PR，issue 草稿在 `~/labs/monocode-issue-drafts/` |
| `feat/gpui-font-smoothing` | Font smoothing 开关（叠在字体设置上） | 同上 |

## 和作者同步（定时任务按这个流程执行）

作者的分支还在频繁开发，build 分支要定期同步。**build 分支用 merge 同步，不用 rebase**：build 分支已经推到 origin，rebase 每次都要强推，定时任务里不能强推。开发分支提 PR 前仍然用 rebase。

1. **检查是否有新提交**：
   ```sh
   cd ~/labs/monocode
   git status --porcelain   # 必须是干净的；不干净就停止，报告给用户
   git switch build
   git fetch upstream && git fetch origin
   git log --oneline HEAD..upstream/mc/migrate-monocode-to-gpui
   ```
   没有新提交就直接结束。如果 `origin` 上的 build 分支比本地新，先 `git merge --ff-only origin/build`；fast-forward 不了就停止，报告给用户。
2. **打备份标签**，方便回滚：
   ```sh
   git tag backup/sync-$(date +%Y%m%d-%H%M)
   ```
3. **合并**：
   ```sh
   git merge --no-ff upstream/mc/migrate-monocode-to-gpui -m "Merge upstream GPUI branch into build"
   ```
4. **冲突处理规则**：
   - 作者自己的代码，以作者为准。
   - 我们的功能（开发分支和 custom 提交）要保留下来。作者重构了相关代码的，把我们的改动迁移到新结构上，而不是退回旧结构。
   - 我们的 PR 被作者合并后（可能被 squash 或改写过），同步时和我们自己的版本冲突，以作者合进去的版本为准，然后把它从上面的开发分支表里标成已合并。
   - 拿不准的冲突，执行 `git merge --abort`，什么都不推，把冲突文件和原因报告给用户。
5. **跑检查**：
   ```sh
   cargo clippy --locked --manifest-path crates/monocode-gpui/Cargo.toml -p monocode-core -p monocode-gpui --all-targets --profile native-dev -- -D warnings
   npm run native:test
   cargo fmt --manifest-path crates/monocode-gpui/Cargo.toml --all --check
   ```
   - `CARGO_BUILD_JOBS` 没设置时，脚本默认只用 1 个并行任务，可以设成 8。
   - **`cargo fmt --check` 在作者代码上本来就不通过**（2026-10-24 时是 `crates/monocode-gpui/src/monos/tests.rs` 和 `crates/monocode-gpui/src/transcript/dates.rs`）。所以 `npm run native:check` 会以 exit 1 结束，这不算我们的失败。
     - 判断方法：只看报错的文件里有没有我们改过的。拿不准的，在作者基线的干净 worktree 上跑一次对比。
     - 不要去格式化作者的这些文件。`cargo fmt` 碰到它们，就用 `git checkout -- <文件>` 还原。
   - 检查不通过就不推送，报告给用户。修复的代码可以先留在本地，等用户确认。
6. **推送并报告**：检查通过后执行 `git push origin build`，然后给用户一份摘要，包括：
   - 作者这次的新提交列表
   - 解决了哪些冲突、怎么解决的
   - 检查结果
   - 是否影响我们的开发分支，需不需要 rebase
7. **不自动 build、不自动安装 release 版。** 同步完由用户 review，再决定什么时候 build。

## Build 和安装日常用的 release 版

只在用户要求时做，从 build 分支 build：

```sh
cd ~/labs/monocode
CARGO_BUILD_JOBS=8 npm run native:build
# 先退出正在运行的 Monocode Native，再覆盖安装
ditto "crates/monocode-gpui/target/release/Monocode Native.app" "/Applications/Monocode Native.app"
```

| 版本 | 位置 | `MonocodeDataMode` | 数据目录 |
|---|---|---|---|
| release | `/Applications/Monocode Native.app` | `monocode-data` | `~/Library/Application Support/com.monocode.desktop/`（真实数据，偏好在 `monocode.db.gpui.json`） |
| dev | `target/native-dev/Monocode Native.app`（`npm run native:dev`） | `dev-data` | `~/Library/Application Support/Monocode Development/` |

- dev 版和 release 版的 bundle id 都是 `dev.usemono.gpui`，不要同时打开。
- 用 macos-harness 操作 release 版时，用 `--app "/Applications/Monocode Native.app"`。
- 用 release 版之前，先关掉 Tauri 桌面版 MonoCode。两者共用真实数据，同时运行时偏好可能读不全。
- `npm run native:dev` 不监听文件改动，在它的终端里输入 `r` 再回车才会重新 build。

## 已知问题和注意事项

- **背景图存在 dev 数据目录**：选择聊天背景图时，图片会被复制到 `crate::data_dir().join("backgrounds")`。而 `data_dir()` 不管哪种数据模式都返回 `Monocode Development`，所以 release 版的背景图也在 dev 目录里。删掉 dev 数据目录，日常版的背景图就没了。这是作者代码的 bug，可以单独开一个 fix 分支。
- **截图前先 Raise 窗口**：被遮挡的 GPUI 窗口不会重绘，截图会是旧画面。用 macos-harness 对窗口元素 0 执行 `perform_secondary_action --action Raise`。
- **字重偏重**：GPUI 默认会按文字颜色亮度给字形加粗（`vendor/gpui-pre-apple/src/text_system.rs` 的 `glyph_dilation_for_color`）。我们的 Font smoothing 开关默认关闭这个效果。
