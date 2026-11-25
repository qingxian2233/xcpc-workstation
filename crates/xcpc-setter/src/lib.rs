//! AI-assisted problem setting.
//!
//! The setter states what they want (background, difficulty, topics, intent); the model
//! proposes concepts and writes the chosen one out with a reference solution, a brute
//! force, a generator and the wrong solutions the tests must break; this machine's C++
//! compiler then checks all of it before anything reaches the workspace.

pub mod commit;
pub mod judge;
pub mod llm;
pub mod pipeline;
pub mod prompts;
pub mod types;
pub mod verify;

use serde_json::Value;

pub use commit::{calibration, commit, statement};
pub use llm::Llm;
pub use types::{Brief, Draft, Idea, Tier};
pub use verify::{Report, verify};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: impl std::fmt::Display) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

/// Brief → three concepts.
pub fn ideas(llm: &Llm, brief: &Brief, examples: &[String]) -> Result<Vec<Idea>, Error> {
    let v = llm.json(&prompts::ideas(brief, examples))?;
    let list = v.get("ideas").cloned().unwrap_or(Value::Null);
    let mut ideas: Vec<Idea> = serde_json::from_value(list)
        .map_err(|e| Error::new("upstream", format!("模型返回的构思格式不对：{e}")))?;
    if ideas.is_empty() {
        return Err(Error::new("upstream", "模型没有给出构思"));
    }
    if let Some(t) = brief.tier {
        for i in &mut ideas {
            i.rating = t.clamp(if i.rating == 0 { t.range().0 } else { i.rating });
        }
    }
    ideas.truncate(4);
    Ok(ideas)
}

fn parse_draft(v: Value, brief: &Brief, idea: &Idea) -> Result<Draft, Error> {
    let mut d: Draft = serde_json::from_value(v)
        .map_err(|e| Error::new("upstream", format!("模型返回的题目格式不对：{e}")))?;
    if d.title.trim().is_empty() || d.legend.trim().is_empty() {
        return Err(Error::new("upstream", "模型返回的题目不完整"));
    }
    d.idea = idea.clone();
    d.normalize(brief.tier);
    Ok(d)
}

/// Brief + concept → full draft.
pub fn write(llm: &Llm, brief: &Brief, idea: &Idea) -> Result<Draft, Error> {
    parse_draft(llm.json(&prompts::draft(brief, idea))?, brief, idea)
}

/// Applies a free-form change request to a draft.
pub fn revise(llm: &Llm, brief: &Brief, draft: &Draft, instruction: &str) -> Result<Draft, Error> {
    parse_draft(
        llm.json(&prompts::revise(brief, draft, instruction))?,
        brief,
        &draft.idea,
    )
}
