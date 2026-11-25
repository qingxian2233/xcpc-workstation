<p align="center">
  <img src="./assets/brand/icon-128.png" width="96" height="96" alt="XCPC Workstation" />
</p>

<h1 align="center">XCPC Workstation</h1>

<p align="center">
  本地优先的 XCPC 工作站 —— 以开放目录题库为底座，覆盖个人题库与多人 Git 协同出题、活引用组卷与 PDF 出版，并持续演进至本地 Polygon 与训练复盘。
</p>

> **本 fork 新增 Node.js 多端版本**：Rust 核心经 napi-rs 驱动，一个 Node 服务同时供给桌面（Electron）、浏览器与手机，界面全新重写。见 [docs/node.md](./docs/node.md)。

<p align="center">
  <strong>简体中文</strong> · <a href="./README_EN.md">English</a>
</p>

<p align="center">
  <a href="https://github.com/lihaoze123/xcpc-workstation/releases"><img src="https://img.shields.io/github/v/release/lihaoze123/xcpc-workstation?include_prereleases&label=Release&color=0B6989" alt="Release" /></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-52525B" alt="License" /></a>
  <img src="https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-52525B" alt="Platform" />
  <img src="https://img.shields.io/badge/QQ%E7%BE%A4-1134941278-0B6989" alt="QQ 群 1134941278" />
</p>

<p align="center">
  <img src="./docs/images/studio.png" width="100%" alt="XCPC Workstation 出版工作室：左侧大纲与难度分布、中间 Markdown/LaTeX 编辑器、右侧毫秒级 olymp-classic 实时预览" />
</p>

---

## 核心特性

- **零 TeX 环境，毫秒级实时预览**：基于原生 [Typst](https://typst.app/) 引擎与分块增量光栅化，无需安装数 GB 的 TeX Live。内嵌严格对齐 `olymp.sty` 中文版的 `olymp-classic` 模板与高度还原 CCF CSP/NOI 官方版式的 `cnoi-classic` 模板，支持单题、带封面与试题列表的整本试题册、可选奇偶留白页、跨页样例表与 A4 题解册。
- **完整 Typst 生态与可编程排版**：原生支持 `@preview/*`（按需自动拉取并复用系统 Typst 缓存）与 `@local/*` 外部库，并内置 `cetz`（矢量绘图）、`pinit`（公式高亮与字段标注）与 `mannot`（公式下划线注解）等离线回退包；配合 `templates/preamble.typ` 自定义宏与 Typst 脚本编程能力，在 Markdown 题面中直接绘制图论/几何矢量图、标注递推公式或程序化生成表格。
- **开放目录 + 活引用组卷与多人 Git 协同**：全部项目数据（含 `templates/preamble.typ` 与定稿快照）以纯文本 Markdown + TOML 存储在开放目录中，个人配置与 `.xcpc/` 本地缓存彻底隔离，既可维护个人题库也可独立建仓多人 Git 协同出题；比赛以活引用组织题目并仅覆写元数据，定稿后将源文件与两本 PDF 写入 `snapshot/` 永久只读复现。
- **AI Agent / MCP 一等公民**：桌面端、`xcpc` CLI 与 `xcpc mcp`（stdio）共用同一套 Rust Core。提供搜索、读写题面、组卷、编译诊断、页面渲染与导出 PDF 等完整 MCP 工具，支持 `--read-only` 只读守卫与外部修改无闪白实时同步。
- **为出题人打磨的写作、定制与双语界面**：内置简体中文 / English 实时热切换与多语言声明式模板参数；提供 `/` 斜杠命令菜单（按出版语言自动适配片段）、编辑器内悬浮迷你公式预览、左右分栏拖拽样例管理、轻量素材库、专注写作模式、实用子集 Vim 模式、在 `templates/*.typ` 扩展自定义模板，以及通过全局 `theme.toml` 自定义配色与编辑器字体。

<p align="center">
  <img src="./docs/images/library.png" width="100%" alt="XCPC Workstation 题库中心：多维筛选、难度着色、比赛引用与即时题面预览" />
</p>

---

## 下载与安装

前往 [GitHub Releases](https://github.com/lihaoze123/xcpc-workstation/releases) 下载最新 `v0.2.0-beta.4` 安装包（所有桌面包均附带 `xcpc` 命令行工具）：

| 平台 | 推荐安装包 | 说明 |
|---|---|---|
| **Windows (x86_64)** | `xcpc-workstation-*-windows-x86_64-setup.exe` | Inno Setup 每用户安装器（免管理员权限，可选将 `xcpc` 加入 PATH，支持应用内自动更新） |
| **Windows (x86_64)** | `xcpc-workstation-*-windows-x86_64.zip` | 绿色便携包（含桌面端与 `xcpc.exe`） |
| **Debian / Ubuntu** | `xcpc-workstation_*_amd64.deb` | `sudo apt install ./xcpc-workstation_*_amd64.deb` |
| **Arch Linux** | `xcpc-workstation-*-x86_64.pkg.tar.zst` | `sudo pacman -U xcpc-workstation-*-x86_64.pkg.tar.zst` |
| **Linux 通用** | `xcpc-workstation-*-linux-x86_64.AppImage` / `.tar.gz` | AppImage 赋权即可运行；`.tar.gz` 支持应用内自动更新 |
| **纯 CLI / MCP** | `xcpc-cli-*-{linux,windows}-x86_64.{tar.gz,zip}` | 仅包含无头 `xcpc` 二进制，适合服务器与 Agent 环境 |

> **Linux 运行依赖**：桌面端基于原生 GTK4 + WebKitGTK 6.0，`.deb` 与 `.pkg.tar.zst` 会自动声明依赖；使用 AppImage 或 `.tar.gz` 时请确保系统已安装 `gtk4` 与 `webkitgtk-6.0`。

---

## 快速开始

### 桌面端

1. 启动 **XCPC Workstation**。
2. 首次打开可直接点击 **「打开示例」**，一键释放内置示例工作区并进入出版工作室体验；或点击 **「新建工作区」** 创建自己的题库。
3. 使用 `Ctrl+K` 快速跳转题目/比赛与执行动作，使用 `Ctrl+/` 查看全部快捷键。

### 命令行（CLI）

```bash
# 初始化一个带内置示例的工作区
xcpc init --example ./my-xcpc

# 构建整本试题册与题解册 PDF（输出至 my-xcpc/build/；也可用 oi-mock 体验 CNOI/OI 模板）
xcpc -w ./my-xcpc build contest welcome-round
xcpc -w ./my-xcpc build contest welcome-round --editorial

# 编译与规范检查（适合 CI 或脚本集成）
xcpc -w ./my-xcpc check contest welcome-round --json
```

---

## AI Agent / MCP 接入

将以下配置添加到 Claude Desktop、Cursor、Codex 或其他支持 stdio MCP 的客户端（或在桌面端 **「设置 → 工作区」** 一键复制当前工作区的配置）：

```json
{
  "mcpServers": {
    "xcpc-workstation": {
      "command": "xcpc",
      "args": ["-w", "/path/to/your/workspace", "mcp"]
    }
  }
}
```

如需限制 Agent 仅作审题与诊断而不修改工作区，可在 `args` 中追加 `"--read-only"`。

---

## 中文字体说明

发行版二进制已内置 **Latin Modern**（正文、数学、等宽）与 **JetBrains Mono**（代码块），安装后即可开箱生成完整 PDF；中文字符默认自动回退到系统字体（如 Windows 宋体/黑体/楷体或 Linux 的 Noto CJK）。

如需获得与 `olymp.sty`（`ctex` `fontset=founder`）基准 **100% 一致** 的中文版式，可将方正书宋 / 黑体 / 楷体 / 仿宋字体文件放入可执行文件同级的 `fonts/` 目录，或通过环境变量 `XCPC_FONT_DIRS` 指定字体目录。

---

## 路线图与当前状态

当前处于 **`v0.2.0-beta.4`（Public Beta）**：

- **Phase 1–2.5（已完成）**：开放目录题库与多人 Git 协同工作区、活引用组卷 / 定稿 / 归档、`olymp-classic` / `cnoi-classic` / `minimal` 声明式模板引擎、出版工作室、简体中文 / English 国际化、斜杠命令 / 素材库 / 悬浮公式预览 / Vim 模式 / 开放定制、CLI 与 MCP 服务。
- **Phase 3（规划中）**：本地 Polygon —— 基于 Wasmtime + fuel 计量的跨平台可复现题目包构建、数据生成与对拍、Polygon 导入。
- **Phase 4（规划中）**：Codeforces / AtCoder / QOJ 训练同步、训练看板与待补队列。

完整设计规格与阶段细节见 [`docs/`](./docs/README.md)（[产品规格](./docs/product.md)、[系统架构](./docs/architecture.md)、[题面版式](./docs/statement-style.md)、[设计系统](./docs/design-system.md)、[路线图](./docs/roadmap.md)）。

---

<details>
<summary><strong>从源码构建与开发测试（Nix Flake）</strong></summary>

仓库使用 Nix Flake 锁定 Rust 工具链、NSH 0.6.0 与基准字体：

```bash
# 进入开发环境
nix develop

# 启动桌面端（加载示例工作区）
cd apps/desktop && XCPC_WORKSPACE=../../examples/workspace nsh dev

# 运行全量检查与回归测试
cargo fmt --all -- --check
cargo clippy --workspace --exclude xcpc-desktop --all-targets -- -D warnings
cargo test --workspace --exclude xcpc-desktop
scripts/olymp-regress.sh
cd apps/desktop && nsh check && nsh test

# 本地打包（默认关闭 UPX，如需开启可传 --upx）
scripts/package.sh --target linux
```

</details>

---

## 社区与反馈

- **QQ 用户交流群**：`1134941278`（欢迎反馈排版细节、模板定制与使用建议）
- **问题与建议**：欢迎提交 [GitHub Issue](https://github.com/lihaoze123/xcpc-workstation/issues)，反馈 Bug 时可在桌面端 **「设置 → 关于 → 复制诊断信息」** 一键附带环境信息。

## 致谢与许可证

- 题面默认版式致谢 [`olymp.sty`](https://github.com/GassaFM/olymp.sty/)，预置 CNOI 模板灵感致谢 [`Wallbreaker5th/fuzzy-cnoi-statement`](https://github.com/Wallbreaker5th/fuzzy-cnoi-statement)。
- 底层能力基于 [Typst](https://typst.app/)、[NSH](https://github.com/longbridge/nsh) 与 [MiTeX](https://github.com/mitex-rs/mitex)；内置离线回退包包含 [CeTZ](https://github.com/cetz-package/cetz)（LGPL-3.0）、[oxifmt](https://github.com/PgBiel/typst-oxifmt)、[Pinit](https://github.com/OrangeX4/typst-pinit)、[Mannot](https://github.com/y-nakagami/mannot) 与 [Tiptoe](https://github.com/Mc-Zen/tiptoe)。
- 本项目采用 [Apache License 2.0](./LICENSE) 开源许可。
