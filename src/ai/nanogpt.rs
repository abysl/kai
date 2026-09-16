#[cfg(not(target_arch = "wasm32"))]
use super::provider::http_error;
use super::provider::{Credentials, Provider};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";
pub const THINKING_MODEL: &str = "z-ai/glm-5.3-flash";
pub const FASTEST_MODEL: &str = "gemini-2.5-flash-lite-preview-09-2025";
pub const ENDPOINT: &str = "https://nano-gpt.com/api/v1/chat/completions";
pub const KEY_ENV: &str = "NANOGPT_API_KEY";
pub const MISSING_KEY: &str = "Set NANOGPT_API_KEY to use the AI opponent";

pub fn api_key() -> String {
    std::env::var(KEY_ENV)
        .ok()
        .filter(|key| !key.trim().is_empty())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: u64,
    pub cached_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Clone)]
pub struct Client {
    #[cfg(not(target_arch = "wasm32"))]
    agent: ureq::Agent,
    key: String,
    provider: Provider,
    pub model: String,
    canned: Option<Arc<Mutex<VecDeque<Reply>>>>,
}

pub const CANNED_EXHAUSTED: &str = "the canned client has no more replies";

impl Client {
    pub fn new(model: impl Into<String>) -> Self {
        Self::configured(model, Credentials::from_env(Provider::NanoGpt))
    }

    pub fn configured(model: impl Into<String>, credentials: Credentials) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build(),
            key: credentials.key.0.trim().into(),
            provider: credentials.provider,
            model: model.into(),
            canned: None,
        }
    }

    pub fn canned(model: impl Into<String>, replies: Vec<Reply>) -> Self {
        Self {
            canned: Some(Arc::new(Mutex::new(replies.into_iter().collect()))),
            ..Self::new(model)
        }
    }

    pub fn is_canned(&self) -> bool {
        self.canned.is_some()
    }

    pub fn canned_left(&self) -> usize {
        self.canned
            .as_ref()
            .map(|replies| replies.lock().len())
            .unwrap_or(0)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn chat(
        &self,
        messages: &[Value],
        tools: &[Value],
        require_tool: bool,
    ) -> Result<Reply, String> {
        if let Some(replies) = &self.canned {
            return replies
                .lock()
                .pop_front()
                .ok_or_else(|| CANNED_EXHAUSTED.to_string());
        }
        if self.key.trim().is_empty() {
            return Err(self.missing_key());
        }
        let body = json!({
            "model": self.model,
            "messages": messages,
            "tools": tools,
            "tool_choice": if require_tool { "required" } else { "auto" },
        });
        let body = body.to_string();
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.send(&body) {
                Ok(reply) => return Ok(reply),
                Err(error) if attempt <= RETRIES && retryable(&error) => {
                    std::thread::sleep(Duration::from_secs(RETRY_PAUSE_SECS * attempt));
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn missing_key(&self) -> String {
        format!(
            "Enter your {} API key in AI settings (or set {})",
            self.provider.label(),
            self.provider.key_env()
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn chat_async(
        &self,
        messages: &[Value],
        tools: &[Value],
        require_tool: bool,
    ) -> Result<Reply, String> {
        if let Some(replies) = &self.canned {
            return replies
                .lock()
                .pop_front()
                .ok_or_else(|| CANNED_EXHAUSTED.into());
        }
        if self.key.trim().is_empty() {
            return Err(self.missing_key());
        }
        let body = json!({ "model": self.model, "messages": messages, "tools": tools,
            "tool_choice": if require_tool { "required" } else { "auto" } })
        .to_string();
        let value = super::web_http::request(
            self.provider,
            self.provider.endpoint(),
            Some(&self.key),
            Some(&body),
            240_000,
        )
        .await?;
        checked_reply(&value)
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub async fn chat_async(
        &self,
        messages: &[Value],
        tools: &[Value],
        require_tool: bool,
    ) -> Result<Reply, String> {
        assert!(
            self.is_canned(),
            "async native tests must not make paid requests"
        );
        self.chat(messages, tools, require_tool)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send(&self, body: &str) -> Result<Reply, String> {
        let response = self
            .agent
            .post(self.provider.endpoint())
            .set("Authorization", &format!("Bearer {}", self.key))
            .set("Content-Type", "application/json")
            .send_string(body)
            .map_err(|error| match error {
                ureq::Error::Status(code, _) => http_error(self.provider, code),
                _ => format!(
                    "{}: Network Error or request timed out",
                    self.provider.label()
                ),
            })?;
        let value: Value = response
            .into_json()
            .map_err(|_| format!("{} returned invalid JSON", self.provider.label()))?;
        checked_reply(&value)
    }
}

pub const REQUEST_TIMEOUT_SECS: u64 = 240;
pub const RETRIES: u64 = 2;
pub const RETRY_PAUSE_SECS: u64 = 5;

pub fn retryable(error: &str) -> bool {
    error.contains("timed out")
        || error.contains("nanogpt 429")
        || error.contains("nanogpt 5")
        || error.contains("NanoGPT 429")
        || error.contains("NanoGPT 5")
        || error.contains("OpenRouter 429")
        || error.contains("OpenRouter 5")
        || error.contains("Network Error")
}

pub fn checked_reply(value: &Value) -> Result<Reply, String> {
    if !value["choices"][0]["message"].is_object() || value.get("error").is_some() {
        return Err(
            "The provider returned no assistant response; check model access and credit".into(),
        );
    }
    Ok(parse_reply(value))
}

pub fn parse_reply(value: &Value) -> Reply {
    let message = &value["choices"][0]["message"];
    let text = |key: &str| message[key].as_str().unwrap_or_default().to_string();
    let tool_calls = message["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .enumerate()
                .filter_map(|(index, call)| {
                    let function = &call["function"];
                    Some(ToolCall {
                        id: call["id"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("call-{index}")),
                        name: function["name"].as_str()?.to_string(),
                        arguments: match &function["arguments"] {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Reply {
        content: text("content"),
        reasoning: if message["reasoning_content"].is_string() {
            text("reasoning_content")
        } else {
            text("reasoning")
        },
        tool_calls,
        prompt_tokens: value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        cached_tokens: cached_tokens(&value["usage"]),
        completion_tokens: value["usage"]["completion_tokens"].as_u64().unwrap_or(0),
    }
}

pub fn cached_tokens(usage: &Value) -> u64 {
    usage["prompt_cache_hit_tokens"]
        .as_u64()
        .or_else(|| usage["prompt_tokens_details"]["cached_tokens"].as_u64())
        .unwrap_or(0)
}

pub fn assistant_message(reply: &Reply) -> Value {
    let mut message = json!({ "role": "assistant", "content": reply.content });
    if !reply.tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(
            reply
                .tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": { "name": call.name, "arguments": call.arguments },
                    })
                })
                .collect(),
        );
    }
    message
}

pub fn tool_result(call: &ToolCall, content: &str) -> Value {
    json!({ "role": "tool", "tool_call_id": call.id, "content": content })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_timeout_a_rate_limit_and_a_server_error_are_retried_but_a_bad_request_is_not() {
        assert!(retryable(
            "nanogpt: Network Error: timed out reading response"
        ));
        assert!(retryable("nanogpt 429: slow down"));
        assert!(retryable("nanogpt 502: bad gateway"));
        assert!(!retryable("nanogpt 400: unknown model"));
        assert!(!retryable("nanogpt reply: expected value"));
        assert_eq!(RETRIES, 2);
    }

    use super::*;

    #[test]
    fn a_canned_client_hands_out_its_replies_in_order_and_never_touches_the_network() {
        let client = Client::canned(
            DEFAULT_MODEL,
            vec![
                Reply {
                    content: "first".into(),
                    ..Reply::default()
                },
                Reply {
                    content: "second".into(),
                    ..Reply::default()
                },
            ],
        );
        assert!(client.is_canned());
        assert!(!Client::new(DEFAULT_MODEL).is_canned());
        let twin = client.clone();
        assert_eq!(client.chat(&[], &[], true).unwrap().content, "first");
        assert_eq!(
            twin.chat(&[], &[], false).unwrap().content,
            "second",
            "a clone shares the queue, as the brain clones its client per decision"
        );
        assert_eq!(client.canned_left(), 0);
        assert_eq!(
            client.chat(&[], &[], true).err().as_deref(),
            Some(CANNED_EXHAUSTED)
        );
    }

    #[test]
    fn a_reply_yields_its_text_reasoning_and_tool_calls() {
        let value = json!({
            "choices": [{ "message": {
                "content": "",
                "reasoning_content": "play the poro",
                "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "move", "arguments": "{\"card\":7,\"zone\":\"base\"}" } }]
            }}],
            "usage": { "prompt_tokens": 12, "completion_tokens": 30 }
        });
        let reply = parse_reply(&value);
        assert_eq!(reply.reasoning, "play the poro");
        assert_eq!(reply.tool_calls[0].name, "move");
        assert!(reply.tool_calls[0].arguments.contains("base"));
        assert_eq!((reply.prompt_tokens, reply.completion_tokens), (12, 30));
        assert_eq!(reply.cached_tokens, 0);
        let echoed = assistant_message(&reply);
        assert_eq!(echoed["tool_calls"][0]["function"]["name"], "move");
        assert_eq!(
            tool_result(&reply.tool_calls[0], "ok")["tool_call_id"],
            "c1"
        );
    }

    #[test]
    fn the_cache_hit_count_is_read_from_either_usage_shape() {
        assert_eq!(
            cached_tokens(
                &json!({ "prompt_tokens": 7080, "prompt_cache_hit_tokens": 6912, "prompt_cache_miss_tokens": 168 })
            ),
            6912
        );
        assert_eq!(
            cached_tokens(
                &json!({ "prompt_tokens": 7080, "prompt_tokens_details": { "cached_tokens": 6400 } })
            ),
            6400
        );
        assert_eq!(cached_tokens(&json!({ "prompt_tokens": 12 })), 0);
        let value = json!({
            "choices": [{ "message": { "content": "hi" } }],
            "usage": { "prompt_tokens": 100, "completion_tokens": 3, "prompt_cache_hit_tokens": 64 }
        });
        assert_eq!(parse_reply(&value).cached_tokens, 64);
    }

    #[test]
    fn missing_credentials_fail_before_sending_a_request() {
        let mut client = Client::new(DEFAULT_MODEL);
        client.key.clear();
        assert_eq!(
            client.chat(&[], &[], false).unwrap_err(),
            client.missing_key()
        );
    }
}
