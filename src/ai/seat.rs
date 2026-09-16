use crate::ai::driver::MindKind;
use crate::ai::local::{self, Config};
use crate::deck::import::ImportedDeck;
use std::path::{Path, PathBuf};

pub const DEFAULT_NAME: &str = "bot";

pub fn dir() -> PathBuf {
    #[cfg(target_arch = "wasm32")]
    return PathBuf::new();
    #[cfg(not(target_arch = "wasm32"))]
    crate::os::paths::config_dir().join("ai")
}

pub fn chat_path() -> PathBuf {
    dir().join(local::CHAT_FILE)
}

pub fn say(text: &str) {
    #[cfg(target_arch = "wasm32")]
    local::say(text);
    #[cfg(not(target_arch = "wasm32"))]
    say_at(&chat_path(), text);
}

pub fn say_at(path: &Path, text: &str) {
    append_chat(path, &format!("you: {}", text.trim()));
}

pub const FREE_BRAIN_DEAF: &str =
    "the free brain does not read chat — a model takes over when the AI is next added";

pub fn switch_model(model: &str) {
    #[cfg(target_arch = "wasm32")]
    local::switch_model(model);
    #[cfg(not(target_arch = "wasm32"))]
    switch_model_at(&chat_path(), model);
}

pub fn switch_live_model(status: &Status, model: &str) -> String {
    if status.kind.is_llm() {
        switch_model(model);
        format!("model switched to {model}")
    } else {
        format!("{model} takes over when the AI is next added")
    }
}

pub fn switch_model_at(path: &Path, model: &str) {
    append_chat(path, &format!("model: {}", model.trim()));
}

fn append_chat(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

pub fn chat_lines() -> Vec<String> {
    #[cfg(target_arch = "wasm32")]
    return local::chat_lines();
    #[cfg(not(target_arch = "wasm32"))]
    chat_lines_at(&chat_path())
}

pub fn chat_lines_at(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn clear_chat() {
    #[cfg(target_arch = "wasm32")]
    local::clear_chat();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::fs::remove_file(chat_path());
}

#[derive(bevy::prelude::Resource, Debug, Clone)]
pub struct AiLobby {
    pub kind: MindKind,
    pub model: String,
    pub status: String,
    pub draft: String,
    pub credentials: super::provider::Credentials,
    pub configured: bool,
    pub setup: super::setup::Setup,
}

impl Default for AiLobby {
    fn default() -> Self {
        Self {
            kind: MindKind::Llm,
            model: super::nanogpt::DEFAULT_MODEL.to_string(),
            status: String::new(),
            draft: String::new(),
            credentials: super::provider::Credentials::from_env(super::provider::Provider::NanoGpt),
            configured: false,
            setup: super::setup::Setup::default(),
        }
    }
}

pub const BRAIN_PRESETS: [(&str, MindKind, &str); 3] = [
    ("random", MindKind::Random, ""),
    ("fast", MindKind::Llm, super::nanogpt::DEFAULT_MODEL),
    ("thinking", MindKind::Llm, super::nanogpt::THINKING_MODEL),
];

impl AiLobby {
    pub fn preset_is(&self, kind: MindKind, model: &str) -> bool {
        self.kind == kind && (!kind.is_llm() || self.model == model)
    }

    pub fn pick_preset(&mut self, kind: MindKind, model: &str) -> bool {
        if self.preset_is(kind, model) {
            return false;
        }
        self.kind = kind;
        if kind.is_llm() {
            self.model = model.to_string();
        }
        true
    }

    pub fn brain_label(&self) -> String {
        if self.kind.is_llm() {
            self.model.clone()
        } else {
            self.kind.label().to_string()
        }
    }
}

pub type Status = local::Status;

pub fn status() -> Option<Status> {
    local::status()
}

pub fn stop() {
    local::stop();
}

pub fn start(
    deck: Option<(&ImportedDeck, &str)>,
    battlefield: usize,
    lobby: &AiLobby,
) -> Result<(), String> {
    let mut config = Config::new(dir(), lobby.kind, &lobby.model);
    config.credentials = lobby.credentials.clone();
    if let Some((deck, label)) = deck {
        config = config.with_deck(deck.clone(), label, battlefield.saturating_sub(1));
    }
    local::start(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::brain::Brain;
    use crate::ai::driver::{Chat, Out};

    #[test]
    fn the_chat_file_round_trips_between_the_table_and_the_seat() {
        let dir = std::env::temp_dir().join(format!(
            "kai-chat-{}-{}",
            std::process::id(),
            crate::os::entropy::secret()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        let path = dir.join(local::CHAT_FILE);
        assert!(chat_lines_at(&path).is_empty());
        say_at(&path, "  play something aggressive ");
        switch_model_at(&path, "z-ai/glm-5.3-flash");
        assert_eq!(
            chat_lines_at(&path),
            [
                "you: play something aggressive",
                "model: z-ai/glm-5.3-flash"
            ]
        );
        let mut brain = Brain::new(
            crate::ai::nanogpt::Client::new("deepseek/deepseek-v4-flash"),
            crate::ai::cards::CardTexts::default(),
            None,
        );
        let mut chat = Chat::new(Some(path.clone()));
        let mut out = Out::capture();
        assert!(chat.poll(&mut brain, &mut out), "a player line is fresh");
        assert_eq!(chat.messages, ["play something aggressive"]);
        assert_eq!(brain.model(), "z-ai/glm-5.3-flash");
        assert_eq!(out.take(), ["ai model → z-ai/glm-5.3-flash"]);
        assert!(
            !chat.poll(&mut brain, &mut out),
            "nothing new the second time"
        );
        chat.say("on it");
        assert_eq!(
            chat_lines_at(&path).last().map(String::as_str),
            Some("bot: on it")
        );
        assert!(
            !chat.poll(&mut brain, &mut out),
            "its own reply is not a message"
        );
        say_at(&path, "thanks");
        assert!(chat.poll(&mut brain, &mut out));
        assert_eq!(chat.messages, ["play something aggressive", "thanks"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_player_line_typed_while_the_bot_decides_is_not_skipped_by_its_reply() {
        let dir = std::env::temp_dir().join(format!(
            "kai-chat-race-{}-{}",
            std::process::id(),
            crate::os::entropy::secret()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        let path = dir.join(local::CHAT_FILE);
        let mut brain = Brain::new(
            crate::ai::nanogpt::Client::new("deepseek/deepseek-v4-flash"),
            crate::ai::cards::CardTexts::default(),
            None,
        );
        let mut chat = Chat::new(Some(path.clone()));
        let mut out = Out::capture();
        say_at(&path, "hello");
        assert!(chat.poll(&mut brain, &mut out));
        say_at(&path, "and attack the left battlefield");
        chat.say("hi there");
        assert!(
            chat.poll(&mut brain, &mut out),
            "the second player line is fresh"
        );
        assert_eq!(chat.messages, ["hello", "and attack the left battlefield"]);
        assert!(!chat.poll(&mut brain, &mut out));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_lobby_presets_switch_between_the_free_brain_and_the_models() {
        let mut lobby = AiLobby::default();
        assert_eq!(lobby.kind, MindKind::Llm);
        assert!(lobby.preset_is(MindKind::Llm, crate::ai::nanogpt::DEFAULT_MODEL));
        assert!(lobby.pick_preset(MindKind::Random, ""));
        assert_eq!(lobby.brain_label(), "random");
        assert_eq!(
            lobby.model,
            crate::ai::nanogpt::DEFAULT_MODEL,
            "the model is kept for when the player comes back to it"
        );
        assert!(!lobby.pick_preset(MindKind::Random, ""));
        assert!(lobby.pick_preset(MindKind::Llm, crate::ai::nanogpt::THINKING_MODEL));
        assert_eq!(lobby.brain_label(), crate::ai::nanogpt::THINKING_MODEL);
        assert_eq!(BRAIN_PRESETS.len(), 3);
    }
}
