//! Turning a verified draft into an ordinary problem directory.
//!
//! Layout follows the problem package conventions (docs/product.md §7.3): statement and
//! samples as usual, `solutions/std.cpp` (main), `solutions/brute.cpp`, the wrong
//! solutions, `generators/random.cpp`, plus `notes.md` for the setters' eyes only.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use xcpc_domain::ProblemSummary;
use xcpc_storage::Workspace;

use crate::Error;
use crate::types::{Brief, Draft};
use crate::verify::{Report, Status};

fn storage(e: xcpc_storage::Error) -> Error {
    Error::new(e.code(), e.human_message())
}

/// `1500` → `1.5s`.
fn seconds(ms: u32) -> String {
    if ms % 1000 == 0 {
        format!("{}s", ms / 1000)
    } else {
        format!("{}s", ms as f64 / 1000.0)
    }
}

/// A free problem id based on the draft's slug.
fn free_id(ws: &Workspace, draft: &Draft) -> Result<String, Error> {
    let base = if draft.slug.is_empty() {
        "problem".to_owned()
    } else {
        draft.slug.clone()
    };
    let taken = ws.list_problems().map_err(storage)?;
    if !taken.contains(&base) {
        return Ok(base);
    }
    (2..1000)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken.contains(id))
        .ok_or_else(|| Error::new("conflict", "没有可用的题目 ID"))
}

pub fn statement(draft: &Draft) -> String {
    let mut s = String::new();
    let mut section = |title: &str, body: &str| {
        if !body.trim().is_empty() {
            let _ = write!(s, "## {title}\n\n{}\n\n", body.trim());
        }
    };
    section("题目背景", &draft.background);
    section("题目描述", &draft.legend);
    section("输入格式", &draft.input);
    section("输出格式", &draft.output);
    let explanations: Vec<String> = draft
        .samples
        .iter()
        .enumerate()
        .filter(|(_, x)| !x.explanation.trim().is_empty())
        .map(|(i, x)| {
            if draft.samples.len() == 1 {
                x.explanation.trim().to_owned()
            } else {
                format!("**样例 {}**：{}", i + 1, x.explanation.trim())
            }
        })
        .collect();
    section("样例解释", &explanations.join("\n\n"));
    section("数据范围", &draft.constraints);
    section("提示", &draft.note);
    s.trim_end().to_owned() + "\n"
}

fn problem_toml(draft: &Draft) -> String {
    let mut t = toml::Table::new();
    t.insert("title".into(), draft.title.trim().into());
    if draft.rating > 0 {
        t.insert("rating".into(), (draft.rating as i64).into());
    }
    if !draft.tags.is_empty() {
        t.insert(
            "tags".into(),
            toml::Value::Array(draft.tags.iter().map(|x| x.trim().into()).collect()),
        );
    }
    let mut limits = toml::Table::new();
    limits.insert("time".into(), seconds(draft.limits.time_ms).into());
    limits.insert(
        "memory".into(),
        format!("{}MB", draft.limits.memory_mb).into(),
    );
    t.insert("limits".into(), limits.into());

    let mut solutions = toml::Table::new();
    let mut std = toml::Table::new();
    std.insert("main".into(), true.into());
    solutions.insert("std".into(), std.into());
    let mut brute = toml::Table::new();
    brute.insert(
        "expect".into(),
        toml::Value::Array(vec!["ac".into(), "tle".into()]),
    );
    solutions.insert("brute".into(), brute.into());
    for w in &draft.wrong {
        let mut e = toml::Table::new();
        e.insert(
            "expect".into(),
            toml::Value::Array(vec!["wa".into(), "tle".into(), "re".into()]),
        );
        solutions.insert(w.name.clone(), e.into());
    }
    t.insert("solutions".into(), solutions.into());
    toml::to_string(&t).unwrap_or_default()
}

fn notes(brief: &Brief, draft: &Draft, report: Option<&Report>) -> String {
    let mut s = String::from(
        "# 命题笔记\n\n> 由出题助手生成，仅供命题组内部参考，不会排进题面。\n\n## 需求\n\n",
    );
    if let Some(t) = brief.tier {
        let (lo, hi) = t.range();
        let _ = writeln!(s, "- 难度：{}（CF {lo}–{hi}）", t.label());
    }
    if !brief.tags.is_empty() {
        let _ = writeln!(s, "- 标签：{}", brief.tags.join("、"));
    }
    if !brief.background.trim().is_empty() {
        let _ = writeln!(s, "- 背景：{}", brief.background.trim());
    }
    if !brief.notes.trim().is_empty() {
        let _ = writeln!(s, "- 命题意图：{}", brief.notes.trim());
    }
    let i = &draft.idea;
    let _ = write!(
        s,
        "\n## 构思\n\n{}\n\n- 做法：{}\n- 复杂度：{}\n",
        i.summary.trim(),
        i.solution.trim(),
        i.complexity.trim()
    );
    if !draft.wrong.is_empty() {
        s.push_str("\n## 要卡掉的做法\n\n");
        for w in &draft.wrong {
            let _ = writeln!(s, "- `solutions/{}.cpp`：{}", w.name, w.approach.trim());
        }
    }
    if let Some(r) = report {
        let _ = write!(s, "\n## 验证（本机 {}）\n\n", r.compiler);
        for c in &r.checks {
            let mark = match c.status {
                Status::Ok => "✓",
                Status::Warn => "!",
                Status::Fail => "✗",
                _ => "·",
            };
            let _ = writeln!(
                s,
                "- {mark} {}：{}",
                c.label,
                c.detail.lines().next().unwrap_or("")
            );
        }
    }
    s.push_str(
        "\n## 复现\n\n生成器用法：`random <small|large> <seed>`，同样的参数生成同样的数据。\n",
    );
    s
}

/// Writes `draft` as a new problem (and appends it to `contest`). Returns its id.
pub fn commit(
    ws: &Workspace,
    brief: &Brief,
    draft: &Draft,
    report: Option<&Report>,
    contest: Option<&str>,
) -> Result<String, Error> {
    if draft.title.trim().is_empty() || draft.legend.trim().is_empty() {
        return Err(Error::new("invalid", "题目还不完整"));
    }
    if let Some(c) = contest {
        let meta = ws.load_contest(c).map_err(storage)?;
        if meta.meta.frozen {
            return Err(Error::new("contest_frozen", "这场比赛已冻结，不能再加题"));
        }
    }
    let id = free_id(ws, draft)?;
    let mut files = BTreeMap::new();
    files.insert("problem.toml".to_owned(), problem_toml(draft));
    files.insert("statement.md".to_owned(), statement(draft));
    if !draft.editorial.trim().is_empty() {
        files.insert(
            "editorial.md".to_owned(),
            draft.editorial.trim().to_owned() + "\n",
        );
    }
    files.insert("notes.md".to_owned(), notes(brief, draft, report));
    files.insert("solutions/std.cpp".to_owned(), draft.std.clone());
    files.insert("solutions/brute.cpp".to_owned(), draft.brute.clone());
    files.insert("generators/random.cpp".to_owned(), draft.generator.clone());
    for w in &draft.wrong {
        files.insert(format!("solutions/{}.cpp", w.name), w.code.clone());
    }
    ws.write_problem_files(&id, &files, None).map_err(storage)?;

    let samples: Vec<(String, String)> = draft
        .samples
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let out = report
                .and_then(|r| r.samples.get(i))
                .map(|r| r.output.clone())
                .unwrap_or_else(|| s.output.clone());
            (s.input.clone(), out)
        })
        .collect();
    ws.save_samples(&id, &samples).map_err(storage)?;
    if let Some(c) = contest {
        ws.add_problem_to_contest(c, &id).map_err(storage)?;
    }
    Ok(id)
}

/// Library problems near the brief's difficulty and topics, to calibrate the model.
pub fn calibration(library: &[ProblemSummary], brief: &Brief) -> Vec<String> {
    let Some(tier) = brief.tier else {
        return Vec::new();
    };
    let (lo, hi) = tier.range();
    let mut scored: Vec<(usize, String)> = library
        .iter()
        .filter(|p| p.rating.is_some_and(|r| r + 200 >= lo && r <= hi + 200))
        .map(|p| {
            let overlap = p
                .tags
                .iter()
                .filter(|t| brief.tags.iter().any(|b| b.eq_ignore_ascii_case(t)))
                .count();
            (
                overlap,
                format!(
                    "{}（{}，{}）",
                    p.title,
                    p.rating.unwrap_or(0),
                    p.tags.join(", ")
                ),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(4).map(|(_, s)| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Sample, WrongSolution};

    #[test]
    fn writes_a_problem_package() {
        let dir = tempfile::tempdir().unwrap();
        let ws = xcpc_storage::init_workspace(dir.path()).unwrap();
        let mut d = Draft {
            slug: "sum".into(),
            title: "求和".into(),
            rating: 800,
            tags: vec!["implementation".into()],
            legend: "给定 $a, b$，求 $a+b$。".into(),
            input: "一行两个整数。".into(),
            output: "一个整数。".into(),
            constraints: "$|a|, |b| \\le 10^9$".into(),
            samples: vec![Sample {
                input: "1 2\n".into(),
                output: "3\n".into(),
                explanation: "$1+2=3$。".into(),
            }],
            std: "int main(){}".into(),
            brute: "int main(){}".into(),
            generator: "int main(){}".into(),
            wrong: vec![WrongSolution {
                name: "int".into(),
                approach: "用 int 溢出".into(),
                code: "int main(){}".into(),
            }],
            ..Default::default()
        };
        d.normalize(None);
        let id = commit(&ws, &Brief::default(), &d, None, None).unwrap();
        assert_eq!(id, "sum");
        let p = ws.load_problem("sum").unwrap();
        assert_eq!(p.meta.title, "求和");
        assert_eq!(p.samples[0].output, "3\n");
        assert!(p.statements[0].file.text.contains("## 样例解释"));
        assert!(
            dir.path()
                .join("problems/sum/solutions/wrong-int.cpp")
                .is_file()
        );
        assert!(
            dir.path()
                .join("problems/sum/generators/random.cpp")
                .is_file()
        );
        // A second one with the same slug gets a fresh id.
        assert_eq!(
            commit(&ws, &Brief::default(), &d, None, None).unwrap(),
            "sum-2"
        );
    }
}
