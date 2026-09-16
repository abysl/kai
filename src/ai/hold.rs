use crate::table::auto::{self, Offer, Prefs};
use crate::table::{plate, plugin_ui, primary};
use agni_sim::wire::{ArrowKind, PluginView};
use serde_json::Value;
use std::collections::BTreeSet;

pub const LEAF_WORDS: [&str; 6] = [
    "my_turn",
    "playable_action",
    "opponent_played",
    "showdown",
    "card_named",
    "passes",
];

pub const ANY_OF: &str = "any_of";

pub const CONDITION_WORDS: [&str; 7] = [
    LEAF_WORDS[0],
    LEAF_WORDS[1],
    LEAF_WORDS[2],
    LEAF_WORDS[3],
    LEAF_WORDS[4],
    LEAF_WORDS[5],
    ANY_OF,
];

pub const BOT_PREFS: Prefs = Prefs {
    auto_pass: true,
    ask_anyway: false,
    order_triggers: true,
    assign_damage: true,
};

pub const HOLD_TOOL: &str = "Sleep through a stretch: the table makes the forced moves for you (passes, an empty turn's end turn, prompts with one legal answer) until the condition holds, then wakes you with a note of what happened. until: my_turn (your next turn begins), playable_action (you can play, react, march or activate), opponent_played (the opponent puts anything on the chain — a card or a trigger), showdown (a showdown or combat opens), card_named (card: the name of the card that must enter the board — your base or a battlefield), passes (count: how many forced passes go by), any_of (any_of: the conditions, any one of which ends the hold). A question only you can answer, a trigger order or damage assignment that matters, your own turn with plays open and a message from the player always wake you. This ends the decision.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    MyTurn,
    PlayableAction,
    OpponentPlayed,
    Showdown,
    CardNamed(String),
    Passes(u32),
    AnyOf(Vec<Condition>),
}

impl Condition {
    pub fn parse(arguments: &Value) -> Result<Self, String> {
        let listed = |value: &Value| -> Vec<String> {
            match value {
                Value::String(word) => vec![word.trim().to_ascii_lowercase()],
                Value::Array(items) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|word| word.trim().to_ascii_lowercase())
                    .collect(),
                _ => Vec::new(),
            }
        };
        let mut words = listed(&arguments["until"]);
        if words.is_empty() {
            return Err(format!(
                "hold needs until: one of {}",
                CONDITION_WORDS.join(", ")
            ));
        }
        if words == [ANY_OF] {
            words = listed(&arguments[ANY_OF]);
            if words.is_empty() {
                return Err("hold any_of needs the conditions listed in any_of".to_string());
            }
        }
        let mut parts = Vec::new();
        for word in &words {
            if word == ANY_OF {
                return Err("any_of lists the conditions themselves, it cannot nest".to_string());
            }
            let part = Self::word(word, arguments)?;
            if !parts.contains(&part) {
                parts.push(part);
            }
        }
        if parts.len() == 1 {
            return Ok(parts.remove(0));
        }
        Ok(Self::AnyOf(parts))
    }

    fn word(word: &str, arguments: &Value) -> Result<Self, String> {
        match word {
            "my_turn" => Ok(Self::MyTurn),
            "playable_action" => Ok(Self::PlayableAction),
            "opponent_played" => Ok(Self::OpponentPlayed),
            "showdown" => Ok(Self::Showdown),
            "card_named" => arguments["card"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| Self::CardNamed(name.to_string()))
                .ok_or_else(|| {
                    "card_named needs card: the name of the card to wait for".to_string()
                }),
            "passes" => arguments["count"]
                .as_u64()
                .filter(|count| *count >= 1)
                .and_then(|count| u32::try_from(count).ok())
                .map(Self::Passes)
                .ok_or_else(|| {
                    "passes needs count: how many forced passes to let through, at least 1"
                        .to_string()
                }),
            other => Err(format!(
                "unknown hold condition {other:?}: one of {}",
                CONDITION_WORDS.join(", ")
            )),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::MyTurn => "your next turn begins".to_string(),
            Self::PlayableAction => "you have a playable action".to_string(),
            Self::OpponentPlayed => "the opponent puts something on the chain".to_string(),
            Self::Showdown => "a showdown or combat opens".to_string(),
            Self::CardNamed(name) => format!("{name} enters the board"),
            Self::Passes(1) => "one pass has gone by".to_string(),
            Self::Passes(count) => format!("{count} passes have gone by"),
            Self::AnyOf(parts) => parts
                .iter()
                .map(Self::describe)
                .collect::<Vec<_>>()
                .join(" or "),
        }
    }
}

fn turn_key(view: &PluginView) -> Option<(u32, u8)> {
    plate::turn_of(view).map(|turn| (turn.number, turn.seat))
}

pub fn showdown_of(view: &PluginView) -> Option<String> {
    if let Some(line) = view
        .status
        .iter()
        .find(|line| line.starts_with("showdown at ") || line.starts_with("combat at "))
    {
        return Some(line.clone());
    }
    view.arrows
        .iter()
        .find(|arrow| matches!(arrow.kind, ArrowKind::Attack | ArrowKind::Combat))
        .map(|arrow| match arrow.to {
            agni_sim::wire::TargetRef::Zone(zone) => format!("an attack on {{zone {zone}}}"),
            _ => "a combat".to_string(),
        })
}

pub fn new_lines(seen: &[String], fresh: &[String]) -> Vec<String> {
    let most = seen.len().min(fresh.len());
    let overlap = (0..=most)
        .rev()
        .find(|&count| seen[seen.len() - count..] == fresh[..count])
        .unwrap_or(0);
    fresh[overlap..].to_vec()
}

#[derive(Debug, Clone)]
pub struct Hold {
    pub until: Condition,
    turn: Option<(u32, u8)>,
    items: BTreeSet<u16>,
    board: BTreeSet<u32>,
    narration: Vec<String>,
    in_fight: bool,
    fight_opened: Option<String>,
    opponent_played: bool,
    passes: u32,
    answered: u32,
    happened: Vec<String>,
}

impl Hold {
    pub fn new(until: Condition, view: &PluginView, board: &[(u32, String)]) -> Self {
        Self {
            until,
            turn: turn_key(view),
            items: view.chain.iter().map(|row| row.item).collect(),
            board: board.iter().map(|(id, _)| *id).collect(),
            narration: view.narration.clone(),
            in_fight: showdown_of(view).is_some(),
            fight_opened: None,
            opponent_played: false,
            passes: 0,
            answered: 0,
            happened: Vec::new(),
        }
    }

    pub fn observe(&mut self, view: &PluginView, me: u8) {
        for row in &view.chain {
            if !self.items.insert(row.item) || row.seat == me {
                continue;
            }
            self.opponent_played = true;
            self.happened.push(match row.card {
                Some(card) => format!("{{seat {}}} put {{card {card}}} on the chain", row.seat),
                None => format!("{{seat {}}} put a face-down card on the chain", row.seat),
            });
        }
        for line in new_lines(&self.narration, &view.narration) {
            self.happened.push(line);
        }
        self.narration = view.narration.clone();
        let fight = showdown_of(view);
        if let Some(line) = fight.as_ref().filter(|_| !self.in_fight) {
            self.fight_opened = Some(line.clone());
            self.happened.push(format!("a fight opened: {line}"));
        }
        self.in_fight = fight.is_some();
    }

    pub fn pressed(&mut self, label: &str, pressed: Pressed, why: Option<&str>) {
        match pressed {
            Pressed::Pass | Pressed::EndTurn => self.passes += 1,
            Pressed::Roll => {
                self.answered += 1;
                self.happened.push(format!("you sent your {label}"));
            }
            Pressed::Forced => {
                self.answered += 1;
                self.happened.push(format!(
                    "you answered \"{}\" with {label}",
                    why.unwrap_or_default()
                ));
            }
        }
    }

    pub fn met(&self, view: &PluginView, me: u8, board: &[(u32, String)]) -> Option<String> {
        self.check(&self.until, view, me, board)
    }

    fn check(
        &self,
        condition: &Condition,
        view: &PluginView,
        me: u8,
        board: &[(u32, String)],
    ) -> Option<String> {
        match condition {
            Condition::MyTurn => turn_key(view)
                .filter(|(number, seat)| *seat == me && Some((*number, *seat)) != self.turn)
                .map(|(number, _)| format!("your turn {number} has begun")),
            Condition::PlayableAction => (auto::offer(view, me) == Offer::Choice)
                .then(|| "you have something to play".to_string()),
            Condition::OpponentPlayed => self
                .opponent_played
                .then(|| "the opponent put something on the chain".to_string()),
            Condition::Showdown => self
                .fight_opened
                .as_ref()
                .map(|line| format!("a fight opened: {line}")),
            Condition::CardNamed(name) => board
                .iter()
                .find(|(id, held)| held.eq_ignore_ascii_case(name) && !self.board.contains(id))
                .map(|(id, held)| format!("{held} (#{id}) entered the board")),
            Condition::Passes(count) => {
                (self.passes >= *count).then(|| format!("{} forced passes went by", self.passes))
            }
            Condition::AnyOf(parts) => parts
                .iter()
                .find_map(|part| self.check(part, view, me, board)),
        }
    }

    pub fn report(&self, why: &str) -> Vec<String> {
        let mut lines = vec![format!(
            "You held until {}; the hold ended because {why}.",
            self.until.describe()
        )];
        lines.extend(self.happened.iter().cloned());
        lines.push(format!(
            "Meanwhile the table passed {} for you and answered {} forced prompt{}.",
            times(self.passes),
            self.answered,
            if self.answered == 1 { "" } else { "s" }
        ));
        lines
    }
}

fn times(count: u32) -> String {
    match count {
        0 => "never".to_string(),
        1 => "once".to_string(),
        2 => "twice".to_string(),
        count => format!("{count} times"),
    }
}

pub fn breaks(view: &PluginView, me: u8) -> Option<String> {
    match auto::offer(view, me) {
        Offer::Forced(_) => view
            .prompt
            .as_ref()
            .filter(|summary| {
                auto::manual_kind(summary, &BOT_PREFS) && auto::remaining(summary) > 1
            })
            .map(|_| "an ordering only you can choose".to_string()),
        Offer::Choice => match &view.prompt {
            Some(summary) if summary.seat == me => {
                Some(format!("a question only you can answer: {}", summary.why))
            }
            Some(_) => Some("you have something to play".to_string()),
            None if primary::pass_index(view).is_some() => None,
            None => Some("your turn has plays open".to_string()),
        },
        Offer::Nothing | Offer::Theirs | Offer::Pass(_) | Offer::EndTurn(_) => None,
    }
}

pub fn is_quiet(view: &PluginView, me: u8) -> bool {
    plugin_ui::enforced(view) && auto::offer(view, me).is_quiet() && breaks(view, me).is_none()
}

pub const FREE_TABLE: &str =
    "the table is free: nothing is forced, so there is nothing to hold through";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    Pass,
    EndTurn,
    Forced,
    Roll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Idle,
    Press {
        index: usize,
        label: String,
        pressed: Pressed,
    },
    Model {
        held: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub reason: String,
    pub again: bool,
}

#[derive(Debug, Default)]
pub struct Pilot {
    hold: Option<Hold>,
    refused: Option<PluginView>,
    passes: u32,
    answered: u32,
    pub quiet: u64,
    pub woken: u64,
}

impl Pilot {
    pub fn step(
        &mut self,
        view: &PluginView,
        me: u8,
        board: &[(u32, String)],
        fresh_chat: bool,
    ) -> Step {
        if let Some(hold) = self.hold.as_mut() {
            hold.observe(view, me);
        }
        if fresh_chat {
            return self.wake("the player wrote to you");
        }
        if !plugin_ui::enforced(view) {
            return self.free();
        }
        let offered = auto::offer(view, me);
        if matches!(offered, Offer::Nothing | Offer::Theirs) {
            return Step::Idle;
        }
        if let Some(why) = self
            .hold
            .as_ref()
            .and_then(|hold| hold.met(view, me, board))
        {
            return self.wake(&why);
        }
        match offered {
            Offer::Nothing | Offer::Theirs => Step::Idle,
            Offer::Pass(index) => self.press(view, index, Pressed::Pass),
            Offer::EndTurn(index) => self.press(view, index, Pressed::EndTurn),
            Offer::Forced(index) => match breaks(view, me) {
                Some(why) => self.wake(&why),
                None if auto::is_commit(view, index) => self.press(view, index, Pressed::Roll),
                None => self.press(view, index, Pressed::Forced),
            },
            Offer::Choice => match breaks(view, me) {
                Some(why) => self.wake(&why),
                None => match primary::pass_index(view).filter(|_| self.hold.is_some()) {
                    Some(index) => self.press(view, index, Pressed::Pass),
                    None => self.wake(""),
                },
            },
        }
    }

    fn press(&mut self, view: &PluginView, index: usize, pressed: Pressed) -> Step {
        let label = view
            .affordances
            .get(index)
            .map(|affordance| affordance.label.clone())
            .unwrap_or_default();
        let why = view.prompt.as_ref().map(|summary| summary.why.clone());
        if let Some(hold) = self.hold.as_mut() {
            hold.pressed(&label, pressed, why.as_deref());
        }
        match pressed {
            Pressed::Pass | Pressed::EndTurn => self.passes += 1,
            Pressed::Forced | Pressed::Roll => self.answered += 1,
        }
        self.quiet += 1;
        Step::Press {
            index,
            label,
            pressed,
        }
    }

    fn wake(&mut self, why: &str) -> Step {
        self.woken += 1;
        let held = match self.hold.take() {
            Some(hold) => hold.report(why),
            None => Vec::new(),
        };
        Step::Model { held }
    }

    fn free(&mut self) -> Step {
        match self.hold.take() {
            Some(hold) => {
                self.woken += 1;
                Step::Model {
                    held: hold.report("the table went free"),
                }
            }
            None => Step::Model { held: Vec::new() },
        }
    }

    pub fn hold(
        &mut self,
        until: Condition,
        view: &PluginView,
        me: u8,
        board: &[(u32, String)],
    ) -> Result<(), Dropped> {
        match self.judge(until, view, me, board) {
            Ok(hold) => {
                self.refused = None;
                self.hold = Some(hold);
                Ok(())
            }
            Err(reason) => {
                let again = self.refused.as_ref() == Some(view);
                self.refused = Some(view.clone());
                Err(Dropped { reason, again })
            }
        }
    }

    fn judge(
        &self,
        until: Condition,
        view: &PluginView,
        me: u8,
        board: &[(u32, String)],
    ) -> Result<Hold, String> {
        if !plugin_ui::enforced(view) {
            return Err(FREE_TABLE.to_string());
        }
        if let Some(why) = breaks(view, me) {
            return Err(format!("{why}; decide it before holding"));
        }
        let hold = Hold::new(until, view, board);
        if let Some(already) = hold.met(view, me, board) {
            return Err(format!("{}: {already} already", hold.until.describe()));
        }
        Ok(hold)
    }

    pub fn holding(&self) -> Option<&Condition> {
        self.hold.as_ref().map(|hold| &hold.until)
    }

    pub fn release(&mut self) {
        self.hold = None;
    }

    pub fn stretch(&mut self) -> Option<String> {
        let (passes, answered) = (
            std::mem::take(&mut self.passes),
            std::mem::take(&mut self.answered),
        );
        let prompts = format!(
            "{answered} forced prompt{}",
            if answered == 1 { "" } else { "s" }
        );
        match (passes, answered) {
            (0, 0) => None,
            (passes, 0) => Some(format!("passed {} with nothing to do", times(passes))),
            (0, _) => Some(format!("answered {prompts} with nothing to decide")),
            (passes, _) => Some(format!(
                "passed {} and answered {prompts} with nothing to decide",
                times(passes)
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Affordance, AffordanceKind, ChainRow, Legal, LegalKind, PromptSummary};
    use serde_bytes::ByteBuf;
    use serde_json::json;

    fn plain(label: &str, hotkey: Option<&str>, card: Option<u32>) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![1]),
            card,
        }
    }

    fn turn_line(number: u32, seat: u8) -> String {
        format!("turn {number} · {{seat {seat}}} · action phase · rules enforced")
    }

    fn passing(number: u32, seat: u8) -> PluginView {
        PluginView {
            status: vec![turn_line(number, seat)],
            affordances: vec![
                plain("pass", Some("w"), None),
                plain("free table", None, None),
            ],
            ..Default::default()
        }
    }

    fn waiting(number: u32, seat: u8) -> PluginView {
        PluginView {
            status: vec![
                turn_line(number, seat),
                "waiting for {seat 1}: respond or pass".into(),
            ],
            affordances: vec![plain("free table", None, None)],
            ..Default::default()
        }
    }

    fn my_turn(number: u32) -> PluginView {
        PluginView {
            status: vec![turn_line(number, 0)],
            affordances: vec![
                plain("end turn", Some("space"), None),
                plain("free table", None, None),
            ],
            ..Default::default()
        }
    }

    fn with_play(mut view: PluginView) -> PluginView {
        view.legal = vec![Legal {
            card: 9,
            kinds: vec![LegalKind::Play { accelerate: false }],
            zones: vec![13],
            ..Default::default()
        }];
        view
    }

    fn asked(why: &str, min: u8, max: u8, options: Vec<Affordance>) -> PluginView {
        PluginView {
            status: vec![turn_line(3, 1)],
            affordances: options,
            prompt: Some(PromptSummary {
                seat: 0,
                why: why.into(),
                min,
                max,
                picked: 0,
                optional: min == 0,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_hold_condition_parses_from_the_tool_arguments_and_refuses_what_it_cannot_read() {
        assert_eq!(
            Condition::parse(&json!({ "until": "my_turn" })),
            Ok(Condition::MyTurn)
        );
        assert_eq!(
            Condition::parse(&json!({ "until": " Opponent_Played " })),
            Ok(Condition::OpponentPlayed)
        );
        assert_eq!(
            Condition::parse(&json!({ "until": "card_named", "card": "Vi" })),
            Ok(Condition::CardNamed("Vi".into()))
        );
        assert_eq!(
            Condition::parse(&json!({ "until": "passes", "count": 3 })),
            Ok(Condition::Passes(3))
        );
        assert_eq!(
            Condition::parse(
                &json!({ "until": "any_of", "any_of": ["my_turn", "showdown", "my_turn"] })
            ),
            Ok(Condition::AnyOf(vec![
                Condition::MyTurn,
                Condition::Showdown
            ]))
        );
        assert_eq!(
            Condition::parse(&json!({ "until": ["playable_action", "passes"], "count": 2 })),
            Ok(Condition::AnyOf(vec![
                Condition::PlayableAction,
                Condition::Passes(2)
            ])),
            "a list in until is any_of without the word"
        );
        assert_eq!(
            Condition::parse(&json!({ "until": ["showdown"] })),
            Ok(Condition::Showdown),
            "a list of one is that one"
        );
        let refused = |arguments: Value| Condition::parse(&arguments).unwrap_err();
        assert!(refused(json!({})).starts_with("hold needs until"));
        assert!(refused(json!({ "until": "card_named" })).contains("card"));
        assert!(refused(json!({ "until": "passes", "count": 0 })).contains("at least 1"));
        assert!(refused(json!({ "until": "any_of" })).contains("any_of"));
        assert!(refused(json!({ "until": ["my_turn", "any_of"] })).contains("nest"));
        assert!(refused(json!({ "until": "sunrise" })).contains("sunrise"));
        assert_eq!(
            Condition::AnyOf(vec![Condition::MyTurn, Condition::Passes(1)]).describe(),
            "your next turn begins or one pass has gone by"
        );
        assert_eq!(
            Condition::OpponentPlayed.describe(),
            "the opponent puts something on the chain",
            "a trigger wakes it too, so the words do not say card"
        );
        assert!(HOLD_TOOL.contains("any_of") && HOLD_TOOL.contains("card_named"));
        for word in CONDITION_WORDS {
            assert!(HOLD_TOOL.contains(word), "the tool text names {word}");
        }
        assert!(!LEAF_WORDS.contains(&ANY_OF));
        assert_eq!(&CONDITION_WORDS[..6], &LEAF_WORDS[..]);
        assert_eq!(CONDITION_WORDS[6], ANY_OF);
    }

    #[test]
    fn new_narration_is_the_part_past_the_overlap_of_a_rolling_window() {
        let seen: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let fresh: Vec<String> = ["b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
        assert_eq!(new_lines(&seen, &fresh), ["d", "e"]);
        assert_eq!(new_lines(&seen, &seen), Vec::<String>::new());
        assert_eq!(new_lines(&[], &fresh), fresh);
        let apart: Vec<String> = ["x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(new_lines(&seen, &apart), apart);
    }

    #[test]
    fn a_pilot_passes_the_quiet_stretches_without_the_model_and_reports_them_once() {
        let mut pilot = Pilot::default();
        assert_eq!(pilot.step(&waiting(2, 1), 0, &[], false), Step::Idle);
        for _ in 0..3 {
            assert_eq!(
                pilot.step(&passing(2, 1), 0, &[], false),
                Step::Press {
                    index: 0,
                    label: "pass".into(),
                    pressed: Pressed::Pass
                }
            );
        }
        assert_eq!(
            pilot.step(&my_turn(3), 0, &[], false),
            Step::Press {
                index: 0,
                label: "end turn".into(),
                pressed: Pressed::EndTurn
            },
            "an empty turn of the bot's own is ended without the model"
        );
        let discard = asked(
            "discard a card",
            2,
            2,
            vec![
                plain("{card 5}", None, Some(5)),
                plain("{card 6}", None, Some(6)),
            ],
        );
        assert_eq!(
            pilot.step(&discard, 0, &[], false),
            Step::Press {
                index: 0,
                label: "{card 5}".into(),
                pressed: Pressed::Forced
            }
        );
        assert_eq!(pilot.quiet, 5);
        assert_eq!(
            pilot.step(&with_play(my_turn(3)), 0, &[], false),
            Step::Model { held: Vec::new() }
        );
        assert_eq!(
            pilot.stretch().as_deref(),
            Some("passed 4 times and answered 1 forced prompt with nothing to decide")
        );
        assert_eq!(pilot.stretch(), None, "one line per stretch");
        assert_eq!(pilot.woken, 1);
        let mut only_passes = Pilot::default();
        only_passes.step(&passing(2, 1), 0, &[], false);
        assert_eq!(
            only_passes.stretch().as_deref(),
            Some("passed once with nothing to do")
        );
        let choose = asked(
            "{card 9}: choose a unit",
            1,
            1,
            vec![
                plain("{card 5}", None, Some(5)),
                plain("{card 6}", None, Some(6)),
            ],
        );
        assert_eq!(
            only_passes.step(&choose, 0, &[], false),
            Step::Model { held: Vec::new() },
            "a real choice goes to the model"
        );
        assert_eq!(
            only_passes.step(&waiting(2, 1), 0, &[], true),
            Step::Model { held: Vec::new() },
            "a chat line wakes the model even with nothing on the table"
        );
    }

    #[test]
    fn a_trigger_order_or_damage_assignment_that_matters_goes_to_the_model_but_the_last_pick_does_not(
    ) {
        let mut pilot = Pilot::default();
        let order = asked(
            "order your triggers (last placed resolves first)",
            2,
            2,
            vec![
                plain("{card 12} trigger", None, Some(12)),
                plain("{card 13} trigger", None, Some(13)),
            ],
        );
        assert_eq!(
            pilot.step(&order, 0, &[], false),
            Step::Model { held: Vec::new() }
        );
        let mut last = order.clone();
        last.affordances.remove(0);
        last.prompt.as_mut().unwrap().picked = 1;
        assert!(matches!(
            pilot.step(&last, 0, &[], false),
            Step::Press {
                pressed: Pressed::Forced,
                ..
            }
        ));
        let assign = asked(
            "assign 3 damage: who takes lethal next?",
            2,
            2,
            vec![
                plain("{card 20} (lethal 2)", None, Some(20)),
                plain("{card 21} (lethal 1)", None, Some(21)),
            ],
        );
        assert_eq!(
            pilot.step(&assign, 0, &[], false),
            Step::Model { held: Vec::new() }
        );
    }

    #[test]
    fn a_hold_passes_through_responses_until_its_condition_and_wakes_with_what_happened() {
        let mut pilot = Pilot::default();
        let theirs = passing(2, 1);
        pilot.hold(Condition::MyTurn, &theirs, 0, &[]).unwrap();
        assert_eq!(pilot.holding(), Some(&Condition::MyTurn));
        let mut spell = theirs.clone();
        spell.chain = vec![ChainRow {
            item: 4,
            card: Some(40),
            seat: 1,
        }];
        spell.narration = vec!["{card 40} is played".into()];
        spell.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![13],
            ..Default::default()
        }];
        assert!(
            matches!(
                pilot.step(&spell, 0, &[], false),
                Step::Press {
                    pressed: Pressed::Pass,
                    ..
                }
            ),
            "a reaction on offer is passed through while holding for my turn"
        );
        let mut resolved = theirs.clone();
        resolved.narration = vec!["{card 40} is played".into(), "{card 40} resolves".into()];
        assert!(matches!(
            pilot.step(&resolved, 0, &[], false),
            Step::Press { .. }
        ));
        assert_eq!(pilot.step(&waiting(2, 1), 0, &[], false), Step::Idle);
        let Step::Model { held } = pilot.step(&with_play(my_turn(3)), 0, &[], false) else {
            panic!("my turn wakes the model");
        };
        assert_eq!(
            held,
            [
                "You held until your next turn begins; the hold ended because your turn 3 has begun.",
                "{seat 1} put {card 40} on the chain",
                "{card 40} is played",
                "{card 40} resolves",
                "Meanwhile the table passed twice for you and answered 0 forced prompts.",
            ]
        );
        assert_eq!(pilot.holding(), None);
        assert_eq!(
            pilot.stretch().as_deref(),
            Some("passed twice with nothing to do")
        );
    }

    #[test]
    fn every_condition_is_judged_over_the_view_and_a_question_breaks_any_hold() {
        let theirs = passing(2, 1);
        let mut opponent = Pilot::default();
        opponent
            .hold(Condition::OpponentPlayed, &theirs, 0, &[])
            .unwrap();
        assert!(matches!(
            opponent.step(&theirs, 0, &[], false),
            Step::Press { .. }
        ));
        let mut mine_on_chain = theirs.clone();
        mine_on_chain.chain = vec![ChainRow {
            item: 1,
            card: Some(5),
            seat: 0,
        }];
        assert!(
            matches!(
                opponent.step(&mine_on_chain, 0, &[], false),
                Step::Press { .. }
            ),
            "my own chain item is not the opponent playing"
        );
        let mut played = mine_on_chain.clone();
        played.chain.push(ChainRow {
            item: 2,
            card: None,
            seat: 1,
        });
        let Step::Model { held } = opponent.step(&played, 0, &[], false) else {
            panic!("the opponent played");
        };
        assert!(held[0].ends_with("because the opponent put something on the chain."));
        assert!(held.contains(&"{seat 1} put a face-down card on the chain".to_string()));
        let mut fight = Pilot::default();
        let mut in_showdown = theirs.clone();
        in_showdown
            .status
            .push("showdown at {zone 9} · {seat 1} attacks {seat 0}".into());
        fight
            .hold(Condition::Showdown, &in_showdown, 0, &[])
            .unwrap();
        assert!(
            matches!(fight.step(&in_showdown, 0, &[], false), Step::Press { .. }),
            "the showdown already open when the hold was set is not news"
        );
        assert!(matches!(
            fight.step(&theirs, 0, &[], false),
            Step::Press { .. }
        ));
        assert!(matches!(
            fight.step(&in_showdown, 0, &[], false),
            Step::Model { .. }
        ));
        let mut named = Pilot::default();
        named
            .hold(
                Condition::CardNamed("Vi".into()),
                &theirs,
                0,
                &[(50, "Vi".into())],
            )
            .unwrap();
        assert!(matches!(
            named.step(&theirs, 0, &[(50, "Vi".into())], false),
            Step::Press { .. }
        ));
        let landed = [(50, "Vi".into()), (51, "vi".into())];
        assert_eq!(
            named.step(&waiting(2, 1), 0, &landed, false),
            Step::Idle,
            "a met condition waits for a view the seat can act on"
        );
        assert_eq!(named.holding(), Some(&Condition::CardNamed("Vi".into())));
        let Step::Model { held } = named.step(&theirs, 0, &landed, false) else {
            panic!("a second Vi entered");
        };
        assert!(held[0].contains("vi (#51) entered the board"));
        let mut counted = Pilot::default();
        counted.hold(Condition::Passes(2), &theirs, 0, &[]).unwrap();
        assert!(matches!(
            counted.step(&theirs, 0, &[], false),
            Step::Press { .. }
        ));
        assert!(matches!(
            counted.step(&theirs, 0, &[], false),
            Step::Press { .. }
        ));
        assert!(matches!(
            counted.step(&theirs, 0, &[], false),
            Step::Model { .. }
        ));
        let mut playable = Pilot::default();
        playable
            .hold(
                Condition::AnyOf(vec![Condition::MyTurn, Condition::PlayableAction]),
                &theirs,
                0,
                &[],
            )
            .unwrap();
        let mut react = theirs.clone();
        react.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![13],
            ..Default::default()
        }];
        let Step::Model { held } = playable.step(&react, 0, &[], false) else {
            panic!("a playable action wakes the any_of hold");
        };
        assert!(held[0].contains("because you have something to play"));
        let mut broken = Pilot::default();
        broken.hold(Condition::MyTurn, &theirs, 0, &[]).unwrap();
        let choose = asked(
            "{card 9}: choose a unit",
            1,
            1,
            vec![
                plain("{card 5}", None, Some(5)),
                plain("{card 6}", None, Some(6)),
            ],
        );
        let Step::Model { held } = broken.step(&choose, 0, &[], false) else {
            panic!("a question breaks the hold");
        };
        assert!(held[0].contains("a question only you can answer: {card 9}: choose a unit"));
        let mut chatted = Pilot::default();
        chatted.hold(Condition::MyTurn, &theirs, 0, &[]).unwrap();
        let Step::Model { held } = chatted.step(&theirs, 0, &[], true) else {
            panic!("the player breaks the hold");
        };
        assert!(held[0].contains("because the player wrote to you"));
        assert_eq!(chatted.holding(), None);
    }
    #[test]
    fn a_hold_is_refused_while_a_decision_is_open_and_a_lone_roll_is_sent_without_the_model() {
        let mut pilot = Pilot::default();
        let choose = asked(
            "{card 9}: choose a unit",
            1,
            1,
            vec![
                plain("{card 5}", None, Some(5)),
                plain("{card 6}", None, Some(6)),
            ],
        );
        let refused = pilot.hold(Condition::MyTurn, &choose, 0, &[]).unwrap_err();
        assert!(
            refused
                .reason
                .starts_with("a question only you can answer: {card 9}: choose a unit"),
            "{}",
            refused.reason
        );
        assert!(!refused.again, "the first refusal asks the model again");
        let refused = pilot.hold(Condition::MyTurn, &choose, 0, &[]).unwrap_err();
        assert!(
            refused.again,
            "a second refusal on the same view moves the table on"
        );
        let refused = pilot
            .hold(Condition::OpponentPlayed, &with_play(my_turn(3)), 0, &[])
            .unwrap_err();
        assert!(
            refused.reason.starts_with("your turn has plays open") && !refused.again,
            "{}",
            refused.reason
        );
        let mut behind = choose.clone();
        behind.prompt.as_mut().unwrap().seat = 1;
        behind.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![13],
            ..Default::default()
        }];
        let refused = pilot.hold(Condition::MyTurn, &behind, 0, &[]).unwrap_err();
        assert!(
            refused.reason.starts_with("you have something to play"),
            "a legal row behind the opponent's prompt is not their question: {}",
            refused.reason
        );
        assert_eq!(pilot.holding(), None);
        let mut react = passing(2, 1);
        react.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![13],
            ..Default::default()
        }];
        assert_eq!(
            pilot.hold(Condition::MyTurn, &react, 0, &[]),
            Ok(()),
            "a reaction on offer is passed through, so it does not refuse a hold"
        );
        assert!(matches!(
            pilot.step(&react, 0, &[], false),
            Step::Press {
                pressed: Pressed::Pass,
                ..
            }
        ));
        let mut shuffle = PluginView {
            status: vec![turn_line(2, 1)],
            affordances: vec![
                Affordance {
                    kind: AffordanceKind::Commit { roll: 7 },
                    ..plain("roll", None, None)
                },
                plain("free table", None, None),
            ],
            prompt: Some(PromptSummary {
                seat: 1,
                why: "roll to shuffle 2 recycled cards".into(),
                min: 2,
                max: 2,
                picked: 2,
                optional: false,
            }),
            ..Default::default()
        };
        let Step::Press { label, pressed, .. } = pilot.step(&shuffle, 0, &[], false) else {
            panic!("the roll is sent");
        };
        assert_eq!((label.as_str(), pressed), ("roll", Pressed::Roll));
        shuffle.prompt = None;
        assert!(matches!(
            pilot.step(&shuffle, 0, &[], false),
            Step::Press {
                pressed: Pressed::Roll,
                ..
            }
        ));
        let Step::Model { held } = pilot.step(&with_play(my_turn(3)), 0, &[], false) else {
            panic!("my turn wakes the hold");
        };
        assert!(held.contains(&"you sent your roll".to_string()));
        assert!(held.last().is_some_and(
            |line| line.ends_with("passed once for you and answered 2 forced prompts.")
        ));
        assert_eq!(
            pilot.stretch().as_deref(),
            Some("passed once and answered 2 forced prompts with nothing to decide")
        );
    }

    fn free_table(mut view: PluginView) -> PluginView {
        for line in &mut view.status {
            *line = line.replace("rules enforced", "free table");
        }
        view
    }

    #[test]
    fn a_free_table_is_the_models_alone_and_holds_nothing() {
        let mut pilot = Pilot::default();
        assert_eq!(
            pilot.step(&free_table(my_turn(3)), 0, &[], false),
            Step::Model { held: Vec::new() },
            "end turn alone on a free table is not an empty turn: the model keeps the rules"
        );
        assert_eq!(
            pilot.step(&free_table(passing(2, 1)), 0, &[], false),
            Step::Model { held: Vec::new() }
        );
        assert_eq!(
            pilot.step(&free_table(waiting(2, 1)), 0, &[], false),
            Step::Model { held: Vec::new() }
        );
        let refused = pilot
            .hold(Condition::MyTurn, &free_table(passing(2, 1)), 0, &[])
            .unwrap_err();
        assert_eq!(refused.reason, FREE_TABLE);
        assert_eq!(pilot.stretch(), None, "nothing was pressed");
        assert!(!is_quiet(&free_table(passing(2, 1)), 0));
        assert!(is_quiet(&passing(2, 1), 0));
        let mut went_free = Pilot::default();
        went_free
            .hold(Condition::MyTurn, &passing(2, 1), 0, &[])
            .unwrap();
        let Step::Model { held } = went_free.step(&free_table(passing(2, 1)), 0, &[], false) else {
            panic!("a table that went free wakes the hold");
        };
        assert!(held[0].ends_with("because the table went free."));
        assert_eq!(went_free.holding(), None);
    }

    #[test]
    fn quiet_is_what_the_pilot_would_send_without_the_model() {
        let order = asked(
            "order your triggers (last placed resolves first)",
            2,
            2,
            vec![
                plain("{card 12} trigger", None, Some(12)),
                plain("{card 13} trigger", None, Some(13)),
            ],
        );
        assert!(auto::offer(&order, 0).is_quiet());
        assert!(
            !is_quiet(&order, 0),
            "an order that matters goes to the model"
        );
        let discard = asked(
            "discard a card",
            2,
            2,
            vec![
                plain("{card 5}", None, Some(5)),
                plain("{card 6}", None, Some(6)),
            ],
        );
        assert!(is_quiet(&discard, 0));
        assert!(is_quiet(&my_turn(3), 0));
        assert!(!is_quiet(&with_play(my_turn(3)), 0));
        assert!(!is_quiet(&waiting(2, 1), 0));
    }
}
