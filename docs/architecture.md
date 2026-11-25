# 系统架构

## 1. 技术栈与平台

| 维度 | 选型 |
| :--- | :--- |
| 平台 | **只在 Linux 上开发和验收**。macOS / Windows 不验收，但能低成本避免的平台特有写法要避免。Web 版远期待定 |
| 应用框架 | **NSH 0.6（[`nsh.dev`](https://nsh.dev/)）**：Rust Core + TSX UI，`nsh init --template mixed` |
| UI 组件 | **以 `nsh/base` 为基础**，需要时可以基于 NSH 自己写组件（如预览页面的 `PageImage`）：`Canvas`、`Editor`、`RichEditor`、`DiffView`、`DataTable`、`Command`、`SplitView`、`plot` 等。[`hero-nsh`](https://c-w-xiaohei.github.io/hero-nsh/) 只作样式参考，不作为依赖。视觉规范见 [design-system.md](./design-system.md) |
| 排版引擎 | 嵌入原生 Rust `typst`：`comemo` 增量编译、`fontdb` 加载字体、`typst-render` 输出像素缓冲（预览）或 PNG、`typst-pdf` 导出 PDF |
| 题面字体 | Latin Modern（正文、数学、样例与行内等宽）、JetBrains Mono（代码块）和方正（`foundertype-ctex-fonts`），由 `flake.nix` 锁定并通过 `XCPC_FONT_DIRS` 提供；release 构建内嵌 Latin Modern 与 JetBrains Mono，见 [statement-style.md](./statement-style.md#17-字体) |
| 格式转换 | `pulldown-cmark`（GFM + `ENABLE_MATH`）+ `mitex`；LaTeX 输入用 `tylax`；导出为 Markdown 用 `typlite`/`tylax` |
| 持久化 | 开放的、人类可读的目录（单一事实源，兼容 Git）+ SQLite 派生数据 |
| 凭据 | 系统钥匙串（`keyring`） |
| Agent 接口 | `xcpc mcp`（stdio）+ `xcpc <subcommand> --json` |
| 视觉基准 | `tectonic` 编译参考模板，生成基准 PDF |
| 题目包（Phase 3） | 内嵌 `wasmtime`（fuel 计 tick）；wasi-sdk + `testlib.h` 由 `xcpc.lock` 锁定、按需下载 |

## 2. 总体结构

**同一个 Rust Core** 同时驱动 NSH 桌面 GUI、无头的 `xcpc mcp` 和 `xcpc` CLI，三者之间不重复实现任何逻辑。

```mermaid
flowchart TD
    subgraph Agents["外部 AI Agents"]
        MCPClient["MCP Client (stdio)"]
        CLICaller["Shell / CLI (--json)"]
    end

    subgraph UI["UI 层 (NSH TSX + nsh/base)"]
        Rail["左侧导航 (总览 / 题库 / 比赛 / 设置)"]
        Studio["出版工作室 (Editor + 页面预览)"]
        Others["题库中心 / 训练看板 / Cmd+K"]
    end

    subgraph Entry["接入层"]
        Bridge["NSH Native Module (Rust ↔ TSX)"]
        MCP["xcpc-mcp: stdio MCP Server"]
        CLI["xcpc CLI: JSON 输出 & 确定性退出码"]
    end

    subgraph Core["Rust Core"]
        Store["xcpc-storage: 开放目录 + 原子写入 (+ 监听 / 索引)"]
        Model["xcpc-domain: Problem / Contest / TrainingLog"]
        Conv["xcpc-convert: Markdown → Typst 胶水层"]
        Lint["xcpc-lint"]
        Engine["xcpc-engine: typst + comemo + fontdb"]
        Render["typst-render (像素缓冲 / PNG) & typst-pdf"]
        Pkg["xcpc-packages: Polygon / Companion / ..."]
        Judge["xcpc-judge: 工具链 + wasm 运行器 + 构建 / 对拍"]
        Sync["xcpc-sync: 外部 OJ 同步"]
    end

    UI <--> Bridge
    MCPClient <--> MCP
    CLICaller <--> CLI
    Bridge <--> Core
    MCP <--> Core
    CLI <--> Core
    MCP -.->|"文件监听热刷新"| UI
    Store <--> Model
    Model --> Conv
    Conv --> Engine
    Engine --> Render
    Pkg <--> Model
    Judge <--> Store
    Sync <--> Store
```

## 3. Workspace 划分

```text
crates/
  xcpc-domain     # Problem、Contest（活引用 + 元数据覆写 + 冻结）、TrainingLog、状态派生规则
  xcpc-storage    # 开放目录读写、原子写入、冻结只读守卫、路径沙箱；后续加入 notify 监听和 SQLite 索引
  xcpc-convert    # Markdown + Math → Typst（pulldown-cmark + mitex）、章节识别、图片路径映射；后续加入 tylax
                  # publish 模块：读工作区 → 转换 → 编译 → 诊断映射回 Markdown，GUI / CLI / MCP 共用
  xcpc-engine     # typst World、olymp-classic 模板（内嵌）、字体加载、render/pdf
  xcpc-lint       # 题面检查与印刷前预检（Phase 2）
  xcpc-judge      # 本地 Polygon（Phase 3）：Toolchain / Language、wasmtime 运行器、build / stress、缓存
  xcpc-packages   # Polygon 导入（Phase 3）、Competitive Companion 等
  xcpc-sync       # 外部 OJ 同步（后续阶段）
  xcpc-mcp        # `xcpc` 二进制：CLI 子命令 + `xcpc mcp`
apps/
  desktop         # NSH mixed app：TSX UI + 调用 Core 的 Rust native module
tests/fixtures/
  olymp-ref/      # 参考模板生成的基准 PDF 及其源文件
```

依赖方向：`domain` ← `storage` / `convert` / `lint` / `packages` / `sync`；`convert` → `engine`，`convert` 的 `publish` 模块还依赖 `storage`。`xcpc-mcp` 和 `apps/desktop` 只是外壳，不放业务逻辑。

## 4. 领域模型

```mermaid
erDiagram
    PROBLEM ||--o{ PROBLEM_SOURCE : "identified_by"
    PROBLEM ||--o{ CONTEST_ITEM : "referenced_by"
    CONTEST ||--|{ CONTEST_ITEM : "contains"
    PROBLEM ||--o{ TRAINING_EVENT : "tracked_in"
    PROBLEM_SOURCE ||--o{ SYNCED_SUBMISSION : "matched_by"

    PROBLEM {
        string id PK "目录名 slug，不可改"
        string title
        string format "md | latex | typst"
        string[] tags
        int rating
        json limits "time / memory / input / output"
    }

    PROBLEM_SOURCE {
        string problem_id FK
        string platform "codeforces | atcoder | qoj | luogu | ..."
        string id "平台题号"
        string url
    }

    CONTEST {
        string id PK
        string name
        string location
        string date
        string language "chinese | english"
        bool cover
        bool is_frozen
    }

    CONTEST_ITEM {
        string contest_id FK
        string problem_id FK
        string label "A, B, C..."
        json overrides "仅元数据: title / limits / show-metadata"
    }

    TRAINING_EVENT {
        string problem_id FK
        string kind "mark | vp | upsolve | review"
        string result "ac | wa | tle | upsolved | ..."
        int duration_sec
        string[] mistake_tags
        datetime at
    }

    SYNCED_SUBMISSION {
        string platform
        string problem_id "平台题号"
        string verdict
        datetime at
    }
```

- **训练状态是派生值**：`status(problem) = max(TRAINING_EVENT 的结果, 通过 PROBLEM_SOURCE 匹配到的 SYNCED_SUBMISSION 的结果)`。它不写入 `problem.toml`。
- **覆写只限元数据**：渲染时把 `CONTEST_ITEM.overrides` 合并到题目元数据上。题面正文不能覆写。
- **冻结**：渲染改为读取 `contests/<id>/snapshot/`，不再读题库。

## 5. 本地存储

开放目录是**单一事实源**。题库检索用 Core 的内存索引，不落盘；SQLite 只存放外部 OJ 同步数据（Phase 4），删除后可以重新同步。

```text
xcpc-workspace/
├── .xcpc/                    # 纯本地派生目录，固定写入 .gitignore，绝不进 Git
│   └── oj_sync.db            # 外部 OJ 同步明细、Rating、参赛记录、赛程缓存（Phase 4）
├── xcpc.lock                 # 工具链锁定（Phase 3，进 Git）
├── workspace.toml            # 工作区配置：默认 template、`[template-options.<template>]`、标签词典（不含个人 OJ 账号）
├── AGENTS.md                 # 工作区级 Agent 指南（`xcpc init` 时写入的静态文本，之后归用户）
├── templates/                # 工作区 Typst 模板（`<name>.typ`，初始化时按所选预设落盘，进 Git，全部可自由编辑）
│   └── preamble.typ          # 可选：工作区级 Typst 宏与样式注入（保留名，进 Git，不列为可选比赛模板）
├── assets/                   # 工作区级共享静态资源（如通用校徽/Logo）
├── problems/
│   └── cf-1999a/
│       ├── problem.toml      # 元数据、限制、`[[source]]`、单题级 `[template-options.<template>]`；不含训练状态
│       ├── statement.md      # 或 statement.zh.md / statement.en.md
│       ├── editorial.md
│       ├── samples/          # 01.in, 01.ans, ...
│       ├── assets/
│       ├── checker.cpp, validator.cpp, interactor.cpp   # 题目包（Phase 3），均可选
│       ├── generators/
│       ├── solutions/
│       └── tests/            # 手写测试；生成的数据在 build/problems/<slug>/
├── contests/
│   └── icpc-2026-regional/
│       ├── contest.toml      # 元信息、题目引用、元数据/单题版式覆写、`template` 与 `[template-options.<template>]`
│       ├── template.typ      # 可选：单场比赛专属模板（存在即优先于 `contest.toml` 的 `template`）
│       ├── assets/           # 比赛专属配图/封面徽标
│       └── snapshot/         # 仅在冻结后存在：每题源文件副本 + 生效 `template.typ`/preamble/合并参数 + 当时的两本 PDF（进 Git）
├── sheets/                   # 题单（后续阶段）
└── training/
    └── logs.jsonl            # 追加写入的训练事件
```

- 工作区的根目录以 `workspace.toml` 为标志（Phase 1 可以是空文件）。`xcpc` 依次用 `--workspace`、`XCPC_WORKSPACE`、当前目录向上查找来确定工作区，桌面端用后两者。
- **个人数据与项目数据解耦**：外观主题（用户全局 `<dataDir>/theme.toml`，不设工作区级 `.xcpc/theme.toml`）、编辑器与界面偏好、OJ 账号及系统钥匙串凭据均属于用户个人配置，不写入任何工作区；工作区内的 `.xcpc/` 始终是纯本地派生目录，由 `init_workspace` 固定写入 `.gitignore`，绝不提交到 Git；影响编译结果的公共宏放在 `templates/preamble.typ` 随工作区纳入 Git。
- 导出的 PDF 默认写到工作区的 `build/`（派生物，建议加入 `.gitignore`）：Phase 1–2 默认为 `build/<id>.pdf` / `build/<id>-editorial.pdf`；**Phase 2.5 起**在 Core（`crates/xcpc-convert/src/publish.rs`）统一改为按标题命名（如 `build/春季训练赛-试题册.pdf` / `build/春季训练赛-题解.pdf`，自动替换路径非法字符），GUI、CLI 与 MCP 保持一致；CLI 的 `-o` / `--output` 与 MCP 的 `output` 参数仍可覆盖输出路径。
- **内置示例工作区（Phase 2.5）**：示例工作区静态资源打包进二进制，由 Core 提供「释放到指定目录」接口，供 CLI `xcpc init --example` 与 GUI「打开示例」共用。
- **Phase 1** 直接扫描目录。**Phase 2** 起 Core 维护内存索引（元数据 + 题面/题解的规范化纯文本）：桌面端启动时扫描一遍，之后由 `notify` 增量更新（同时监听 `workspace.toml`、`templates/*.typ`（含 `templates/preamble.typ`）与 `contests/*/template.typ`，外部编辑器保存后立即热刷新预览与模板 schema）；CLI 和 MCP 每次启动现扫。不引入 SQLite / FTS5，也不做 JSON Schema。
- **TOML 写回**：一律用 `toml_edit` 做局部修改，保留注释、顺序和未知字段；读取仍用 `serde`。GUI 和 CLI 通过 Core 的语义操作写入（增删题、移动、设置/删除覆写、比赛元数据、冻结）。
- **并发写入**：Core 记录每个打开文件上次读写时的内容哈希，保存前比对，磁盘已变则返回 `conflict`，不静默覆盖；自己写入引起的 `notify` 事件靠哈希过滤。

## 6. 格式转换管线

**不手写 Markdown / LaTeX / Typst 解析器。**

```mermaid
flowchart LR
    MD["Markdown + Math (默认)"] --> P1["pulldown-cmark (GFM + ENABLE_MATH) + mitex"]
    TEX["Polygon LaTeX (后续)"] --> P2["tylax"]
    TYP["Typst"] --> P3["直通"]
    P1 --> Glue["胶水层: 二级标题→olymp 章节 / samples / 图片路径"]
    P2 --> Glue
    P3 --> Glue
    Glue --> Typst["typst 增量编译 (olymp-classic)"]
```

- 胶水层负责三件事：按二级标题识别章节（规则见 [product.md](./product.md#21-题面结构)）、从 `samples/` 生成样例表、把图片路径映射到题目的 `assets/`。处理 Polygon LaTeX 时，还要转换 `\epigraph`、`\illustration` 等宏。
- **模板 Schema 与单入口渲染**：所有生效模板均为磁盘上的 `.typ` 文件（`contests/<id>/template.typ` 优先于 `templates/<name>.typ`；预设 `olymp-classic` / `cnoi-classic` / `minimal` 仅作初始化与添加模板的种子，不做内置回退）。`xcpc-engine` 对模板轻量求值导出 `#let schema`（规范见 [statement-style.md §2.3](./statement-style.md#23-模板声明式-schemalet-schema)）。转换管线按「模板默认 → `workspace.toml` → `contest.toml` → `problem.toml` / `[[problem]]`」合并参数，生成单入口 `#render(doc)` 调用，并为模板未导出的 `section` / `samples` / `subsection` / `runin` 自动回退到 `/xcpc/defaults.typ`。
- 数学定界符：LaTeX 里的普通括号（`(`、`[`、`\lceil` 等）不会缩放，转换成 Typst 时要写成转义或符号形式（`\(`、`\[`、`#sym.ceil.l`），避免 Typst 自动配对放大；只有 `\left…\right` 才转换成 `lr()`。
- 导出到 Hydro / QOJ 等 OJ 时，用 `typlite` / `tylax` 把 Typst 转回 Markdown（后续阶段）。
- **PDF 元数据与书签（Phase 2.5）**：导出 PDF 时由 Core 统一写入文档元数据，并复用 `olymp-classic.typ` 中已有的书签（`bookmarked: true`）：
  - `Title`：`春季训练赛 试题册` / `春季训练赛 题解` / `A. Two Sum`（按比赛试题册、比赛题解或单题标题生成）；
  - `Author`：比赛的作者字段，没有就不写；
  - `Creator`：`XCPC Workstation <版本>`；
  - `Producer`：保留 Typst 默认值。

## 7. Agent / MCP / CLI

### 7.1 运行方式

- `xcpc mcp`：stdio MCP 服务，不需要启动 GUI。
- `xcpc <subcommand> --json`：输出结构化结果，退出码确定。
- 桌面端打开同一工作区时，Agent 的写入通过 `notify` 文件监听让 GUI 刷新（Phase 2），不做本地 IPC。

### 7.2 MCP Tools

| 分组 | Tools | 阶段 |
| :--- | :--- | :--- |
| 出版与视觉自检 | `compile_and_diagnose`（结构化诊断：错误位置、总页数、溢出警告；Phase 2 起并入 lint 结果）、`render_page_png`、`export_pdf`（单题 / 整本 / 题解册）、`list_templates`（列出内置与自定义模板及其 schema） | Phase 1 / 2.5 |
| 题库与组卷 | `search_problems`、`get_problem`、`get_contest`、`write_problem`、`compose_contest`、`set_override`（支持单题级模板参数覆写）、`set_template_options`（更新工作区/比赛/单题的模板选择与参数）、`freeze_contest` | Phase 2 / 2.5 |
| 题目包 | `build_package`（结构化报告）、`stress_package`（反例种子与输入）；`--read-only` 下不可用 | Phase 3 |
| 训练 | `get_training_dashboard`、`log_training_record`、`generate_upsolve_plan` | Phase 4 |

**MCP 没有解冻工具**，解冻只能由人在 GUI 或 CLI 中完成。

Phase 2 的工具：

- `search_problems`：可选的文本、标签、rating 区间、来源平台；返回元数据列表（ID、标题、rating、标签、来源、有无题解），不含正文。
- `get_problem`：解析后的元数据、`statement*.md` / `editorial.md` 原文及其内容哈希、样例文件列表。
- `get_contest`：不传 `id` 时列出所有比赛；传 `id` 时返回元数据、题目列表（label、ID、覆写、每题起止页）和是否冻结。
- `write_problem`：`id`、`files: {相对路径: 内容}`、可选的 `expected_hash`。题目不存在就创建，存在就逐个原子写入；路径只能是题目目录内的 `problem.toml`、`statement*.md`、`editorial.md`、`samples/*`、`solutions/*`、`generators/*`、`tests/*`、`checker.cpp`、`validator.cpp`、`interactor.cpp`。`problem.toml` 给的是全文，Core 只校验能否解析。
- `compose_contest`：`id`、有序的题目 ID 列表、可选的比赛元数据。比赛不存在就创建；保留下来的题保留覆写，移除的题连同覆写删除，题号由 Core 重排。
- `set_override`：`contest`、`label`、`fields`（`title` / `limits` / `show-metadata` / `options`，`null` 表示删除该覆写）。
- `list_templates`：列出工作区 `templates/*.typ` 的全部模板及其解析后的 `schema`，以及可供安装的预设种子列表（`olymp-classic`、`cnoi-classic`、`minimal`）。
- `set_template_options`：按 `scope`（`workspace` / `contest` / `problem`）切换 `template` 或更新指定模板命名空间下的参数键值（`null` 表示删除覆写、恢复继承；遵循冻结与只读守卫）。
- `freeze_contest`：冻结规则见 [product.md](./product.md#3-比赛contest)。

对应的 CLI：`xcpc search`、`xcpc show`、`xcpc template list|schema|add|fork`、`xcpc contest compose|override|freeze|unfreeze`、`xcpc init [dir]`（Phase 2.5 起支持 `--example` 释放内置示例工作区）；lint 并入 `xcpc check`，不单设命令。

Phase 1 的三个工具参数相同：`target`（`problem` / `contest`）、`id`、可选的 `book`（`statements` / `editorial`）和 `language`（只对单题有效）。`render_page_png` 另有 `page`（从 1 开始）和 `scale`，直接返回图片、不写文件；`export_pdf` 另有 `output`（相对工作区的路径，未指定时走 Core 默认路径，Phase 2.5 起按标题命名，见 §5）。诊断的位置指向工作区里的源文件（`problems/a/statement.md` 的行列），溢出版心的内容是带页码的警告。CLI 对应 `xcpc check|render|build <problem|contest> <id> [--editorial]`，退出码 0 成功、1 文档有错误、2 用法或工作区错误；`--json` 输出同样的结构。

### 7.3 安全边界（在 Core 层强制执行）

- **原子写入**：先写临时文件，再 rename 替换。
- **冻结只读**：`is_frozen = true` 的比赛及其快照禁止一切写入，写操作返回明确的错误码。解冻只开放给 GUI 和 CLI（需二次确认），不开放给 MCP。
- **只读模式**：`xcpc mcp --read-only`。
- **路径沙箱**：所有文件访问限制在工作区内，防止路径穿越。

### 7.4 错误文案集中化（Phase 2.5）

- 人类可读的中文错误文案集中维护在 Core 层，供桌面 GUI 与 CLI 文本输出共用，统一按三段式组织（人话说明现象 → 可点击/可定位的位置 → 仅在有明确下一步时给出操作建议；规则拒绝如冻结只读不当错误处理，详规范见 [design-system.md §7](./design-system.md#7-文案克制规范)）。
- 默认人类可读主文案不暴露底层错误码与英文技术原文（GUI 收入「复制详情」）；CLI `--json` 与 MCP 响应继续保留完整的结构化错误码与细节字段。

## Tauri macOS 桌面宿主

用户要求的 macOS 分发使用 `apps/tauri`：保留 `apps/web` 的 React 页面，由 Tauri 2 / 系统 WebKit 承载；原 Node 接口由同进程 Rust 兼容层实现，经 `crates/xcpc-runtime` 直连 Core。编译产物运行不依赖 Node 或 Electron。原 NSH、CLI、MCP 及 Web/Electron 源码保留，详见 [tauri.md](tauri.md)。
