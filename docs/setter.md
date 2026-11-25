# 出题助手

命题人给出需求，模型构思并写出完整的题，本机编译器验证之后再存进题库。入口：顶栏右上角的「出题」，或比赛编辑页左侧题目列表底部的「出一道题」。

## 流程

1. **需求**：难度（签到 / 简单 / 中档 / 困难 / 防 AK，各自对应 CF 区间）、考察方向（标签）、命题意图（想考什么、想卡掉什么做法）、可选的题目背景，以及存入题库还是某场比赛。
2. **构思**：模型给出 3 个方案，每个包含一句话题意、标准做法、复杂度、要卡掉的做法和卡法。生成时会附上题库里难度相近、标签重合的几道题的标题，用来校准难度。
3. **成稿**：选定方案后，模型写出题面各节、2 组带解释的样例、题解，以及 `std`（标程）、`brute`（暴力）、`random`（生成器）和每种要卡掉的做法各一份 `wrong-*`。
4. **验证**（需要本机 C++ 编译器）：
   - 编译全部程序（macOS 的 clang 没有 `bits/stdc++.h`，验证时自动提供一份）。
   - 用 std 运行样例，**样例输出以 std 为准**，与模型手算不一致时提示核对样例解释；同时用 brute 交叉检查。
   - std 与 brute 在随机小数据上对拍（最多 300 组、约 20 秒）。
   - std 在 3 组最大数据上测速：超过时限算失败，超过一半给提示。
   - 每份 wrong 在全部测试上必须出现答案错误、超时或运行错误，否则算没卡住。
   - 不通过时把失败项（含反例）发回给模型修复，最多自动修 2 轮。
5. **预览与修改**：草稿写进一个临时工作区，用当前工作区的模板真实排版；可以用一句话让模型修改（「n 改到 10⁶」「样例 2 换成一条链」），修改后自动重新验证。
6. **保存**：按题目包约定写入：

```text
problems/<id>/
  problem.toml          # title、rating、tags、limits、[solutions.*]
  statement.md          # 题目背景 / 题目描述 / 输入格式 / 输出格式 / 样例解释 / 数据范围
  editorial.md
  notes.md              # 命题笔记：需求、构思、要卡掉的做法、验证结果；不排进题面
  samples/01.in .ans …  # 输出来自 std
  solutions/std.cpp brute.cpp wrong-*.cpp
  generators/random.cpp # random <small|large> <seed>
```

## 说明

- 本机验证是给命题人的快速检查，用的是系统编译器和真实耗时，**不是**路线图第 3 阶段可复现的构建（wasm + tick）。时间以本机为准，换机器会有出入。
- 被验证的是模型写的代码，在临时目录里运行，有时限和输出上限，但没有内存限制和系统调用沙箱。每次运行都由你手动发起。
- 发给模型服务的内容：你的需求、选定的方案、草稿本身，以及题库里几道相近题的标题、难度和标签。不会发送题库里的题面。
- 模型服务在「设置 → 模型服务」里先选**接口格式**：
  - **Chat Completions**（`/chat/completions`）：大多数服务都支持，如 DeepSeek、通义、Kimi、智谱、Ollama、各类中转。用 `response_format` 要求返回 JSON，推理强度走 `reasoning_effort`。
  - **Responses**（`/responses`）：OpenAI 的新接口。系统提示放进 `instructions`，JSON 模式走 `text.format`，推理强度走 `reasoning.effort`。
  - **Messages**（`/messages`）：Anthropic Claude 及兼容它的服务。认证用 `x-api-key` 加 `anthropic-version` 请求头，系统提示放在顶层 `system`；推理强度不是「默认」时发 `thinking: {type: "adaptive"}` 和 `output_config.effort`，「默认」时两者都不发，方便兼容服务。这个格式没有 JSON 模式开关，靠提示词要求只输出 JSON。
  - 服务不支持 JSON 模式开关时会自动去掉它重试；回复不是合法 JSON 时会再追问一次。
- 密钥在 macOS 上存进钥匙串，在其他系统上存进偏好设置旁边一个只有本人可读的文件；也可以用环境变量 `XCPC_LLM_API_KEY` 提供。

## 在 macOS 上准备编译器

1. 打开「终端」，运行 `xcode-select --install`，在弹窗里点「安装」（不需要完整的 Xcode）。
2. 装好后运行 `clang++ --version`，能看到版本号即可。
3. 回到应用的「设置 → 本机编译器」点「重新检测」。

想用 Homebrew 的 g++ 时，设置环境变量 `XCPC_CXX`（例如 `/opt/homebrew/bin/g++-14`）。

## 开发与测试

```sh
node scripts/mock-llm.mjs 4600          # 假的模型服务：第一版 std 故意带 bug，用来走通修复流程
cargo run -p xcpc-server --bin xcpc-serve -- <工作区>   # 无窗口运行同一套后端，打印带 token 的地址
cargo test -p xcpc-setter
```
