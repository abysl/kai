use bevy::prelude::*;

pub const FILE_NAME: &str = "name.txt";
pub const WEB_KEY: &str = "kai.name";
pub const MAX_LEN: usize = 24;

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerName {
    pub name: String,
}

pub fn tidy(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_control() {
            continue;
        }
        if out.chars().count() >= MAX_LEN {
            break;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

pub fn load() -> PlayerName {
    PlayerName {
        name: platform::load().map(|name| tidy(&name)).unwrap_or_default(),
    }
}

pub fn store(name: &PlayerName) {
    if let Err(error) = platform::store(&name.name) {
        warn!("player name not saved: {error}");
    }
}

pub fn sync(name: Res<PlayerName>, mut last: Local<Option<String>>) {
    if last.as_ref() == Some(&name.name) {
        return;
    }
    *last = Some(name.name.clone());
    crate::net::set_player_name(&name.name);
    if !name.is_added() {
        store(&name);
    }
}

pub fn register(app: &mut App) {
    app.insert_resource(load()).add_systems(Update, sync);
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::path::PathBuf;

    fn path() -> PathBuf {
        crate::os::paths::config_dir().join(super::FILE_NAME)
    }

    pub fn load() -> Option<String> {
        load_at(&path())
    }

    pub fn store(name: &str) -> Result<(), String> {
        store_at(&path(), name)
    }

    pub fn load_at(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    pub fn store_at(path: &std::path::Path, name: &str) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        std::fs::write(path, name).map_err(|error| format!("{}: {error}", path.display()))
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or("no window")?
            .local_storage()
            .map_err(|_| "localStorage unavailable")?
            .ok_or_else(|| "localStorage disabled".to_string())
    }

    pub fn load() -> Option<String> {
        storage().ok()?.get_item(super::WEB_KEY).ok()?
    }

    pub fn store(name: &str) -> Result<(), String> {
        storage()?
            .set_item(super::WEB_KEY, name)
            .map_err(|_| "localStorage refused the name".to_string())
    }
}

pub fn name_field(ui: &mut bevy_egui::egui::Ui, name: &mut PlayerName, width: f32) -> bool {
    use bevy_egui::egui;
    let tokens = crate::theme::tokens(ui.ctx());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("your name").color(tokens.ink_weak));
        let mut draft = name.name.clone();
        let field = ui.add(
            egui::TextEdit::singleline(&mut draft)
                .hint_text(crate::net::default_player_name())
                .char_limit(MAX_LEN)
                .desired_width(width),
        );
        if field.changed() {
            name.name = tidy(&draft);
            changed = true;
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_stripped_of_control_characters_and_capped() {
        assert_eq!(tidy("  Rae  "), "Rae");
        assert_eq!(tidy("ra\ne\t"), "rae");
        assert_eq!(tidy(&"x".repeat(40)).chars().count(), MAX_LEN);
        assert_eq!(tidy("   "), "");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_name_round_trips_through_its_file() {
        let dir = std::env::temp_dir().join(format!("kai-name-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("kai").join(FILE_NAME);
        assert_eq!(platform::load_at(&path), None);
        platform::store_at(&path, "Rae").unwrap();
        assert_eq!(platform::load_at(&path).as_deref(), Some("Rae"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_name_falls_back_to_the_platform_default() {
        crate::net::set_player_name("  ");
        assert_eq!(crate::net::player_name(), crate::net::default_player_name());
        crate::net::set_player_name("Rae");
        assert_eq!(crate::net::player_name(), "Rae");
        crate::net::set_player_name("");
    }
}
