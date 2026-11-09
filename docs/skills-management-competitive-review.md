# Agent Skills 管理工具功能对照

截至 2026-11-09，以 Skills Manage 的 `a319ca6` 为基线。最值得关注的差距是项目级部署、更新差异与恢复、公开技能发现。项目级部署解决技能作用域问题，差异与恢复降低现有用户更新的风险，公开发现让用户从关键词开始寻找技能。

## 当前已经具备的能力

- 共享库、本地扫描、来源分组和 13 个内置工具的用户级部署。Codex 插件与系统技能可见且只读。也能扫描用户选择的文件夹并导入共享库。[扫描](../apps/desktop/src-tauri/src/scanner.rs#L174) · [工具目标](../apps/desktop/src-tauri/src/targets.rs#L15)
- GitHub URL 导入、浏览本人及已收藏仓库、来源匹配、检查更新和逐项应用更新。更新会刷新已启用的副本，旧内容移入回收站。[GitHub 目录](../apps/desktop/src-tauri/src/catalog.rs#L97) · [更新入口](../apps/desktop/src/components/SyncView.tsx#L83) · [更新实现](../apps/desktop/src-tauri/src/github.rs#L797)
- 导入时可选择多项技能和多个目标。Rust 已提供对单目标批量启停的命令。已支持 `.skillpack` 导入导出、工具兼容性提示和分工具版本。[导入流程](../apps/desktop/src/App.tsx#L439) · [批量命令](../apps/desktop/src-tauri/src/lib.rs#L247) · [兼容性检查](../apps/desktop/src-tauri/src/compat.rs#L205) · [版本选择](../apps/desktop/src-tauri/src/variants.rs#L361)

## 功能差距

| 能力 | 竞品已实现 | Skills Manage 当前差距 |
| --- | --- | --- |
| 项目级部署 | Skills Manager 能查看项目内技能并与中央库双向同步。Skill Deck 可在全局和项目作用域管理技能，也可复制到其他项目。[Skills Manager](https://github.com/xingkongliang/skills-manager/blob/v1.43.3/README.md#features) · [Skill Deck](https://github.com/Hccake/skill-deck/blob/v1.8.0/README.md#-core-capabilities) | 已能扫描指定目录并导入。部署目标来自用户主目录，缺少按仓库保存、扫描和写入的独立作用域。 |
| 更新差异与恢复 | Skills Hub 提供文件差异、受影响部署和最近一次更新前的恢复快照。CC Switch 可从应用内恢复卸载时的备份。[Skills Hub v1.1.0](https://github.com/qufei1993/skills-hub/releases/tag/v1.1.0) · [CC Switch 手册](https://github.com/farion1231/cc-switch/blob/v4.0.28/docs/user-manual/en/3-extensions/3.3-skills.md#restore-from-backup) | 已能检查更新、逐项更新并保留旧文件。更新前缺少逐文件差异和受影响目标预览，回收站缺少恢复入口。 |
| 公开技能发现 | CC Switch 支持在应用内搜索 skills.sh，并能管理参与发现的 GitHub 仓库。[CC Switch 手册](https://github.com/farion1231/cc-switch/blob/v4.0.28/docs/user-manual/en/3-extensions/3.3-skills.md#discover-skills) | 现有发现入口围绕已知 URL、本人仓库和收藏仓库，也能为已有技能补全来源。缺少从关键词搜索公开目录并导入的入口。 |
| 预设与批量维护 | Skills Hub 可保存技能组合及每项的工具分配，应用到全局或项目，记录结果并撤销符合条件的新部署。CC Switch 可在技能与工具的矩阵里多选后更新、卸载或按工具列启停。[Skills Hub v1.1.0](https://github.com/qufei1993/skills-hub/releases/tag/v1.1.0) · [CC Switch 手册](https://github.com/farion1231/cc-switch/blob/v4.0.28/docs/user-manual/en/3-extensions/3.3-skills.md#page-overview) | 导入时能批量分发。缺少可复用的命名组合、应用记录，以及面向已安装技能的多选批量更新。 |
| 安装安全审计 | Skillshare 在安装时静态扫描提示注入、外传和凭据访问等模式，默认阻止达到 `CRITICAL` 的发现，并允许显式覆盖。[安装规则](https://skillshare.runkids.cc/docs/reference/commands/install/#security-scanning) · [审计规则](https://github.com/runkids/skillshare/blob/v0.30.1/website/docs/reference/commands/audit.md) | 已有目标兼容性和压缩包完整性检查。缺少面向恶意指令和脚本的安装前检查。 |
| 已安装技能诊断 | SkillCat 汇总断链、副本漂移、来源冲突和不同作用域的同名覆盖，并展示每项证据。实际触发统计需要启用 OpenCode bridge。[SkillCat 官方功能](https://skillcat.konata9.cc/) | 现有列表能提示部分同名冲突、元数据缺失和工具兼容问题。缺少统一的部署诊断与技能实际触发记录。 |
| CLI 与项目复现 | Skills Manager 的 CLI 和桌面共用库和状态，部分命令支持 `--dry-run` 和 JSON 输出。Vercel `skills` 可从项目 `skills-lock.json` 恢复已记录的技能，冻结模式校验内容哈希。[Skills Manager CLI](https://github.com/xingkongliang/skills-manager/blob/v1.43.3/README.md) · [Vercel 项目恢复](https://github.com/vercel-labs/skills/blob/v1.7.6/docs/commands.md#restore-skills-from-the-project-lock-file) | 当前操作通过 Tauri 命令执行，能读取用户级锁文件辅助识别来源。缺少独立管理 CLI，以及从版本控制中的项目清单恢复技能的流程。 |
| 跨设备备份与同步 | Skills Manager 支持用私有 GitHub 仓库或其他 Git 远端同步共享库、处理冲突及恢复快照。[Skills Manager](https://github.com/xingkongliang/skills-manager/blob/v1.43.3/README.md#backup--multi-device-sync) | `.skillpack` 可手动迁移技能，GitHub 功能面向上游来源。缺少共享库的远端备份、增量同步和冲突处理。 |
| 自定义目标与 Windows WSL | Skills Manager 可为内置工具覆盖路径或注册自定义工具。Skill Deck 可在 Windows 应用中切换已安装的 WSL 2 发行版。[Skills Manager](https://github.com/xingkongliang/skills-manager/blob/v1.43.3/README.md#features) · [Skill Deck](https://github.com/Hccake/skill-deck/blob/v1.8.0/README.md#-core-capabilities) | 目标由内置工具表解析到用户主目录。缺少用户定义目标路径和 WSL 发行版切换。 |
| 导入格式与阅读 | Skill Deck 支持 GitLab 等 Git 来源、远程 `SKILL.md` 和 ZIP/tar 下载，并可并排阅读原文与简体中文翻译。[Skill Deck](https://github.com/Hccake/skill-deck/blob/v1.8.0/README.md#-core-capabilities) | 导入入口为本地目录、GitHub 和 `.skillpack`。已支持双语界面和 `SKILL.md` 预览，缺少这些直接导入方式与正文翻译。 |

## 优先级判断

1. **更新差异和单次恢复**。现有更新路径已保留旧文件。验证时用一个已部署到多个工具的技能检查差异、恢复内容与各目标状态，并覆盖某个目标有独立修改的场景。
2. **项目级部署和预设**。同一台机器上的多个项目可以使用不同组合。验收时确认两个项目可独立应用预设，用户级目录及未托管文件保持原样，并检查项目目录或链接重叠时的行为。
3. **公开发现和安装前检查**。搜索结果可以复用现有 GitHub 导入流程。验收时验证从关键词到预览及导入的完整路径，并分别记录来源、兼容性和静态审计结果。公开目录排名只反映发现信号，静态审计仍可能漏检。

样本选取了五款功能相近的桌面工具，以及用于比较审计和项目复现的 Skillshare、Vercel `skills`。竞品结论依据官方手册、版本说明或源码文档。本项目结论依据前端入口和已注册的 Rust 命令。竞品安装体验、扫描准确率与 Windows 上的实际稳定性仍需实测。
