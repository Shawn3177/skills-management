# Skills Manage v1 计划

## Summary

做一个类似 CC Switch 的本地桌面管理软件，但 v1 聚焦在 Agent Skills 管理：用一个本地“共享 skills 仓库”统一管理 Claude Code、Codex、VS Code 多扩展里的 skills。用户只安装或导入一次 skill，之后通过每个工具的开关决定是否启用；还能导出独立 `.skillpack` 文件，拷贝到另一台电脑后直接导入恢复。

v1 Windows 优先，但按可公开分发的软件设计，预留 macOS / Linux 支持。产品形态包含主窗口、系统托盘、安装包、portable 版、本地数据库、备份、导入导出和自动更新预留。

## Technical Stack

- 桌面框架：Tauri 2。
- 前端：React + TypeScript + Vite。
- UI：Tailwind CSS + Radix UI / shadcn 风格组件 + lucide-react 图标。
- 后端：Rust，负责文件系统、适配器扫描、目录链接、复制、备份、校验、压缩包和数据库。
- 本地数据库：SQLite，保存 skills、目标工具、启用状态、导入记录、冲突记录和备份记录。
- 配置文件：`~/.skills-manage/settings.json`，保存设备级偏好、窗口状态、手动路径和链接模式。
- 本地数据根目录：`~/.skills-manage/`；Windows 上等价于 `%USERPROFILE%\.skills-manage\`。
- 共享仓库：`~/.skills-manage/library/`，每个 skill 作为完整目录管理，必须包含 `SKILL.md`，可带 `scripts/`、`references/`、`assets/` 等支持文件。
- 打包分发：Tauri bundle，Windows 优先生成 MSI 安装包和 portable ZIP。
- 自动更新：预留 `tauri-plugin-updater`。
- 托盘：Tauri tray icon，用于快速打开主窗口、刷新扫描、查看最近启用状态。
- Deep Link：预留 `skillsmanage://`，后续用于一键导入 `.skillpack` 或远程 skill 源。

## Key Decisions

- 不再使用 Electron 作为 v1 默认路线。原因是产品目标接近 CC Switch 这类轻量本地管理台，Tauri 2 更适合公开分发、托盘常驻和跨平台安装包。
- 前端只负责界面和用户交互，不直接接触系统文件。所有读写、链接、删除、打包、备份都通过 Rust command 完成。
- 所有写入目标工具目录的操作都必须先备份，再使用原子写入或可回滚步骤，避免损坏用户现有配置。
- Windows 默认优先使用目录联接或符号链接把共享仓库里的 skill 暴露到各工具目录；权限或兼容性失败时回退为托管副本。
- 删除默认是软删除到 `~/.skills-manage/trash/`，并移除各工具中的托管链接；硬删除必须二次确认，且绝不删除非本工具托管的原始文件。

## App Behavior

- 首次启动：扫描 Codex、Claude Code 和 VS Code 常见 skill 路径，列出已发现 skills，识别重复项，让用户导入到共享仓库。
- 主界面：左侧为 skill 列表，右侧显示 `name`、`description`、来源路径、文件校验、支持文件、目标工具启用状态和健康状态。
- 托盘：提供打开主窗口、刷新扫描、查看最近启用/禁用结果、退出应用。
- 适配器：内置 Codex、Claude Code、Agent Skills 标准路径适配器，并提供 VS Code 多扩展预设和手动路径配置。Codex 适配器同时识别官方 `$HOME/.agents/skills` 与当前机器可见的 `%USERPROFILE%\.codex\skills` 作为发现来源。
- 冲突处理：导入同名 skill 时提供跳过、覆盖、另存为副本三种选择；默认另存为副本，避免静默覆盖。
- 修复功能：检测断开的链接、丢失的 `SKILL.md`、非法 frontmatter、目标工具目录缺失，并给出一键修复或禁用。
- 打包迁移：导出 `.skillpack`，本质是 ZIP 包，包含 `manifest.json`、完整 skill 目录、校验和和适配器启用状态；默认排除 `.git`、`.env`、`node_modules`、缓存和构建产物。

## Architecture

### Frontend

- `AppShell`：主布局、导航、主题和全局状态。
- `SkillsList`：skills 列表、搜索、过滤、重复项提示。
- `SkillDetail`：skill 元数据、文件结构、健康检查和目标启用开关。
- `ImportExportPanel`：导入目录、导入 ZIP / `.skillpack`、导出 `.skillpack`。
- `SettingsPanel`：共享仓库路径、目标工具路径、链接策略、备份策略、更新设置。
- `TrayBridge`：接收托盘触发的刷新或窗口事件。

### Rust Backend

- `adapter`：每个目标工具一个适配器，实现扫描、启用、禁用、校验和修复。
- `library`：共享仓库管理、导入、复制、重命名、软删除。
- `pack`：`.skillpack` 导入导出、校验和、排除规则。
- `storage`：SQLite schema、查询、事务和迁移。
- `backup`：目标文件写入前备份、自动轮转、恢复。
- `fs_ops`：跨平台文件操作、符号链接/目录联接、权限检查、原子写入。
- `health`：断链、缺失 `SKILL.md`、frontmatter、hash 变化检测。

## Public Interfaces

### `SkillRecord`

- `id`
- `name`
- `description`
- `sourceKind`
- `sourcePath`
- `libraryPath`
- `hash`
- `createdAt`
- `updatedAt`
- `enabledTargets`
- `healthStatus`
- `supportFiles`

### `ToolAdapter`

- `id`
- `displayName`
- `platforms`
- `candidatePaths`
- `scan()`
- `enable(skill, mode)`
- `disable(skill)`
- `validateTarget()`
- `repair(skill)`

### `.skillpack/manifest.json`

- `formatVersion`
- `packageId`
- `createdAt`
- `skills[]`
- `checksums`
- `targetStates`
- `exclusions`
- `sourceAppVersion`

## Distribution

- Windows v1：MSI 安装包 + portable ZIP。
- macOS / Linux：暂不作为首发重点，但代码结构按跨平台保留。
- 更新：先手动发布 GitHub Releases，后续接入 Tauri updater。
- 签名：v1 开发期可不签名；公开发布稳定版时准备 Windows 代码签名证书，减少系统安全提示。
- 最低系统：Windows 10+。

## MVP Scope

1. Tauri 2 应用骨架、主窗口和托盘。
2. SQLite 初始化和 `~/.skills-manage/` 数据目录。
3. 扫描 Codex / Claude Code / VS Code 常见 skills 目录。
4. 识别 `SKILL.md`、解析 frontmatter、计算 hash。
5. 导入 skill 到共享仓库。
6. 在 Codex 和 Claude Code 目标目录启用/禁用 skill。
7. 断链和缺失文件健康检查。
8. 导出和导入 `.skillpack`。
9. Windows MSI 和 portable ZIP 打包。

## Test Plan

- 扫描：能识别只有 `SKILL.md` 的简单 skill，也能识别带 `scripts/`、`references/` 的目录型 skill。
- 启停：对 Codex、Claude、VS Code 任一目标开关时，只创建或移除托管链接，不影响共享仓库原件。
- 备份：写入目标工具目录前有备份，失败时能恢复。
- 打包迁移：导出 `.skillpack` 后在空目录模拟另一台电脑导入，校验文件、metadata、启用状态都能恢复。
- 冲突：同名、同 hash、同名不同 hash、损坏包、缺失 `SKILL.md` 都有明确结果。
- Windows 路径：覆盖中文路径、空格、用户名含特殊字符、长路径、权限不足、链接创建失败的回退副本路径。
- 安全：确认 `.env`、认证配置、缓存、`node_modules` 不会被默认导出。
- 分发：MSI 安装、portable ZIP 启动、卸载后用户 skill 原件不丢失。

## Assumptions

- v1 本地优先，不做账号系统、在线 marketplace 和云同步。
- VS Code “多扩展适配”先做成预设 + 手动路径框架，不承诺每个扩展都有完全相同的启停语义。
- skill 以 Agent Skills 开放目录格式为核心。
- 公开发布时先支持 Windows，后续再补 macOS / Linux 打包和签名。

## References

- [CC Switch](https://ccswitch.ai/)
- [CC Switch GitHub](https://github.com/farion1231/cc-switch)
- [Tauri 2](https://v2.tauri.app/)
- [Codex Agent Skills](https://developers.openai.com/codex/skills)
- [Claude Code Skills](https://code.claude.com/docs/en/skills)
- [Agent Skills Specification](https://agentskills.io/specification)
