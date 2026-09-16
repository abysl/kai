use super::*;
use agni_sim::wire::{AffordanceKind, PluginView, PromptSummary};
use bevy::input::touch::Touches;
use serde::{Deserialize, Serialize};

pub const AUTO_DELAY_MS: u32 = 1100;
pub const PENDING_LABEL: &str = "passing…";
pub const PENDING_HINT: &str = "ctrl holds";
pub const PASSING_LABEL: &str = "passing…";
pub const HOLD_CHIP: &str = "holding";
pub const HOLD_PHASE_CHIP: &str = "holding this phase";
pub const ORDER_TRIGGERS_WHY: &str = "order your triggers";
pub const ASSIGN_WHY: &str = "assign ";
pub const ASSIGN_WHY_TAIL: &str = " damage: who takes lethal next?";
pub const PHASES: [&str; 9] = [
    "setup",
    "awaken step",
    "beginning phase",
    "channel step",
    "draw step",
    "action phase",
    "ending step",
    "cleanup",
    "expiration step",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefs {
    pub auto_pass: bool,
    pub ask_anyway: bool,
    pub order_triggers: bool,
    pub assign_damage: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            auto_pass: true,
            ask_anyway: false,
            order_triggers: false,
            assign_damage: false,
        }
    }
}

impl From<&Tuning> for Prefs {
    fn from(tuning: &Tuning) -> Self {
        Self {
            auto_pass: tuning.auto_pass,
            ask_anyway: tuning.ask_anyway,
            order_triggers: tuning.order_triggers,
            assign_damage: tuning.assign_damage,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Stop {
    pub phase: String,
    pub mine: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stops(pub BTreeSet<Stop>);

impl Stops {
    pub fn set(&self, phase: &str, mine: bool) -> bool {
        self.0.contains(&Stop {
            phase: phase.to_string(),
            mine,
        })
    }

    pub fn toggle(&mut self, phase: &str, mine: bool) -> bool {
        let stop = Stop {
            phase: phase.to_string(),
            mine,
        };
        if self.0.remove(&stop) {
            false
        } else {
            self.0.insert(stop);
            true
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseKey {
    pub turn: u32,
    pub seat: u8,
    pub phase: String,
}

pub fn phase_of(view: &PluginView) -> Option<PhaseKey> {
    plate::turn_of(view).map(|turn| PhaseKey {
        turn: turn.number,
        seat: turn.seat,
        phase: turn.phase,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HoldFocus {
    #[default]
    Off,
    ThisPhase(PhaseKey),
    Held,
}

impl HoldFocus {
    pub fn holding(&self, view: &PluginView) -> bool {
        match self {
            HoldFocus::Off => false,
            HoldFocus::Held => true,
            HoldFocus::ThisPhase(key) => phase_of(view).as_ref() == Some(key),
        }
    }

    pub fn settled(self, view: &PluginView) -> Self {
        match (&self, phase_of(view)) {
            (HoldFocus::ThisPhase(key), Some(now)) if *key != now => HoldFocus::Off,
            _ => self,
        }
    }

    pub fn toggled_phase(self, view: &PluginView) -> Self {
        match (self, phase_of(view)) {
            (HoldFocus::ThisPhase(_), _) => HoldFocus::Off,
            (_, Some(key)) => HoldFocus::ThisPhase(key),
            (held, None) => held,
        }
    }

    pub fn toggled_held(self) -> Self {
        match self {
            HoldFocus::Held => HoldFocus::Off,
            _ => HoldFocus::Held,
        }
    }

    pub fn chip(&self) -> Option<&'static str> {
        match self {
            HoldFocus::Off => None,
            HoldFocus::ThisPhase(_) => Some(HOLD_PHASE_CHIP),
            HoldFocus::Held => Some(HOLD_CHIP),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub turn: Option<u32>,
    pub items: BTreeSet<u16>,
    pub prompt: bool,
}

impl Mark {
    pub fn of(view: &PluginView) -> Self {
        Self {
            turn: phase_of(view).map(|key| key.turn),
            items: view.chain.iter().map(|row| row.item).collect(),
            prompt: view.prompt.is_some(),
        }
    }

    pub fn something_new(&self, view: &PluginView) -> bool {
        let fresh = Mark::of(view);
        fresh.turn != self.turn
            || (fresh.prompt && !self.prompt)
            || fresh.items.iter().any(|item| !self.items.contains(item))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PassThrough(pub Option<Mark>);

impl PassThrough {
    pub fn armed(&self) -> bool {
        self.0.is_some()
    }

    pub fn toggled(self, view: &PluginView) -> Self {
        if self.armed() {
            return PassThrough(None);
        }
        PassThrough(primary::pass_index(view).map(|_| Mark::of(view)))
    }

    pub fn disarmed(self) -> Self {
        PassThrough(None)
    }

    pub fn settled(self, view: &PluginView) -> Self {
        match &self.0 {
            Some(mark) if mark.something_new(view) => PassThrough(None),
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    NoPass,
    Off,
    Response,
    Stop,
    Hold,
    Theirs,
    Asked,
    Choice,
    Escape,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Pass { index: usize, after_ms: u32 },
    Answer { index: usize, after_ms: u32 },
    Wait(Wait),
}

impl Decision {
    pub fn fires(&self) -> Option<(usize, u32)> {
        match self {
            Decision::Pass { index, after_ms } | Decision::Answer { index, after_ms } => {
                Some((*index, *after_ms))
            }
            Decision::Wait(_) => None,
        }
    }
}

fn moves_besides(view: &PluginView, primary: Option<usize>) -> bool {
    view.legal.iter().any(|row| !row.kinds.is_empty())
        || view.shown().any(|(index, affordance)| {
            Some(index) != primary
                && affordance.enabled
                && !hud::is_free_table_offer(&affordance.label)
                && !matches!(affordance.kind, AffordanceKind::Reveal { .. })
        })
}

pub fn has_move(view: &PluginView, primary: usize) -> bool {
    moves_besides(view, Some(primary))
}

pub fn manual_kind(summary: &PromptSummary, prefs: &Prefs) -> bool {
    let why = summary.why.as_str();
    (prefs.order_triggers && why.starts_with(ORDER_TRIGGERS_WHY))
        || (prefs.assign_damage && why.starts_with(ASSIGN_WHY) && why.ends_with(ASSIGN_WHY_TAIL))
}

pub fn remaining(summary: &PromptSummary) -> usize {
    usize::from(summary.max.saturating_sub(summary.picked))
}

fn open_shape(view: &PluginView, summary: &PromptSummary) -> Option<Wait> {
    if summary.optional || summary.min != summary.max || summary.max == 0 {
        return Some(Wait::Choice);
    }
    if plugin_ui::cancel_index(view).is_some() {
        return Some(Wait::Escape);
    }
    None
}

fn forced_index(view: &PluginView, summary: &PromptSummary) -> Option<usize> {
    let enabled = || view.shown().filter(|(_, affordance)| affordance.enabled);
    let cards: Vec<usize> = enabled()
        .filter(|(_, affordance)| affordance.card.is_some())
        .map(|(index, _)| index)
        .collect();
    let others = enabled().any(|(_, affordance)| {
        affordance.card.is_none()
            && !hud::is_free_table_offer(&affordance.label)
            && !matches!(affordance.kind, AffordanceKind::Reveal { .. })
    });
    let remaining = remaining(summary);
    match cards.first() {
        Some(first) if !others && remaining >= 1 && cards.len() == remaining => Some(*first),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    Nothing,
    Theirs,
    Pass(usize),
    EndTurn(usize),
    Forced(usize),
    Choice,
}

impl Offer {
    pub fn index(self) -> Option<usize> {
        match self {
            Offer::Pass(index) | Offer::EndTurn(index) | Offer::Forced(index) => Some(index),
            Offer::Nothing | Offer::Theirs | Offer::Choice => None,
        }
    }

    pub fn is_quiet(self) -> bool {
        self.index().is_some()
    }
}

fn lone_roll(view: &PluginView) -> Option<usize> {
    if view.legal.iter().any(|row| !row.kinds.is_empty()) {
        return None;
    }
    let mut roll = None;
    for (index, affordance) in view.shown() {
        if !affordance.enabled
            || hud::is_free_table_offer(&affordance.label)
            || matches!(affordance.kind, AffordanceKind::Reveal { .. })
        {
            continue;
        }
        if !matches!(affordance.kind, AffordanceKind::Commit { .. }) {
            return None;
        }
        roll.get_or_insert(index);
    }
    roll
}

fn beyond_pass(view: &PluginView) -> Offer {
    if let Some(index) = lone_roll(view) {
        return Offer::Forced(index);
    }
    if moves_besides(view, None) {
        Offer::Choice
    } else {
        Offer::Nothing
    }
}

fn behind_their_prompt(view: &PluginView) -> Offer {
    if let Some(index) = lone_roll(view) {
        return Offer::Forced(index);
    }
    if view.legal.iter().any(|row| !row.kinds.is_empty()) {
        Offer::Choice
    } else {
        Offer::Theirs
    }
}

pub fn is_commit(view: &PluginView, index: usize) -> bool {
    view.affordances
        .get(index)
        .is_some_and(|affordance| matches!(affordance.kind, AffordanceKind::Commit { .. }))
}

pub fn offer(view: &PluginView, me: u8) -> Offer {
    if let Some(summary) = &view.prompt {
        if summary.seat != me {
            return behind_their_prompt(view);
        }
        if let Some(index) = lone_roll(view) {
            return Offer::Forced(index);
        }
        if open_shape(view, summary).is_some() {
            return Offer::Choice;
        }
        return match forced_index(view, summary) {
            Some(index) => Offer::Forced(index),
            None => Offer::Choice,
        };
    }
    if let Some(index) = primary::pass_index(view) {
        return if has_move(view, index) {
            Offer::Choice
        } else {
            Offer::Pass(index)
        };
    }
    if let Some(index) = primary::end_turn_index(view) {
        return if has_move(view, index) {
            Offer::Choice
        } else {
            Offer::EndTurn(index)
        };
    }
    beyond_pass(view)
}

fn answer(view: &PluginView, summary: &PromptSummary, prefs: &Prefs, offered: Offer) -> Decision {
    if prefs.ask_anyway {
        return Decision::Wait(Wait::Asked);
    }
    if let Some(wait) = open_shape(view, summary) {
        return Decision::Wait(wait);
    }
    if manual_kind(summary, prefs) {
        return Decision::Wait(Wait::Manual);
    }
    match offered {
        Offer::Forced(index) if !is_commit(view, index) => Decision::Answer {
            index,
            after_ms: AUTO_DELAY_MS,
        },
        _ => Decision::Wait(Wait::Choice),
    }
}

pub fn decide(
    view: &PluginView,
    me: u8,
    prefs: &Prefs,
    stops: &Stops,
    hold: &HoldFocus,
    through: &PassThrough,
) -> Decision {
    if let Some(summary) = &view.prompt {
        if summary.seat != me {
            return Decision::Wait(Wait::Theirs);
        }
        return answer(view, summary, prefs, offer(view, me));
    }
    let Some(index) = primary::pass_index(view) else {
        return Decision::Wait(Wait::NoPass);
    };
    if through.armed() {
        return Decision::Pass {
            index,
            after_ms: AUTO_DELAY_MS,
        };
    }
    if !prefs.auto_pass {
        return Decision::Wait(Wait::Off);
    }
    if hold.holding(view) {
        return Decision::Wait(Wait::Hold);
    }
    if let Some(key) = phase_of(view) {
        if stops.set(&key.phase, key.seat == me) {
            return Decision::Wait(Wait::Stop);
        }
    }
    match offer(view, me) {
        Offer::Pass(index) => Decision::Pass {
            index,
            after_ms: AUTO_DELAY_MS,
        },
        _ => Decision::Wait(Wait::Response),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timer {
    armed: Option<(PluginView, f32)>,
    fired: Option<PluginView>,
}

impl Timer {
    pub fn tick(&mut self, view: &PluginView, after_ms: u32, now: f32) -> bool {
        if self.fired.as_ref() == Some(view) {
            return false;
        }
        match &self.armed {
            Some((armed, at)) if armed == view => {
                if now - at >= after_ms as f32 / 1000.0 {
                    self.fired = Some(view.clone());
                    self.armed = None;
                    true
                } else {
                    false
                }
            }
            _ => {
                self.armed = Some((view.clone(), now));
                false
            }
        }
    }

    pub fn reset(&mut self) {
        self.armed = None;
    }

    pub fn progress(&self, view: &PluginView, after_ms: u32, now: f32) -> Option<f32> {
        match &self.armed {
            Some((armed, at)) if armed == view && after_ms > 0 => {
                Some(((now - at) / (after_ms as f32 / 1000.0)).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }

    pub fn pending(&self) -> bool {
        self.armed.is_some()
    }
}

#[derive(Resource, Debug, Default)]
pub struct Auto {
    pub hold: HoldFocus,
    pub through: PassThrough,
    pub timer: Timer,
}

impl Auto {
    pub fn settle(&mut self, view: &PluginView) {
        let hold = self.hold.clone().settled(view);
        if hold != self.hold {
            self.hold = hold;
        }
        let through = self.through.clone().settled(view);
        if through != self.through {
            self.through = through;
        }
    }

    pub fn toggle_through(&mut self, view: &PluginView) {
        self.through = std::mem::take(&mut self.through).toggled(view);
    }

    pub fn disarm_through(&mut self) {
        if self.through.armed() {
            self.through = std::mem::take(&mut self.through).disarmed();
        }
    }

    pub fn hold_phase(&mut self, view: &PluginView) {
        self.hold = std::mem::take(&mut self.hold).toggled_phase(view);
    }

    pub fn hold_held(&mut self) {
        self.hold = std::mem::take(&mut self.hold).toggled_held();
    }
}

pub fn shift_held(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

pub fn control_held(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

pub fn effective_hold(persisted: &HoldFocus, control_down: bool) -> HoldFocus {
    if control_down {
        HoldFocus::Held
    } else {
        persisted.clone()
    }
}

pub fn passing_label(through: &PassThrough, primary: &primary::Primary) -> Option<&'static str> {
    (through.armed() && matches!(primary.label.as_str(), "pass" | "resolve"))
        .then_some(PASSING_LABEL)
}

#[allow(clippy::too_many_arguments)]
pub fn auto_pilot(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut contexts: EguiContexts,
    panel: Res<plugin_ui::PluginPanel>,
    tuning: Res<Tuning>,
    my_seat: Res<MySeat>,
    menu: Res<crate::menu::Menu>,
    mut auto: ResMut<Auto>,
    mut sender: hud::Sender,
) {
    let view = &panel.view;
    if !menu.at_table() {
        auto.disarm_through();
        auto.timer.reset();
        return;
    }
    auto.settle(view);
    let typing = contexts
        .ctx_mut()
        .is_ok_and(|context| context.egui_wants_keyboard_input());
    let any_key = keys.get_just_pressed().next().is_some();
    if !typing && shift_held(&keys) && keys.just_pressed(KeyCode::Space) {
        auto.toggle_through(view);
    } else if any_key || mouse.get_just_pressed().next().is_some() || touches.any_just_pressed() {
        auto.disarm_through();
    }
    let hold = effective_hold(&auto.hold, !typing && control_held(&keys));
    let decision = decide(
        view,
        my_seat.0 .0,
        &Prefs::from(&*tuning),
        &tuning.stops,
        &hold,
        &auto.through,
    );
    match decision.fires() {
        Some((index, after_ms)) => {
            if auto.timer.tick(view, after_ms, time.elapsed_secs()) {
                sender.fire(&view.affordances[index]);
            }
        }
        None => auto.timer.reset(),
    }
}

pub fn sync_ui_scale(tuning: Res<Tuning>, mut zoom: ResMut<crate::viewport::UiZoom>) {
    if zoom.ui_scale != tuning.ui_scale {
        zoom.ui_scale = tuning.ui_scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Affordance, ChainRow, Legal, LegalKind};

    #[test]
    fn a_pending_pass_reports_how_far_along_its_delay_is_and_nothing_once_it_fired() {
        let view = PluginView::default();
        let mut timer = Timer::default();
        assert_eq!(timer.progress(&view, 1000, 5.0), None);
        assert!(!timer.tick(&view, 1000, 5.0));
        assert_eq!(timer.progress(&view, 1000, 5.0), Some(0.0));
        assert_eq!(timer.progress(&view, 1000, 5.5), Some(0.5));
        assert!(timer.tick(&view, 1000, 6.0));
        assert_eq!(timer.progress(&view, 1000, 6.0), None);
    }

    #[test]
    fn holding_control_is_full_control_for_as_long_as_it_is_down_and_leaves_nothing_behind() {
        assert_eq!(effective_hold(&HoldFocus::Off, true), HoldFocus::Held);
        assert_eq!(effective_hold(&HoldFocus::Off, false), HoldFocus::Off);
        let phase = HoldFocus::ThisPhase(PhaseKey {
            turn: 3,
            seat: 1,
            phase: "action".into(),
        });
        assert_eq!(effective_hold(&phase, false), phase);
        assert_eq!(effective_hold(&phase, true), HoldFocus::Held);
    }
    use serde_bytes::ByteBuf;

    fn offer_row(label: &str, hotkey: Option<&str>, card: Option<u32>) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![1]),
            card,
        }
    }

    fn pass_view(turn_seat: u8, phase: &str) -> PluginView {
        PluginView {
            status: vec![
                format!("turn 4 · {{seat {turn_seat}}} · {phase} · rules enforced"),
                "points · {seat 0} 2 · {seat 1} 3".into(),
            ],
            affordances: vec![
                offer_row("pass", Some("w"), None),
                offer_row("free table", None, None),
            ],
            ..Default::default()
        }
    }

    fn with_react(mut view: PluginView) -> PluginView {
        view.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![13],
            ..Default::default()
        }];
        view
    }

    fn prompt(min: u8, max: u8, picked: u8, why: &str, options: Vec<Affordance>) -> PluginView {
        PluginView {
            status: vec!["turn 4 · {seat 0} · action phase · rules enforced".into()],
            affordances: options,
            prompt: Some(PromptSummary {
                seat: 0,
                why: why.into(),
                min,
                max,
                picked,
                optional: min == 0,
            }),
            ..Default::default()
        }
    }

    fn decide_default(view: &PluginView) -> Decision {
        decide(
            view,
            0,
            &Prefs::default(),
            &Stops::default(),
            &HoldFocus::Off,
            &PassThrough::default(),
        )
    }

    #[test]
    fn a_pass_with_no_response_row_is_sent_after_the_delay_and_a_response_holds() {
        let view = pass_view(1, "action phase");
        assert_eq!(
            decide_default(&view),
            Decision::Pass {
                index: 0,
                after_ms: AUTO_DELAY_MS
            }
        );
        assert_eq!(
            decide_default(&with_react(view.clone())),
            Decision::Wait(Wait::Response)
        );
        let mut activation = view.clone();
        activation
            .affordances
            .push(offer_row("Lillia: play a Sprite", None, Some(3)));
        assert_eq!(
            decide_default(&activation),
            Decision::Wait(Wait::Response),
            "an activation offer is a move too"
        );
        let mut march = view.clone();
        march.legal = vec![Legal {
            card: 9,
            kinds: vec![LegalKind::March],
            zones: vec![13],
            ..Default::default()
        }];
        assert_eq!(decide_default(&march), Decision::Wait(Wait::Response));
        let mut resolve = view.clone();
        resolve.chain = vec![ChainRow {
            item: 1,
            card: Some(40),
            seat: 1,
        }];
        assert!(matches!(decide_default(&resolve), Decision::Pass { .. }));
        let end_turn = PluginView {
            status: view.status.clone(),
            affordances: vec![offer_row("end turn", Some("space"), None)],
            ..Default::default()
        };
        assert_eq!(
            decide_default(&end_turn),
            Decision::Wait(Wait::NoPass),
            "end turn is never pressed for the player"
        );
        let off = Prefs {
            auto_pass: false,
            ..Prefs::default()
        };
        assert_eq!(
            decide(
                &view,
                0,
                &off,
                &Stops::default(),
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Off)
        );
    }

    #[test]
    fn a_stop_on_this_phase_holds_and_a_stop_on_the_other_turn_does_not() {
        let mut stops = Stops::default();
        assert!(stops.toggle("action phase", false));
        let theirs = pass_view(1, "action phase");
        assert_eq!(
            decide(
                &theirs,
                0,
                &Prefs::default(),
                &stops,
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Stop)
        );
        let mine = pass_view(0, "action phase");
        assert!(matches!(
            decide(
                &mine,
                0,
                &Prefs::default(),
                &stops,
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Pass { .. }
        ));
        let other_phase = pass_view(1, "beginning phase");
        assert!(matches!(
            decide(
                &other_phase,
                0,
                &Prefs::default(),
                &stops,
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Pass { .. }
        ));
        assert!(!stops.toggle("action phase", false));
        assert!(stops.0.is_empty());
    }

    #[test]
    fn held_never_passes_and_this_phase_clears_when_the_phase_changes() {
        let view = pass_view(1, "action phase");
        assert_eq!(
            decide(
                &view,
                0,
                &Prefs::default(),
                &Stops::default(),
                &HoldFocus::Held,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Hold)
        );
        let this_phase = HoldFocus::Off.toggled_phase(&view);
        assert_eq!(
            this_phase,
            HoldFocus::ThisPhase(PhaseKey {
                turn: 4,
                seat: 1,
                phase: "action phase".into()
            })
        );
        assert_eq!(this_phase.chip(), Some(HOLD_PHASE_CHIP));
        assert_eq!(HoldFocus::Held.chip(), Some(HOLD_CHIP));
        assert_eq!(HoldFocus::Off.chip(), None);
        assert_eq!(
            decide(
                &view,
                0,
                &Prefs::default(),
                &Stops::default(),
                &this_phase,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Hold)
        );
        assert_eq!(this_phase.clone().settled(&view), this_phase);
        let later = pass_view(1, "ending step");
        assert_eq!(this_phase.clone().settled(&later), HoldFocus::Off);
        assert_eq!(HoldFocus::Held.settled(&later), HoldFocus::Held);
        assert_eq!(this_phase.toggled_phase(&view), HoldFocus::Off);
        assert_eq!(HoldFocus::Off.toggled_held(), HoldFocus::Held);
        assert_eq!(HoldFocus::Held.toggled_held(), HoldFocus::Off);
        assert_eq!(
            HoldFocus::Held.toggled_phase(&PluginView::default()),
            HoldFocus::Held,
            "a view without a turn line cannot name a phase"
        );
    }

    #[test]
    fn pass_through_passes_with_a_response_after_the_same_delay_and_disarms_on_news() {
        let quiet = pass_view(1, "action phase");
        let armed = PassThrough::default().toggled(&quiet);
        assert!(armed.armed());
        let through = decide(
            &with_react(quiet.clone()),
            0,
            &Prefs::default(),
            &Stops::default(),
            &HoldFocus::Held,
            &armed,
        );
        let plain = decide_default(&quiet);
        assert_eq!(through.fires(), Some((0, AUTO_DELAY_MS)));
        assert_eq!(
            through.fires().map(|(_, after)| after),
            plain.fires().map(|(_, after)| after),
            "the delay is identical with and without a response row"
        );
        assert_eq!(armed.clone().settled(&quiet), armed);
        let between = PluginView {
            status: vec![
                quiet.status[0].clone(),
                "waiting for {seat 1}: respond or pass".into(),
            ],
            affordances: Vec::new(),
            ..quiet.clone()
        };
        assert_eq!(
            armed.clone().settled(&between),
            armed,
            "the other seat's window is not news"
        );
        assert_eq!(
            decide(
                &between,
                0,
                &Prefs::default(),
                &Stops::default(),
                &HoldFocus::Off,
                &armed
            ),
            Decision::Wait(Wait::NoPass)
        );
        let mut resolved = quiet.clone();
        resolved.chain = vec![ChainRow {
            item: 1,
            card: Some(40),
            seat: 1,
        }];
        let armed_on_chain = PassThrough::default().toggled(&resolved);
        assert_eq!(armed_on_chain.clone().settled(&quiet), armed_on_chain);
        let mut newer = resolved.clone();
        newer.chain.push(ChainRow {
            item: 2,
            card: Some(41),
            seat: 1,
        });
        assert!(!armed_on_chain.clone().settled(&newer).armed());
        let mut asked = quiet.clone();
        asked.prompt = Some(PromptSummary {
            seat: 0,
            why: "discard a card".into(),
            min: 1,
            max: 1,
            picked: 0,
            optional: false,
        });
        assert!(!armed.clone().settled(&asked).armed());
        let next_turn = PluginView {
            status: vec!["turn 5 · {seat 0} · action phase · rules enforced".into()],
            ..quiet.clone()
        };
        assert!(!armed.clone().settled(&next_turn).armed());
        assert!(!armed.clone().toggled(&quiet).armed());
        assert!(
            !PassThrough::default()
                .toggled(&PluginView {
                    affordances: vec![offer_row("end turn", Some("space"), None)],
                    ..quiet.clone()
                })
                .armed(),
            "pass-through arms only while a pass is offered"
        );
        let primary = primary::primary_of(&quiet).unwrap();
        assert_eq!(passing_label(&armed, &primary), Some(PASSING_LABEL));
        assert_eq!(passing_label(&PassThrough::default(), &primary), None);
    }

    #[test]
    fn a_forced_pick_is_answered_and_choices_escapes_and_manual_kinds_are_not() {
        let forced = prompt(
            2,
            2,
            0,
            "discard a card",
            vec![
                offer_row("{card 5}", None, Some(5)),
                offer_row("{card 6}", None, Some(6)),
                offer_row("free table", None, None),
            ],
        );
        assert_eq!(
            decide_default(&forced),
            Decision::Answer {
                index: 0,
                after_ms: AUTO_DELAY_MS
            }
        );
        let second = prompt(
            2,
            2,
            1,
            "discard a card",
            vec![offer_row("{card 6}", None, Some(6))],
        );
        assert_eq!(
            decide_default(&second),
            Decision::Answer {
                index: 0,
                after_ms: AUTO_DELAY_MS
            }
        );
        let one_of_two = prompt(
            1,
            1,
            0,
            "{card 9}: choose a unit",
            vec![
                offer_row("{card 5}", None, Some(5)),
                offer_row("{card 6}", None, Some(6)),
            ],
        );
        assert_eq!(decide_default(&one_of_two), Decision::Wait(Wait::Choice));
        let optional = prompt(
            0,
            2,
            0,
            "set aside up to 2 cards to redraw",
            vec![
                offer_row("set aside {card 5}", None, Some(5)),
                offer_row("keep", None, None),
            ],
        );
        assert_eq!(decide_default(&optional), Decision::Wait(Wait::Choice));
        let with_zone = prompt(
            1,
            1,
            0,
            "where does {card 9} enter?",
            vec![offer_row("{zone 3}", None, None)],
        );
        assert_eq!(decide_default(&with_zone), Decision::Wait(Wait::Choice));
        let escapable = prompt(
            1,
            1,
            0,
            "{card 9}: choose a unit",
            vec![
                offer_row("{card 5}", None, Some(5)),
                offer_row("cancel", Some("x"), None),
            ],
        );
        assert_eq!(decide_default(&escapable), Decision::Wait(Wait::Escape));
        let ask = Prefs {
            ask_anyway: true,
            ..Prefs::default()
        };
        assert_eq!(
            decide(
                &forced,
                0,
                &ask,
                &Stops::default(),
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Asked)
        );
        let triggers = prompt(
            1,
            1,
            0,
            "order your triggers (last placed resolves first)",
            vec![offer_row("{card 12} trigger", None, Some(12))],
        );
        assert!(matches!(decide_default(&triggers), Decision::Answer { .. }));
        let mine = Prefs {
            order_triggers: true,
            ..Prefs::default()
        };
        assert_eq!(
            decide(
                &triggers,
                0,
                &mine,
                &Stops::default(),
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Manual)
        );
        let assign = prompt(
            1,
            1,
            0,
            "assign 3 damage: who takes lethal next?",
            vec![offer_row("{card 20} (lethal 2)", None, Some(20))],
        );
        assert!(matches!(decide_default(&assign), Decision::Answer { .. }));
        let assign_mine = Prefs {
            assign_damage: true,
            ..Prefs::default()
        };
        assert_eq!(
            decide(
                &assign,
                0,
                &assign_mine,
                &Stops::default(),
                &HoldFocus::Off,
                &PassThrough::default()
            ),
            Decision::Wait(Wait::Manual)
        );
        let mut theirs = forced.clone();
        theirs.prompt.as_mut().unwrap().seat = 1;
        assert_eq!(decide_default(&theirs), Decision::Wait(Wait::Theirs));
        let armed = PassThrough(Some(Mark::of(&forced)));
        assert_eq!(
            decide(
                &escapable,
                0,
                &Prefs::default(),
                &Stops::default(),
                &HoldFocus::Off,
                &armed
            ),
            Decision::Wait(Wait::Escape),
            "pass-through never answers a question"
        );
    }

    #[test]
    fn the_offer_is_the_one_judgement_of_whether_the_seat_has_anything_beyond_a_pass() {
        let quiet = pass_view(1, "action phase");
        assert_eq!(offer(&quiet, 0), Offer::Pass(0));
        assert!(Offer::Pass(0).is_quiet());
        assert_eq!(offer(&with_react(quiet.clone()), 0), Offer::Choice);
        let mut activation = quiet.clone();
        activation
            .affordances
            .push(offer_row("Lillia: play a Sprite", None, Some(3)));
        assert_eq!(offer(&activation, 0), Offer::Choice);
        let end_turn = PluginView {
            status: quiet.status.clone(),
            affordances: vec![
                offer_row("end turn", Some("space"), None),
                offer_row("free table", None, None),
            ],
            ..Default::default()
        };
        assert_eq!(
            offer(&end_turn, 0),
            Offer::EndTurn(0),
            "end turn alone is nothing to do for a bot, though decide never presses it for a player"
        );
        assert_eq!(decide_default(&end_turn), Decision::Wait(Wait::NoPass));
        let mut plays = end_turn.clone();
        plays.legal = vec![Legal {
            card: 9,
            kinds: vec![LegalKind::Play { accelerate: false }],
            zones: vec![13],
            ..Default::default()
        }];
        assert_eq!(offer(&plays, 0), Offer::Choice);
        let waiting = PluginView {
            status: vec![
                quiet.status[0].clone(),
                "waiting for {seat 1}: respond or pass".into(),
            ],
            affordances: vec![offer_row("free table", None, None)],
            ..Default::default()
        };
        assert_eq!(offer(&waiting, 0), Offer::Nothing);
        assert!(!Offer::Nothing.is_quiet());
        let forced = prompt(
            2,
            2,
            0,
            "discard a card",
            vec![
                offer_row("{card 5}", None, Some(5)),
                offer_row("{card 6}", None, Some(6)),
            ],
        );
        assert_eq!(offer(&forced, 0), Offer::Forced(0));
        assert_eq!(remaining(forced.prompt.as_ref().unwrap()), 2);
        let mut theirs = forced.clone();
        theirs.prompt.as_mut().unwrap().seat = 1;
        assert_eq!(offer(&theirs, 0), Offer::Theirs);
        let one_of_two = prompt(
            1,
            1,
            0,
            "{card 9}: choose a unit",
            vec![
                offer_row("{card 5}", None, Some(5)),
                offer_row("{card 6}", None, Some(6)),
            ],
        );
        assert_eq!(offer(&one_of_two, 0), Offer::Choice);
        let escapable = prompt(
            1,
            1,
            0,
            "{card 9}: choose a unit",
            vec![
                offer_row("{card 5}", None, Some(5)),
                offer_row("cancel", Some("x"), None),
            ],
        );
        assert_eq!(offer(&escapable, 0), Offer::Choice);
        let optional = prompt(
            0,
            2,
            0,
            "set aside up to 2 cards to redraw",
            vec![
                offer_row("set aside {card 5}", None, Some(5)),
                offer_row("keep", None, None),
            ],
        );
        assert_eq!(offer(&optional, 0), Offer::Choice);
        let roll = |prompt_seat: Option<u8>| PluginView {
            status: quiet.status.clone(),
            affordances: vec![
                Affordance {
                    kind: AffordanceKind::Commit { roll: 7 },
                    ..offer_row("roll", None, None)
                },
                offer_row("free table", None, None),
            ],
            prompt: prompt_seat.map(|seat| PromptSummary {
                seat,
                why: "roll to shuffle 2 recycled cards".into(),
                min: 2,
                max: 2,
                picked: 2,
                optional: false,
            }),
            ..Default::default()
        };
        assert_eq!(
            offer(&roll(Some(1)), 0),
            Offer::Forced(0),
            "a lone roll behind another seat's prompt is still the seat's forced move"
        );
        assert_eq!(decide_default(&roll(Some(1))), Decision::Wait(Wait::Theirs));
        assert_eq!(
            offer(&roll(Some(0)), 0),
            Offer::Forced(0),
            "the roll on my own shuffle prompt is the same lone roll"
        );
        assert_eq!(
            decide_default(&roll(Some(0))),
            Decision::Wait(Wait::Choice),
            "a player still clicks their own roll"
        );
        assert_eq!(offer(&roll(None), 0), Offer::Forced(0));
        assert_eq!(decide_default(&roll(None)), Decision::Wait(Wait::NoPass));
        let mut roll_and_play = roll(None);
        roll_and_play.legal = plays.legal.clone();
        assert_eq!(offer(&roll_and_play, 0), Offer::Choice);
        assert_eq!(offer(&with_react(roll(Some(1))), 0), Offer::Choice);
        for view in [&quiet, &end_turn, &forced, &one_of_two, &optional] {
            let fires = decide_default(view).fires().map(|(index, _)| index);
            let quiet_index = match offer(view, 0) {
                Offer::Pass(index) | Offer::Forced(index) => Some(index),
                _ => None,
            };
            assert_eq!(
                fires, quiet_index,
                "the player's auto-pilot fires exactly on the pass and the forced pick the offer names"
            );
        }
    }

    #[test]
    fn the_timer_fires_once_per_view_after_the_delay_and_rearms_on_a_new_view() {
        let view = pass_view(1, "action phase");
        let delay = AUTO_DELAY_MS as f32 / 1000.0;
        let mut timer = Timer::default();
        assert!(!timer.tick(&view, AUTO_DELAY_MS, 10.0));
        assert!(timer.pending());
        assert!(!timer.tick(&view, AUTO_DELAY_MS, 10.0 + delay * 0.8));
        assert!(timer.tick(&view, AUTO_DELAY_MS, 10.0 + delay));
        assert!(
            !timer.tick(&view, AUTO_DELAY_MS, 12.0 + delay),
            "never twice"
        );
        assert!(!timer.pending());
        let later = pass_view(1, "ending step");
        assert!(!timer.tick(&later, AUTO_DELAY_MS, 12.0));
        assert!(
            !timer.tick(&view, AUTO_DELAY_MS, 12.3),
            "a changed view re-arms"
        );
        assert!(!timer.tick(&later, AUTO_DELAY_MS, 12.4));
        assert!(timer.tick(&later, AUTO_DELAY_MS, 12.4 + delay));
        timer.reset();
        assert!(!timer.pending());
    }

    #[test]
    fn the_phase_words_are_the_engines_and_the_prefs_read_from_tuning() {
        let labels: Vec<&str> = agni_riftbound_turns::state::Phase::ALL
            .iter()
            .map(|phase| phase.label())
            .collect();
        assert_eq!(labels, PHASES);
        let mut view = pass_view(0, "action phase");
        view.status[0] = "turn 7 · {seat 1} · draw step · rules enforced".into();
        assert_eq!(
            phase_of(&view),
            Some(PhaseKey {
                turn: 7,
                seat: 1,
                phase: "draw step".into()
            })
        );
        assert_eq!(phase_of(&PluginView::default()), None);
        let prefs = Prefs::from(&Tuning {
            auto_pass: false,
            ask_anyway: true,
            order_triggers: true,
            assign_damage: true,
            ..Tuning::default()
        });
        assert_eq!(
            prefs,
            Prefs {
                auto_pass: false,
                ask_anyway: true,
                order_triggers: true,
                assign_damage: true
            }
        );
        assert_eq!(Prefs::from(&Tuning::default()), Prefs::default());
    }
}
