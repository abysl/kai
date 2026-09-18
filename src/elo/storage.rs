use super::History;

pub(super) fn load() -> Result<History, String> {
    decode(platform::load()?)
}

fn decode(text: Option<String>) -> Result<History, String> {
    text.map_or_else(|| Ok(History::default()), |text| History::decode(&text))
}

pub(super) fn store(history: &History) -> Result<(), String> {
    let text = serde_json::to_string(history).map_err(|error| error.to_string())?;
    platform::store(&text)
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::path::Path;

    const FILE_NAME: &str = "personal-elo.json";

    pub fn load() -> Result<Option<String>, String> {
        load_in(&crate::os::paths::config_dir())
    }

    pub fn store(text: &str) -> Result<(), String> {
        store_in(&crate::os::paths::config_dir(), text)
    }

    pub(super) fn load_in(dir: &Path) -> Result<Option<String>, String> {
        match std::fs::read_to_string(dir.join(FILE_NAME)) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub(super) fn store_in(dir: &Path, text: &str) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let temporary = dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
        let result = std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, dir.join(FILE_NAME)));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result.map_err(|error| error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    const WEB_KEY: &str = "kai.personal-elo";

    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or("no window")?
            .local_storage()
            .map_err(|_| "localStorage unavailable")?
            .ok_or_else(|| "localStorage disabled".to_string())
    }

    pub fn load() -> Result<Option<String>, String> {
        storage()?
            .get_item(WEB_KEY)
            .map_err(|_| "Could not read personal Elo.".to_string())
    }

    pub fn store(text: &str) -> Result<(), String> {
        storage()?
            .set_item(WEB_KEY, text)
            .map_err(|_| "Could not save personal Elo.".to_string())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::elo::Outcome;

    #[test]
    fn restart_undo_and_correction_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("kai-elo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut original = decode(platform::load_in(&dir).unwrap()).unwrap();
        assert_eq!(original.rating(), 1200);
        original.record(1600, Outcome::Win).unwrap();
        original.record(800, Outcome::Loss).unwrap();
        platform::store_in(&dir, &serde_json::to_string(&original).unwrap()).unwrap();
        let mut restarted = decode(platform::load_in(&dir).unwrap()).unwrap();
        assert_eq!(restarted, original);
        restarted.undo().unwrap();
        assert_eq!(restarted.rating(), 1229);
        restarted.record(800, Outcome::Draw).unwrap();
        assert_eq!(restarted.rating(), 1215);
        platform::store_in(&dir, &serde_json::to_string(&restarted).unwrap()).unwrap();
        assert_eq!(decode(platform::load_in(&dir).unwrap()).unwrap(), restarted);
        platform::store_in(&dir, "damaged").unwrap();
        assert!(decode(platform::load_in(&dir).unwrap()).is_err());
        assert_eq!(platform::load_in(&dir).unwrap().as_deref(), Some("damaged"));
        let blocked = dir.join("file");
        std::fs::write(&blocked, "keep").unwrap();
        assert!(platform::store_in(&blocked, "fail").is_err());
        assert!(platform::load_in(&blocked).is_err());
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "keep");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
