//! Does the draft hold up? Compiles everything, answers the samples with the reference
//! solution, stress-tests it against the brute force, times it on maximal data, and makes
//! sure every approach the setter wanted to break really breaks.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::judge::{self, Outcome, Sandbox, Toolchain};
use crate::types::Draft;

const STRESS_SEEDS: u64 = 300;
const STRESS_BUDGET: Duration = Duration::from_secs(20);
const LARGE_SEEDS: u64 = 3;
const WRONG_SMALL_SEEDS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Running,
    Ok,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub key: String,
    pub label: String,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SampleResult {
    /// What the reference solution prints: this is what gets saved.
    pub output: String,
    /// Whether the model's hand-computed answer agreed.
    pub claimed_matches: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counterexample {
    pub input: String,
    pub expected: String,
    pub got: String,
    /// Which program gave `got`.
    pub who: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    pub ok: bool,
    pub compiler: String,
    pub checks: Vec<Check>,
    pub samples: Vec<SampleResult>,
    pub stress_runs: u64,
    pub std_max_ms: u64,
    pub counterexample: Option<Counterexample>,
}

impl Report {
    /// Problems worth sending back to the model.
    pub fn failures(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .checks
            .iter()
            .filter(|c| c.status == Status::Fail)
            .map(|c| format!("{}：{}", c.label, c.detail))
            .collect();
        if let Some(c) = &self.counterexample {
            out.push(format!(
                "反例输入：\n{}\n期望（std）：{}\n{} 输出：{}",
                clip(&c.input, 1500),
                clip(&c.expected, 400),
                c.who,
                clip(&c.got, 400)
            ));
        }
        out
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…（已截断）", s.chars().take(n).collect::<String>())
    }
}

struct Progress<'a> {
    report: Report,
    notify: &'a dyn Fn(&Report),
}

impl Progress<'_> {
    fn set(&mut self, key: &str, status: Status, detail: impl Into<String>) {
        if let Some(c) = self.report.checks.iter_mut().find(|c| c.key == key) {
            c.status = status;
            c.detail = detail.into();
        }
        (self.notify)(&self.report);
    }
    fn skip_rest(&mut self) {
        for c in &mut self.report.checks {
            if c.status == Status::Pending {
                c.status = Status::Skipped;
            }
        }
    }
}

fn outcome_text(o: &Outcome, limit: Duration) -> String {
    match o {
        Outcome::Ok => "正常".into(),
        Outcome::Timeout => format!("超过 {} ms", limit.as_millis()),
        Outcome::Crash(why) => format!("运行错误（{why}）"),
        Outcome::OutputLimit => "输出过多".into(),
    }
}

/// Runs every check, calling `notify` whenever one changes.
pub fn verify(draft: &Draft, tc: &Toolchain, notify: &dyn Fn(&Report)) -> Result<Report, Error> {
    let mut checks = vec![
        ("compile", "编译"),
        ("samples", "样例"),
        ("stress", "对拍"),
        ("speed", "标程速度"),
        ("wrong", "卡掉错误做法"),
    ];
    if draft.wrong.is_empty() {
        checks.pop();
    }
    let mut p = Progress {
        report: Report {
            compiler: tc.version.clone(),
            checks: checks
                .iter()
                .map(|(k, l)| Check {
                    key: (*k).into(),
                    label: (*l).into(),
                    status: Status::Pending,
                    detail: String::new(),
                })
                .collect(),
            ..Default::default()
        },
        notify,
    };
    let tl = Duration::from_millis(draft.limits.time_ms as u64);
    let sb = Sandbox::new()?;

    // ---- Compile -----------------------------------------------------------------------
    p.set("compile", Status::Running, "");
    let mut sources: Vec<(String, &str)> = vec![
        ("std".into(), draft.std.as_str()),
        ("brute".into(), draft.brute.as_str()),
        ("gen".into(), draft.generator.as_str()),
    ];
    for w in &draft.wrong {
        sources.push((w.name.clone(), w.code.as_str()));
    }
    let mut bins: Vec<(String, PathBuf)> = Vec::new();
    for (name, src) in &sources {
        if src.trim().is_empty() {
            p.set("compile", Status::Fail, format!("缺少 {name} 的代码"));
            p.skip_rest();
            return Ok(p.report);
        }
        match judge::compile(tc, &sb, name, src) {
            Ok(bin) => bins.push((name.clone(), bin)),
            Err(log) => {
                p.set("compile", Status::Fail, format!("{name} 编译失败：\n{log}"));
                p.skip_rest();
                return Ok(p.report);
            }
        }
    }
    let bin = |n: &str| {
        bins.iter()
            .find(|(k, _)| k == n)
            .map(|(_, b)| b.clone())
            .unwrap()
    };
    let (std_bin, brute_bin, gen_bin) = (bin("std"), bin("brute"), bin("gen"));
    p.set("compile", Status::Ok, format!("{} 个程序", bins.len()));

    // ---- Samples -----------------------------------------------------------------------
    p.set("samples", Status::Running, "");
    if draft.samples.is_empty() {
        p.set("samples", Status::Fail, "没有样例");
        p.skip_rest();
        return Ok(p.report);
    }
    let mut corrected = 0;
    for (i, s) in draft.samples.iter().enumerate() {
        let r = judge::run(&std_bin, &[], &s.input, tl * 3);
        if r.outcome != Outcome::Ok {
            p.set(
                "samples",
                Status::Fail,
                format!(
                    "std 在样例 {} 上{}",
                    i + 1,
                    outcome_text(&r.outcome, tl * 3)
                ),
            );
            p.skip_rest();
            return Ok(p.report);
        }
        let b = judge::run(&brute_bin, &[], &s.input, Duration::from_secs(10));
        if b.outcome == Outcome::Ok && !judge::same_tokens(&b.stdout, &r.stdout) {
            p.report.counterexample = Some(Counterexample {
                input: s.input.clone(),
                expected: r.stdout.clone(),
                got: b.stdout,
                who: "brute".into(),
            });
            p.set(
                "samples",
                Status::Fail,
                format!("样例 {} 上 std 与 brute 的输出不同", i + 1),
            );
            p.skip_rest();
            return Ok(p.report);
        }
        let claimed_matches = judge::same_tokens(&s.output, &r.stdout);
        if !claimed_matches {
            corrected += 1;
        }
        p.report.samples.push(SampleResult {
            output: normalize_output(&r.stdout),
            claimed_matches,
        });
    }
    if corrected > 0 {
        p.set(
            "samples",
            Status::Warn,
            format!("{corrected} 组样例的手算答案与标程不同，已改用标程输出，请核对样例解释"),
        );
    } else {
        p.set(
            "samples",
            Status::Ok,
            format!("{} 组，手算答案与标程一致", draft.samples.len()),
        );
    }

    // ---- Stress ------------------------------------------------------------------------
    p.set("stress", Status::Running, "");
    let started = Instant::now();
    let mut small_cases: Vec<(String, String)> = Vec::new();
    let mut brute_slow = 0;
    for seed in 1..=STRESS_SEEDS {
        if started.elapsed() > STRESS_BUDGET && seed > 30 {
            break;
        }
        let seed_s = seed.to_string();
        let g = judge::run(&gen_bin, &["small", &seed_s], "", Duration::from_secs(5));
        if g.outcome != Outcome::Ok || g.stdout.trim().is_empty() {
            p.set(
                "stress",
                Status::Fail,
                format!(
                    "生成器 small {seed} {}",
                    if g.outcome == Outcome::Ok {
                        "没有输出".into()
                    } else {
                        outcome_text(&g.outcome, Duration::from_secs(5))
                    }
                ),
            );
            p.skip_rest();
            return Ok(p.report);
        }
        let s = judge::run(&std_bin, &[], &g.stdout, tl * 3);
        if s.outcome != Outcome::Ok {
            p.report.counterexample = Some(Counterexample {
                input: g.stdout.clone(),
                expected: String::new(),
                got: s.stderr.chars().take(400).collect(),
                who: "std".into(),
            });
            p.set(
                "stress",
                Status::Fail,
                format!(
                    "std 在小数据（种子 {seed}）上{}",
                    outcome_text(&s.outcome, tl * 3)
                ),
            );
            p.skip_rest();
            return Ok(p.report);
        }
        let b = judge::run(&brute_bin, &[], &g.stdout, Duration::from_secs(2));
        match b.outcome {
            Outcome::Ok if !judge::same_tokens(&b.stdout, &s.stdout) => {
                p.report.counterexample = Some(Counterexample {
                    input: g.stdout.clone(),
                    expected: s.stdout.clone(),
                    got: b.stdout,
                    who: "brute".into(),
                });
                p.report.stress_runs = seed;
                p.set(
                    "stress",
                    Status::Fail,
                    format!("第 {seed} 组随机小数据上 std 与 brute 不一致"),
                );
                p.skip_rest();
                return Ok(p.report);
            }
            Outcome::Ok => {}
            _ => brute_slow += 1,
        }
        p.report.stress_runs = seed;
        if small_cases.len() < WRONG_SMALL_SEEDS {
            small_cases.push((g.stdout, s.stdout));
        }
        if seed % 25 == 0 {
            p.set("stress", Status::Running, format!("{seed} 组"));
        }
    }
    let runs = p.report.stress_runs;
    if brute_slow * 2 > runs {
        p.set(
            "stress",
            Status::Fail,
            format!("brute 在 {brute_slow}/{runs} 组小数据上没能跑完，小数据要更小"),
        );
        p.skip_rest();
        return Ok(p.report);
    }
    p.set("stress", Status::Ok, format!("{runs} 组随机小数据全部一致"));

    // ---- Speed -------------------------------------------------------------------------
    p.set("speed", Status::Running, "");
    let mut large_cases: Vec<(String, String)> = Vec::new();
    let mut worst = Duration::ZERO;
    for seed in 1..=LARGE_SEEDS {
        let seed_s = seed.to_string();
        let g = judge::run(&gen_bin, &["large", &seed_s], "", Duration::from_secs(20));
        if g.outcome != Outcome::Ok || g.stdout.trim().is_empty() {
            p.set(
                "speed",
                Status::Fail,
                format!(
                    "生成器 large {seed} {}",
                    if g.outcome == Outcome::Ok {
                        "没有输出".into()
                    } else {
                        outcome_text(&g.outcome, Duration::from_secs(20))
                    }
                ),
            );
            p.skip_rest();
            return Ok(p.report);
        }
        let s = judge::run(&std_bin, &[], &g.stdout, tl * 3);
        if s.outcome != Outcome::Ok {
            p.set(
                "speed",
                Status::Fail,
                format!(
                    "std 在最大数据（种子 {seed}）上{}",
                    outcome_text(&s.outcome, tl * 3)
                ),
            );
            p.skip_rest();
            return Ok(p.report);
        }
        worst = worst.max(s.time);
        large_cases.push((g.stdout, s.stdout));
    }
    p.report.std_max_ms = worst.as_millis() as u64;
    let ms = worst.as_millis();
    if worst > tl {
        p.set(
            "speed",
            Status::Fail,
            format!(
                "std 在最大数据上用了 {ms} ms，超过时限 {} ms",
                tl.as_millis()
            ),
        );
        p.skip_rest();
        return Ok(p.report);
    } else if worst * 2 > tl {
        p.set(
            "speed",
            Status::Warn,
            format!("最大数据 {ms} ms，超过时限的一半"),
        );
    } else {
        p.set(
            "speed",
            Status::Ok,
            format!("最大数据 {ms} ms，时限 {} ms", tl.as_millis()),
        );
    }

    // ---- Wrong solutions ---------------------------------------------------------------
    if !draft.wrong.is_empty() {
        p.set("wrong", Status::Running, "");
        let mut survivors = Vec::new();
        let mut verdicts = Vec::new();
        for w in &draft.wrong {
            let wb = bin(&w.name);
            let verdict =
                small_cases
                    .iter()
                    .chain(large_cases.iter())
                    .find_map(|(input, expected)| {
                        let r = judge::run(&wb, &[], input, tl);
                        match r.outcome {
                            Outcome::Ok if judge::same_tokens(&r.stdout, expected) => None,
                            Outcome::Ok => Some("答案错误"),
                            Outcome::Timeout => Some("超时"),
                            _ => Some("运行错误"),
                        }
                    });
            match verdict {
                Some(v) => verdicts.push(format!("{}：{v}", w.name)),
                None => survivors.push(w.name.clone()),
            }
        }
        if survivors.is_empty() {
            p.set("wrong", Status::Ok, verdicts.join("；"));
        } else {
            p.set(
                "wrong",
                Status::Fail,
                format!("{} 没有被卡掉，需要更强的数据", survivors.join("、")),
            );
        }
    }

    p.report.ok = p
        .report
        .checks
        .iter()
        .all(|c| matches!(c.status, Status::Ok | Status::Warn | Status::Skipped));
    (p.notify)(&p.report);
    Ok(p.report)
}

/// Trailing spaces off every line, exactly one final newline.
fn normalize_output(s: &str) -> String {
    let mut out: String = s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}
