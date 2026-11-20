# 由程序管理的 Skill 与一键更新

## 背景

目标：在 Skills Manage 里点一次更新，所有启用的 AI 都用上最新的 skill。

普通 skill 本来就能做到：更新共享库后，`refresh_managed_copies` 会把新内容复制到每个启用的工具。

Agent Reach（`Panniantong/Agent-Reach`）是例外。它的 skill 由程序生成：

- 仓库里的 `agent_reach/skill/SKILL.md` 是模板，frontmatter 里写着 `version: "{{VERSION}}"`，正文里有 `{{CHANNEL_TABLE}}`。`agent-reach setup` 和 `agent-reach update` 会用本机已装程序的版本号和渠道表填好它（上游 `runtime/manage.py` 的 `render_skill`）。
- skill 里的命令依赖同版本的程序。模板是 1.6、程序是 1.5 时，`channels`、`read`、`update` 都会报 `invalid choice`。
- 程序会自己写 `~/.agents/skills/agent-reach`（Codex 原生会读这里），再给 `~/.claude/skills`、`~/.cursor/skills` 等建链接；Windows 上建不了链接时改为复制。如果那里已有一个 `name: agent-reach` 的真实文件夹，程序会直接覆盖它。

所以 Skills Manage 从 GitHub 复制这个 skill 会出两个问题：各工具拿到的是没填好的模板，程序也没有升级；而且 Skills Manage 和程序会写同一批路径，谁最后写，文件就是谁的。

## 决定

1. **拒绝模板**：从 GitHub 导入或更新时，如果解压出来的 SKILL.md 的 **frontmatter** 里有 `{{UPPER_CASE}}` 占位符，就停止写入，并提示改用程序更新。只看 frontmatter，因为正文里出现模板语法可能是 skill 本身在举例。检查放在 `install_from_snapshot` 和 `apply_skill_update`，覆盖单个导入、批量导入、按 source 更新、按识别出的来源更新这四条路径。
2. **一个路径只能有一个写入者**：在 skill 详情的「更新方式」里可以设置程序的更新命令（预填 `<folder> update`），保存到 `~/.skills-manage/program-updates.json`。保存时，如果共享库里有同名 skill，先把它和它托管到各工具的副本软删除，之后由程序负责放置 skill。这个 skill 详情页上的「加入共享库」按钮会被禁用，避免再加回来。
3. **执行方式**：由 Rust 后端直接执行命令，不经过 shell，不弹控制台窗口（`CREATE_NO_WINDOW`）；超时 30 分钟（与 Agent Reach 自己同步时的超时一致）；退出码为 0 才算成功，失败时把最后几行输出带回界面。程序结束后最多再读 2 秒输出，防止它后台起的子进程一直占着管道。这个命令不持有写锁，因为程序只写它自己的目录。
4. **一键更新**：更新页的「全部更新」会依次做三件事：检查 GitHub；逐个更新有新版本的 skill；逐个运行已设置的程序命令。某一项失败不影响其他项，最后汇总报告。表头会显示进度，如「正在更新 2/5：agent-reach」。程序命令就显示在列表里，点之前就能看到要运行什么；命令只在设置时由用户输入并保存一次。

## 不做的

- 不内置任何第三方程序的命令表。命令由用户确认，预填值只是猜测。
- 不在 Skills Manage 里渲染模板。渠道表来自程序内部，只有程序自己能生成。
- 不检查程序是否有新版本。程序自己的 update 在已经是最新时也会做同步，所以每次都运行一次。
