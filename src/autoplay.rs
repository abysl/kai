use crate::ai::random::{self, Choice, Rng};
use crate::ai::soak::turn_of;
use crate::deck::pinned::TableLegends;
use crate::table::{Recovery, SessionRole};
use agni_net::session::SeatInfo;
use agni_sim::wire::{PluginView, ZoneDecl};
use bevy::prelude::*;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use web_time::Instant;

pub const EVENT_PREFIX: &str = "KAI_EVENT ";
pub const ENV_VAR: &str = "KAI_AUTOPLAY";
pub const ARG: &str = "--autoplay";
pub const QUERY_PARAM: &str = "autoplay";
pub const INTENT_EXTRA: &str = "autoplay";
pub const INTENT_EXTRA_B64: &str = "autoplay_b64";
pub const RECENT_KEEP: usize = 512;
pub const MESH_WAIT_MS: u64 = 60_000;
pub const FOLD_WAIT_MS: u64 = 5_000;
pub const MAX_STALLS: u32 = 3;
pub const JOIN_RETRY_MS: u64 = 3_000;
pub const MAX_JOIN_RETRIES: u32 = 20;
pub const LINGER_MS: u64 = 1_500;
pub const HOST_LINGER_MS: u64 = 10_000;
pub const IDLE_FAIL_MS: u64 = 60_000;
pub const EXIT_NO_OUTCOME: i32 = 3;
pub const SWITCH_TO_ENFORCED: &str = "switch to rules enforced";
pub const SWITCH_PREFIX: &str = "switch to ";
pub const FREE_TABLE_LINE: &str = "mode: free table";
pub const DEFAULT_TIMEOUT_S: u64 = 600;
pub const DEFAULT_TURN_CAP: u32 = 60;
pub const DEFAULT_PACE_MS: u64 = 300;
pub const DEFAULT_PLAYERS: u8 = 2;
pub const DEFAULT_HOST_DECK: &str = "lillia-house";
pub const DEFAULT_JOIN_DECK: &str = "irelia-house";
pub const SAVED_PREFIX: &str = "saved:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckChoice {
    Pool(String),
    Saved(String),
}

impl DeckChoice {
    pub fn parse(spec: &str) -> Result<Self, String> {
        match spec.strip_prefix(SAVED_PREFIX) {
            Some(label) if label.trim().is_empty() => Err(format!(
                "{SAVED_PREFIX} needs the label of a held history row"
            )),
            Some(label) => Ok(Self::Saved(label.trim().to_string())),
            None => crate::deck::pool::find(spec)
                .map(|deck| Self::Pool(deck.slug))
                .map_err(|error| error.to_string()),
        }
    }

    pub fn word(&self) -> String {
        match self {
            Self::Pool(slug) => slug.clone(),
            Self::Saved(label) => format!("{SAVED_PREFIX}{label}"),
        }
    }
}

pub fn saved_row_by_label(
    rows: &[crate::deck::history::SavedRow],
    label: &str,
) -> Option<spirit_sdk::CiHash> {
    let wanted = label.trim();
    rows.iter()
        .filter(|row| row.held)
        .find(|row| row.label.trim() == wanted)
        .or_else(|| {
            rows.iter()
                .filter(|row| row.held)
                .find(|row| row.label.trim().eq_ignore_ascii_case(wanted))
        })
        .map(|row| row.ci)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Host,
    Join { host: String },
}

impl Role {
    pub fn word(&self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Join { .. } => "join",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    #[default]
    Riftbound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Brain {
    Random {
        #[serde(default)]
        seed: u64,
    },
}

impl Default for Brain {
    fn default() -> Self {
        Brain::Random { seed: 0 }
    }
}

impl Brain {
    pub fn seed(&self) -> u64 {
        match self {
            Brain::Random { seed } => *seed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Until {
    #[default]
    Winner,
    Seated,
    Turns(u32),
}

impl Until {
    pub fn word(&self) -> String {
        match self {
            Until::Winner => "winner".into(),
            Until::Seated => "seated".into(),
            Until::Turns(n) => format!("turns:{n}"),
        }
    }
}

fn yes() -> bool {
    true
}

fn default_timeout_s() -> u64 {
    DEFAULT_TIMEOUT_S
}

fn default_turn_cap() -> u32 {
    DEFAULT_TURN_CAP
}

fn default_pace_ms() -> u64 {
    DEFAULT_PACE_MS
}

fn default_players() -> u8 {
    DEFAULT_PLAYERS
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub role: Role,
    #[serde(default)]
    pub game: Game,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck: Option<String>,
    #[serde(default = "yes")]
    pub enforced: bool,
    #[serde(default)]
    pub brain: Brain,
    #[serde(default)]
    pub until: Until,
    #[serde(default = "default_timeout_s")]
    pub timeout_s: u64,
    #[serde(default = "default_turn_cap")]
    pub turn_cap: u32,
    #[serde(default)]
    pub battlefield: usize,
    #[serde(default = "default_pace_ms")]
    pub pace_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_players")]
    pub players: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<bool>,
}

impl Plan {
    pub fn parse(json: &str) -> Result<Self, String> {
        let plan: Plan = serde_json::from_str(json).map_err(|error| error.to_string())?;
        plan.deck_choice()?;
        if plan.players == 0 {
            return Err("players must be at least 1".into());
        }
        Ok(plan)
    }

    pub fn example_host() -> Self {
        Self {
            role: Role::Host,
            game: Game::Riftbound,
            deck: Some(DEFAULT_HOST_DECK.into()),
            enforced: true,
            brain: Brain::Random { seed: 7 },
            until: Until::Winner,
            timeout_s: DEFAULT_TIMEOUT_S,
            turn_cap: DEFAULT_TURN_CAP,
            battlefield: 0,
            pace_ms: DEFAULT_PACE_MS,
            name: Some("host-desktop".into()),
            players: DEFAULT_PLAYERS,
            exit: None,
        }
    }

    pub fn example_join(host: &str) -> Self {
        Self {
            role: Role::Join { host: host.into() },
            deck: Some(DEFAULT_JOIN_DECK.into()),
            brain: Brain::Random { seed: 8 },
            name: Some("joiner-web".into()),
            ..Self::example_host()
        }
    }

    pub fn deck_prefix(&self) -> String {
        self.deck.clone().unwrap_or_else(|| match self.role {
            Role::Host => DEFAULT_HOST_DECK.into(),
            Role::Join { .. } => DEFAULT_JOIN_DECK.into(),
        })
    }

    pub fn deck_choice(&self) -> Result<DeckChoice, String> {
        DeckChoice::parse(&self.deck_prefix())
    }

    pub fn deck_slug(&self) -> String {
        match self.deck_choice() {
            Ok(choice) => choice.word(),
            Err(_) => self.deck_prefix(),
        }
    }

    pub fn player_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("autoplay-{}", self.role.word()))
    }

    pub fn host_target(&self) -> Option<&str> {
        match &self.role {
            Role::Host => None,
            Role::Join { host } => Some(host),
        }
    }

    pub fn exits(&self, exit_default: bool) -> bool {
        self.exit.unwrap_or(exit_default)
    }

    pub fn is_host(&self) -> bool {
        self.host_target().is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterSeat {
    pub seat: u8,
    pub name: String,
    pub host: bool,
    pub connected: bool,
}

impl From<&SeatInfo> for RosterSeat {
    fn from(info: &SeatInfo) -> Self {
        Self {
            seat: info.seat,
            name: info.name.clone(),
            host: info.host,
            connected: info.connected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeKind {
    Winner,
    Turns,
    Seated,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub result: OutcomeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<u8>,
    pub turns: u32,
    pub sent: u32,
    pub refused: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub status: Vec<String>,
}

impl Outcome {
    pub fn succeeded(&self, until: &Until) -> bool {
        match (self.result, until) {
            (OutcomeKind::Failed, _) => false,
            (OutcomeKind::Winner, Until::Winner | Until::Turns(_)) => self.winner.is_some(),
            (OutcomeKind::Turns, Until::Turns(n)) => self.turns >= *n,
            (OutcomeKind::Seated, Until::Seated) => true,
            _ => false,
        }
    }

    pub fn agree(&self, other: &Outcome) -> Result<(), String> {
        if self.result == OutcomeKind::Failed || other.result == OutcomeKind::Failed {
            return Err(format!(
                "a side failed: {} / {}",
                self.reason.as_deref().unwrap_or("ok"),
                other.reason.as_deref().unwrap_or("ok")
            ));
        }
        if self.result != other.result {
            return Err(format!(
                "results differ: {:?} vs {:?}",
                self.result, other.result
            ));
        }
        if self.winner != other.winner {
            return Err(format!(
                "winners differ: {:?} vs {:?}",
                self.winner, other.winner
            ));
        }
        if self.turns != other.turns {
            return Err(format!(
                "turn counts differ: {} vs {}",
                self.turns, other.turns
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    Installed {
        role: String,
        until: String,
        platform: String,
    },
    Meshed {
        node: String,
        ticket: String,
    },
    Hosting {
        node: String,
        ticket: String,
        name: String,
        seat: u8,
    },
    Joining {
        host: String,
    },
    Seated {
        seat: u8,
        role: String,
    },
    Roster {
        seats: Vec<RosterSeat>,
    },
    Dealt {
        seat: u8,
        deck: String,
        battlefield: String,
    },
    Started {
        first: u8,
        enforced: bool,
    },
    Turn {
        n: u32,
    },
    Sent {
        n: u32,
        kind: String,
        label: String,
    },
    Refused {
        text: String,
    },
    Warn {
        text: String,
    },
    Outcome(Outcome),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(flatten)]
    pub event: Event,
    pub ms: u64,
}

impl Record {
    pub fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|error| {
            format!(
                "{{\"event\":\"warn\",\"text\":\"record did not serialise: {error}\",\"ms\":{}}}",
                self.ms
            )
        })
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        let json = line.strip_prefix(EVENT_PREFIX).unwrap_or(line);
        serde_json::from_str(json.trim()).map_err(|error| error.to_string())
    }
}

pub fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        "desktop"
    }
}

struct Installed {
    plan: Plan,
    exit: bool,
    at: Instant,
    outcome: Option<bool>,
}

static INSTALLED: Mutex<Option<Installed>> = Mutex::new(None);
static INSTALL_FAILED: AtomicBool = AtomicBool::new(false);
static RECENT: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub fn plan_text(value: &str) -> Result<String, String> {
    match value.trim().strip_prefix('@') {
        Some(path) => {
            std::fs::read_to_string(path).map_err(|error| format!("plan file {path}: {error}"))
        }
        None => Ok(value.to_string()),
    }
}

pub fn from_env_and_args() -> Option<String> {
    if let Ok(value) = std::env::var(ENV_VAR) {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == ARG {
            return args.next();
        }
        if let Some(value) = arg.strip_prefix(&format!("{ARG}=")) {
            return Some(value.to_string());
        }
    }
    None
}

pub fn install(json: &str, exit_default: bool) -> Result<(), String> {
    let text = plan_text(json);
    let plan = text.and_then(|text| Plan::parse(&text));
    let plan = match plan {
        Ok(plan) => plan,
        Err(error) => {
            let reason = format!("install: {error}");
            emit_record(Record {
                event: Event::Outcome(Outcome {
                    result: OutcomeKind::Failed,
                    winner: None,
                    turns: 0,
                    sent: 0,
                    refused: 0,
                    reason: Some(reason.clone()),
                    status: Vec::new(),
                }),
                ms: 0,
            });
            *INSTALLED.lock() = None;
            INSTALL_FAILED.store(true, Ordering::Relaxed);
            return Err(reason);
        }
    };
    let exit = plan.exits(exit_default);
    INSTALL_FAILED.store(false, Ordering::Relaxed);
    let installed = Event::Installed {
        role: plan.role.word().into(),
        until: plan.until.word(),
        platform: platform().into(),
    };
    *INSTALLED.lock() = Some(Installed {
        plan,
        exit,
        at: Instant::now(),
        outcome: None,
    });
    emit(installed);
    Ok(())
}

pub fn installed_plan() -> Option<Plan> {
    INSTALLED.lock().as_ref().map(|held| held.plan.clone())
}

pub fn player_name() -> Option<String> {
    INSTALLED
        .lock()
        .as_ref()
        .map(|held| held.plan.player_name())
}

pub fn since_install_ms() -> u64 {
    INSTALLED
        .lock()
        .as_ref()
        .map(|held| held.at.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

pub fn drain_events() -> Vec<String> {
    RECENT.lock().drain(..).collect()
}

pub fn node_id() -> Option<String> {
    crate::net::node::get().map(|node| node.node_id)
}

pub fn exit_code() -> i32 {
    match INSTALLED.lock().as_ref() {
        None if INSTALL_FAILED.load(Ordering::Relaxed) => 1,
        None => 0,
        Some(held) => match held.outcome {
            Some(true) => 0,
            Some(false) => 1,
            None => EXIT_NO_OUTCOME,
        },
    }
}

fn note_outcome(success: bool) {
    if let Some(held) = INSTALLED.lock().as_mut() {
        held.outcome = Some(success);
    }
}

fn emit_record(record: Record) {
    let line = record.line();
    println!("{EVENT_PREFIX}{line}");
    bevy::log::info!(target: "kai::autoplay", "{EVENT_PREFIX}{line}");
    let mut recent = RECENT.lock();
    recent.push_back(line);
    while recent.len() > RECENT_KEEP {
        recent.pop_front();
    }
}

fn emit(event: Event) {
    emit_record(Record {
        event,
        ms: since_install_ms(),
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckInfo {
    pub label: String,
    pub battlefields: usize,
    pub needs_choice: bool,
    pub battlefield: Option<String>,
}

pub struct Observed<'a> {
    pub node: Option<(String, String)>,
    pub node_status: String,
    pub role: SessionRole,
    pub status: &'a str,
    pub recovery: &'a Recovery,
    pub roster: &'a [SeatInfo],
    pub my_seat: u8,
    pub view: &'a PluginView,
    pub zones: &'a [ZoneDecl],
    pub legends: TableLegends,
    pub enforced_now: bool,
    pub pinned_side: Option<String>,
    pub deck: Option<DeckInfo>,
    pub refusal: Option<(String, f64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Prepare { enforced: bool },
    HostTable,
    Join(String),
    GoToTable,
    PinSide(String),
    SeatDeck(String),
    SeatSaved(String),
    SetBattlefield(usize),
    Fire(usize),
    Drop { card: u32, zone: u16, hidden: bool },
    Rejoin(String),
    Leave,
    Exit(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Booting,
    Opening,
    Dialing,
    Pinning,
    Gathering,
    Rolling,
    Playing,
    Done,
}

#[derive(Debug, Clone)]
struct Inflight {
    view: PluginView,
    at: u64,
}

#[derive(Debug)]
pub struct Machine {
    plan: Plan,
    exit: bool,
    rng: Rng,
    phase: Phase,
    pub events: Vec<Record>,
    outcome: Option<Outcome>,
    sent: u32,
    refused: u32,
    turn: Option<u32>,
    last_roster: Vec<RosterSeat>,
    last_refusal: Option<(String, f64)>,
    inflight: Option<Inflight>,
    last_sent: Option<u64>,
    stalls: u32,
    asked: bool,
    asked_at: u64,
    saw_joining: bool,
    join_retries: u32,
    rejoined: bool,
    deck_asked: bool,
    idle: Option<(PluginView, u64)>,
    done_at: Option<u64>,
    finished: bool,
}

impl Machine {
    pub fn new(plan: Plan, exit: bool) -> Self {
        let seed = plan.brain.seed();
        Self {
            plan,
            exit,
            rng: Rng::seeded(seed),
            phase: Phase::Booting,
            events: Vec::new(),
            outcome: None,
            sent: 0,
            refused: 0,
            turn: None,
            last_roster: Vec::new(),
            last_refusal: None,
            inflight: None,
            last_sent: None,
            stalls: 0,
            asked: false,
            asked_at: 0,
            saw_joining: false,
            join_retries: 0,
            rejoined: false,
            deck_asked: false,
            idle: None,
            done_at: None,
            finished: false,
        }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    fn record(&mut self, event: Event, now: u64) {
        self.events.push(Record {
            event: event.clone(),
            ms: now,
        });
        emit_record(Record {
            event,
            ms: since_install_ms(),
        });
    }

    fn warn(&mut self, text: String, now: u64) {
        self.record(Event::Warn { text }, now);
    }

    fn finish(&mut self, now: u64, result: OutcomeKind, reason: Option<String>, view: &PluginView) {
        if self.outcome.is_some() {
            return;
        }
        let outcome = Outcome {
            result,
            winner: view.winner,
            turns: self.turn.unwrap_or(0),
            sent: self.sent,
            refused: self.refused,
            reason,
            status: view.status.clone(),
        };
        let success = outcome.succeeded(&self.plan.until);
        note_outcome(success);
        self.outcome = Some(outcome.clone());
        self.phase = Phase::Done;
        self.done_at = Some(now);
        self.inflight = None;
        self.record(Event::Outcome(outcome), now);
    }

    pub fn fail(&mut self, now: u64, reason: impl Into<String>, view: &PluginView) {
        self.finish(now, OutcomeKind::Failed, Some(reason.into()), view);
    }

    pub fn joined(&mut self, now: u64) {
        let host = self.plan.host_target().unwrap_or_default().to_string();
        self.record(Event::Joining { host }, now);
    }

    pub fn step(&mut self, now: u64, obs: &Observed) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.outcome.is_some() {
            let waited = now.saturating_sub(self.done_at.unwrap_or(now));
            if !self.finished && waited >= self.linger(obs) {
                self.finished = true;
                effects.push(Effect::Leave);
                if self.exit {
                    let success = self
                        .outcome
                        .as_ref()
                        .is_some_and(|outcome| outcome.succeeded(&self.plan.until));
                    effects.push(Effect::Exit(success));
                }
            }
            return effects;
        }
        if now > self.plan.timeout_s.saturating_mul(1000) {
            self.fail(now, "timeout", obs.view);
            return effects;
        }
        self.watch_roster(obs, now);
        self.watch_turn(obs, now);
        let fresh_refusal = self.watch_refusals(obs, now);
        if obs.role == SessionRole::Ended {
            self.ended(now, obs, &mut effects);
            return effects;
        }
        if self.rejoined && obs.role == SessionRole::Solo && obs.status.starts_with("join failed") {
            self.fail(now, format!("session ended: {}", obs.status), obs.view);
            return effects;
        }
        match self.phase {
            Phase::Booting => self.booting(now, obs),
            Phase::Opening => self.opening(now, obs, &mut effects),
            Phase::Dialing => self.dialing(now, obs, &mut effects),
            Phase::Pinning => self.pinning(now, obs, &mut effects),
            Phase::Gathering => self.gathering(now, obs),
            Phase::Rolling => self.rolling(now, obs, fresh_refusal, &mut effects),
            Phase::Playing => self.playing(now, obs, fresh_refusal, &mut effects),
            Phase::Done => {}
        }
        if matches!(
            self.phase,
            Phase::Pinning | Phase::Gathering | Phase::Rolling | Phase::Playing
        ) {
            self.pick_battlefield(now, obs, &mut effects);
        }
        effects
    }

    fn pick_battlefield(&mut self, now: u64, obs: &Observed, effects: &mut Vec<Effect>) {
        let Some(deck) = obs.deck.as_ref().filter(|deck| deck.needs_choice) else {
            return;
        };
        let mut index = self.plan.battlefield;
        if index >= deck.battlefields {
            index = deck.battlefields.saturating_sub(1);
            self.warn(
                format!(
                    "battlefield {} is past the deck's {}; using {index}",
                    self.plan.battlefield, deck.battlefields
                ),
                now,
            );
        }
        effects.push(Effect::SetBattlefield(index));
    }

    fn linger(&self, obs: &Observed) -> u64 {
        if !self.plan.is_host() {
            return LINGER_MS;
        }
        let peers_gone = obs
            .roster
            .iter()
            .filter(|seat| !seat.host)
            .all(|seat| !seat.connected);
        if peers_gone {
            LINGER_MS
        } else {
            HOST_LINGER_MS
        }
    }

    fn watch_roster(&mut self, obs: &Observed, now: u64) {
        let seats: Vec<RosterSeat> = obs.roster.iter().map(RosterSeat::from).collect();
        if seats != self.last_roster {
            self.last_roster = seats.clone();
            self.record(Event::Roster { seats }, now);
        }
    }

    fn watch_turn(&mut self, obs: &Observed, now: u64) {
        if let Some(n) = turn_of(&obs.view.status) {
            if self.turn != Some(n) {
                self.turn = Some(n);
                self.record(Event::Turn { n }, now);
            }
        }
    }

    fn watch_refusals(&mut self, obs: &Observed, now: u64) -> bool {
        if obs.refusal.is_none() || obs.refusal == self.last_refusal {
            return false;
        }
        self.last_refusal = obs.refusal.clone();
        if let Some((text, _)) = &obs.refusal {
            self.refused += 1;
            let text = text.clone();
            self.record(Event::Refused { text }, now);
        }
        true
    }

    fn ended(&mut self, now: u64, obs: &Observed, effects: &mut Vec<Effect>) {
        match self.phase {
            Phase::Booting | Phase::Opening => {
                self.fail(now, format!("host: {}", obs.status), obs.view)
            }
            Phase::Dialing => self.fail(now, format!("join: {}", obs.status), obs.view),
            _ => match obs.recovery {
                Recovery::Rejoin { host } if !self.rejoined => {
                    self.rejoined = true;
                    self.warn(format!("rejoin: {}", obs.status), now);
                    effects.push(Effect::Rejoin(host.clone()));
                }
                _ => self.fail(now, format!("session ended: {}", obs.status), obs.view),
            },
        }
    }

    fn booting(&mut self, now: u64, obs: &Observed) {
        match &obs.node {
            Some((node, ticket)) => {
                self.record(
                    Event::Meshed {
                        node: node.clone(),
                        ticket: ticket.clone(),
                    },
                    now,
                );
                self.phase = match self.plan.role {
                    Role::Host => Phase::Opening,
                    Role::Join { .. } => Phase::Dialing,
                };
            }
            None if now >= MESH_WAIT_MS => {
                self.fail(now, format!("mesh: {}", obs.node_status), obs.view);
            }
            None => {}
        }
    }

    fn opening(&mut self, now: u64, obs: &Observed, effects: &mut Vec<Effect>) {
        if !self.asked {
            self.asked = true;
            self.asked_at = now;
            effects.push(Effect::Prepare {
                enforced: self.plan.enforced,
            });
            effects.push(Effect::HostTable);
            effects.push(Effect::GoToTable);
            return;
        }
        self.stage_deck(now, obs, effects);
        match obs.role {
            SessionRole::Host => {
                let (node, ticket) = obs.node.clone().unwrap_or_default();
                self.record(
                    Event::Hosting {
                        node,
                        ticket,
                        name: self.plan.player_name(),
                        seat: obs.my_seat,
                    },
                    now,
                );
                self.record(
                    Event::Seated {
                        seat: obs.my_seat,
                        role: "host".into(),
                    },
                    now,
                );
                self.phase = Phase::Pinning;
            }
            SessionRole::Solo => self.fail(now, format!("host: {}", obs.status), obs.view),
            _ => {}
        }
    }

    fn dialing(&mut self, now: u64, obs: &Observed, effects: &mut Vec<Effect>) {
        let host = self.plan.host_target().unwrap_or_default().to_string();
        if !self.asked {
            self.asked = true;
            self.asked_at = now;
            effects.push(Effect::Prepare {
                enforced: self.plan.enforced,
            });
            effects.push(Effect::Join(host));
            return;
        }
        match obs.role {
            SessionRole::Client => {
                self.stage_deck(now, obs, effects);
                self.record(
                    Event::Seated {
                        seat: obs.my_seat,
                        role: "client".into(),
                    },
                    now,
                );
                effects.push(Effect::GoToTable);
                self.phase = Phase::Pinning;
            }
            SessionRole::Joining => self.saw_joining = true,
            SessionRole::Solo
                if (self.saw_joining || obs.status.starts_with("join failed"))
                    && now.saturating_sub(self.asked_at) >= JOIN_RETRY_MS =>
            {
                if self.join_retries >= MAX_JOIN_RETRIES {
                    self.fail(now, format!("join: {}", obs.status), obs.view);
                    return;
                }
                self.join_retries += 1;
                self.warn(
                    format!("join retry {}: {}", self.join_retries, obs.status),
                    now,
                );
                self.saw_joining = false;
                self.asked_at = now;
                effects.push(Effect::Join(host));
            }
            _ => {}
        }
    }

    fn pinning(&mut self, now: u64, obs: &Observed, effects: &mut Vec<Effect>) {
        if let Some(mine) = obs.legends.mine.clone() {
            let deck = crate::deck::pool::of_legend(&mine)
                .map(|deck| deck.label)
                .unwrap_or(mine);
            let battlefield = obs
                .deck
                .as_ref()
                .filter(|record| record.label == deck)
                .and_then(|record| record.battlefield.clone())
                .unwrap_or_default();
            self.record(
                Event::Dealt {
                    seat: obs.my_seat,
                    deck,
                    battlefield,
                },
                now,
            );
            self.phase = Phase::Gathering;
            return;
        }
        self.stage_deck(now, obs, effects);
    }

    fn stage_deck(&mut self, now: u64, obs: &Observed, effects: &mut Vec<Effect>) {
        let choice = match self.plan.deck_choice() {
            Ok(choice) => choice,
            Err(error) => {
                self.fail(now, format!("deck: {error}"), obs.view);
                return;
            }
        };
        let seated_here = match &choice {
            DeckChoice::Pool(slug) => {
                let label = crate::deck::pool::of_slug(slug).map(|deck| deck.label);
                obs.deck
                    .as_ref()
                    .is_some_and(|deck| Some(&deck.label) == label.as_ref())
            }
            DeckChoice::Saved(_) => self.deck_asked && obs.deck.is_some(),
        };
        match &choice {
            DeckChoice::Pool(slug) if obs.enforced_now => {
                if obs.pinned_side.as_deref() != Some(slug.as_str()) {
                    effects.push(Effect::PinSide(slug.clone()));
                }
            }
            DeckChoice::Pool(slug) if !seated_here && !self.deck_asked => {
                self.deck_asked = true;
                effects.push(Effect::SeatDeck(slug.clone()));
            }
            DeckChoice::Saved(_) if obs.enforced_now => {
                self.fail(
                    now,
                    format!(
                        "deck: {} seats on free tables only, and this table is rules enforced",
                        choice.word()
                    ),
                    obs.view,
                );
                return;
            }
            DeckChoice::Saved(label) if !self.deck_asked => {
                self.deck_asked = true;
                effects.push(Effect::SeatSaved(label.clone()));
            }
            _ => {}
        }
        let _ = seated_here;
    }

    fn gathering(&mut self, now: u64, obs: &Observed) {
        let connected = obs.roster.iter().filter(|seat| seat.connected).count();
        if connected < usize::from(self.plan.players) {
            return;
        }
        if self.plan.until == Until::Seated {
            if obs.legends.theirs.len() + 1 < usize::from(self.plan.players) {
                return;
            }
            self.finish(now, OutcomeKind::Seated, None, obs.view);
            return;
        }
        self.phase = Phase::Rolling;
    }

    fn rolling(
        &mut self,
        now: u64,
        obs: &Observed,
        fresh_refusal: bool,
        effects: &mut Vec<Effect>,
    ) {
        if turn_of(&obs.view.status).is_some() {
            let first = crate::table::plate::turn_of(obs.view)
                .map(|turn| turn.seat)
                .unwrap_or(0);
            let enforced = crate::table::plugin_ui::enforced(obs.view);
            self.record(Event::Started { first, enforced }, now);
            self.phase = Phase::Playing;
            self.playing(now, obs, fresh_refusal, effects);
            return;
        }
        let all_dealt = obs.legends.theirs.len() + 1 >= usize::from(self.plan.players);
        let options = lobby_options(obs.view, obs.my_seat, self.plan.enforced, all_dealt);
        if !all_dealt {
            self.idle = None;
        }
        self.act(now, obs, fresh_refusal, options, effects);
    }

    fn playing(
        &mut self,
        now: u64,
        obs: &Observed,
        fresh_refusal: bool,
        effects: &mut Vec<Effect>,
    ) {
        if obs.view.winner.is_some() {
            self.finish(now, OutcomeKind::Winner, None, obs.view);
            return;
        }
        let turn = self.turn.unwrap_or(0);
        if let Until::Turns(n) = self.plan.until {
            if turn >= n {
                self.finish(now, OutcomeKind::Turns, None, obs.view);
                return;
            }
        }
        if turn > self.plan.turn_cap {
            self.fail(now, "turn cap", obs.view);
            return;
        }
        let options = game_options(obs.view, obs.my_seat);
        self.act(now, obs, fresh_refusal, options, effects);
    }

    fn act(
        &mut self,
        now: u64,
        obs: &Observed,
        fresh_refusal: bool,
        options: Vec<Choice>,
        effects: &mut Vec<Effect>,
    ) {
        if let Some(inflight) = &self.inflight {
            if *obs.view != inflight.view || fresh_refusal {
                self.inflight = None;
                self.stalls = 0;
            } else if now.saturating_sub(inflight.at) >= FOLD_WAIT_MS {
                self.stalls += 1;
                self.inflight = None;
                let stalls = self.stalls;
                self.warn(
                    format!("stall {stalls}: no fold and no refusal in {FOLD_WAIT_MS} ms"),
                    now,
                );
                if stalls >= MAX_STALLS {
                    self.fail(now, "stuck", obs.view);
                }
                return;
            } else {
                return;
            }
        }
        if self
            .last_sent
            .is_some_and(|at| now.saturating_sub(at) < self.plan.pace_ms)
        {
            return;
        }
        let Some(choice) = random::pick(&mut self.rng, &options) else {
            self.idle_watch(now, obs);
            return;
        };
        self.idle = None;
        let zones = obs.zones;
        let zone_name = |zone: u16| crate::table::plugin_ui::zone_label(zones, zone);
        let label = choice.describe(&zone_name);
        let kind = match &choice {
            Choice::Action { .. } => "action",
            Choice::Move { hidden: false, .. } => "move",
            Choice::Move { hidden: true, .. } => "hide",
        };
        effects.push(match choice {
            Choice::Action { index, .. } => Effect::Fire(index),
            Choice::Move { card, zone, hidden } => Effect::Drop { card, zone, hidden },
        });
        self.sent += 1;
        let n = self.sent;
        self.record(
            Event::Sent {
                n,
                kind: kind.into(),
                label,
            },
            now,
        );
        self.inflight = Some(Inflight {
            view: obs.view.clone(),
            at: now,
        });
        self.last_sent = Some(now);
    }

    fn idle_watch(&mut self, now: u64, obs: &Observed) {
        match &self.idle {
            Some((view, since)) if view == obs.view => {
                if now.saturating_sub(*since) >= IDLE_FAIL_MS {
                    self.fail(
                        now,
                        format!("stuck: no options and no fold for {IDLE_FAIL_MS} ms"),
                        obs.view,
                    );
                }
            }
            _ => self.idle = Some((obs.view.clone(), now)),
        }
    }
}

fn is_switch(label: &str) -> bool {
    label.starts_with(SWITCH_PREFIX)
}

fn is_start(label: &str) -> bool {
    label == "go first" || (label.starts_with("let ") && label.ends_with(" go first"))
}

pub fn lobby_options(view: &PluginView, me: u8, enforced: bool, all_dealt: bool) -> Vec<Choice> {
    let free_now = view.status.iter().any(|line| line == FREE_TABLE_LINE);
    if enforced && free_now {
        if let Some(index) = view
            .affordances
            .iter()
            .position(|affordance| affordance.enabled && affordance.label == SWITCH_TO_ENFORCED)
        {
            return vec![Choice::Action {
                index,
                label: SWITCH_TO_ENFORCED.into(),
            }];
        }
    }
    random::options(view, me)
        .into_iter()
        .filter(|choice| match choice {
            Choice::Action { label, .. } => !is_switch(label) && (all_dealt || !is_start(label)),
            Choice::Move { .. } => true,
        })
        .collect()
}

pub fn game_options(view: &PluginView, me: u8) -> Vec<Choice> {
    random::options(view, me)
        .into_iter()
        .filter(|choice| match choice {
            Choice::Action { label, .. } => !is_switch(label),
            Choice::Move { .. } => true,
        })
        .collect()
}

#[derive(Resource, Default)]
pub struct Autoplay(pub Option<Machine>);

pub struct AutoplayPlugin;

impl Plugin for AutoplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Autoplay>().add_systems(
            Update,
            step.after(crate::table::auto::auto_pilot)
                .after(crate::net::drain_net)
                .after(crate::deck::pinned::pin_decks)
                .before(crate::net::redeal_after_new_game),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Watch<'w> {
    pub info: ResMut<'w, crate::table::SessionInfo>,
    pub my_seat: Res<'w, crate::table::MySeat>,
    pub panel: Res<'w, crate::table::plugin_ui::PluginPanel>,
    pub table: Res<'w, crate::table::GameTable>,
    pub mirror: Res<'w, crate::table::Mirror>,
    pub refusals: Res<'w, crate::table::toast::Refusals>,
    pub time: Res<'w, Time>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Steer<'w> {
    pub choice: ResMut<'w, crate::net::TableChoice>,
    pub tuning: ResMut<'w, crate::Tuning>,
    pub pinned: ResMut<'w, crate::deck::pinned::PinnedDeck>,
    pub seated: ResMut<'w, crate::deck::import::SeatedDeck>,
    pub import: ResMut<'w, crate::deck::import::ImportPanel>,
    pub art: ResMut<'w, crate::render::art::ArtCache>,
    pub menu: ResMut<'w, crate::menu::Menu>,
    pub host: ResMut<'w, crate::net::HostState>,
    pub client: ResMut<'w, crate::net::ClientState>,
    pub new_games: ResMut<'w, crate::net::NewGameWatch>,
    pub generation: Res<'w, crate::table::DealGeneration>,
    pub sender: crate::table::hud::Sender<'w>,
    pub drops: MessageWriter<'w, crate::table::CardDropped>,
    pub exits: MessageWriter<'w, AppExit>,
}

pub fn deck_info_of(record: &crate::deck::import::SeatedDeckRecord) -> DeckInfo {
    let entries = crate::deck::battlefield::options(record);
    DeckInfo {
        label: crate::deck::history::label(&record.deck),
        battlefields: crate::deck::battlefield::held(entries),
        needs_choice: crate::deck::battlefield::needs_choice(record),
        battlefield: crate::deck::battlefield::chosen_name(record).map(str::to_string),
    }
}

fn deck_info(seated: &crate::deck::import::SeatedDeck) -> Option<DeckInfo> {
    seated.0.as_ref().map(deck_info_of)
}

fn live_ticket(node: &crate::net::node::Node) -> String {
    spirit_node::iroh_tickets::endpoint::EndpointTicket::from(node.endpoint.addr()).to_string()
}

pub fn step(
    mut autoplay: ResMut<Autoplay>,
    mut watch: Watch,
    mut steer: Steer,
    mut deck_trace: Local<Option<Option<DeckInfo>>>,
) {
    if autoplay.0.is_none() {
        let Some(plan) = installed_plan() else {
            return;
        };
        let exit = INSTALLED.lock().as_ref().is_some_and(|held| held.exit);
        autoplay.0 = Some(Machine::new(plan, exit));
    }
    let Some(machine) = autoplay.0.as_mut() else {
        return;
    };
    if machine.finished() {
        return;
    }
    let now = since_install_ms();
    let node = crate::net::node::get().map(|node| (node.node_id.clone(), live_ticket(&node)));
    let legends = crate::deck::pinned::legends_on_table(&watch.table.0, watch.my_seat.0);
    let refusal = watch
        .refusals
        .current
        .as_ref()
        .map(|refusal| (refusal.text.clone(), refusal.at));
    let deck = deck_info(steer.seated.bypass_change_detection());
    let observed = Observed {
        node,
        node_status: crate::net::node::status(),
        role: watch.info.role,
        status: &watch.info.status,
        recovery: &watch.info.recovery,
        roster: &watch.info.roster,
        my_seat: watch.my_seat.0 .0,
        view: &watch.panel.view,
        zones: &watch.mirror.view.zones,
        legends,
        enforced_now: crate::net::rules_enforced(&watch.info, &steer.choice),
        pinned_side: steer.pinned.bypass_change_detection().side.clone(),
        deck,
        refusal,
    };
    if matches!(machine.phase(), Phase::Dialing | Phase::Pinning) {
        let generation = steer.generation.0;
        if steer.new_games.bypass_change_detection().seen != generation {
            steer.new_games.bypass_change_detection().seen = generation;
        }
    }
    if deck_trace.as_ref() != Some(&observed.deck) {
        debug!(target: "kai::autoplay", "deck {:?} auto_deal={} role={:?}", observed.deck, steer.import.bypass_change_detection().auto_deal, observed.role);
        *deck_trace = Some(observed.deck.clone());
    }
    let effects = machine.step(now, &observed);
    for effect in effects {
        apply(machine, now, effect, &mut watch, &mut steer);
    }
}

fn seat_saved(label: &str, seat: agni_core::PlayerId, steer: &mut Steer) -> Result<usize, String> {
    let game = agni_riftbound::GAME;
    let rows = crate::deck::history::store::rows(game);
    let ci = saved_row_by_label(&rows, label).ok_or_else(|| {
        format!(
            "no held history row is labelled {label:?} ({} rows held)",
            rows.iter().filter(|row| row.held).count()
        )
    })?;
    let deck = crate::deck::history::store::recall(game, ci)
        .ok_or_else(|| format!("history row {label:?} is listed but its bytes are not held"))?;
    if deck.game() != crate::net::TableGame::Riftbound {
        return Err(format!("history row {label:?} is not a riftbound deck"));
    }
    Ok(crate::deck::import::seat_deck(
        deck,
        seat,
        &mut steer.seated,
        &mut steer.art,
        None,
    ))
}

fn apply(machine: &mut Machine, now: u64, effect: Effect, watch: &mut Watch, steer: &mut Steer) {
    match effect {
        Effect::Prepare { enforced } => {
            steer.choice.game = crate::net::TableGame::Riftbound;
            steer.choice.enforced = enforced;
            steer.choice.options = None;
            steer.tuning.bypass_change_detection().auto_pass = false;
        }
        Effect::HostTable => crate::net::host_table(&mut watch.info),
        Effect::Join(host) => match crate::net::join_by_ticket(&host) {
            Ok(_) => machine.joined(now),
            Err(error) => machine.fail(now, format!("join: {error}"), &watch.panel.view),
        },
        Effect::GoToTable => {
            if steer.menu.screen != crate::menu::Screen::Table {
                steer.menu.screen = crate::menu::Screen::Table;
                steer.menu.sheet = None;
            }
        }
        Effect::PinSide(slug) => steer.pinned.side = Some(slug),
        Effect::SeatDeck(slug) => match crate::deck::pool::deck(&slug) {
            Ok(deck) => {
                crate::deck::import::seat_deck(
                    crate::deck::import::ImportedDeck::Riftbound(deck),
                    watch.my_seat.0,
                    &mut steer.seated,
                    &mut steer.art,
                    None,
                );
                steer.import.auto_deal = true;
            }
            Err(error) => machine.fail(now, format!("deal: {error}"), &watch.panel.view),
        },
        Effect::SeatSaved(label) => match seat_saved(&label, watch.my_seat.0, steer) {
            Ok(_) => steer.import.auto_deal = true,
            Err(error) => machine.fail(now, format!("deck: {error}"), &watch.panel.view),
        },
        Effect::SetBattlefield(index) => {
            if let Some(record) = steer.seated.0.as_mut() {
                record.battlefield = Some(index);
                record.battlefield_played = false;
            }
        }
        Effect::Fire(index) => {
            if let Some(affordance) = watch.panel.view.affordances.get(index) {
                steer.sender.fire(affordance);
            }
        }
        Effect::Drop { card, zone, hidden } => {
            let to = agni_core::Zone::Plugin(zone);
            let seat = match agni_sim::wire::zone_owner(&watch.mirror.view.zones, to) {
                Some(agni_sim::wire::ZoneOwner::PerSeat) => watch.my_seat.0,
                _ => agni_core::PlayerId(0),
            };
            steer.drops.write(crate::table::CardDropped {
                card: agni_core::CardId(card),
                to,
                seat,
                index: watch.table.0.in_area(seat, to).count(),
                hidden,
            });
        }
        Effect::Rejoin(host) => crate::net::rejoin(&host),
        Effect::Leave => {
            crate::net::leave_session(&mut watch.info, &mut steer.host, &mut steer.client);
        }
        Effect::Exit(success) => {
            steer.exits.write(if success {
                AppExit::Success
            } else {
                AppExit::error()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_examples_round_trip_and_defaults_fill_in() {
        let host = Plan::example_host();
        let json = serde_json::to_string(&host).unwrap();
        assert_eq!(Plan::parse(&json).unwrap(), host);
        let join = Plan::example_join("abc");
        let json = serde_json::to_string(&join).unwrap();
        assert!(
            json.contains("\"role\":{\"join\":{\"host\":\"abc\"}}"),
            "{json}"
        );
        assert_eq!(Plan::parse(&json).unwrap(), join);
        let bare = Plan::parse(r#"{"role":"host"}"#).unwrap();
        assert_eq!(bare.deck_slug(), DEFAULT_HOST_DECK);
        assert_eq!(bare.player_name(), "autoplay-host");
        assert!(bare.enforced);
        assert_eq!(bare.brain, Brain::Random { seed: 0 });
        assert_eq!(bare.until, Until::Winner);
        assert_eq!(bare.timeout_s, DEFAULT_TIMEOUT_S);
        assert_eq!(bare.turn_cap, DEFAULT_TURN_CAP);
        assert_eq!(bare.pace_ms, DEFAULT_PACE_MS);
        assert_eq!(bare.players, DEFAULT_PLAYERS);
        assert!(bare.exits(true));
        assert!(!bare.exits(false));
        let joiner = Plan::parse(
            r#"{"role":{"join":{"host":"x"}},"until":{"turns":3},"exit":true,"deck":"nasus"}"#,
        )
        .unwrap();
        assert_eq!(joiner.deck_slug(), "nasus-thundertrees");
        assert_eq!(joiner.player_name(), "autoplay-join");
        assert_eq!(joiner.until, Until::Turns(3));
        assert_eq!(joiner.until.word(), "turns:3");
        assert!(joiner.exits(false));
        assert_eq!(joiner.host_target(), Some("x"));
    }

    #[test]
    fn a_saved_deck_is_named_by_label_and_plays_on_any_table() {
        let plan =
            Plan::parse(r#"{"role":"host","enforced":false,"deck":"saved:Lillia Aggro"}"#).unwrap();
        assert_eq!(
            plan.deck_choice().unwrap(),
            DeckChoice::Saved("Lillia Aggro".into())
        );
        assert_eq!(plan.deck_slug(), "saved:Lillia Aggro");
        let enforced = Plan::parse(r#"{"role":"host","deck":"saved:Lillia Aggro"}"#).unwrap();
        assert!(
            enforced.enforced,
            "every card is scripted, so a saved deck plays rules enforced too"
        );
        let blank =
            Plan::parse(r#"{"role":"host","enforced":false,"deck":"saved: "}"#).unwrap_err();
        assert!(blank.contains("label"), "{blank}");
        assert_eq!(
            DeckChoice::parse("nasus").unwrap(),
            DeckChoice::Pool("nasus-thundertrees".into())
        );
        let row = |label: &str, held: bool| crate::deck::history::SavedRow {
            ci: spirit_sdk::CiHash::from_hash(spirit_sdk::BlobHash::of(label.as_bytes())),
            label: label.into(),
            signer: None,
            held,
        };
        let rows = vec![
            row("Lillia Aggro", false),
            row("lillia aggro", true),
            row("Lillia Aggro ", true),
        ];
        assert_eq!(saved_row_by_label(&rows, "Lillia Aggro"), Some(rows[2].ci));
        assert_eq!(saved_row_by_label(&rows, "LILLIA AGGRO"), Some(rows[1].ci));
        assert_eq!(saved_row_by_label(&rows, "Nasus"), None);
    }

    #[test]
    fn a_typo_or_an_unknown_deck_is_refused_at_parse() {
        let error = Plan::parse(r#"{"role":"host","seeed":1}"#).unwrap_err();
        assert!(error.contains("unknown field"), "{error}");
        assert!(Plan::parse(r#"{"until":"winner"}"#)
            .unwrap_err()
            .contains("role"));
        let error = Plan::parse(r#"{"role":"host","deck":"zzz"}"#).unwrap_err();
        assert!(error.contains("no deck"), "{error}");
        assert!(Plan::parse(r#"{"role":"host","players":0}"#).is_err());
    }

    #[test]
    fn a_record_is_one_tagged_line_with_its_millis() {
        let record = Record {
            event: Event::Hosting {
                node: "n".into(),
                ticket: "t".into(),
                name: "h".into(),
                seat: 0,
            },
            ms: 12,
        };
        let line = record.line();
        assert_eq!(
            line,
            r#"{"event":"hosting","node":"n","ticket":"t","name":"h","seat":0,"ms":12}"#
        );
        assert_eq!(
            Record::parse(&format!("{EVENT_PREFIX}{line}")).unwrap(),
            record
        );
        let outcome = Record {
            event: Event::Outcome(Outcome {
                result: OutcomeKind::Winner,
                winner: Some(1),
                turns: 9,
                sent: 40,
                refused: 0,
                reason: None,
                status: vec!["turn 9 · seat 1 · action phase · rules enforced".into()],
            }),
            ms: 5000,
        };
        let line = outcome.line();
        assert!(
            line.starts_with(r#"{"event":"outcome","result":"winner","winner":1,"turns":9"#),
            "{line}"
        );
        assert!(!line.contains("reason"));
        assert_eq!(Record::parse(&line).unwrap(), outcome);
        let failed = Record {
            event: Event::Outcome(Outcome {
                result: OutcomeKind::Failed,
                winner: None,
                turns: 3,
                sent: 4,
                refused: 1,
                reason: Some("stuck".into()),
                status: Vec::new(),
            }),
            ms: 1,
        };
        assert!(failed.line().contains(r#""result":"failed""#));
        assert!(failed.line().contains(r#""reason":"stuck""#));
        assert_eq!(Record::parse(&failed.line()).unwrap(), failed);
    }

    fn outcome(result: OutcomeKind, winner: Option<u8>, turns: u32) -> Outcome {
        Outcome {
            result,
            winner,
            turns,
            sent: 0,
            refused: 0,
            reason: (result == OutcomeKind::Failed).then(|| "x".to_string()),
            status: Vec::new(),
        }
    }

    #[test]
    fn two_sides_agree_only_on_the_same_winner_and_turn_count() {
        let a = outcome(OutcomeKind::Winner, Some(1), 12);
        assert!(a.agree(&outcome(OutcomeKind::Winner, Some(1), 12)).is_ok());
        assert!(a
            .agree(&outcome(OutcomeKind::Winner, Some(0), 12))
            .unwrap_err()
            .contains("winners differ"));
        assert!(a
            .agree(&outcome(OutcomeKind::Winner, Some(1), 13))
            .unwrap_err()
            .contains("turn counts differ"));
        assert!(a
            .agree(&outcome(OutcomeKind::Failed, None, 3))
            .unwrap_err()
            .contains("failed"));
        assert!(a
            .agree(&outcome(OutcomeKind::Turns, Some(1), 12))
            .unwrap_err()
            .contains("results differ"));
        assert!(a.succeeded(&Until::Winner));
        assert!(a.succeeded(&Until::Turns(30)));
        assert!(!a.succeeded(&Until::Seated));
        assert!(outcome(OutcomeKind::Turns, None, 5).succeeded(&Until::Turns(5)));
        assert!(!outcome(OutcomeKind::Turns, None, 4).succeeded(&Until::Turns(5)));
        assert!(outcome(OutcomeKind::Seated, None, 0).succeeded(&Until::Seated));
        assert!(!outcome(OutcomeKind::Failed, None, 0).succeeded(&Until::Winner));
    }

    #[test]
    fn the_entry_reads_the_env_then_the_args_and_a_file_by_at() {
        let dir = std::env::temp_dir().join(format!("kai-autoplay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.json");
        std::fs::write(&path, r#"{"role":"host"}"#).unwrap();
        let text = plan_text(&format!("@{}", path.display())).unwrap();
        assert_eq!(text, r#"{"role":"host"}"#);
        assert!(plan_text("@/nonexistent/plan.json")
            .unwrap_err()
            .contains("plan file"));
        assert_eq!(
            plan_text(r#"{"role":"host"}"#).unwrap(),
            r#"{"role":"host"}"#
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seat_info(seat: u8, host: bool, connected: bool) -> SeatInfo {
        SeatInfo {
            seat,
            name: format!("seat {seat}"),
            host,
            connected,
            color: 0,
            playmat: None,
        }
    }

    fn observe<'a>(
        role: SessionRole,
        status: &'a str,
        roster: &'a [SeatInfo],
        view: &'a PluginView,
        recovery: &'a Recovery,
    ) -> Observed<'a> {
        Observed {
            node: Some(("node".into(), "ticket".into())),
            node_status: "serving".into(),
            role,
            status,
            recovery,
            roster,
            my_seat: 0,
            view,
            zones: &[],
            legends: TableLegends::default(),
            enforced_now: true,
            pinned_side: None,
            deck: None,
            refusal: None,
        }
    }

    #[test]
    fn the_battlefield_is_picked_at_the_table_in_any_phase_once_the_deck_asks() {
        let view = PluginView::default();
        let recovery = Recovery::Nothing;
        let present = [seat_info(0, true, true), seat_info(1, false, true)];
        let asks = DeckInfo {
            label: "Lillia (house)".into(),
            battlefields: 3,
            needs_choice: true,
            battlefield: None,
        };
        let mut joiner = Machine::new(Plan::example_join("h"), false);
        joiner.phase = Phase::Gathering;
        let mut obs = observe(SessionRole::Client, "seated", &present, &view, &recovery);
        obs.deck = Some(asks.clone());
        assert_eq!(
            joiner.step(10, &obs),
            vec![Effect::SetBattlefield(0)],
            "a joiner dealt before it could pick still picks after the deal"
        );
        joiner.phase = Phase::Rolling;
        assert_eq!(joiner.step(20, &obs), vec![Effect::SetBattlefield(0)]);
        obs.deck = Some(DeckInfo {
            needs_choice: false,
            battlefield: Some("Rockfall Path".into()),
            ..asks.clone()
        });
        assert!(
            !joiner
                .step(30, &obs)
                .iter()
                .any(|effect| matches!(effect, Effect::SetBattlefield(_))),
            "a chosen battlefield is not chosen again"
        );
        let mut far = Plan::example_join("h");
        far.battlefield = 7;
        let mut host = Machine::new(far, false);
        host.phase = Phase::Playing;
        let mut obs = observe(SessionRole::Host, "hosting", &present, &view, &recovery);
        obs.deck = Some(asks);
        let effects = host.step(40, &obs);
        assert!(effects.contains(&Effect::SetBattlefield(2)), "{effects:?}");
        let mut booting = Machine::new(Plan::example_join("h"), false);
        let mut obs = observe(SessionRole::Solo, "", &present, &view, &recovery);
        obs.deck = Some(DeckInfo {
            label: "x".into(),
            battlefields: 3,
            needs_choice: true,
            battlefield: None,
        });
        assert!(
            !booting
                .step(1, &obs)
                .iter()
                .any(|effect| matches!(effect, Effect::SetBattlefield(_))),
            "nothing is picked before the table is in sight"
        );
    }

    #[test]
    fn the_host_lingers_until_its_peers_are_gone_or_ten_seconds_pass() {
        let view = PluginView::default();
        let recovery = Recovery::Nothing;
        let present = [seat_info(0, true, true), seat_info(1, false, true)];
        let gone = [seat_info(0, true, true), seat_info(1, false, false)];
        let mut host = Machine::new(Plan::example_host(), false);
        host.finish(100, OutcomeKind::Seated, None, &view);
        let obs = observe(SessionRole::Host, "hosting", &present, &view, &recovery);
        assert!(host.step(100 + LINGER_MS, &obs).is_empty());
        assert!(host.step(100 + HOST_LINGER_MS - 1, &obs).is_empty());
        let obs = observe(SessionRole::Host, "hosting", &gone, &view, &recovery);
        assert_eq!(host.step(100 + LINGER_MS, &obs), vec![Effect::Leave]);
        assert!(host.finished());
        let mut seated = Plan::example_host();
        seated.until = Until::Seated;
        let mut slow = Machine::new(seated, true);
        slow.finish(0, OutcomeKind::Seated, None, &view);
        let obs = observe(SessionRole::Host, "hosting", &present, &view, &recovery);
        assert_eq!(
            slow.step(HOST_LINGER_MS, &obs),
            vec![Effect::Leave, Effect::Exit(true)]
        );
        let mut joiner = Machine::new(Plan::example_join("h"), false);
        joiner.finish(0, OutcomeKind::Seated, None, &view);
        let obs = observe(SessionRole::Client, "seated", &present, &view, &recovery);
        assert_eq!(joiner.step(LINGER_MS, &obs), vec![Effect::Leave]);
    }

    #[test]
    fn a_refused_rejoin_fails_instead_of_waiting_for_the_timeout() {
        let view = PluginView::default();
        let present = [seat_info(0, true, true), seat_info(1, false, true)];
        let mut joiner = Machine::new(Plan::example_join("h"), false);
        joiner.phase = Phase::Playing;
        let rejoin = Recovery::Rejoin { host: "h".into() };
        let obs = observe(
            SessionRole::Ended,
            "session over: peer closed",
            &present,
            &view,
            &rejoin,
        );
        assert_eq!(joiner.step(10, &obs), vec![Effect::Rejoin("h".into())]);
        let nothing = Recovery::Nothing;
        let obs = observe(
            SessionRole::Solo,
            "join failed: host refused: no open table",
            &present,
            &view,
            &nothing,
        );
        assert!(joiner.step(20, &obs).is_empty());
        let outcome = joiner.outcome().expect("a failed outcome");
        assert_eq!(outcome.result, OutcomeKind::Failed);
        assert_eq!(
            outcome.reason.as_deref(),
            Some("session ended: join failed: host refused: no open table")
        );
    }

    #[test]
    fn a_seat_with_no_options_and_no_fold_for_a_minute_is_stuck() {
        let view = PluginView::default();
        let present = [seat_info(0, true, true), seat_info(1, false, true)];
        let nothing = Recovery::Nothing;
        let mut host = Machine::new(Plan::example_host(), false);
        host.phase = Phase::Playing;
        let obs = observe(SessionRole::Host, "hosting", &present, &view, &nothing);
        assert!(host.step(1, &obs).is_empty());
        assert!(host.step(IDLE_FAIL_MS, &obs).is_empty());
        assert!(host.outcome().is_none());
        assert!(host.step(IDLE_FAIL_MS + 1, &obs).is_empty());
        let outcome = host.outcome().expect("stuck");
        assert_eq!(outcome.result, OutcomeKind::Failed);
        assert!(outcome
            .reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("stuck")));
    }

    #[test]
    fn the_lobby_filter_never_switches_modes_by_accident() {
        use agni_sim::wire::{Affordance, AffordanceKind};
        use serde_bytes::ByteBuf;
        let offer = |label: &str, kind: AffordanceKind| Affordance {
            label: label.into(),
            hotkey: None,
            enabled: true,
            kind,
            data: ByteBuf::from(vec![1]),
            card: None,
        };
        let mut view = PluginView {
            affordances: vec![
                offer("go first", AffordanceKind::Plain),
                offer("let {seat 1} go first", AffordanceKind::Plain),
                offer("switch to free table", AffordanceKind::Plain),
            ],
            status: vec!["you won the roll".into(), "mode: rules enforced".into()],
            ..PluginView::default()
        };
        let labels = |choices: Vec<Choice>| -> Vec<String> {
            choices
                .into_iter()
                .map(|choice| match choice {
                    Choice::Action { label, .. } => label,
                    Choice::Move { .. } => "move".into(),
                })
                .collect()
        };
        assert_eq!(
            labels(lobby_options(&view, 0, true, true)),
            vec!["go first", "let {seat 1} go first"]
        );
        assert!(lobby_options(&view, 0, true, false).is_empty());
        view.affordances[2] = offer(SWITCH_TO_ENFORCED, AffordanceKind::Plain);
        view.status[1] = FREE_TABLE_LINE.into();
        assert_eq!(
            labels(lobby_options(&view, 0, true, true)),
            vec![SWITCH_TO_ENFORCED]
        );
        assert_eq!(
            labels(lobby_options(&view, 0, false, true)),
            vec!["go first", "let {seat 1} go first"]
        );
        view.affordances = vec![
            offer("roll", AffordanceKind::Commit { roll: 1 }),
            offer("free table", AffordanceKind::Plain),
        ];
        assert_eq!(labels(lobby_options(&view, 0, true, false)), vec!["roll"]);
        assert_eq!(labels(game_options(&view, 0)), vec!["roll"]);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod game_tests {
    use super::*;
    use crate::deck::import::{face_map, ImportedDeck, SeatedDeckRecord};
    use crate::table::plugin_ui::{action_bytes, RollSecrets};
    use agni_core::PlayerId;
    use agni_engine_host::{load_plugin, PLUGIN_GAS_BUDGET};
    use agni_net::session::{ClientSession, HostSession, WireIntent};
    use agni_sim::engine::PluginModule;
    use agni_sim::log::{LogEntry, TableConfig};
    use agni_sim::wire::{AffordanceKind, ZoneOwner};
    use serde_bytes::ByteBuf;
    use std::path::PathBuf;

    const TICK_MS: u64 = 20;
    const MAX_TICKS: u64 = 200_000;

    fn plugin() -> Box<dyn PluginModule> {
        let path = std::env::var("AGNI_RIFTBOUND_WASM")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/plugins/riftbound.wasm")
            });
        let bytes = std::fs::read(&path)
            .expect("the riftbound plugin is built (plugin-build) or named by AGNI_RIFTBOUND_WASM");
        Box::new(load_plugin(&bytes, PLUGIN_GAS_BUDGET).expect("the plugin loads"))
    }

    struct Side {
        machine: Machine,
        seat: u8,
        role: SessionRole,
        status: String,
        secrets: RollSecrets,
        deck: Option<SeatedDeckRecord>,
        pinned_side: Option<String>,
        auto_deal: bool,
        refusal: Option<(String, f64)>,
        refusals: u32,
        replica: Option<ClientSession>,
        entropy: Rng,
        left: bool,
        exit: Option<bool>,
        cached: Option<(usize, SessionRole, PluginView)>,
    }

    impl Side {
        fn new(plan: Plan, seat: u8) -> Self {
            Self {
                machine: Machine::new(plan, true),
                seat,
                role: SessionRole::Solo,
                status: String::new(),
                secrets: RollSecrets::default(),
                deck: None,
                pinned_side: None,
                auto_deal: false,
                refusal: None,
                refusals: 0,
                replica: None,
                entropy: Rng::seeded(1000 + u64::from(seat)),
                left: false,
                exit: None,
                cached: None,
            }
        }

        fn refuse(&mut self, text: String) {
            self.refusals += 1;
            self.refusal = Some((text, f64::from(self.refusals)));
        }

        fn seat_pool_deck(&mut self, slug: &str) {
            let deck = crate::deck::pool::deck(slug).expect("a pool deck");
            let deck = ImportedDeck::Riftbound(deck);
            self.deck = Some(SeatedDeckRecord {
                seat: PlayerId(self.seat),
                faces: face_map(&deck),
                deck,
                battlefield: None,
                battlefield_played: false,
            });
            self.auto_deal = true;
        }
    }

    struct Harness {
        host: HostSession,
        sides: Vec<Side>,
        now: u64,
    }

    impl Harness {
        fn open(seed: u64) -> Self {
            let (engine, _) = crate::engine::session_engine();
            let host = HostSession::with_engine(
                "autoplay-host",
                TableConfig {
                    zones: agni_riftbound::zone_table(),
                    counters: agni_riftbound::counter_table(),
                    options: agni_riftbound::TableOptions::genesis(None, true).map(ByteBuf::from),
                    ..TableConfig::default()
                },
                engine,
                Some(plugin()),
            )
            .expect("the host opens");
            let mut host_plan = Plan::example_host();
            host_plan.brain = Brain::Random { seed };
            host_plan.pace_ms = 0;
            let mut join_plan = Plan::example_join("host-node");
            join_plan.brain = Brain::Random { seed: seed + 1 };
            join_plan.pace_ms = 0;
            Self {
                host,
                sides: vec![Side::new(host_plan, 0), Side::new(join_plan, 1)],
                now: 0,
            }
        }

        fn relay(&mut self, entries: &[LogEntry]) {
            for side in &mut self.sides {
                if let Some(replica) = side.replica.as_mut() {
                    for entry in entries {
                        replica
                            .apply(entry.clone())
                            .expect("the replica folds every entry");
                    }
                }
            }
        }

        fn view(&mut self, index: usize) -> PluginView {
            let key = (self.host.log().len(), self.sides[index].role);
            if let Some((len, role, view)) = &self.sides[index].cached {
                if (*len, *role) == key {
                    return view.clone();
                }
            }
            let seat = self.sides[index].seat;
            let view = match self.sides[index].replica.as_mut() {
                Some(replica) => replica.plugin_view(seat),
                None => self.host.plugin_view(seat),
            };
            self.sides[index].cached = Some((key.0, key.1, view.clone()));
            view
        }

        fn table(&self, index: usize) -> agni_core::Table {
            match self.sides[index].replica.as_ref() {
                Some(replica) => replica.table(),
                None => self.host.table(),
            }
        }

        fn intent(&mut self, index: usize, intent: WireIntent) {
            let seat = self.sides[index].seat;
            match self.host.intent(seat, intent) {
                Ok(entries) => self.relay(&entries),
                Err(error) => self.sides[index].refuse(error.to_string()),
            }
        }

        fn reveal_all(&mut self) {
            for index in 0..self.sides.len() {
                loop {
                    let view = self.view(index);
                    let Some(reveal) = view.affordances.iter().position(|affordance| {
                        affordance.enabled
                            && matches!(affordance.kind, AffordanceKind::Reveal { .. })
                    }) else {
                        break;
                    };
                    let secret = self.sides[index].entropy.secret();
                    let fresh = move || secret;
                    let Some(data) = action_bytes(
                        &view.affordances[reveal],
                        &mut self.sides[index].secrets,
                        &fresh,
                    ) else {
                        break;
                    };
                    self.intent(
                        index,
                        WireIntent::Game {
                            data: ByteBuf::from(data),
                        },
                    );
                }
            }
        }

        fn deal_if_ready(&mut self, index: usize) {
            let seat = self.sides[index].seat;
            let live = matches!(
                self.sides[index].role,
                SessionRole::Host | SessionRole::Client
            );
            let ready = self.sides[index].deck.as_ref().is_some_and(|record| {
                live && self.sides[index].auto_deal
                    && !crate::deck::battlefield::needs_choice(record)
            });
            if !ready {
                return;
            }
            let record = self.sides[index].deck.as_ref().expect("checked above");
            let groups = crate::deck::import::deal_plan_for(record);
            let (entries, faces) = self.host.deal_groups(seat, groups).expect("the deal lands");
            self.relay(&entries);
            if let Some(replica) = self.sides[index].replica.as_mut() {
                replica.add_faces(faces);
            }
            self.sides[index].auto_deal = false;
        }

        fn apply(&mut self, index: usize, effect: Effect) {
            match effect {
                Effect::Prepare { .. } | Effect::GoToTable => {}
                Effect::HostTable => {
                    self.sides[index].role = SessionRole::Host;
                    self.sides[index].status = "hosting — table visible to your peers".into();
                }
                Effect::Join(_) => {
                    let now = self.now;
                    self.sides[index].machine.joined(now);
                    self.sides[index].role = SessionRole::Joining;
                    self.sides[index].status = "joining table over iroh…".into();
                }
                Effect::PinSide(slug) => {
                    self.sides[index].pinned_side = Some(slug.clone());
                    self.sides[index].seat_pool_deck(&slug);
                }
                Effect::SeatDeck(slug) => self.sides[index].seat_pool_deck(&slug),
                Effect::SeatSaved(label) => {
                    self.sides[index].refuse(format!("no saved deck {label:?} in the harness"))
                }
                Effect::SetBattlefield(battlefield) => {
                    if let Some(record) = self.sides[index].deck.as_mut() {
                        record.battlefield = Some(battlefield);
                    }
                }
                Effect::Fire(affordance) => {
                    let view = self.view(index);
                    let secret = self.sides[index].entropy.secret();
                    let fresh = move || secret;
                    let Some(data) = action_bytes(
                        &view.affordances[affordance],
                        &mut self.sides[index].secrets,
                        &fresh,
                    ) else {
                        self.sides[index].refuse("no bytes for the affordance".into());
                        return;
                    };
                    self.intent(
                        index,
                        WireIntent::Game {
                            data: ByteBuf::from(data),
                        },
                    );
                }
                Effect::Drop { card, zone, hidden } => {
                    let to = agni_core::Zone::Plugin(zone);
                    let seat = match agni_sim::wire::zone_owner(&self.host.view().zones, to) {
                        Some(ZoneOwner::PerSeat) => self.sides[index].seat,
                        _ => 0,
                    };
                    let target = self.table(index).in_area(PlayerId(seat), to).count() as u32;
                    let intent = if hidden {
                        WireIntent::MoveHidden {
                            card,
                            to,
                            seat,
                            index: target,
                        }
                    } else {
                        WireIntent::Move {
                            card,
                            to,
                            seat,
                            index: target,
                        }
                    };
                    self.intent(index, intent);
                }
                Effect::Rejoin(_) => panic!("no rejoin is expected in process"),
                Effect::Leave => self.sides[index].left = true,
                Effect::Exit(success) => self.sides[index].exit = Some(success),
            }
        }

        fn seat_joiner(&mut self, index: usize) {
            let name = self.sides[index].machine.plan().player_name();
            let (seat, _) = self.host.join(&name).expect("the joiner takes a seat");
            assert_eq!(seat, self.sides[index].seat);
            let (engine, _) = crate::engine::session_engine();
            let replica = ClientSession::from_welcome_with(
                seat,
                self.host.roster(),
                self.host.log().to_vec(),
                engine,
                Some(plugin()),
            )
            .expect("the replica folds the welcome");
            self.sides[index].replica = Some(replica);
            self.sides[index].role = SessionRole::Client;
            self.sides[index].status = format!("seated as player {}", seat + 1);
        }

        fn tick(&mut self) {
            self.now += TICK_MS;
            for index in 0..self.sides.len() {
                if self.sides[index].role == SessionRole::Joining {
                    self.seat_joiner(index);
                }
                self.deal_if_ready(index);
                let view = self.view(index);
                let zones = self.host.view().zones.clone();
                let roster = self.host.roster();
                let legends = crate::deck::pinned::legends_on_table(
                    &self.table(index),
                    PlayerId(self.sides[index].seat),
                );
                let side = &mut self.sides[index];
                let observed = Observed {
                    node: Some((
                        format!("node-{}", side.seat),
                        format!("ticket-{}", side.seat),
                    )),
                    node_status: "serving".into(),
                    role: side.role,
                    status: &side.status,
                    recovery: &Recovery::Nothing,
                    roster: &roster,
                    my_seat: side.seat,
                    view: &view,
                    zones: &zones,
                    legends,
                    enforced_now: side.machine.plan().enforced,
                    pinned_side: side.pinned_side.clone(),
                    deck: side.deck.as_ref().map(deck_info_of),
                    refusal: side.refusal.clone(),
                };
                let effects = side.machine.step(self.now, &observed);
                for effect in effects {
                    self.apply(index, effect);
                }
            }
            self.reveal_all();
        }

        fn finished(&self) -> bool {
            self.sides.iter().all(|side| side.machine.finished())
        }

        fn events(&self, index: usize, name: &str) -> Vec<&Record> {
            self.sides[index]
                .machine
                .events
                .iter()
                .filter(|record| {
                    serde_json::to_value(&record.event)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("event")
                                .and_then(|tag| tag.as_str().map(str::to_string))
                        })
                        .as_deref()
                        == Some(name)
                })
                .collect()
        }
    }

    #[test]
    fn host_and_joiner_autoplay_in_process_to_the_same_winner_and_turn_count() {
        let mut harness = Harness::open(7);
        let mut ticks = 0;
        while !harness.finished() {
            harness.tick();
            ticks += 1;
            assert!(
                ticks < MAX_TICKS,
                "the game did not finish in {MAX_TICKS} ticks"
            );
        }
        let host = harness.sides[0]
            .machine
            .outcome()
            .expect("host outcome")
            .clone();
        let joiner = harness.sides[1]
            .machine
            .outcome()
            .expect("joiner outcome")
            .clone();
        assert_eq!(
            host.result,
            OutcomeKind::Winner,
            "host: {:?} after {:?}",
            host.reason,
            harness.sides[0].machine.events.last()
        );
        assert_eq!(
            joiner.result,
            OutcomeKind::Winner,
            "joiner: {:?}",
            joiner.reason
        );
        host.agree(&joiner).expect("both replicas agree");
        assert!(host.turns >= 1 && host.turns <= DEFAULT_TURN_CAP);
        assert!(host.sent > 0 && joiner.sent > 0);
        assert!(
            host.refused <= crate::ai::soak::MAX_REFUSALS,
            "host refusals {}",
            host.refused
        );
        assert!(
            joiner.refused <= crate::ai::soak::MAX_REFUSALS,
            "joiner refusals {}",
            joiner.refused
        );
        for (index, role) in [(0, "host"), (1, "client")] {
            assert_eq!(harness.events(index, "meshed").len(), 1);
            let seated = harness.events(index, "seated");
            assert_eq!(seated.len(), 1);
            assert_eq!(
                seated[0].event,
                Event::Seated {
                    seat: index as u8,
                    role: role.into()
                }
            );
            assert_eq!(harness.events(index, "dealt").len(), 1);
            let started = harness.events(index, "started");
            assert_eq!(started.len(), 1);
            assert!(matches!(
                started[0].event,
                Event::Started { enforced: true, .. }
            ));
            assert_eq!(harness.events(index, "outcome").len(), 1);
            let turns = harness.events(index, "turn");
            assert!(
                matches!(turns.last().map(|record| &record.event), Some(Event::Turn { n }) if *n == host.turns)
            );
            assert!(
                harness.events(index, "warn").is_empty(),
                "{:?}",
                harness.events(index, "warn")
            );
            assert!(harness.sides[index].left);
            assert_eq!(harness.sides[index].exit, Some(true));
        }
        assert_eq!(harness.events(0, "hosting").len(), 1);
        assert_eq!(harness.events(1, "joining").len(), 1);
        let roster = harness.events(1, "roster");
        assert!(matches!(
            &roster.last().expect("a roster").event,
            Event::Roster { seats } if seats.len() == 2 && seats.iter().all(|seat| seat.connected)
        ));
    }

    #[test]
    fn until_seated_stops_once_both_seats_are_connected() {
        let mut harness = Harness::open(3);
        for side in &mut harness.sides {
            let mut plan = side.machine.plan().clone();
            plan.until = Until::Seated;
            side.machine = Machine::new(plan, false);
        }
        let mut ticks = 0;
        while !harness.finished() {
            harness.tick();
            ticks += 1;
            assert!(ticks < 10_000);
        }
        for index in 0..2 {
            let outcome = harness.sides[index].machine.outcome().expect("an outcome");
            assert_eq!(outcome.result, OutcomeKind::Seated, "{:?}", outcome.reason);
            assert_eq!(outcome.sent, 0);
            assert!(harness.sides[index].left);
            assert_eq!(harness.sides[index].exit, None);
        }
    }
}
