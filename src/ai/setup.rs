use super::driver::MindKind;
use super::provider::{self, Credentials, Model, Provider};
use parking_lot::Mutex;
use std::sync::Arc;

type ModelResult = Arc<Mutex<Option<Result<Vec<Model>, String>>>>;

#[derive(Debug, Clone, Default)]
pub struct Setup {
    pub credentials: Credentials,
    pub model: String,
    pub random: bool,
    pub search: String,
    pub models: Vec<Model>,
    pub error: String,
    pending: Option<ModelResult>,
}

impl Setup {
    pub fn open(lobby: &super::seat::AiLobby) -> Self {
        let mut setup = Self {
            credentials: lobby.credentials.clone(),
            model: lobby.model.clone(),
            random: !lobby.kind.is_llm(),
            ..Self::default()
        };
        if !setup.random {
            setup.refresh();
        }
        setup
    }

    pub fn choose_provider(&mut self, provider: Provider) {
        if self.credentials.provider == provider {
            return;
        }
        self.credentials = Credentials::from_env(provider);
        self.model.clear();
        self.search.clear();
        self.models.clear();
        self.refresh();
    }

    pub fn refresh(&mut self) {
        self.error.clear();
        let result = Arc::new(Mutex::new(None));
        self.pending = Some(result.clone());
        let provider = self.credentials.provider;
        #[cfg(not(target_arch = "wasm32"))]
        if std::thread::Builder::new()
            .name("ai-model-list".into())
            .spawn(move || {
                *result.lock() = Some(provider::fetch_models(provider));
            })
            .is_err()
        {
            self.pending = None;
            self.error = "Could not start model discovery. Try refreshing.".into();
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            *result.lock() = Some(provider::fetch_models(provider).await);
        });
    }

    pub fn poll(&mut self) {
        let Some(result) = self
            .pending
            .as_ref()
            .and_then(|pending| pending.lock().take())
        else {
            return;
        };
        self.pending = None;
        match result {
            Ok(models) => {
                if !models.iter().any(|model| model.id == self.model) {
                    self.model.clear();
                }
                self.models = models;
            }
            Err(error) => self.error = error,
        }
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.random {
            return Ok(());
        }
        self.credentials.validate(&self.model)?;
        if !self.models.iter().any(|model| model.id == self.model) {
            return Err("Load the model list and choose a tool-capable model".into());
        }
        Ok(())
    }

    pub fn apply(&self, lobby: &mut super::seat::AiLobby) -> Result<(), String> {
        self.validate()?;
        lobby.credentials = self.credentials.clone();
        lobby.model = self.model.clone();
        lobby.kind = if self.random {
            MindKind::Random
        } else {
            MindKind::Llm
        };
        lobby.configured = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_requires_credentials_and_a_discovered_model() {
        let mut setup = Setup::default();
        assert!(setup.validate().is_err());
        setup.credentials.key.0 = "synthetic-key".into();
        setup.model = "model".into();
        assert!(setup.validate().is_err());
        setup.models.push(Model {
            id: "model".into(),
            name: "Model".into(),
        });
        assert!(setup.validate().is_ok());
        let mut lobby = super::super::seat::AiLobby::default();
        setup.apply(&mut lobby).unwrap();
        assert!(lobby.configured);
        assert_eq!(lobby.credentials.key.0, "synthetic-key");
        assert!(!format!("{lobby:?}").contains("synthetic-key"));
        setup.random = true;
        setup.credentials.key.0.clear();
        setup.model.clear();
        assert!(setup.validate().is_ok());
    }

    #[test]
    fn late_results_from_an_old_provider_cannot_replace_the_new_list() {
        let old = Arc::new(Mutex::new(None));
        let current = Arc::new(Mutex::new(Some(Ok(vec![Model {
            id: "new".into(),
            name: "New".into(),
        }]))));
        let mut setup = Setup {
            pending: Some(current),
            ..Setup::default()
        };
        *old.lock() = Some(Ok::<_, String>(vec![Model {
            id: "old".into(),
            name: "Old".into(),
        }]));
        setup.poll();
        assert_eq!(setup.models[0].id, "new");
        assert!(!setup.loading());
    }
}
