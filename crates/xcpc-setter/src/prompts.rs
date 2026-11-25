//! What the model is told. Kept in one place so it can be tuned without touching the flow.

use serde_json::json;

use crate::llm::Message;
use crate::types::{Brief, Draft, Idea, Tier};

const ROLE: &str = "你是一位经验丰富的 ICPC / CCPC / NOI 命题人，擅长设计原创、严谨、难度准确的算法竞赛题。\
你与另一位命题人合作：对方给出需求，你负责构思与撰写。你的产出会被程序自动编译、对拍和检查，所以代码必须真实可运行，不能敷衍。";

fn brief_text(b: &Brief) -> String {
    let tier = match b.tier {
        Some(t) => {
            let (lo, hi) = t.range();
            format!("{}（Codeforces {lo}–{hi}）", t.label())
        }
        None => "未指定，由你判断".into(),
    };
    let mut s = String::new();
    s.push_str(&format!("- 目标难度：{tier}\n"));
    s.push_str(&format!(
        "- 考察方向（标签）：{}\n",
        if b.tags.is_empty() {
            "未指定".into()
        } else {
            b.tags.join("、")
        }
    ));
    s.push_str(&format!(
        "- 题目背景：{}\n",
        if b.background.trim().is_empty() {
            "未指定，请自拟一个简洁、不喧宾夺主的背景"
        } else {
            b.background.trim()
        }
    ));
    s.push_str(&format!(
        "- 命题意图 / 备注：{}\n",
        if b.notes.trim().is_empty() {
            "无"
        } else {
            b.notes.trim()
        }
    ));
    s
}

/// Brief → three distinct concepts.
pub fn ideas(brief: &Brief, examples: &[String]) -> Vec<Message> {
    let mut user = format!(
        "请根据下面的需求，构思 3 个**彼此不同**的原创题目方案。\n\n{}\n",
        brief_text(brief)
    );
    if !examples.is_empty() {
        user.push_str("\n命题组题库里相近难度的题目（用来校准难度感，不要照抄）：\n");
        for e in examples {
            user.push_str(&format!("- {e}\n"));
        }
    }
    user.push_str(
        "\n要求：\n\
         1. 难度要落在目标区间内：思维难度和实现量都要匹配，不要用一个经典模板题充数。\n\
         2. 如果备注里提到要卡掉某种做法，每个方案都要说明数据如何卡掉它（例如把 n 设到 2e5 让 O(n^2) 超时，或构造让贪心出错的结构）。\n\
         3. 题意必须能用简短的语言讲清楚，答案唯一（便于逐字比较输出），避免浮点误差和多解。\n\
         4. 不要复述已知的经典原题。\n\n\
         只输出 JSON：{\"ideas\":[{\"title\":\"中文题目名\",\"summary\":\"一两句话的题意\",\"solution\":\"标准做法的要点\",\
         \"complexity\":\"标准做法的时间复杂度\",\"killed\":[{\"approach\":\"要卡掉的做法\",\"how\":\"数据如何卡掉它\"}],\"rating\":1700}]}",
    );
    vec![Message::system(ROLE), Message::user(user)]
}

const DRAFT_SCHEMA: &str = r#"{
  "slug": "英文短横线目录名，如 tree-beacon",
  "title": "中文题目名",
  "rating": 1700,
  "tags": ["dp", "trees"],
  "limits": {"timeMs": 1000, "memoryMb": 256},
  "background": "题目背景（Markdown，可为空）",
  "legend": "题目描述（Markdown）",
  "input": "输入格式（Markdown）",
  "output": "输出格式（Markdown）",
  "constraints": "数据范围（Markdown，所有变量都要有明确上下界）",
  "note": "提示（可为空）",
  "samples": [{"input": "…", "output": "…", "explanation": "这组样例为什么是这个答案"}],
  "editorial": "题解（Markdown）：思路、正确性、复杂度，并说明被卡掉的做法为何不行",
  "std": "C++17 标准程序",
  "brute": "C++17 暴力程序",
  "generator": "C++17 数据生成器",
  "wrong": [{"name": "wrong-greedy", "approach": "这个做法是什么", "code": "C++17 代码"}]
}"#;

const CODE_RULES: &str = "代码约定（程序会被自动编译运行，务必遵守）：\n\
- 全部是 C++17，可以 #include <bits/stdc++.h>，从标准输入读、向标准输出写，main 返回 0。不要读写文件，不要输出调试信息。\n\
- std：正确且足够快，在最大数据下运行时间不超过时限的一半。\n\
- brute：直接按定义暴力，越简单越好，只需在小数据上正确。std 和 brute 必须是两种独立的写法。\n\
- generator：用法为 `gen <small|large> <seed>`。用 std::mt19937_64 并以 seed 播种，同样参数必须输出同样数据。\n\
  small 生成 brute 能在 0.1 秒内跑完的极小数据（例如 n ≤ 8），要覆盖边界与特殊结构；large 生成接近数据范围上限的数据，并包含能卡掉 wrong 的结构。生成的数据必须满足数据范围。\n\
- wrong：备注里要卡掉的每种做法各写一份「看起来对但会被卡」的实现（超时或答案错误），没有要卡的做法时为空数组。\n";

const TEXT_RULES: &str = "题面约定：\n\
- 使用简体中文 Markdown，数学用 $…$ / $$…$$ 的 LaTeX。各字段只写正文，不要写「## 输入格式」这样的标题。\n\
- 恰好给出 2 组样例：第 1 组小而直观，第 2 组能体现一个容易想错的情形；输入输出不超过 15 行，每组都写样例解释。\n\
- 样例的 output 请你手算填写，程序会用 std 的结果核对，所以请认真算。\n\
- 答案必须唯一，按逐个 token 比较。\n";

/// Brief + chosen concept → full problem.
pub fn draft(brief: &Brief, idea: &Idea) -> Vec<Message> {
    let idea_json = serde_json::to_string_pretty(idea).unwrap_or_default();
    let user = format!(
        "需求：\n{}\n选定的方案：\n{idea_json}\n\n请把它写成一道完整的题目。\n\n{TEXT_RULES}\n{CODE_RULES}\n只输出一个 JSON 对象，结构如下：\n{DRAFT_SCHEMA}",
        brief_text(brief)
    );
    vec![Message::system(ROLE), Message::user(user)]
}

/// Current draft + an instruction from the setter (or a failure report) → updated draft.
pub fn revise(brief: &Brief, current: &Draft, instruction: &str) -> Vec<Message> {
    let current_json = serde_json::to_string_pretty(current).unwrap_or_default();
    let user = format!(
        "需求：\n{}\n当前题目（JSON）：\n{current_json}\n\n修改要求：\n{instruction}\n\n\
         请只做必要的修改，没有涉及的字段保持原样；改动题意时要同步修改样例、题解和所有代码。\n\n{TEXT_RULES}\n{CODE_RULES}\n\
         输出修改后的完整 JSON 对象，结构与当前题目相同。",
        brief_text(brief)
    );
    vec![Message::system(ROLE), Message::user(user)]
}

/// A report of what verification found, phrased as a fix request.
pub fn repair_instruction(failures: &[String]) -> String {
    let mut s =
        String::from("自动验证发现了以下问题，请修正（优先修代码，题意确实有误再改题面）：\n");
    for f in failures {
        s.push_str("- ");
        s.push_str(f);
        s.push('\n');
    }
    s
}

/// Tier list for the UI.
pub fn tiers() -> serde_json::Value {
    json!(
        Tier::ALL
            .iter()
            .map(|t| {
                let (lo, hi) = t.range();
                json!({"id": t, "label": t.label(), "min": lo, "max": hi})
            })
            .collect::<Vec<_>>()
    )
}
