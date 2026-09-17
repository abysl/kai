use super::auto::Stops;
use super::coach::Seen;
use super::dim;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
use std::path::PathBuf;

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tuning {
    pub hand_gap: f32,
    pub hand_droop: f32,
    pub hand_curve: f32,
    pub hand_y: f32,
    pub hand_z: f32,
    pub hover_rise: f32,
    #[serde(default = "default_hand_rise")]
    pub hand_rise: f32,
    pub lift: f32,
    pub ease_rate: f32,
    pub preview_scale: f32,
    pub foil_chance: f32,
    pub foil_alpha: f32,
    pub foil_strength: f32,
    pub foil_frequency: f32,
    pub foil_sparks: f32,
    pub foil_spark_density: f32,
    pub zoom: f32,
    pub pitch_deg: f32,
    pub pan_x: f32,
    pub pan_z: f32,
    #[serde(default = "default_camera_speed")]
    pub camera_speed: f32,
    #[serde(default)]
    pub view_version: u32,
    #[serde(default)]
    pub playmat: String,
    #[serde(default = "default_auto_pass")]
    pub auto_pass: bool,
    #[serde(default)]
    pub ask_anyway: bool,
    #[serde(default)]
    pub order_triggers: bool,
    #[serde(default)]
    pub assign_damage: bool,
    #[serde(default)]
    pub confirm_end_turn: bool,
    #[serde(default)]
    pub fast_anim: bool,
    #[serde(default)]
    pub hand_left: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub colour_blind: bool,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub stops: Stops,
    #[serde(default)]
    pub coach: Seen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Dark, Theme::Light];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
}

fn default_auto_pass() -> bool {
    true
}

fn default_ui_scale() -> f32 {
    1.0
}

fn default_camera_speed() -> f32 {
    0.0
}

fn default_hand_rise() -> f32 {
    0.5
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            hand_gap: 0.06,
            hand_droop: 0.02,
            hand_curve: 0.045,
            hand_y: 1.15,
            hand_z: 3.4,
            hover_rise: 0.12,
            hand_rise: default_hand_rise(),
            lift: 0.5,
            ease_rate: 14.0,
            preview_scale: 2.2,
            foil_chance: 0.2,
            foil_alpha: 0.75,
            foil_strength: 0.8,
            foil_frequency: 3.0,
            foil_sparks: 1.2,
            foil_spark_density: 16.0,
            zoom: 1.0,
            pitch_deg: dim::PITCH_ARENA,
            pan_x: 0.0,
            pan_z: 0.0,
            camera_speed: default_camera_speed(),
            view_version: dim::VIEW_VERSION,
            playmat: String::new(),
            auto_pass: default_auto_pass(),
            ask_anyway: false,
            order_triggers: false,
            assign_damage: false,
            confirm_end_turn: false,
            fast_anim: false,
            hand_left: false,
            ui_scale: default_ui_scale(),
            colour_blind: false,
            theme: Theme::System,
            stops: Stops::default(),
            coach: Seen::default(),
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn tuning_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("AGNI_TUNING") {
        return explicit.into();
    }
    let dir = crate::os::paths::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("tuning.json");
    let legacy = PathBuf::from("tuning.json");
    if !path.exists() && legacy.exists() {
        let _ = std::fs::copy(&legacy, &path);
    }
    path
}

impl Tuning {
    pub fn hand_spacing(&self) -> f32 {
        crate::table::dim::CARD_W + self.hand_gap
    }

    pub fn load() -> Self {
        Self::stored().unwrap_or_default().normalized()
    }

    pub fn normalized(mut self) -> Self {
        if super::playmat::retired_choice(&self.playmat) {
            self.playmat.clear();
        }
        if !(dim::ZOOM_MIN..=dim::ZOOM_MAX).contains(&self.zoom)
            || self.view_version < dim::VIEW_VERSION
        {
            let fresh = Tuning::default();
            self.zoom = fresh.zoom;
            self.pitch_deg = fresh.pitch_deg;
            self.camera_speed = fresh.camera_speed;
            self.pan_x = 0.0;
            self.pan_z = 0.0;
            self.hand_y = fresh.hand_y;
            self.hover_rise = fresh.hover_rise;
            self.hand_rise = fresh.hand_rise;
            self.view_version = dim::VIEW_VERSION;
        }
        self.pitch_deg = self.pitch_deg.clamp(dim::PITCH_MIN, dim::PITCH_TOP_DOWN);
        self.ui_scale = if self.ui_scale.is_finite() {
            self.ui_scale
                .clamp(crate::viewport::UI_SCALE_MIN, crate::viewport::UI_SCALE_MAX)
        } else {
            default_ui_scale()
        };
        self
    }

    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    fn stored() -> Option<Self> {
        None
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn stored() -> Option<Self> {
        std::fs::read_to_string(tuning_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save(&self) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let path = tuning_path();
            match serde_json::to_string_pretty(self) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&path, json) {
                        bevy::log::warn!("tuning save failed ({}): {e}", path.display());
                    }
                }
                Err(e) => bevy::log::warn!("tuning serialize failed: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_orbit_distance_is_read_as_the_old_camera_and_reset() {
        let legacy = Tuning {
            zoom: 8.49,
            pitch_deg: 45.0,
            camera_speed: 6.0,
            pan_x: 3.0,
            ..Tuning::default()
        }
        .normalized();
        assert_eq!(legacy.zoom, 1.0);
        assert_eq!(legacy.pitch_deg, dim::PITCH_ARENA);
        assert_eq!(legacy.camera_speed, 0.0);
        assert_eq!(legacy.pan_x, 0.0);
        assert_eq!(legacy.view_version, dim::VIEW_VERSION);
    }

    #[test]
    fn an_older_view_version_is_reframed_to_the_arena_camera() {
        let top_down = Tuning {
            pitch_deg: dim::PITCH_TOP_DOWN,
            hand_y: 0.7,
            view_version: 0,
            ..Tuning::default()
        }
        .normalized();
        assert_eq!(top_down.pitch_deg, dim::PITCH_ARENA);
        assert_eq!(top_down.hand_y, Tuning::default().hand_y);
        assert_eq!(top_down.view_version, dim::VIEW_VERSION);
    }

    #[test]
    fn a_fit_multiplier_and_a_tilt_survive_normalisation() {
        let tuned = Tuning {
            zoom: 1.3,
            pitch_deg: 60.0,
            ..Tuning::default()
        }
        .normalized();
        assert_eq!(tuned.zoom, 1.3);
        assert_eq!(tuned.pitch_deg, 60.0);
        let flat = Tuning {
            pitch_deg: 10.0,
            ..Tuning::default()
        }
        .normalized();
        assert_eq!(flat.pitch_deg, dim::PITCH_MIN);
    }

    #[test]
    fn the_play_preferences_default_on_an_old_file_and_round_trip_with_the_stops() {
        let old: Tuning = serde_json::from_str(r#"{"zoom": 1.0, "view_version": 3}"#).unwrap();
        assert!(
            old.auto_pass,
            "auto-pass is on until the player turns it off"
        );
        assert!(!old.ask_anyway && !old.order_triggers && !old.assign_damage);
        assert!(!old.confirm_end_turn && !old.fast_anim && !old.hand_left);
        assert_eq!(old.ui_scale, 1.0);
        assert!(!old.colour_blind);
        assert_eq!(old.theme, Theme::System);
        assert!(old.stops.0.is_empty());
        let mut tuned = Tuning {
            auto_pass: false,
            ui_scale: 1.3,
            theme: Theme::Dark,
            ..Tuning::default()
        };
        tuned.stops.toggle("beginning phase", true);
        let json = serde_json::to_string(&tuned).unwrap();
        let back: Tuning = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tuned);
        assert!(back.stops.set("beginning phase", true));
        assert!(!back.stops.set("beginning phase", false));
        let wild = Tuning {
            ui_scale: 9.0,
            ..Tuning::default()
        }
        .normalized();
        assert_eq!(wild.ui_scale, crate::viewport::UI_SCALE_MAX);
        let nan = Tuning {
            ui_scale: f32::NAN,
            ..Tuning::default()
        }
        .normalized();
        assert_eq!(nan.ui_scale, 1.0);
    }
}
