use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Provider {
    OpenRouter,
    #[default]
    NanoGpt,
}

impl Provider {
    pub const ALL: [Self; 2] = [Self::OpenRouter, Self::NanoGpt];

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::NanoGpt => "NanoGPT",
        }
    }

    pub fn endpoint(self) -> &'static str {
        match self {
            Self::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            Self::NanoGpt => "https://nano-gpt.com/api/v1/chat/completions",
        }
    }

    pub fn models_url(self) -> &'static str {
        match self {
            Self::OpenRouter => "https://openrouter.ai/api/v1/models",
            Self::NanoGpt => "https://nano-gpt.com/api/v1/models?detailed=true",
        }
    }

    pub fn key_env(self) -> &'static str {
        match self {
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::NanoGpt => "NANOGPT_API_KEY",
        }
    }
}

#[derive(Clone, Default)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub provider: Provider,
    pub key: Secret,
}

impl Credentials {
    pub fn from_env(provider: Provider) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let key = std::env::var(provider.key_env()).unwrap_or_default();
        #[cfg(target_arch = "wasm32")]
        let key = String::new();
        Self {
            provider,
            key: Secret(key),
        }
    }

    pub fn validate(&self, model: &str) -> Result<(), String> {
        if self.key.0.trim().is_empty() {
            return Err(format!(
                "Enter your {} API key in AI settings",
                self.provider.label()
            ));
        }
        if self.key.0.trim().chars().any(char::is_control) {
            return Err("The API key contains an invalid control character".into());
        }
        if model.trim().is_empty() || model.chars().any(char::is_control) {
            return Err("Choose a model in AI settings".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub name: String,
}

pub fn parse_models(provider: Provider, value: &Value) -> Result<Vec<Model>, String> {
    let rows = value["data"]
        .as_array()
        .ok_or("The provider returned an invalid model list")?;
    let mut models: Vec<Model> = rows
        .iter()
        .filter_map(|row| {
            let supports_tools = match provider {
                Provider::OpenRouter => row["supported_parameters"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item == "tools")),
                Provider::NanoGpt => row["capabilities"]["tool_calling"].as_bool() == Some(true),
            };
            if !supports_tools {
                return None;
            }
            let id = row["id"].as_str()?.trim();
            if id.is_empty() || id.chars().any(char::is_control) {
                return None;
            }
            Some(Model {
                id: id.into(),
                name: row["name"].as_str().unwrap_or(id).into(),
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    if models.is_empty() {
        return Err("No tool-capable models were returned. Try refreshing the list.".into());
    }
    Ok(models)
}

pub fn http_error(provider: Provider, code: u16) -> String {
    let reason = match code {
        401 | 403 => "check your API key and model access",
        402 => "check your provider credit or subscription",
        429 => "rate limited; try again shortly",
        500..=599 => "provider unavailable; try again shortly",
        _ => "request rejected; check your model selection",
    };
    format!("{} {code}: {reason}", provider.label())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_models(provider: Provider) -> Result<Vec<Model>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let response = agent
        .get(provider.models_url())
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(code, _) => http_error(provider, code),
            _ => format!("{} model list could not be reached", provider.label()),
        })?;
    let value = response
        .into_json()
        .map_err(|_| "The provider returned an invalid model list")?;
    parse_models(provider, &value)
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_models(provider: Provider) -> Result<Vec<Model>, String> {
    let value =
        super::web_http::request(provider, provider.models_url(), None, None, 30_000).await?;
    parse_models(provider, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn models_are_filtered_sorted_and_deduplicated() {
        let models = parse_models(
            Provider::OpenRouter,
            &json!({"data": [
                {"id": "b", "supported_parameters": ["tools"]},
                {"id": "a", "name": "Alpha", "supported_parameters": ["tools"]},
                {"id": "b", "supported_parameters": ["tools"]},
                {"id": "chat-only", "supported_parameters": []}
            ]}),
        )
        .unwrap();
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(models[0].name, "Alpha");
        let nano = parse_models(
            Provider::NanoGpt,
            &json!({"data": [
                {"id": "yes", "capabilities": {"tool_calling": true}},
                {"id": "no", "capabilities": {"tool_calling": false}},
                {"id": "unknown"}
            ]}),
        )
        .unwrap();
        assert_eq!(nano.len(), 1);
        assert_eq!(nano[0].id, "yes");
        assert!(parse_models(Provider::NanoGpt, &json!({"error": "no"})).is_err());
        assert!(parse_models(Provider::OpenRouter, &json!({"data": []})).is_err());
    }

    #[test]
    fn keys_are_validated_and_never_debugged() {
        let mut credentials = Credentials::default();
        assert!(credentials.validate("model").is_err());
        credentials.key.0 = "test-secret-value".into();
        assert!(!format!("{credentials:?}").contains("test-secret-value"));
        assert!(credentials.validate("model").is_ok());
        assert!(credentials.validate("").is_err());
        credentials.key.0 = "bad\r\nheader".into();
        assert!(credentials.validate("model").is_err());
    }
}
