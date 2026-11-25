//! What the setter asks for and what the model hands back. Field names are camelCase on
//! the wire so the web UI can use them as they are.

use serde::{Deserialize, Serialize};

/// Difficulty as a setter thinks of it, each with the Codeforces range it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Warmup,
    Easy,
    Medium,
    Hard,
    Challenge,
}

impl Tier {
    pub const ALL: [Tier; 5] = [
        Tier::Warmup,
        Tier::Easy,
        Tier::Medium,
        Tier::Hard,
        Tier::Challenge,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tier::Warmup => "签到",
            Tier::Easy => "简单",
            Tier::Medium => "中档",
            Tier::Hard => "困难",
            Tier::Challenge => "防 AK",
        }
    }

    /// Inclusive Codeforces rating range.
    pub fn range(self) -> (u32, u32) {
        match self {
            Tier::Warmup => (800, 1100),
            Tier::Easy => (1200, 1500),
            Tier::Medium => (1600, 1900),
            Tier::Hard => (2000, 2400),
            Tier::Challenge => (2500, 3500),
        }
    }

    pub fn clamp(self, rating: u32) -> u32 {
        let (lo, hi) = self.range();
        // Codeforces ratings come in steps of 100.
        (rating.clamp(lo, hi) / 100) * 100
    }
}

/// The setter's request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Brief {
    /// Story or setting; empty lets the model invent one.
    pub background: String,
    pub tier: Option<Tier>,
    pub tags: Vec<String>,
    /// Intent: what to test, which approaches to break, pitfalls to include.
    pub notes: String,
}

/// An approach the problem must reject, and how the data rejects it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Killed {
    pub approach: String,
    pub how: String,
}

/// A one-paragraph concept, before anything is written out.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Idea {
    pub title: String,
    /// The task in one or two sentences.
    pub summary: String,
    /// The intended solution.
    pub solution: String,
    pub complexity: String,
    pub killed: Vec<Killed>,
    pub rating: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sample {
    pub input: String,
    /// The model's hand-computed answer; verification replaces it with the reference
    /// solution's and reports a disagreement.
    pub output: String,
    pub explanation: String,
}

/// A deliberately wrong (or too slow) solution the tests must reject.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WrongSolution {
    /// File-name-safe: `wrong-greedy`.
    pub name: String,
    pub approach: String,
    pub code: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    /// Milliseconds.
    pub time_ms: u32,
    /// Megabytes.
    pub memory_mb: u32,
}

/// A complete problem, ready to verify and save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Draft {
    /// The problem's directory name: lowercase letters, digits and `-`.
    pub slug: String,
    pub title: String,
    pub rating: u32,
    pub tags: Vec<String>,
    pub limits: Limits,
    pub background: String,
    pub legend: String,
    pub input: String,
    pub output: String,
    pub constraints: String,
    pub note: String,
    pub samples: Vec<Sample>,
    pub editorial: String,
    /// Reference solution (C++17, stdin → stdout).
    pub std: String,
    /// Obviously-correct slow solution for stress testing.
    pub brute: String,
    /// `gen <small|large> <seed>` prints one test.
    pub generator: String,
    pub wrong: Vec<WrongSolution>,
    pub idea: Idea,
}

impl Draft {
    /// Fills in what the model may leave out and normalizes the rest.
    pub fn normalize(&mut self, tier: Option<Tier>) {
        self.slug = slugify(if self.slug.trim().is_empty() {
            &self.title
        } else {
            &self.slug
        });
        if self.limits.time_ms == 0 {
            self.limits.time_ms = 1000;
        }
        self.limits.time_ms = self.limits.time_ms.clamp(100, 20_000);
        if self.limits.memory_mb == 0 {
            self.limits.memory_mb = 256;
        }
        if let Some(t) = tier {
            let r = if self.rating == 0 {
                self.idea.rating
            } else {
                self.rating
            };
            self.rating = t.clamp(if r == 0 { t.range().0 } else { r });
        }
        self.tags.retain(|t| !t.trim().is_empty());
        for (i, w) in self.wrong.iter_mut().enumerate() {
            let base = slugify(&w.name);
            w.name = if base.is_empty() {
                format!("wrong-{}", i + 1)
            } else if base.starts_with("wrong") {
                base
            } else {
                format!("wrong-{base}")
            };
        }
    }
}

/// `"Tree Beacons!"` → `tree-beacons`; empty when nothing usable is left.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if (c == '-' || c == '_' || c.is_whitespace())
            && !out.ends_with('-')
            && !out.is_empty()
        {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    out.chars()
        .take(48)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Tree Beacons!"), "tree-beacons");
        assert_eq!(slugify("  crc--check "), "crc-check");
        assert_eq!(slugify("树上信标"), "");
    }

    #[test]
    fn tiers_clamp_to_hundreds() {
        assert_eq!(Tier::Medium.clamp(1750), 1700);
        assert_eq!(Tier::Medium.clamp(2300), 1900);
        assert_eq!(Tier::Warmup.clamp(0), 800);
    }
}
