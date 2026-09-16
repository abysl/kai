use super::hud::{self, Hud};
use super::*;
use agni_sim::wire::{Affordance, AffordanceKind, PluginView, TurnInfo};

pub const DRAG_GUARD_SECS: f32 = 0.3;
pub const DRAWER_GUARD_SECS: f32 = 0.3;
pub const CHOOSE_LABEL: &str = "choose";
pub const CONFIRM_LABELS: [&str; 3] = ["done", "keep", "confirm"];
pub const CONFIRM_SECS: f32 = 3.0;
pub const CONFIRM_LABEL: &str = "press again";

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct TurnActivity {
    turn: Option<(u32, u8)>,
    acted: bool,
}

impl TurnActivity {
    pub fn observe_turn(&mut self, turn: Option<&TurnInfo>) {
        let key = turn.map(|info| (info.number, info.seat));
        if key != self.turn {
            self.turn = key;
            self.acted = false;
        }
    }

    pub fn note_action(&mut self, affordance: &Affordance) {
        let hotkey = affordance.hotkey.as_deref();
        if hotkey != Some(plugin_ui::PASS_KEY) && hotkey != Some(plugin_ui::ADVANCE_KEY) {
            self.acted = true;
        }
    }

    pub fn acted(&self) -> bool {
        self.acted
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Plain,
    HoldPhase,
    HoldHeld,
    Through,
}

pub fn click_of(ctrl: bool, shift: bool) -> Click {
    match (ctrl, shift) {
        (true, true) => Click::HoldHeld,
        (true, false) => Click::HoldPhase,
        (false, true) => Click::Through,
        (false, false) => Click::Plain,
    }
}

pub fn needs_confirm(
    view: &PluginView,
    primary: &Primary,
    confirm_end_turn: bool,
    no_actions_yet: bool,
) -> bool {
    plugin_ui::enforced(view)
        && primary.label == "end turn"
        && ((confirm_end_turn && primary.tone == Tone::Amber) || no_actions_yet)
}

pub fn confirm_live(armed: Option<f32>, now: f32) -> bool {
    armed.is_some_and(|at| now - at <= CONFIRM_SECS)
}

pub fn confirm_step(armed: Option<f32>, now: f32) -> (bool, Option<f32>) {
    if confirm_live(armed, now) {
        (true, None)
    } else {
        (false, Some(now))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Amber,
    Green,
    Grey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Primary {
    pub affordance: Option<usize>,
    pub label: String,
    pub tone: Tone,
}

impl Primary {
    pub fn enabled(&self) -> bool {
        self.affordance.is_some()
    }
}

fn index_of(view: &PluginView, matches: impl Fn(&Affordance) -> bool) -> Option<usize> {
    view.affordances
        .iter()
        .position(|affordance| affordance.enabled && matches(affordance))
}

pub fn pass_index(view: &PluginView) -> Option<usize> {
    index_of(view, |affordance| {
        affordance.hotkey.as_deref() == Some(plugin_ui::PASS_KEY)
    })
}

pub fn end_turn_index(view: &PluginView) -> Option<usize> {
    index_of(view, |affordance| {
        affordance.hotkey.as_deref() == Some(plugin_ui::ADVANCE_KEY)
    })
}

pub fn roll_index(view: &PluginView) -> Option<usize> {
    index_of(view, |affordance| {
        matches!(affordance.kind, AffordanceKind::Commit { .. })
    })
}

pub fn confirm_index(view: &PluginView) -> Option<usize> {
    let mine = view.prompt.is_some();
    mine.then(|| {
        index_of(view, |affordance| {
            affordance.card.is_none()
                && affordance.hotkey.as_deref() != Some(plugin_ui::ESCAPE_KEY)
                && CONFIRM_LABELS.contains(&affordance.label.as_str())
        })
    })
    .flatten()
}

pub fn plays_remain(view: &PluginView, primary: usize) -> bool {
    !plugin_ui::enforced(view)
        || view
            .legal
            .iter()
            .any(|row| highlight::actionable(&row.kinds))
        || view.shown().any(|(index, affordance)| {
            index != primary
                && affordance.enabled
                && !hud::is_free_table_offer(&affordance.label)
                && !matches!(affordance.kind, AffordanceKind::Reveal { .. })
        })
}

pub fn primary_of(view: &PluginView) -> Option<Primary> {
    if view.status.is_empty() && view.affordances.is_empty() {
        return None;
    }
    if let Some(index) = confirm_index(view) {
        let picked = view.prompt.as_ref().map_or(0, |summary| summary.picked);
        let tone = if plays_remain(view, index) && picked == 0 {
            Tone::Amber
        } else {
            Tone::Green
        };
        let seat = view.prompt.as_ref().map_or(0, |summary| summary.seat);
        return Some(Primary {
            affordance: Some(index),
            label: plugin_ui::primary_label(view, seat)
                .unwrap_or_else(|| view.affordances[index].label.clone()),
            tone,
        });
    }
    if let Some(index) = pass_index(view) {
        let label = if view.chain.is_empty() {
            "pass"
        } else {
            "resolve"
        };
        let tone = if plays_remain(view, index) {
            Tone::Amber
        } else {
            Tone::Green
        };
        return Some(Primary {
            affordance: Some(index),
            label: label.into(),
            tone,
        });
    }
    if let Some(index) = end_turn_index(view) {
        let tone = if plays_remain(view, index) {
            Tone::Amber
        } else {
            Tone::Green
        };
        return Some(Primary {
            affordance: Some(index),
            label: "end turn".into(),
            tone,
        });
    }
    if let Some(index) = roll_index(view) {
        return Some(Primary {
            affordance: Some(index),
            label: view.affordances[index].label.clone(),
            tone: Tone::Green,
        });
    }
    Some(Primary {
        affordance: None,
        label: "waiting".into(),
        tone: Tone::Grey,
    })
}

pub fn primary_for(view: &PluginView, me: u8) -> Option<Primary> {
    let primary = primary_of(view)?;
    let choosing = primary.affordance.is_none()
        && view
            .prompt
            .as_ref()
            .is_some_and(|summary| summary.seat == me);
    if choosing {
        return Some(Primary {
            affordance: None,
            label: CHOOSE_LABEL.into(),
            tone: Tone::Grey,
        });
    }
    Some(primary)
}

pub fn secondary_of(view: &PluginView, primary: &Primary) -> Option<Primary> {
    let end_turn = end_turn_index(view)?;
    if primary.affordance == Some(end_turn) {
        return None;
    }
    Some(Primary {
        affordance: Some(end_turn),
        label: "end turn".into(),
        tone: Tone::Amber,
    })
}

pub fn is_primary_key(view: &PluginView, primary: &Primary, key: KeyCode, confirm: bool) -> bool {
    let Some(index) = primary.affordance else {
        return false;
    };
    let plugin_fires = view.affordances[index]
        .hotkey
        .as_deref()
        .and_then(plugin_ui::key_of)
        .is_some_and(|own| own == key);
    matches!(key, KeyCode::Space | KeyCode::KeyW) && (!plugin_fires || confirm)
}

pub fn confirm_guards(
    view: &PluginView,
    index: usize,
    confirm_end_turn: bool,
    no_actions_yet: bool,
) -> bool {
    primary_of(view).is_some_and(|primary| {
        primary.affordance == Some(index)
            && needs_confirm(view, &primary, confirm_end_turn, no_actions_yet)
    })
}

pub fn shown_label(
    view: &PluginView,
    primary: &Primary,
    through: &auto::PassThrough,
    confirm_end_turn: bool,
    no_actions_yet: bool,
    confirm_armed: Option<f32>,
    now: f32,
) -> String {
    if let Some(passing) = auto::passing_label(through, primary) {
        return passing.to_string();
    }
    if needs_confirm(view, primary, confirm_end_turn, no_actions_yet)
        && confirm_live(confirm_armed, now)
    {
        return CONFIRM_LABEL.to_string();
    }
    primary.label.clone()
}

pub fn fill(tone: Tone) -> egui::Color32 {
    match tone {
        Tone::Amber => hud::AMBER,
        Tone::Green => hud::GREEN,
        Tone::Grey => hud::GREY,
    }
}

pub fn ink(tone: Tone) -> egui::Color32 {
    match tone {
        Tone::Amber => egui::Color32::from_rgb(28, 22, 8),
        Tone::Green => egui::Color32::from_rgb(244, 250, 244),
        Tone::Grey => hud::INK_WEAK,
    }
}

pub fn guarded(recent: &RecentDrag, now: f32) -> bool {
    recent.0.is_some_and(|(_, at)| now - at < DRAG_GUARD_SECS)
}

pub fn drawer_guarded(moved_at: Option<f32>, now: f32) -> bool {
    moved_at.is_some_and(|at| now - at < DRAWER_GUARD_SECS)
}

pub const KEY_HINT: &str = "space";
pub const LABEL_MIN_PT: f32 = 11.0;

pub fn label_pt(measure: impl Fn(f32) -> f32, room: f32, start: f32) -> f32 {
    let mut pt = start;
    while pt > LABEL_MIN_PT && measure(pt) > room {
        pt -= 1.0;
    }
    pt
}

fn button(
    ui: &mut egui::Ui,
    primary: &Primary,
    size: egui::Vec2,
    key: bool,
    pulse: f32,
    pending: Option<f32>,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let enabled = primary.enabled();
    let fill = if pending.is_some() {
        hud::AMBER.gamma_multiply(0.45)
    } else if enabled {
        fill(primary.tone)
    } else {
        hud::GREY
    };
    let stroke = if pulse > 0.0 {
        egui::Stroke::new(
            2.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, (40.0 + 120.0 * pulse) as u8),
        )
    } else if response.hovered() && enabled {
        egui::Stroke::new(1.5, hud::INK)
    } else {
        egui::Stroke::new(1.0, hud::HAIRLINE)
    };
    let painter = ui.painter();
    painter.rect(rect, size.y / 2.0, fill, stroke, egui::StrokeKind::Inside);
    if let Some(progress) = pending {
        let sweep =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * progress, rect.height()));
        painter.rect_filled(sweep, size.y / 2.0, hud::AMBER);
        painter.text(
            rect.center_bottom() - egui::vec2(0.0, 2.0),
            egui::Align2::CENTER_BOTTOM,
            auto::PENDING_HINT,
            egui::FontId::proportional(9.0),
            hud::INK.gamma_multiply(0.8),
        );
    }
    let ink = ink(primary.tone);
    let hint = (key && enabled).then(|| {
        painter.layout_no_wrap(
            KEY_HINT.to_string(),
            egui::FontId::proportional(10.0),
            ink.gamma_multiply(0.75),
        )
    });
    let room = size.x - 16.0 - hint.as_ref().map_or(0.0, |hint| hint.size().x + 8.0);
    let label_size = label_pt(
        |pt| {
            painter
                .layout_no_wrap(primary.label.clone(), egui::FontId::proportional(pt), ink)
                .size()
                .x
        },
        room,
        if size.y >= hud::PRIMARY_H { 16.0 } else { 13.0 },
    );
    let label = painter.layout_no_wrap(
        primary.label.clone(),
        egui::FontId::proportional(label_size),
        ink,
    );
    let total_w = label.size().x + hint.as_ref().map_or(0.0, |hint| hint.size().x + 8.0);
    let mut at = egui::pos2(
        rect.center().x - total_w / 2.0,
        rect.center().y - label.size().y / 2.0,
    );
    painter.galley(at, label.clone(), ink);
    if let Some(hint) = hint {
        at.x += label.size().x + 8.0;
        at.y = rect.center().y + label.size().y / 2.0 - hint.size().y;
        painter.galley(at, hint, ink);
    }
    enabled && response.clicked()
}

#[allow(clippy::too_many_arguments)]
pub fn primary_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<plugin_ui::PluginPanel>,
    recent_drag: Res<RecentDrag>,
    menu: Res<crate::menu::Menu>,
    tuning: Res<Tuning>,
    my_seat: Res<MySeat>,
    mut auto: ResMut<auto::Auto>,
    mut confirm_armed: Local<Option<f32>>,
    mut drawer_seen: Local<Option<(hud::DrawerState, f32)>>,
    mut sender: hud::Sender,
) -> Result {
    if !menu.at_table() {
        return Ok(());
    }
    let Some(primary) = primary_for(&panel.view, my_seat.0 .0) else {
        return Ok(());
    };
    let secondary = secondary_of(&panel.view, &primary);
    let context = contexts.ctx_mut()?.clone();
    let now = time.elapsed_secs();
    let phone = hud.0.class.is_phone();
    let moved_at = match *drawer_seen {
        Some((state, at)) if state == hud.0.drawer_state => Some(at),
        Some(_) => {
            *drawer_seen = Some((hud.0.drawer_state, now));
            Some(now)
        }
        None => {
            *drawer_seen = Some((hud.0.drawer_state, now));
            None
        }
    };
    let settled = !phone || !drawer_guarded(moved_at, now);
    let pulse = if highlight::acting(&panel.view) {
        plate::pulse(now)
    } else {
        0.0
    };
    let no_actions_yet = !sender.activity.acted();
    let confirm = needs_confirm(
        &panel.view,
        &primary,
        tuning.confirm_end_turn,
        no_actions_yet,
    );
    if !confirm && confirm_armed.is_some() {
        *confirm_armed = None;
    }
    let pending = auto
        .timer
        .progress(&panel.view, auto::AUTO_DELAY_MS, now)
        .filter(|_| primary.affordance.is_some());
    let shown = Primary {
        label: if pending.is_some() {
            auto::PENDING_LABEL.to_string()
        } else {
            shown_label(
                &panel.view,
                &primary,
                &auto.through,
                tuning.confirm_end_turn,
                no_actions_yet,
                *confirm_armed,
                now,
            )
        },
        ..primary.clone()
    };
    let mut fire = None;
    let mut click = Click::Plain;
    let rect = hud.0.primary;
    let pressed = hud::slot(&context, "primary button", rect, |ui| {
        let pressed = button(ui, &shown, rect.size(), !phone, pulse, pending);
        if pressed {
            let modifiers = ui.input(|input| input.modifiers);
            click = click_of(modifiers.command, modifiers.shift);
        }
        pressed
    })
    .inner;
    if pressed && settled && !guarded(&recent_drag, now) {
        match click {
            Click::HoldPhase => auto.hold_phase(&panel.view),
            Click::HoldHeld => auto.hold_held(),
            Click::Through => auto.toggle_through(&panel.view),
            Click::Plain if confirm => {
                let (go, armed) = confirm_step(*confirm_armed, now);
                *confirm_armed = armed;
                if go {
                    fire = primary.affordance;
                }
            }
            Click::Plain => fire = primary.affordance,
        }
    }
    if let Some(secondary) = &secondary {
        let rect = hud.0.secondary;
        let pressed = hud::slot(&context, "secondary button", rect, |ui| {
            button(ui, secondary, rect.size(), false, 0.0, None)
        })
        .inner;
        if pressed && settled && !guarded(&recent_drag, now) {
            fire = secondary.affordance;
        }
    }
    if fire.is_none() && !context.egui_wants_keyboard_input() && !auto::shift_held(&keys) {
        let pressed = keys
            .get_just_pressed()
            .copied()
            .find(|key| is_primary_key(&panel.view, &primary, *key, confirm));
        if pressed.is_some() {
            if confirm {
                let (go, armed) = confirm_step(*confirm_armed, now);
                *confirm_armed = armed;
                if go {
                    fire = primary.affordance;
                }
            } else {
                fire = primary.affordance;
            }
        }
    }
    if let Some(index) = fire {
        sender.fire(&panel.view.affordances[index]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Legal, LegalKind, PromptSummary};
    use serde_bytes::ByteBuf;

    fn offer(label: &str, hotkey: Option<&str>, card: Option<u32>) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![1]),
            card,
        }
    }

    fn enforced(affordances: Vec<Affordance>) -> PluginView {
        PluginView {
            status: vec!["turn 1 · {seat 0} · action phase · rules enforced".into()],
            affordances,
            ..Default::default()
        }
    }

    #[test]
    fn m1_the_pass_and_end_turn_views_name_the_primary_and_its_tone() {
        let pass = enforced(vec![
            offer("pass", Some("w"), None),
            offer("free table", None, None),
        ]);
        let primary = primary_of(&pass).unwrap();
        assert_eq!(primary.label, "pass");
        assert_eq!(primary.affordance, Some(0));
        assert_eq!(primary.tone, Tone::Green, "nothing else to do");
        assert!(secondary_of(&pass, &primary).is_none());

        let mut with_response = enforced(vec![offer("pass", Some("w"), None)]);
        with_response.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            ..Default::default()
        }];
        assert_eq!(primary_of(&with_response).unwrap().tone, Tone::Amber);

        let end_turn = enforced(vec![
            offer("end turn", Some("space"), None),
            offer("free table", None, None),
        ]);
        let primary = primary_of(&end_turn).unwrap();
        assert_eq!(primary.label, "end turn");
        assert_eq!(primary.tone, Tone::Green);
        let mut plays = end_turn.clone();
        plays.legal = vec![Legal {
            card: 70,
            kinds: vec![LegalKind::Play { accelerate: false }],
            ..Default::default()
        }];
        assert_eq!(primary_of(&plays).unwrap().tone, Tone::Amber);
        let mut activation = end_turn.clone();
        activation
            .affordances
            .push(offer("Lillia: play a Sprite", None, Some(3)));
        assert_eq!(primary_of(&activation).unwrap().tone, Tone::Amber);
        let free = PluginView {
            status: vec!["turn 1 · {seat 0} · action phase · free table".into()],
            affordances: vec![offer("end turn", Some("space"), None)],
            ..Default::default()
        };
        assert_eq!(
            primary_of(&free).unwrap().tone,
            Tone::Amber,
            "a free table cannot know whether plays remain, so it never claims green"
        );
    }

    #[test]
    fn m2_a_non_empty_chain_reads_resolve() {
        let mut view = enforced(vec![offer("pass", Some("w"), None)]);
        view.chain = vec![agni_sim::wire::ChainRow {
            item: 1,
            card: Some(40),
            seat: 1,
        }];
        let primary = primary_of(&view).unwrap();
        assert_eq!(primary.label, "resolve");
        assert_eq!(primary.tone, Tone::Green);
    }

    #[test]
    fn m3_a_prompt_with_a_confirm_option_reads_done_and_the_other_seat_is_disabled() {
        let mut prompt = enforced(vec![
            offer("{card 90}", None, Some(90)),
            offer("done", None, None),
            offer("free table", None, None),
        ]);
        prompt.prompt = Some(PromptSummary {
            seat: 0,
            why: "move others too?".into(),
            min: 0,
            max: 1,
            picked: 0,
            optional: false,
        });
        let primary = primary_of(&prompt).unwrap();
        assert_eq!(primary.label, "done");
        assert_eq!(primary.affordance, Some(1));
        assert_eq!(primary.tone, Tone::Amber, "a card is still pickable");
        let mut keep = enforced(vec![offer("keep", None, None)]);
        keep.prompt = prompt.prompt.clone();
        assert_eq!(primary_of(&keep).unwrap().label, "keep");
        assert_eq!(primary_of(&keep).unwrap().tone, Tone::Green);
        let mut cancel_only = enforced(vec![offer("cancel", Some("x"), None)]);
        cancel_only.prompt = prompt.prompt.clone();
        assert_eq!(
            primary_of(&cancel_only).unwrap().affordance,
            None,
            "cancel is never the primary"
        );
        let mine = primary_for(&cancel_only, 0).unwrap();
        assert_eq!(mine.label, CHOOSE_LABEL, "my own pick never reads waiting");
        assert_eq!(mine.tone, Tone::Grey);
        assert!(!mine.enabled());
        assert_eq!(primary_for(&cancel_only, 1).unwrap().label, "waiting");
        assert_eq!(
            label_pt(|pt| pt * 10.0, 120.0, 16.0),
            12.0,
            "a label wider than its button steps its size down to fit"
        );
        assert_eq!(label_pt(|pt| pt * 10.0, 200.0, 16.0), 16.0);
        assert_eq!(label_pt(|_| 1000.0, 120.0, 16.0), LABEL_MIN_PT);
        assert!(drawer_guarded(Some(1.0), 1.2));
        assert!(!drawer_guarded(Some(1.0), 1.4));
        assert!(!drawer_guarded(None, 1.0));

        let theirs = PluginView {
            status: vec![
                "turn 1 · {seat 1} · action phase · rules enforced".into(),
                "waiting for {seat 1}: their action phase".into(),
            ],
            affordances: vec![offer("free table", None, None)],
            ..Default::default()
        };
        let primary = primary_of(&theirs).unwrap();
        assert!(!primary.enabled());
        assert_eq!(primary.tone, Tone::Grey);
        assert_eq!(primary.label, "waiting");
        assert!(primary_of(&PluginView::default()).is_none());
    }

    #[test]
    fn the_opening_roll_is_the_primary_and_the_first_player_choice_stays_on_the_strip() {
        let roll = PluginView {
            status: vec![
                "roll for first player".into(),
                "mode: rules enforced".into(),
            ],
            affordances: vec![Affordance {
                kind: AffordanceKind::Commit { roll: 1 },
                ..offer("roll", None, None)
            }],
            ..Default::default()
        };
        let primary = primary_of(&roll).unwrap();
        assert_eq!(primary.label, "roll");
        assert_eq!(primary.affordance, Some(0));
        assert_eq!(primary.tone, Tone::Green);
        assert!(is_primary_key(&roll, &primary, KeyCode::Space, false));
        let won = PluginView {
            status: vec![
                "roll for first player".into(),
                "you won the roll — who goes first?".into(),
            ],
            affordances: vec![
                offer("go first", None, None),
                offer("let {seat 1} go first", None, None),
                offer("switch to free table", None, None),
            ],
            ..Default::default()
        };
        assert!(
            !primary_of(&won).unwrap().enabled(),
            "who goes first is a choice, not a primary"
        );
    }

    #[test]
    fn space_and_w_press_the_primary_only_when_the_plugin_key_does_not() {
        let pass = enforced(vec![offer("pass", Some("w"), None)]);
        let primary = primary_of(&pass).unwrap();
        assert!(is_primary_key(&pass, &primary, KeyCode::Space, false));
        assert!(
            !is_primary_key(&pass, &primary, KeyCode::KeyW, false),
            "the plugin fires w itself"
        );
        assert!(
            !confirm_guards(&pass, 0, true, false),
            "a pass never needs a confirm"
        );
        let end_turn = enforced(vec![offer("end turn", Some("space"), None)]);
        let primary = primary_of(&end_turn).unwrap();
        assert!(!is_primary_key(&end_turn, &primary, KeyCode::Space, false));
        assert!(is_primary_key(&end_turn, &primary, KeyCode::KeyW, false));
        assert!(!is_primary_key(&end_turn, &primary, KeyCode::Enter, false));
        assert!(
            !confirm_guards(&end_turn, 0, true, false),
            "green needs no confirm"
        );
        let plays = enforced(vec![
            offer("end turn", Some("space"), None),
            offer("play {card 1}", Some("p"), Some(1)),
        ]);
        let primary = primary_of(&plays).unwrap();
        assert!(
            confirm_guards(&plays, 0, true, false),
            "the plugin's space is held back while the confirm applies"
        );
        assert!(!confirm_guards(&plays, 1, true, false));
        assert!(!confirm_guards(&plays, 0, false, false));
        assert!(
            is_primary_key(&plays, &primary, KeyCode::Space, true),
            "and the primary takes space through the confirm instead"
        );
        assert!(!is_primary_key(&plays, &primary, KeyCode::Enter, true));
        let waiting = PluginView {
            status: vec!["waiting for {seat 1}".into()],
            ..Default::default()
        };
        let primary = primary_of(&waiting).unwrap();
        assert!(!is_primary_key(&waiting, &primary, KeyCode::Space, false));
    }

    #[test]
    fn the_drag_guard_swallows_a_click_within_300_ms_of_a_drop() {
        let recent = RecentDrag(Some((Entity::PLACEHOLDER, 10.0)));
        assert!(guarded(&recent, 10.1));
        assert!(!guarded(&recent, 10.4));
        assert!(!guarded(&RecentDrag(None), 0.0));
    }

    #[test]
    fn the_modifiers_pick_hold_and_pass_through_and_the_end_turn_confirm_is_a_second_press() {
        assert_eq!(click_of(false, false), Click::Plain);
        assert_eq!(click_of(true, false), Click::HoldPhase);
        assert_eq!(click_of(true, true), Click::HoldHeld);
        assert_eq!(click_of(false, true), Click::Through);
        let mut plays = enforced(vec![offer("end turn", Some("space"), None)]);
        plays.legal = vec![Legal {
            card: 70,
            kinds: vec![LegalKind::Play { accelerate: false }],
            ..Default::default()
        }];
        let primary = primary_of(&plays).unwrap();
        assert!(needs_confirm(&plays, &primary, true, false));
        assert!(
            !needs_confirm(&plays, &primary, false, false),
            "off by default"
        );
        let quiet = enforced(vec![offer("end turn", Some("space"), None)]);
        assert!(
            !needs_confirm(&quiet, &primary_of(&quiet).unwrap(), true, false),
            "green needs no confirm"
        );
        let free = PluginView {
            status: vec!["turn 1 · {seat 0} · action phase · free table".into()],
            affordances: vec![offer("end turn", Some("space"), None)],
            ..Default::default()
        };
        assert!(!needs_confirm(
            &free,
            &primary_of(&free).unwrap(),
            true,
            false
        ));
        let (go, armed) = confirm_step(None, 10.0);
        assert!(!go);
        assert_eq!(armed, Some(10.0));
        assert_eq!(
            shown_label(
                &plays,
                &primary,
                &auto::PassThrough::default(),
                true,
                false,
                armed,
                11.0
            ),
            CONFIRM_LABEL
        );
        assert_eq!(
            shown_label(
                &plays,
                &primary,
                &auto::PassThrough::default(),
                true,
                false,
                armed,
                14.0
            ),
            "end turn",
            "the arm lapses after three seconds"
        );
        assert_eq!(confirm_step(armed, 12.0), (true, None));
        assert_eq!(confirm_step(armed, 14.0), (false, Some(14.0)));
        let pass = enforced(vec![offer("pass", Some("w"), None)]);
        let primary = primary_of(&pass).unwrap();
        let armed_through = auto::PassThrough::default().toggled(&pass);
        assert_eq!(
            shown_label(&pass, &primary, &armed_through, false, false, None, 0.0),
            auto::PASSING_LABEL
        );
        assert_eq!(
            shown_label(
                &pass,
                &primary,
                &auto::PassThrough::default(),
                false,
                false,
                None,
                0.0
            ),
            "pass"
        );
    }

    #[test]
    fn the_no_actions_confirm_fires_only_when_the_seat_never_acted_this_turn() {
        let end_turn = enforced(vec![offer("end turn", Some("space"), None)]);
        let primary = primary_of(&end_turn).unwrap();
        assert_eq!(primary.tone, Tone::Green, "nothing else to do this turn");
        assert!(
            needs_confirm(&end_turn, &primary, false, true),
            "green still confirms when nothing was played, off by default or not"
        );
        assert!(
            !needs_confirm(&end_turn, &primary, false, false),
            "a normal turn where something happened never confirms"
        );
        let mut activity = TurnActivity::default();
        assert!(!activity.acted(), "a fresh turn starts with nothing done");
        activity.observe_turn(Some(&TurnInfo {
            number: 3,
            seat: 0,
            phase: "action".into(),
            phases: vec![],
            mode: "rules enforced".into(),
        }));
        assert!(!activity.acted());
        activity.note_action(&offer("end turn", Some("space"), None));
        assert!(
            !activity.acted(),
            "ending the turn is never itself an action"
        );
        activity.note_action(&offer("pass", Some("w"), None));
        assert!(!activity.acted(), "passing priority is never an action");
        activity.note_action(&offer("play {card 1}", Some("p"), Some(1)));
        assert!(activity.acted(), "playing a card is an action");
        activity.observe_turn(Some(&TurnInfo {
            number: 4,
            seat: 0,
            phase: "action".into(),
            phases: vec![],
            mode: "rules enforced".into(),
        }));
        assert!(!activity.acted(), "a new turn resets the flag");
        activity.note_action(&offer("play {card 1}", Some("p"), Some(1)));
        activity.observe_turn(Some(&TurnInfo {
            number: 4,
            seat: 0,
            phase: "combat".into(),
            phases: vec![],
            mode: "rules enforced".into(),
        }));
        assert!(
            activity.acted(),
            "the same turn moving to a new phase does not reset the flag"
        );
    }

    #[test]
    fn primary_ui_system_initializes_without_duplicate_turn_activity_access() {
        let mut world = World::new();
        let mut system = IntoSystem::<(), Result<(), BevyError>, _>::into_system(primary_ui);
        system.initialize(&mut world);
    }
}
