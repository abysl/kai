use crate::table::highlight::{rim_color_in, rim_shapes, rim_style, Palette, RimKind, Stroke};
use crate::table::hud::{self, Side, TOUCH_MIN};
use crate::table::{coach, Tuning};
use crate::theme;
use crate::viewport::{back_pressed, consume_back, BackKey, Viewport};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub const HELP_KEY: &str = "?";
pub const TITLE: &str = "help";
pub const RIM_SAMPLE: egui::Vec2 = egui::vec2(28.0, 38.0);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HelpSheet {
    pub open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verb {
    pub action: &'static str,
    pub mouse: &'static str,
    pub touch: &'static str,
    pub key: &'static str,
}

pub const VERBS: [Verb; 17] = [
    Verb {
        action: "inspect",
        mouse: "hover",
        touch: "tap",
        key: "Tab · Shift+Tab",
    },
    Verb {
        action: "pin the inspector · card sheet",
        mouse: "right-click",
        touch: "long-press",
        key: "I",
    },
    Verb {
        action: "select",
        mouse: "click",
        touch: "tap",
        key: "Tab",
    },
    Verb {
        action: "the selected card's default action",
        mouse: "double-click",
        touch: "second tap",
        key: "Enter",
    },
    Verb {
        action: "act with a chip",
        mouse: "click the chip",
        touch: "tap the chip",
        key: "1–9",
    },
    Verb {
        action: "play · move · hide · march",
        mouse: "drag to a lit zone",
        touch: "drag to a lit zone",
        key: "select + chip",
    },
    Verb {
        action: "play to the chain",
        mouse: "drag onto the chain, or double-click",
        touch: "drag onto the ribbon, or second tap",
        key: "P",
    },
    Verb {
        action: "answer a prompt with a card",
        mouse: "click the pulsing card",
        touch: "tap the pulsing card",
        key: "1–9",
    },
    Verb {
        action: "answer with a non-card option",
        mouse: "strip chip",
        touch: "banner chip",
        key: "1–9",
    },
    Verb {
        action: "the primary action",
        mouse: "click the button",
        touch: "tap the button",
        key: "Space",
    },
    Verb {
        action: "cancel · skip · decline",
        mouse: "the hollow chip, or Esc",
        touch: "the hollow chip, or back",
        key: "X · Esc",
    },
    Verb {
        action: "cancel a drag",
        mouse: "release on nothing",
        touch: "release on nothing",
        key: "Esc",
    },
    Verb {
        action: "deselect",
        mouse: "click the felt",
        touch: "tap the felt",
        key: "Esc",
    },
    Verb {
        action: "scroll the hand",
        mouse: "wheel over the fan",
        touch: "swipe the drawer",
        key: "← →",
    },
    Verb {
        action: "camera",
        mouse: "wheel zoom, middle-drag pan",
        touch: "pinch, two-finger pan, double-tap resets",
        key: "Home",
    },
    Verb {
        action: "hold this phase · hold",
        mouse: "Ctrl-click · Ctrl+Shift-click the primary",
        touch: "long-press the turn plate",
        key: "—",
    },
    Verb {
        action: "pass through",
        mouse: "Shift-click the primary",
        touch: "—",
        key: "Shift+Space",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub key: &'static str,
    pub effect: &'static str,
}

pub const HOTKEYS: [Hotkey; 17] = [
    Hotkey {
        key: "Shift+Backspace · Ctrl+Z",
        effect:
            "request undo: one action per press, sent after one second; other players must agree",
    },
    Hotkey {
        key: "Space (W)",
        effect: "the primary button",
    },
    Hotkey {
        key: "Shift+Space",
        effect: "pass through",
    },
    Hotkey {
        key: "Enter",
        effect: "the default action of the selected card",
    },
    Hotkey {
        key: "Esc",
        effect: "cancel or skip when offered; else back one level",
    },
    Hotkey {
        key: "X",
        effect: "the prompt's cancel · skip · no",
    },
    Hotkey {
        key: "1–9",
        effect: "the nth strip chip, else the nth chip of the selected card",
    },
    Hotkey {
        key: "Tab · Shift+Tab",
        effect: "cycle the selection through the lit cards",
    },
    Hotkey {
        key: "I",
        effect: "pin the inspector · open the card sheet",
    },
    Hotkey {
        key: "F · P · R",
        effect: "hide facedown · play from hidden · reveal, when the chip exists",
    },
    Hotkey {
        key: "L · C · K",
        effect: "log · chat · tokens (free table)",
    },
    Hotkey {
        key: "H · ?",
        effect: "this sheet",
    },
    Hotkey {
        key: "Home",
        effect: "reset the camera",
    },
    Hotkey {
        key: "hold Ctrl",
        effect: "full control: stop at every priority window while held",
    },
    Hotkey {
        key: "Ctrl-click · Ctrl+Shift-click",
        effect: "hold this phase · hold",
    },
    Hotkey {
        key: "D · T · E",
        effect: "free table only: draw · trash · exhaust the selected card",
    },
    Hotkey {
        key: "F12",
        effect: "screenshot",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub name: &'static str,
    pub shown_as: &'static str,
    pub icon: Option<&'static str>,
}

pub const STATUSES: [Status; 8] = [
    Status {
        name: "might",
        shown_as: "a pill: printed › current, green above, red below",
        icon: None,
    },
    Status {
        name: "damage",
        shown_as: "a red −N pill",
        icon: None,
    },
    Status {
        name: "stunned",
        shown_as: "a frost frame",
        icon: None,
    },
    Status {
        name: "exhausted",
        shown_as: "the tilt and a dim",
        icon: None,
    },
    Status {
        name: "attacker · defender",
        shown_as: "a sword in the attacker's colour · a shield",
        icon: Some("sword"),
    },
    Status {
        name: "equipped",
        shown_as: "a gear",
        icon: Some("gear"),
    },
    Status {
        name: "empowered",
        shown_as: "a gold frame",
        icon: None,
    },
    Status {
        name: "temporary",
        shown_as: "an hourglass",
        icon: Some("hourglass"),
    },
];

pub fn stroke_words(kind: RimKind) -> &'static str {
    let style = rim_style(kind);
    match (
        style.stroke,
        style.chevron,
        style.tag,
        style.pulses,
        style.ring,
    ) {
        (_, _, _, _, true) => "a ring outside the rim",
        (Stroke::Dotted, _, _, _, _) => "dotted",
        (Stroke::Dashed, _, _, _, _) => "dashed",
        (Stroke::Solid, true, _, _, _) => "solid with a chevron",
        (Stroke::Solid, _, true, _, _) => "solid with a corner tag",
        (Stroke::Solid, _, _, true, _) => "solid, pulsing",
        (Stroke::Solid, _, _, _, _) => "solid",
    }
}

pub fn rim_meaning(kind: RimKind) -> &'static str {
    match kind {
        RimKind::Play => "you may play this",
        RimKind::March => "you may attack with this",
        RimKind::Activate => "you may use this ability",
        RimKind::React => "you may respond with this now",
        RimKind::Answer => "the game is asking about this",
        RimKind::Hide => "you may hide this at a battlefield",
        RimKind::Enemy => "an enemy item aims at this",
    }
}

pub fn legend() -> Vec<(RimKind, &'static str, &'static str, &'static str)> {
    RimKind::ALL
        .iter()
        .map(|kind| {
            (
                *kind,
                kind.legend(),
                rim_meaning(*kind),
                stroke_words(*kind),
            )
        })
        .collect()
}

pub const HELP_LETTER: &str = "h";

pub fn is_help_key(key: &Key) -> bool {
    matches!(key, Key::Character(text) if text.as_str() == HELP_KEY || text.eq_ignore_ascii_case(HELP_LETTER))
}

pub fn help_keys(
    mut typed: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut back: ResMut<BackKey>,
    mut contexts: EguiContexts,
    mut help: ResMut<HelpSheet>,
    mut table_menu: ResMut<hud::TableMenu>,
) {
    let typing = contexts
        .ctx_mut()
        .is_ok_and(|context| context.egui_wants_keyboard_input());
    let asked = typed
        .read()
        .any(|input| input.state.is_pressed() && !input.repeat && is_help_key(&input.logical_key));
    if asked && !typing {
        help.open = !help.open;
        if help.open {
            table_menu.open = false;
        }
        return;
    }
    if !help.open {
        return;
    }
    if !back_pressed(&keys, &back) || typing {
        return;
    }
    help.open = false;
    consume_back(&mut keys, &mut back);
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(text)
            .strong()
            .size(16.0)
            .color(theme::tokens(ui.ctx()).ink),
    );
    ui.add_space(4.0);
}

fn rim_sample(ui: &mut egui::Ui, kind: RimKind, palette: Palette) {
    let (rect, _) = ui.allocate_exact_size(RIM_SAMPLE + egui::vec2(8.0, 8.0), egui::Sense::hover());
    let card = egui::Rect::from_center_size(rect.center(), RIM_SAMPLE);
    ui.painter()
        .rect_filled(card, 3.0, theme::tokens(ui.ctx()).surface_2);
    let style = rim_style(kind);
    let outset = if style.ring { 4.0 } else { 2.0 };
    let outer: Vec<egui::Pos2> = [
        card.left_top(),
        card.right_top(),
        card.right_bottom(),
        card.left_bottom(),
    ]
    .into_iter()
    .map(|p| card.center() + (p - card.center()) * (1.0 + outset / RIM_SAMPLE.x))
    .collect();
    for shape in rim_shapes(outer, style, rim_color_in(kind, palette)) {
        ui.painter().add(shape);
    }
}

pub const ACTION_W: f32 = 132.0;
pub const KEY_W: f32 = 150.0;

fn two_columns(
    ui: &mut egui::Ui,
    left_w: f32,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    let total = ui.available_width();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(left_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(left_w);
                ui.set_max_width(left_w);
                left(ui);
            },
        );
        let rest = (total - left_w - ui.spacing().item_spacing.x).max(80.0);
        ui.allocate_ui_with_layout(
            egui::vec2(rest, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(rest);
                ui.set_max_width(rest);
                right(ui);
            },
        );
    });
}

pub fn help_body(ui: &mut egui::Ui, phone: bool, palette: Palette) {
    ui.label(
        egui::RichText::new("drag a card or press a button; if the table refuses it tells you why")
            .color(theme::tokens(ui.ctx()).ink_weak),
    );
    heading(ui, "the verbs");
    for verb in &VERBS {
        two_columns(
            ui,
            ACTION_W,
            |ui| {
                ui.label(
                    egui::RichText::new(verb.action)
                        .strong()
                        .color(theme::tokens(ui.ctx()).ink),
                );
                ui.label(
                    egui::RichText::new(verb.key)
                        .small()
                        .color(theme::tokens(ui.ctx()).ink_weak),
                );
            },
            |ui| {
                if !phone {
                    ui.label(
                        egui::RichText::new(format!("mouse · {}", verb.mouse))
                            .color(theme::tokens(ui.ctx()).ink_weak),
                    );
                }
                ui.label(
                    egui::RichText::new(format!("touch · {}", verb.touch))
                        .color(theme::tokens(ui.ctx()).ink_weak),
                );
            },
        );
        ui.add_space(4.0);
    }
    if !phone {
        heading(ui, "hotkeys");
        for hotkey in &HOTKEYS {
            two_columns(
                ui,
                KEY_W,
                |ui| {
                    ui.label(
                        egui::RichText::new(hotkey.key)
                            .strong()
                            .color(theme::tokens(ui.ctx()).ink),
                    );
                },
                |ui| {
                    ui.label(
                        egui::RichText::new(hotkey.effect).color(theme::tokens(ui.ctx()).ink_weak),
                    );
                },
            );
        }
    }
    heading(ui, "the rims");
    for (kind, name, meaning, stroke) in legend() {
        ui.horizontal(|ui| {
            rim_sample(ui, kind, palette);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .strong()
                        .color(theme::tokens(ui.ctx()).ink),
                );
                ui.label(
                    egui::RichText::new(format!("{meaning} · {stroke}"))
                        .color(theme::tokens(ui.ctx()).ink_weak),
                );
            });
        });
    }
    heading(ui, "status chips");
    for status in &STATUSES {
        ui.horizontal(|ui| {
            match status.icon {
                Some(icon) => {
                    theme::icon(ui, icon, 20.0, theme::tokens(ui.ctx()).ink);
                }
                None => {
                    ui.add_space(20.0 + ui.spacing().item_spacing.x);
                }
            }
            ui.label(
                egui::RichText::new(status.name)
                    .strong()
                    .color(theme::tokens(ui.ctx()).ink),
            );
            ui.label(egui::RichText::new(status.shown_as).color(theme::tokens(ui.ctx()).ink_weak));
        });
    }
    heading(ui, "coach marks");
    for mark in coach::Mark::ALL {
        ui.label(egui::RichText::new(mark.text()).color(theme::tokens(ui.ctx()).ink_weak));
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "hints show once per device; Settings › play › show hints again brings them back",
        )
        .color(theme::tokens(ui.ctx()).ink_weak)
        .small(),
    );
}

pub fn help_ui(
    mut contexts: EguiContexts,
    viewport: Res<Viewport>,
    tuning: Res<Tuning>,
    mut help: ResMut<HelpSheet>,
) -> Result {
    if !help.open {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let mut open = true;
    let phone = viewport.class.is_phone();
    let palette = Palette::of(&tuning);
    hud::sheet(
        &context,
        "help sheet",
        viewport.class,
        Side::Right,
        TITLE,
        &mut open,
        |ui| {
            ui.set_min_height(TOUCH_MIN);
            help_body(ui, phone, palette);
        },
    );
    if !open {
        help.open = false;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verb_table_and_hotkeys_follow_the_design() {
        let actions: Vec<&str> = VERBS.iter().map(|verb| verb.action).collect();
        assert_eq!(actions[0], "inspect");
        assert_eq!(actions[9], "the primary action");
        assert_eq!(VERBS[9].key, "Space");
        assert!(actions.contains(&"play to the chain"));
        assert!(actions.contains(&"pass through"));
        assert!(VERBS
            .iter()
            .all(|verb| !verb.mouse.is_empty() && !verb.touch.is_empty()));
        let keys: Vec<&str> = HOTKEYS.iter().map(|hotkey| hotkey.key).collect();
        assert!(keys.contains(&"H · ?"));
        assert!(keys.contains(&"L · C · K"));
        assert!(!keys.iter().any(|key| key.starts_with('B') || *key == "F3"));
    }

    #[test]
    fn the_legend_covers_every_rim_with_a_stroke_word() {
        let rows = legend();
        assert_eq!(rows.len(), RimKind::ALL.len());
        for (kind, name, meaning, stroke) in rows {
            assert_eq!(name, kind.legend());
            assert!(!meaning.is_empty());
            assert!(!stroke.is_empty());
        }
        assert_eq!(stroke_words(RimKind::Play), "solid");
        assert_eq!(stroke_words(RimKind::March), "solid with a chevron");
        assert_eq!(stroke_words(RimKind::Activate), "solid with a corner tag");
        assert_eq!(stroke_words(RimKind::React), "dashed");
        assert_eq!(stroke_words(RimKind::Answer), "solid, pulsing");
        assert_eq!(stroke_words(RimKind::Hide), "dotted");
        assert_eq!(stroke_words(RimKind::Enemy), "a ring outside the rim");
        for status in STATUSES {
            if let Some(icon) = status.icon {
                assert!(theme::glyph(icon).is_some(), "{icon} is in the glyph set");
            }
        }
    }

    #[test]
    fn the_help_key_is_the_question_mark_on_any_layout() {
        assert!(is_help_key(&Key::Character("?".into())));
        assert!(!is_help_key(&Key::Character("/".into())));
        assert!(!is_help_key(&Key::Escape));
    }
}
