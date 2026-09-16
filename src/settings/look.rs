use super::{DeckParams, NetParams, PanelMetrics, TableParams};

use crate::table::{colors, dim, MySeat, Theme};
use crate::theme;
use bevy::prelude::*;
use bevy_egui::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraPreset {
    Arena,
    TopDown,
}

impl CameraPreset {
    pub fn pitch(self) -> f32 {
        match self {
            CameraPreset::Arena => dim::PITCH_ARENA,
            CameraPreset::TopDown => dim::PITCH_TOP_DOWN,
        }
    }

    pub fn of_pitch(pitch: f32) -> Option<Self> {
        [CameraPreset::Arena, CameraPreset::TopDown]
            .into_iter()
            .find(|preset| (preset.pitch() - pitch).abs() < 0.5)
    }
}

pub const THEME_TITLE: &str = "theme";
pub const COLOUR_BLIND: &str = "colour-blind palette";
pub const COLOUR_BLIND_NOTE: &str =
    "play rims turn blue, threat rings orange, and every seat swatch gains a shape";
pub const THEME_NOTE: &str = "the table stays dark on its felt; home, lobby and settings follow";

pub fn theme_options() -> Vec<(Theme, &'static str)> {
    Theme::ALL
        .iter()
        .map(|theme| (*theme, theme.label()))
        .collect()
}

pub fn foil_on(chance: f32) -> bool {
    chance > 0.0
}

pub fn foil_chance(on: bool) -> f32 {
    if on {
        crate::table::Tuning::default().foil_chance
    } else {
        0.0
    }
}

fn title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(text)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
}

pub fn look_tab(
    ui: &mut egui::Ui,
    metrics: &PanelMetrics,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
) {
    title(ui, "playmat");
    crate::table::playmat::playmat_section(
        ui,
        metrics,
        &mut table.tuning,
        &mut table.playmats,
        &table.thumbs,
        &decks.art,
        &decks.seated,
    );
    title(ui, "seat colour");
    colors::color_section(
        ui,
        &net.info,
        my_seat,
        &mut table.seat_colors,
        &mut table.picked,
    );
    title(ui, "camera");
    let tuning = &mut table.tuning;
    let live = tuning.bypass_change_detection();
    let before = live.clone();
    let mut preset = CameraPreset::of_pitch(live.pitch_deg);
    ui.horizontal_wrapped(|ui| {
        for (choice, label) in [
            (CameraPreset::Arena, "arena"),
            (CameraPreset::TopDown, "top-down"),
        ] {
            if crate::menu::chip(ui, label, preset == Some(choice)).clicked() {
                preset = Some(choice);
                live.pitch_deg = choice.pitch();
            }
        }
    });
    ui.add(egui::Slider::new(&mut live.zoom, dim::ZOOM_MIN..=dim::ZOOM_MAX).text("zoom"));
    ui.add(egui::Slider::new(&mut live.preview_scale, 1.0..=5.0).text("hover preview size"));
    title(ui, "cards");
    let mut foil = foil_on(live.foil_chance);
    if ui.checkbox(&mut foil, "foil cards").changed() {
        live.foil_chance = foil_chance(foil);
    }
    title(ui, THEME_TITLE);
    let options = theme_options();
    crate::menu::segmented(ui, &mut live.theme, &options);
    ui.label(
        egui::RichText::new(THEME_NOTE)
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
    ui.checkbox(&mut live.colour_blind, COLOUR_BLIND);
    ui.label(
        egui::RichText::new(COLOUR_BLIND_NOTE)
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
    if *live != before {
        tuning.set_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_presets_are_the_two_pitches_and_foil_toggles_the_chance() {
        assert_eq!(CameraPreset::of_pitch(62.0), Some(CameraPreset::Arena));
        assert_eq!(CameraPreset::of_pitch(90.0), Some(CameraPreset::TopDown));
        assert_eq!(CameraPreset::of_pitch(70.0), None);
        assert!(!foil_on(0.0));
        assert!(foil_on(foil_chance(true)));
        assert_eq!(foil_chance(false), 0.0);
    }

    #[test]
    fn the_theme_control_offers_system_dark_and_light_in_that_order() {
        let labels: Vec<&str> = theme_options()
            .into_iter()
            .map(|(_, label)| label)
            .collect();
        assert_eq!(labels, ["system", "dark", "light"]);
        assert_eq!(theme_options()[0].0, Theme::System);
    }
}
