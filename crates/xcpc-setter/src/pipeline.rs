//! write / revise → verify → repair, until it holds up or the rounds run out.

use serde::Serialize;

use crate::judge::Toolchain;
use crate::llm::Llm;
use crate::types::{Brief, Draft, Idea};
use crate::verify::{Report, verify};
use crate::{Error, prompts, revise, write};

/// Automatic repair rounds after the first verification fails.
pub const REPAIR_ROUNDS: usize = 2;

pub enum Start {
    /// Write the chosen concept out.
    Write(Idea),
    /// Apply the setter's change request.
    Revise(Draft, String),
    /// Just check (after hand edits).
    Verify(Draft),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// What is happening, for people: 「撰写题面与代码」.
    pub stage: String,
    /// 0 for the first attempt, then each repair round.
    pub round: usize,
    pub report: Option<Report>,
}

pub struct Outcome {
    pub draft: Draft,
    /// `None` when there is no compiler to verify with.
    pub report: Option<Report>,
    pub rounds: usize,
}

pub fn produce(
    llm: &Llm,
    brief: &Brief,
    toolchain: Option<&Toolchain>,
    start: Start,
    progress: &dyn Fn(Progress),
) -> Result<Outcome, Error> {
    let say = |stage: &str, round: usize, report: Option<Report>| {
        progress(Progress {
            stage: stage.into(),
            round,
            report,
        })
    };

    let mut draft = match start {
        Start::Write(idea) => {
            say("撰写题面、样例与代码", 0, None);
            write(llm, brief, &idea)?
        }
        Start::Revise(d, instruction) => {
            say("按要求修改", 0, None);
            revise(llm, brief, &d, &instruction)?
        }
        Start::Verify(mut d) => {
            d.normalize(brief.tier);
            d
        }
    };
    let Some(tc) = toolchain else {
        return Ok(Outcome {
            draft,
            report: None,
            rounds: 0,
        });
    };

    let mut round = 0;
    loop {
        let report = verify(&draft, tc, &|r| say("验证", round, Some(r.clone())))?;
        if report.ok || round == REPAIR_ROUNDS {
            return Ok(Outcome {
                draft,
                report: Some(report),
                rounds: round,
            });
        }
        round += 1;
        say("根据验证结果修复", round, Some(report.clone()));
        let instruction = prompts::repair_instruction(&report.failures());
        draft = match revise(llm, brief, &draft, &instruction) {
            Ok(d) => d,
            // Keep the last draft and its report rather than losing the work.
            Err(_) => {
                return Ok(Outcome {
                    draft,
                    report: Some(report),
                    rounds: round - 1,
                });
            }
        };
    }
}
