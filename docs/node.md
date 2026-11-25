# Node.js 版（fork）

本 fork 在 [lihaoze123/xcpc-workstation](https://github.com/lihaoze123/xcpc-workstation)（Apache-2.0）的基础上，加了一条 **Node.js 驱动**的多端路线：Rust 核心原样复用，经 napi-rs 暴露给 Node，由一个 HTTP 服务同时供给桌面（Electron）、浏览器和手机。上游的 NSH 桌面端、`xcpc` CLI 与 MCP 均保留不动。

```
crates/xcpc-{domain,storage,convert,engine,lint}   上游核心，不改
crates/xcpc-node        napi-rs 绑定（只有胶水）
packages/core           @xcpc/core：加载 .node，类型声明
packages/server         @xcpc/server：Hono HTTP API + SSE + 静态托管 UI
apps/web                @xcpc/web：React + CodeMirror 6 前端（桌面 / 浏览器 / 手机共用）
apps/electron           桌面壳：进程内起 server，窗口加载 UI
```

## 快速开始

需要 Rust stable 与 Node ≥ 20。

```sh
scripts/fetch-fonts.sh                 # Latin Modern / JetBrains Mono / Noto CJK → .fonts/（不入库）
export XCPC_FONT_DIRS="$PWD/.fonts"
npm install
npm run build                          # 编译原生模块（约 10 分钟，首次）+ 构建前端
npm start -- ~/my-workspace            # http://127.0.0.1:4517
```

- 开发：`npm run dev -- <工作区>`（API :4517 + Vite :5173 热更新）；改 Rust 后 `node packages/core/scripts/build.mjs --debug` 可跳过 LTO 快速出包。
- 手机 / 平板：`npm start -- <工作区> --host 0.0.0.0`，终端会打印带 token 的局域网地址，手机浏览器打开即可（可"添加到主屏幕"）。非回环地址**强制** token。
- 桌面：`npm -w @xcpc/electron start`（首次需 `npm rebuild electron` 下载 Electron 本体）。
- 测试：`npm test`（端到端：示例工作区上的列表、编译、渲染、PDF、冲突检测、路径越界、token）。

题面中文字体：模板首选方正（FZShuSong-Z01 等）。方正字体须自行取得授权，放进 `.fonts/founder/` 即自动优先；否则回退到 Noto Serif/Sans CJK SC。

## HTTP API

| 方法 | 路径 | 说明 |
| :-- | :-- | :-- |
| GET | `/api/workspace` | 当前工作区 |
| POST | `/api/workspace/{open,init,example}` | `{ path }` 打开 / 新建 / 释放示例 |
| GET | `/api/problems?q=&tags=&min=&max=` | 题库检索 |
| GET | `/api/problems/:id` · `/api/contests` · `/api/contests/:id` | 详情 |
| PUT | `/api/problems/:id/samples` | 覆盖样例 |
| GET | `/api/search?q=` | 模糊跳转 |
| GET/PUT | `/api/file?path=` | 读 / 原子写（带 `hash` 做冲突检测，冲突返回 409） |
| GET | `/api/asset?path=` | 图片等二进制 |
| POST | `/api/compile/:kind/:id[?editorial=1]` | `{ unsaved: { path, text } }` 叠加编辑器未保存内容编译，返回报告 + `rev` |
| GET | `/api/page/:kind/:id/:page?scale=&rev=` | 页面 PNG（按 rev 不可变缓存） |
| GET | `/api/pdf/:kind/:id[?download=1]` | 导出 PDF（总是按磁盘内容） |
| GET | `/api/locate/:kind/:id/:page?x=&y=` | 预览坐标 → Markdown 行列 |
| GET | `/api/events` | SSE：`changed`（外部改动，如 git / AI）· `workspace` |

错误统一为 `{ code, message }`，`code` 即核心的稳定错误码（`conflict`、`path_escape`、`contest_frozen`…）。

## 设计取舍

- **不在 JS 里重写核心**。转换、模板、增量编译、诊断回映射、写入安全边界（原子写、冻结只读、路径沙箱）全部在 Rust 里，Node 层只转发。
- 每个打开的文档一个常驻 typst 会话（最多 4 个，LRU），编辑时增量重编；同一文档的并发编译合并为最新一次。
- 预览走 PNG 而不是 SVG：与上游像素一致、浏览器无需字体；分辨率按 ¼ 倍频程取整以复用缓存，换页/重编译双缓冲不闪白。
- UI 不依赖组件库；视觉 token 集中在 `apps/web/src/styles.css`，亮/暗色随系统。
