//! A minimal model client that insists on a JSON object back. Speaks three wire formats:
//! OpenAI Chat Completions (what most compatible services offer), OpenAI Responses, and
//! Anthropic Messages.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Error;

/// Which HTTP API the service speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    /// `POST {base}/chat/completions`.
    #[default]
    Chat,
    /// `POST {base}/responses`.
    Responses,
    /// `POST {base}/messages` (Anthropic).
    Messages,
}

impl ApiFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "chat" => Some(Self::Chat),
            "responses" => Some(Self::Responses),
            "messages" => Some(Self::Messages),
            _ => None,
        }
    }
}

/// Anthropic API version sent with every Messages request.
const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Output cap for one Messages reply: a full problem with four programs is long.
const MESSAGES_MAX_TOKENS: u32 = 32_000;

#[derive(Debug, Clone, Default)]
pub struct Llm {
    pub format: ApiFormat,
    /// `https://api.openai.com/v1` style: the endpoint path is appended.
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// `auto` sends nothing; otherwise the format's reasoning-effort field.
    pub reasoning_effort: String,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: &'static str,
    pub content: String,
}

impl Message {
    pub fn system(s: impl Into<String>) -> Self {
        Self {
            role: "system",
            content: s.into(),
        }
    }
    pub fn user(s: impl Into<String>) -> Self {
        Self {
            role: "user",
            content: s.into(),
        }
    }
    pub fn assistant(s: impl Into<String>) -> Self {
        Self {
            role: "assistant",
            content: s.into(),
        }
    }
}

impl Llm {
    fn client(&self) -> Result<reqwest::blocking::Client, Error> {
        reqwest::blocking::Client::builder()
            // Reasoning models writing a full problem with four programs take minutes.
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| Error::new("upstream", e))
    }

    fn effort(&self) -> Option<&str> {
        Some(self.reasoning_effort.as_str()).filter(|e| !e.is_empty() && *e != "auto")
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base_url.trim_end_matches('/'))
    }

    /// Adds the format's authentication headers.
    fn auth(&self, req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        match (self.format, &self.api_key) {
            (ApiFormat::Messages, Some(key)) => req
                .header("x-api-key", key)
                .header("anthropic-version", ANTHROPIC_VERSION),
            (ApiFormat::Messages, None) => req.header("anthropic-version", ANTHROPIC_VERSION),
            (_, Some(key)) => req.bearer_auth(key),
            (_, None) => req,
        }
    }

    /// Asks for a JSON object. Retries once without the JSON-mode switch when the service
    /// rejects it, and once more when the reply is not parseable JSON.
    pub fn json(&self, messages: &[Message]) -> Result<Value, Error> {
        if self.model.trim().is_empty() {
            return Err(Error::new("not_configured", "请先在设置里选择模型"));
        }
        // Messages has no JSON mode switch; the prompts already ask for JSON only.
        let mut json_mode = self.format != ApiFormat::Messages;
        let mut msgs: Vec<Message> = messages.to_vec();
        for attempt in 0..3 {
            match self.complete(&msgs, json_mode) {
                Ok(text) => match parse_object(&text) {
                    Some(v) => return Ok(v),
                    None if attempt < 2 => {
                        msgs.push(Message::assistant(text));
                        msgs.push(Message::user("上面的回复不是一个合法的 JSON 对象。请只输出 JSON 对象本身，不要任何解释或代码围栏。"));
                    }
                    None => return Err(Error::new("upstream", "模型没有返回合法的 JSON")),
                },
                Err(e) if json_mode && rejects_json_mode(&e.message) => json_mode = false,
                Err(e) => return Err(e),
            }
        }
        Err(Error::new("upstream", "模型没有返回合法的 JSON"))
    }

    fn complete(&self, messages: &[Message], json_mode: bool) -> Result<String, Error> {
        let (path, body) = match self.format {
            ApiFormat::Chat => {
                let mut body = json!({
                    "model": self.model,
                    "messages": messages.iter().map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
                });
                if json_mode {
                    body["response_format"] = json!({"type": "json_object"});
                }
                if let Some(e) = self.effort() {
                    body["reasoning_effort"] = json!(e);
                }
                ("chat/completions", body)
            }
            ApiFormat::Responses => {
                let instructions = system_text(messages);
                let mut body = json!({
                    "model": self.model,
                    "input": messages.iter().filter(|m| m.role != "system").map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
                });
                if !instructions.is_empty() {
                    body["instructions"] = json!(instructions);
                }
                if json_mode {
                    body["text"] = json!({"format": {"type": "json_object"}});
                }
                if let Some(e) = self.effort() {
                    body["reasoning"] = json!({"effort": e});
                }
                ("responses", body)
            }
            ApiFormat::Messages => {
                let system = system_text(messages);
                let mut body = json!({
                    "model": self.model,
                    "max_tokens": MESSAGES_MAX_TOKENS,
                    "messages": messages.iter().filter(|m| m.role != "system").map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
                });
                if !system.is_empty() {
                    body["system"] = json!(system);
                }
                // Adaptive thinking with an effort level; left out entirely on `auto`
                // so compatible services that lack these fields still work.
                if let Some(e) = self.effort() {
                    body["thinking"] = json!({"type": "adaptive"});
                    body["output_config"] = json!({"effort": e});
                }
                ("messages", body)
            }
        };

        let req = self.auth(self.client()?.post(self.url(path)).json(&body));
        let res = req
            .send()
            .map_err(|e| Error::new("upstream", describe(&e)))?;
        let status = res.status();
        let raw = res.text().map_err(|e| Error::new("upstream", e))?;
        let value: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| raw.chars().take(300).collect());
            return Err(Error::new(
                "upstream",
                format!("模型服务返回 HTTP {}：{msg}", status.as_u16()),
            ));
        }
        let text = match self.format {
            ApiFormat::Chat => chat_text(&value),
            ApiFormat::Responses => responses_text(&value),
            ApiFormat::Messages => {
                if value["stop_reason"] == "refusal" {
                    return Err(Error::new("upstream", "模型拒绝了这个请求"));
                }
                messages_text(&value)
            }
        };
        if text.trim().is_empty() {
            return Err(Error::new("upstream", "模型返回了空回复"));
        }
        Ok(text)
    }

    /// Model ids the service offers.
    pub fn models(&self) -> Result<Vec<String>, Error> {
        let req = self.auth(self.client()?.get(self.url("models")));
        let res = req
            .send()
            .map_err(|e| Error::new("upstream", describe(&e)))?;
        let status = res.status();
        let value: Value = res.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("请求失败");
            return Err(Error::new(
                "upstream",
                format!("模型服务返回 HTTP {}：{msg}", status.as_u16()),
            ));
        }
        let list = value
            .get("data")
            .or_else(|| value.get("models"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("upstream", "模型服务没有返回模型列表"))?;
        let mut ids: Vec<String> = list
            .iter()
            .filter_map(|m| {
                m.get("id")
                    .or_else(|| m.get("name"))
                    .and_then(Value::as_str)
            })
            .map(str::to_owned)
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

/// System messages joined, for formats that take them apart from the conversation.
fn system_text(messages: &[Message]) -> String {
    messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A 400 about the JSON-mode switch itself, which some compatible services lack.
fn rejects_json_mode(message: &str) -> bool {
    ["response_format", "json_object", "text.format"]
        .iter()
        .any(|k| message.contains(k))
}

fn chat_text(v: &Value) -> String {
    match &v["choices"][0]["message"]["content"] {
        Value::String(s) => s.clone(),
        // Some services return content parts.
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// `output_text` when present, else the text parts of every `message` output item
/// (reasoning items carry no `output_text` parts and are skipped).
fn responses_text(v: &Value) -> String {
    if let Some(s) = v["output_text"].as_str() {
        return s.to_owned();
    }
    v["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter(|part| part["type"] == "output_text")
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}

/// The `text` blocks of the reply; thinking blocks are skipped.
fn messages_text(v: &Value) -> String {
    v["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}

fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "连接模型服务超时".into()
    } else if e.is_connect() {
        "连不上模型服务，请检查 API 地址和网络".into()
    } else {
        e.to_string()
    }
}

/// The first `{…}` object in `text`, tolerating code fences and chatter around it.
pub fn parse_object(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let start = t.find('{')?;
    let end = t.rfind('}')?;
    match serde_json::from_str::<Value>(&t[start..=end]) {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_json() {
        let v = parse_object("好的：\n```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v["a"], 1);
        assert!(parse_object("no json here").is_none());
    }

    #[test]
    fn extracts_text_from_each_format() {
        let chat = json!({"choices": [{"message": {"content": "{\"x\":1}"}}]});
        assert_eq!(chat_text(&chat), "{\"x\":1}");

        let responses = json!({"output": [
            {"type": "reasoning", "summary": []},
            {"type": "message", "content": [{"type": "output_text", "text": "{\"x\":"}, {"type": "output_text", "text": "2}"}]}
        ]});
        assert_eq!(responses_text(&responses), "{\"x\":2}");
        assert_eq!(responses_text(&json!({"output_text": "ok"})), "ok");

        let messages = json!({"content": [
            {"type": "thinking", "thinking": ""},
            {"type": "text", "text": "{\"x\":3}"}
        ], "stop_reason": "end_turn"});
        assert_eq!(messages_text(&messages), "{\"x\":3}");
    }

    #[test]
    fn formats_round_trip() {
        assert_eq!(ApiFormat::parse("responses"), Some(ApiFormat::Responses));
        assert_eq!(
            serde_json::to_value(ApiFormat::Messages).unwrap(),
            "messages"
        );
        assert_eq!(ApiFormat::parse("nope"), None);
    }
}
