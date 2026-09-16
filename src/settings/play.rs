use super::TableParams;
use crate::table::auto::{self, Stops};

use crate::table::Tuning;
use crate::theme;
use crate::viewport::{UI_SCALE_MAX, UI_SCALE_MIN};
use bevy::prelude::DetectChangesMut;
use bevy_egui::egui;

pub const AUTO_PASS: &str = "auto-pass when I have no response";
pub const ASK_ANYWAY: &str = "ask me anyway for forced choices";
pub const ORDER_TRIGGERS: &str = "order my triggers myself";
pub const ASSIGN_DAMAGE: &str = "assign combat damage myself";
pub const CONFIRM_END_TURN: &str = "confirm end turn while I still have plays";
pub const FAST_ANIM: &str = "fast animations";
pub const HAND_LEFT: &str = "hand on the left";
pub const UI_SCALE: &str = "UI scale";
pub const STOPS_TITLE: &str = "stop and wait on";
pub const SHOW_HINTS: &str = "show hints again";

pub struct Toggle {
    pub label: &'static str,
    pub read: fn(&Tuning) -> bool,
    pub write: fn(&mut Tuning, bool),
}

pub const TOGGLES: [Toggle; 7] = [
    Toggle {
        label: AUTO_PASS,
        read: |tuning| tuning.auto_pass,
        write: |tuning, on| tuning.auto_pass = on,
    },
    Toggle {
        label: ASK_ANYWAY,
        read: |tuning| tuning.ask_anyway,
        write: |tuning, on| tuning.ask_anyway = on,
    },
    Toggle {
        label: ORDER_TRIGGERS,
        read: |tuning| tuning.order_triggers,
        write: |tuning, on| tuning.order_triggers = on,
    },
    Toggle {
        label: ASSIGN_DAMAGE,
        read: |tuning| tuning.assign_damage,
        write: |tuning, on| tuning.assign_damage = on,
    },
    Toggle {
        label: CONFIRM_END_TURN,
        read: |tuning| tuning.confirm_end_turn,
        write: |tuning, on| tuning.confirm_end_turn = on,
    },
    Toggle {
        label: FAST_ANIM,
        read: |tuning| tuning.fast_anim,
        write: |tuning, on| tuning.fast_anim = on,
    },
    Toggle {
        label: HAND_LEFT,
        read: |tuning| tuning.hand_left,
        write: |tuning, on| tuning.hand_left = on,
    },
];

pub fn stop_phases() -> Vec<&'static str> {
    auto::PHASES
        .iter()
        .copied()
        .filter(|phase| *phase != "setup")
        .collect()
}

pub fn stop_rows(stops: &Stops) -> Vec<(&'static str, bool, bool)> {
    stop_phases()
        .into_iter()
        .map(|phase| (phase, stops.set(phase, true), stops.set(phase, false)))
        .collect()
}

fn title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(text)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
}

pub fn play_tab(ui: &mut egui::Ui, table: &mut TableParams) {
    let tuning = &mut table.tuning;
    let live = tuning.bypass_change_detection();
    let before = live.clone();
    title(ui, "play");
    for toggle in &TOGGLES {
        let mut on = (toggle.read)(live);
        if ui.checkbox(&mut on, toggle.label).changed() {
            (toggle.write)(live, on);
        }
    }
    ui.add(egui::Slider::new(&mut live.ui_scale, UI_SCALE_MIN..=UI_SCALE_MAX).text(UI_SCALE));
    title(ui, STOPS_TITLE);
    ui.label(
        egui::RichText::new("auto-pass waits on a ticked phase; left is my turn, right is theirs")
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
    for (phase, mut mine, mut theirs) in stop_rows(&live.stops) {
        ui.horizontal(|ui| {
            if ui.checkbox(&mut mine, "").changed() {
                live.stops.toggle(phase, true);
            }
            if ui.checkbox(&mut theirs, "").changed() {
                live.stops.toggle(phase, false);
            }
            ui.label(egui::RichText::new(phase).color(theme::tokens(ui.ctx()).ink));
        });
    }
    ui.add_space(8.0);
    if ui.button(SHOW_HINTS).clicked() {
        live.coach.reset();
    }
    if *live != before {
        tuning.set_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_play_tab_carries_the_designs_entries_and_writes_them_to_tuning() {
        let labels: Vec<&str> = TOGGLES.iter().map(|toggle| toggle.label).collect();
        assert_eq!(
            labels,
            [
                AUTO_PASS,
                ASK_ANYWAY,
                ORDER_TRIGGERS,
                ASSIGN_DAMAGE,
                CONFIRM_END_TURN,
                FAST_ANIM,
                HAND_LEFT
            ]
        );
        let mut tuning = Tuning::default();
        let defaults: Vec<bool> = TOGGLES
            .iter()
            .map(|toggle| (toggle.read)(&tuning))
            .collect();
        assert_eq!(
            defaults,
            [true, false, false, false, false, false, false],
            "auto-pass is the only preference on by default"
        );
        for toggle in &TOGGLES {
            let was = (toggle.read)(&tuning);
            (toggle.write)(&mut tuning, !was);
            assert_eq!((toggle.read)(&tuning), !was, "{} round-trips", toggle.label);
        }
        assert!(!tuning.auto_pass && tuning.hand_left && tuning.fast_anim);
        assert!(tuning.confirm_end_turn && tuning.order_triggers && tuning.assign_damage);
        assert!(tuning.ask_anyway);
    }

    #[test]
    fn the_stop_list_names_every_phase_but_setup_in_the_engines_order() {
        let phases = stop_phases();
        assert_eq!(phases.len(), 8);
        assert_eq!(phases[0], "awaken step");
        assert_eq!(phases[4], "action phase");
        assert!(!phases.contains(&"setup"));
        let mut stops = Stops::default();
        stops.toggle("action phase", false);
        stops.toggle("beginning phase", true);
        let rows = stop_rows(&stops);
        assert_eq!(rows[4], ("action phase", false, true));
        assert_eq!(rows[1], ("beginning phase", true, false));
        assert!(rows
            .iter()
            .filter(|(phase, _, _)| !matches!(*phase, "action phase" | "beginning phase"))
            .all(|(_, mine, theirs)| !mine && !theirs));
    }
}
