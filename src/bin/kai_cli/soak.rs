use agni_core::{PlayerId, Zone};
use agni_engine_host::{load_plugin, StoreSource, PLUGIN_GAS_BUDGET};
use agni_net::session::{ClientSession, HostSession, WireIntent};
use agni_sim::engine::{Engine, NativeEngine, PluginModule};
use agni_sim::log::TableConfig;
use agni_sim::wire::{AffordanceKind, CounterTarget, PluginView, ZoneDecl, ZoneKind, ZoneOwner};
use kai::ai::driver::{
    actionable, auto_choice, catalog_dir, decision_key, fallback, hold_dropped, load_deck,
    saved_deck, set_catalog_dir, snapshot, Mind, MindKind, Out, Seat,
};
use kai::ai::hold::{self, Pilot, Step};
use kai::ai::nanogpt::Reply;
use kai::ai::random::{self, Choice, Rng};
use kai::ai::soak::{
    turn_of, Ending, GameRecord, Summary, DEFAULT_GAMES, DEFAULT_TURN_CAP, MAX_REFUSALS,
};
use kai::deck::import::{deal_plan_for, face_map, ImportedDeck, SeatedDeckRecord};
use kai::table::plugin_ui::action_bytes;
use serde_bytes::ByteBuf;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const MAX_ROLL_ROUNDS: usize = 64;
const MAX_STEPS: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineChoice {
    Native,
    Wasm,
}

pub struct SoakArgs {
    pub games: u32,
    pub start: u32,
    pub seed: u64,
    pub turn_cap: u32,
    pub decks: [String; 2],
    pub brains: [MindKind; 2],
    pub model: String,
    pub out: PathBuf,
    pub trace: Option<String>,
    pub engine: EngineChoice,
    pub catalog: Option<String>,
    pub jobs: usize,
}

fn usage() -> ! {
    eprintln!(
        "kai-cli soak — self-play games between two saved decks, headless and in-process\n\n\
         usage: kai-cli soak [--games <n>] [--start <game number>] [--seed <u64>] [--turn-cap <n>]\n\
                             [--deck-a <saved deck label|pool:lillia|file|url>] [--deck-b <…>]\n\
                             [--brain-a random|auto|nanogpt] [--brain-b random|auto|nanogpt] [--model <nanogpt model>]\n\
                             [--out <jsonl file>] [--trace <file|->] [--engine native|wasm]\n\
                             [--jobs <parallel games, default one per core>]\n\
                             [--catalog <store dir with the saved decks and the card set>]\n\n\
         The auto brain is the random brain with kai's desktop auto-pilot sending the forced passes and\n\
         picks first; it plays the same game as random on the same seed and faults if it ever fires\n\
         while the seat had another option.\n\
         Deck A and deck B swap seats every game and the first player alternates every two games,\n\
         so over four games each deck sits in each seat and goes first once from each. One JSON line\n\
         per game lands in --out (default soak.jsonl); the summary exits 1 when any game faulted or\n\
         got stuck, or could not be set up. A game replays with the same --seed, --turn-cap, --engine\n\
         and decks and --start <its number> --games 1; each JSON line carries those flags."
    );
    std::process::exit(2)
}

pub fn parse_args(args: &[String]) -> SoakArgs {
    let mut parsed = SoakArgs {
        games: DEFAULT_GAMES,
        start: 1,
        seed: 0x5eed_0000_0000_0000,
        turn_cap: DEFAULT_TURN_CAP,
        decks: ["lillia".into(), "irelia".into()],
        brains: [MindKind::Random, MindKind::Random],
        model: kai::ai::nanogpt::DEFAULT_MODEL.to_string(),
        out: PathBuf::from("soak.jsonl"),
        trace: None,
        engine: EngineChoice::Native,
        catalog: None,
        jobs: std::thread::available_parallelism()
            .map(|cores| cores.get())
            .unwrap_or(1),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--games" => parsed.games = value().parse().unwrap_or_else(|_| usage()),
            "--start" => parsed.start = value().parse().unwrap_or_else(|_| usage()),
            "--seed" => parsed.seed = value().parse().unwrap_or_else(|_| usage()),
            "--turn-cap" => parsed.turn_cap = value().parse().unwrap_or_else(|_| usage()),
            "--deck-a" => parsed.decks[0] = value(),
            "--deck-b" => parsed.decks[1] = value(),
            "--brain-a" => parsed.brains[0] = MindKind::parse(&value()).unwrap_or_else(|| usage()),
            "--brain-b" => parsed.brains[1] = MindKind::parse(&value()).unwrap_or_else(|| usage()),
            "--model" => parsed.model = value(),
            "--out" => parsed.out = PathBuf::from(value()),
            "--trace" => parsed.trace = Some(value()),
            "--engine" => {
                parsed.engine = match value().as_str() {
                    "native" => EngineChoice::Native,
                    "wasm" => EngineChoice::Wasm,
                    _ => usage(),
                }
            }
            "--catalog" => parsed.catalog = Some(value()),
            "--jobs" => parsed.jobs = value().parse().unwrap_or_else(|_| usage()),
            _ => usage(),
        }
    }
    if parsed.games == 0 || parsed.start == 0 || parsed.jobs == 0 {
        usage();
    }
    parsed.jobs = parsed.jobs.min(parsed.games as usize);
    parsed
}

pub struct Modules {
    plugin: Vec<u8>,
    engine: EngineChoice,
    pub note: String,
}

impl Modules {
    pub fn resolve(engine: EngineChoice) -> Result<Self, String> {
        let (plugin, note) = plugin_bytes()?;
        Ok(Self {
            plugin,
            engine,
            note,
        })
    }

    fn engine(&self) -> Box<dyn Engine> {
        match self.engine {
            EngineChoice::Native => Box::new(NativeEngine::new()),
            EngineChoice::Wasm => kai::engine::session_engine().0,
        }
    }

    pub fn engine_label(&self) -> &'static str {
        match self.engine {
            EngineChoice::Native => "native",
            EngineChoice::Wasm => "wasm",
        }
    }

    fn plugin(&self) -> Result<Box<dyn PluginModule>, String> {
        load_plugin(&self.plugin, PLUGIN_GAS_BUDGET)
            .map(|plugin| Box::new(plugin) as Box<dyn PluginModule>)
            .map_err(|fault| format!("the riftbound plugin refused to load: {fault}"))
    }
}

fn plugin_bytes() -> Result<(Vec<u8>, String), String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("AGNI_RIFTBOUND_WASM") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(root) = std::env::var("BEVY_ASSET_ROOT") {
        candidates.push(PathBuf::from(root).join("assets/plugins/riftbound.wasm"));
    }
    candidates.push(PathBuf::from("assets/plugins/riftbound.wasm"));
    candidates
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/plugins/riftbound.wasm"));
    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Ok((bytes, format!("plugin from {}", path.display())));
        }
    }
    if let Some(dir) = catalog_dir() {
        if let Ok((version, module)) = StoreSource::new(&dir, "riftbound").load() {
            return Ok((
                module.bytes,
                format!(
                    "plugin modules/riftbound {} from the store",
                    version.module.version
                ),
            ));
        }
    }
    Err("no riftbound plugin: set AGNI_RIFTBOUND_WASM, run plugin-build in the kai shell, or seed the store".into())
}

pub struct Player {
    pub label: String,
    pub deck: ImportedDeck,
    pub brain: MindKind,
    pub script: Option<Vec<Reply>>,
}

impl Player {
    pub fn brain_label(&self, model: &str) -> String {
        match self.brain {
            MindKind::Llm => format!("nanogpt:{model}"),
            other => other.label().to_string(),
        }
    }

    fn mind(&self, model: &str) -> Mind {
        match self.brain {
            MindKind::Random => Mind::Random,
            MindKind::Auto => Mind::Auto,
            MindKind::Llm => {
                let mut cards = catalog_dir()
                    .as_deref()
                    .map(kai::ai::cards::CardTexts::load)
                    .unwrap_or_default();
                cards.absorb(kai::ai::cards::CardTexts::pool());
                let client = match &self.script {
                    Some(replies) => kai::ai::nanogpt::Client::canned(model, replies.clone()),
                    None => kai::ai::nanogpt::Client::new(model),
                };
                Mind::Llm(Box::new(kai::ai::brain::Brain::new(client, cards, None)))
            }
        }
    }
}

pub fn resolve_deck(source: &str) -> Result<(String, ImportedDeck), String> {
    if let Some(which) = source.strip_prefix("pool:") {
        let found = kai::deck::pool::find(which).map_err(|error| error.to_string())?;
        let deck = kai::deck::pool::deck(&found.slug).map_err(|error| error.to_string())?;
        return Ok((found.label, ImportedDeck::Riftbound(deck)));
    }
    if let Some(deck) = saved_deck(source) {
        let label = kai::deck::history::label(&deck);
        return Ok((label, deck));
    }
    let mut scratch = Seat::new();
    let mut quiet = Out::capture();
    load_deck(&mut scratch, source, &mut quiet);
    match scratch.deck {
        Some(record) => {
            let label = kai::deck::history::label(&record.deck);
            Ok((label, record.deck))
        }
        None => Err(format!(
            "no deck for {source}: not a saved deck label, a pool: deck, a file or a link ({})",
            quiet.take().join(" / ")
        )),
    }
}

pub fn player(source: &str, brain: MindKind) -> Result<Player, String> {
    let (label, deck) = resolve_deck(source)?;
    Ok(Player {
        label,
        deck,
        brain,
        script: None,
    })
}

pub fn layout(game: u32) -> ([usize; 2], u8) {
    let index = game.saturating_sub(1);
    let seats = if index.is_multiple_of(2) {
        [0, 1]
    } else {
        [1, 0]
    };
    let first = ((index / 2) % 2) as u8;
    (seats, first)
}

enum Applied {
    Folded(usize),
    Refused(String),
}

pub struct Table {
    host: HostSession,
    seats: Vec<Seat>,
    zones: Vec<ZoneDecl>,
    rng: Rng,
    trace: Out,
    failure: Option<Ending>,
    refusals: u32,
    last_refusal: String,
    last_actor: usize,
    pub auto_fired: u32,
    pub quiet_steps: u32,
    pub brain_calls: u32,
    pub brain_calls_while_quiet: u32,
    pub held_wakes: u32,
    pub holds: u32,
}

impl Table {
    fn open(modules: &Modules, names: [&str; 2], seed: u64, trace: Out) -> Result<Self, String> {
        let mut host = HostSession::with_engine(
            names[0],
            TableConfig {
                zones: agni_riftbound::zone_table(),
                counters: agni_riftbound::counter_table(),
                ..TableConfig::default()
            },
            modules.engine(),
            Some(modules.plugin()?),
        )
        .map_err(|error| format!("the host could not open: {error}"))?;
        let (joined, _) = host
            .join(names[1])
            .map_err(|error| format!("seat 1 could not join: {error}"))?;
        if joined != 1 {
            return Err(format!("the joiner took seat {joined}, not 1"));
        }
        let zones = host.view().zones.clone();
        let mut seats = Vec::new();
        for seat_no in 0..2u8 {
            let session = ClientSession::from_welcome_with(
                seat_no,
                host.roster(),
                host.log().to_vec(),
                modules.engine(),
                Some(modules.plugin()?),
            )
            .map_err(|fault| format!("seat {seat_no}'s replica failed: {fault}"))?;
            let mut seat = Seat::new();
            seat.session = Some(session);
            seat.seat = seat_no;
            seat.roster = host.roster();
            seats.push(seat);
        }
        Ok(Self {
            host,
            seats,
            zones,
            rng: Rng::seeded(seed),
            trace,
            failure: None,
            refusals: 0,
            last_refusal: String::new(),
            last_actor: 0,
            auto_fired: 0,
            quiet_steps: 0,
            brain_calls: 0,
            brain_calls_while_quiet: 0,
            held_wakes: 0,
            holds: 0,
        })
    }

    fn client(&mut self, seat: usize) -> &mut ClientSession {
        self.seats[seat]
            .session
            .as_mut()
            .expect("every soak seat holds a replica")
    }

    fn relay(&mut self, entries: &[agni_sim::log::LogEntry]) -> Result<(), Ending> {
        for entry in entries {
            for seat in 0..2 {
                match self.client(seat).apply(entry.clone()) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(Ending::EngineFault(format!(
                            "seat {seat}'s replica refused entry {} the host folded",
                            entry.seq
                        )))
                    }
                    Err(fault) => {
                        return Err(Ending::EngineFault(format!(
                            "seat {seat}'s replica failed: {fault}"
                        )))
                    }
                }
            }
        }
        for (seat, face) in self.host.owed_faces() {
            if usize::from(seat) < self.seats.len() {
                self.client(usize::from(seat)).add_faces(vec![face]);
            }
        }
        let expected = self.host.view().plugin_state.clone();
        for seat in 0..2 {
            if self.client(seat).view().plugin_state != expected {
                return Err(Ending::EngineFault(format!(
                    "seat {seat}'s replica folded a different blob than the host at seq {}",
                    self.host.view().next_seq
                )));
            }
        }
        Ok(())
    }

    fn intent(&mut self, seat: usize, intent: WireIntent, what: &str) -> Result<Applied, Ending> {
        self.trace.line(format!(
            "seat {seat} ({}): {what}",
            self.seats[seat].seat_name(seat as u8)
        ));
        match self.host.intent(seat as u8, intent) {
            Ok(entries) => {
                self.relay(&entries)?;
                self.refusals = 0;
                Ok(Applied::Folded(entries.len()))
            }
            Err(error) => {
                let applied = error.applied().to_vec();
                if !applied.is_empty() {
                    self.relay(&applied)?;
                }
                if error.is_engine_fault() {
                    return Err(Ending::EngineFault(format!("{what}: {error}")));
                }
                let reason = self.seats[seat].expand_notice(&error.to_string());
                self.trace.line(format!("  refused: {reason}"));
                self.seats[seat].notices.push(format!("refused: {reason}"));
                self.last_refusal = format!("{what}: {reason}");
                Ok(Applied::Refused(reason))
            }
        }
    }

    fn deal(&mut self, seat: usize, deck: &ImportedDeck) -> Result<(), String> {
        let faces = face_map(deck);
        let mut record = SeatedDeckRecord {
            seat: PlayerId(seat as u8),
            deck: deck.clone(),
            faces,
            battlefield: None,
            battlefield_played: false,
        };
        let held = kai::deck::battlefield::held(kai::deck::battlefield::options(&record));
        if held > 0 {
            record.battlefield = Some(self.rng.below(held));
        }
        let mut groups = deal_plan_for(&record);
        for group in &mut groups {
            if group.shuffle {
                self.rng.shuffle(&mut group.faces);
                group.shuffle = false;
            }
        }
        let (entries, owner_faces) = self
            .host
            .deal_groups(seat as u8, groups)
            .map_err(|error| format!("seat {seat}'s deal failed: {error}"))?;
        self.relay(&entries)
            .map_err(|ending| ending.detail().to_string())?;
        self.client(seat).add_faces(owner_faces);
        self.seats[seat].deck = Some(record);
        self.seats[seat].dealt = true;
        Ok(())
    }

    fn view(&mut self, seat: usize) -> PluginView {
        let view = self.client(seat).plugin_view(seat as u8);
        self.seats[seat].last_view = view.clone();
        view
    }

    fn refuse(&mut self, seat: usize, reason: String) -> Result<Applied, Ending> {
        self.trace.line(format!("  refused: {reason}"));
        self.seats[seat].notices.push(format!("refused: {reason}"));
        Ok(Applied::Refused(reason))
    }

    fn press(&mut self, seat: usize, index: usize) -> Result<Applied, Ending> {
        let Some(affordance) = self.seats[seat].last_view.affordances.get(index).cloned() else {
            return self.refuse(
                seat,
                format!("no action {} — `state` lists them", index + 1),
            );
        };
        if random::PANIC_LABELS.contains(&affordance.label.as_str())
            || self.seats[seat].last_view.is_hidden(index)
        {
            return self.refuse(
                seat,
                format!(
                    "{}: the soak table keeps the rules enforced",
                    affordance.label
                ),
            );
        }
        if !affordance.enabled {
            return self.refuse(
                seat,
                format!("{} is not available right now", affordance.label),
            );
        }
        let secret = self.rng.secret();
        let fresh = move || secret;
        let Some(data) = action_bytes(&affordance, &mut self.seats[seat].secrets, &fresh) else {
            return self.refuse(
                seat,
                format!("{} is not available right now", affordance.label),
            );
        };
        let what = format!("do {} ({})", index + 1, affordance.label);
        self.intent(
            seat,
            WireIntent::Game {
                data: ByteBuf::from(data),
            },
            &what,
        )
    }

    fn ready_view(&mut self, seat: usize) -> Result<(PluginView, bool), Ending> {
        let mut revealed = false;
        loop {
            let view = self.view(seat);
            let reveal = view.affordances.iter().position(|affordance| {
                affordance.enabled && matches!(affordance.kind, AffordanceKind::Reveal { .. })
            });
            match reveal {
                Some(index) => {
                    self.press(seat, index)?;
                    revealed = true;
                }
                None => return Ok((view, revealed)),
            }
        }
    }

    fn auto_reveal(&mut self) -> Result<(), Ending> {
        for seat in 0..2 {
            self.ready_view(seat)?;
        }
        Ok(())
    }

    fn press_labelled(&mut self, seat: usize, label: &str) -> Result<bool, Ending> {
        let view = self.view(seat);
        let Some(index) = view
            .affordances
            .iter()
            .position(|affordance| affordance.enabled && affordance.label == label)
        else {
            return Ok(false);
        };
        self.press(seat, index)?;
        Ok(true)
    }

    fn roll_winner(&mut self) -> Option<usize> {
        (0..2).find(|&seat| {
            self.view(seat).affordances.iter().any(|affordance| {
                affordance.label == "go first" || affordance.label.starts_with("let {seat ")
            })
        })
    }

    fn start(&mut self, first: u8) -> Result<(), String> {
        let fail = |ending: Ending| format!("{}: {}", ending.label(), ending.detail());
        let mut winner = None;
        for _ in 0..MAX_ROLL_ROUNDS {
            for seat in 0..2 {
                self.press_labelled(seat, "roll").map_err(fail)?;
            }
            self.auto_reveal().map_err(fail)?;
            winner = self.roll_winner();
            if winner.is_some() {
                break;
            }
        }
        let winner = winner.ok_or("the roll for first player never settled")?;
        let lobby = self.view(winner);
        if lobby.status.iter().any(|line| line == "mode: free table") {
            self.press_labelled(winner, "switch to rules enforced")
                .map_err(fail)?;
        }
        let label = if usize::from(first) == winner {
            "go first".to_string()
        } else {
            format!("let {{seat {first}}} go first")
        };
        if !self.press_labelled(winner, &label).map_err(fail)? {
            return Err(format!("the roll winner was not offered `{label}`"));
        }
        let opened = self.view(0);
        if !opened
            .status
            .first()
            .is_some_and(|line| line.starts_with("turn 1 ·") && line.ends_with("rules enforced"))
        {
            return Err(format!(
                "the game did not open enforced: {}",
                opened.status.join(" | ")
            ));
        }
        Ok(())
    }

    fn turn(&mut self) -> u32 {
        let status = self.host.plugin_view(0).status;
        turn_of(&status).unwrap_or(0)
    }

    fn winner(&mut self) -> Option<u8> {
        self.host.plugin_view(0).winner
    }

    fn points(&self) -> [i32; 2] {
        let view = self.host.view();
        [0u8, 1].map(|seat| {
            view.counter(CounterTarget::Seat(seat), agni_riftbound::COUNTER_POINTS)
                .unwrap_or(0)
        })
    }

    fn zone_name(&self, zone: u16) -> String {
        self.zones
            .iter()
            .find(|decl| decl.id == zone)
            .map(|decl| decl.name.clone())
            .unwrap_or_else(|| format!("zone-{zone}"))
    }

    fn move_intent(
        &self,
        seat: usize,
        card: u32,
        zone: &ZoneDecl,
        index: Option<u32>,
        hidden: bool,
    ) -> WireIntent {
        let target_seat = if zone.owner == ZoneOwner::Shared {
            0
        } else {
            seat as u8
        };
        let index = index.unwrap_or_else(|| {
            self.seats[seat]
                .table()
                .map(|table| {
                    table
                        .in_area(PlayerId(target_seat), Zone::Plugin(zone.id))
                        .count() as u32
                })
                .unwrap_or(0)
        });
        if hidden {
            WireIntent::MoveHidden {
                card,
                to: Zone::Plugin(zone.id),
                seat: target_seat,
                index,
            }
        } else {
            WireIntent::Move {
                card,
                to: Zone::Plugin(zone.id),
                seat: target_seat,
                index,
            }
        }
    }

    fn choose(&mut self, seat: usize, choice: Choice) -> Result<Applied, Ending> {
        let applied = match choice {
            Choice::Action { index, .. } => self.press(seat, index)?,
            Choice::Move { card, zone, hidden } => {
                let Some(decl) = self.zones.iter().find(|decl| decl.id == zone).cloned() else {
                    return Ok(Applied::Refused(format!("no zone {zone}")));
                };
                let intent = self.move_intent(seat, card, &decl, None, hidden);
                let what = format!(
                    "{} {card} {} ({})",
                    if hidden { "hide" } else { "move" },
                    decl.name,
                    self.seats[seat]
                        .card(card)
                        .map(|held| held.face.name)
                        .unwrap_or_default()
                );
                self.intent(seat, intent, &what)?
            }
        };
        if let Applied::Refused(_) = &applied {
            self.refusals += 1;
            if self.refusals >= MAX_REFUSALS {
                return Err(Ending::Stuck(format!(
                    "{MAX_REFUSALS} consecutive refusals of moves the legal list offered, last {}",
                    self.last_refusal
                )));
            }
        }
        Ok(applied)
    }

    fn command(&mut self, seat: usize, line: &str) -> Result<Applied, Ending> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&verb, rest)) = words.split_first() else {
            return Ok(Applied::Refused("empty command".into()));
        };
        let card_id = |at: usize| rest.get(at).and_then(|text| text.parse::<u32>().ok());
        let refuse = |table: &mut Self, reason: String| table.refuse(seat, reason);
        match verb {
            "do" | "act" => match rest.first().and_then(|text| text.parse::<usize>().ok()) {
                Some(index) if index >= 1 => self.press(seat, index - 1),
                _ => refuse(self, format!("{verb} <action number>")),
            },
            "move" | "play" | "hide" => {
                let Some(card) = card_id(0) else {
                    return refuse(self, format!("{verb} <card id> …"));
                };
                let zone = match verb {
                    "play" => self.seats[seat].zone_of_kind(ZoneKind::Stack, None),
                    "hide" => match rest.get(1) {
                        Some(name) => self.seats[seat].zone_named(name),
                        None => self
                            .zones
                            .iter()
                            .find(|decl| decl.kind == ZoneKind::Battlefield)
                            .cloned(),
                    },
                    _ => rest.get(1).and_then(|name| self.seats[seat].zone_named(name)),
                };
                let Some(zone) = zone else {
                    return refuse(
                        self,
                        format!(
                            "unknown zone — zones: {}",
                            self.zones
                                .iter()
                                .map(|decl| decl.name.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                };
                if self.seats[seat].card(card).is_none() {
                    return refuse(self, format!("no card #{card} on the table"));
                }
                let index = rest.get(2).and_then(|text| text.parse::<u32>().ok());
                let intent = self.move_intent(seat, card, &zone, index, verb == "hide");
                let what = format!("{verb} {card} {}", zone.name);
                self.intent(seat, intent, &what)
            }
            "reveal" => {
                let Some(card) = card_id(0) else {
                    return refuse(self, "reveal <card id>".into());
                };
                self.intent(seat, WireIntent::Reveal { card }, &format!("reveal {card}"))
            }
            "state" | "view" | "help" => Ok(Applied::Refused(String::new())),
            other => refuse(
                self,
                format!("the soak table does not take `{other}`: the decks are dealt and the rules enforced; use do, move, play, hide or reveal"),
            ),
        }
    }

    fn situation(
        &mut self,
        seat: usize,
        label: &str,
        held: &[String],
    ) -> kai::ai::brain::Situation {
        let held: Vec<String> = held
            .iter()
            .map(|line| self.seats[seat].expand_notice(line))
            .collect();
        let state = snapshot(&mut self.seats[seat]);
        let card_names: Vec<String> = self.seats[seat]
            .table()
            .map(|table| {
                table
                    .cards()
                    .iter()
                    .map(|card| card.face.name.clone())
                    .filter(|name| !name.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let zone_names: Vec<String> = self.zones.iter().map(|decl| decl.name.clone()).collect();
        kai::ai::brain::Situation {
            seat_name: self.seats[seat].seat_name(seat as u8),
            state,
            card_names,
            zone_names,
            messages: Vec::new(),
            decks: Vec::new(),
            deck_loaded: Some(label.to_string()),
            dealt: true,
            held,
        }
    }

    fn nudge(&mut self, seat: usize) -> Result<Applied, Ending> {
        let view = self.view(seat);
        let Some(index) = fallback(&view, seat as u8) else {
            return Ok(Applied::Refused(String::new()));
        };
        let pressed = view.affordances[index].label.clone();
        self.trace.line(format!(
            "  ai idle with {pressed} still offered: pressing it so the table moves on"
        ));
        self.press(seat, index)
    }

    fn think(
        &mut self,
        seat: usize,
        brain: &mut kai::ai::brain::Brain,
        label: &str,
        held: &[String],
    ) -> Result<Applied, Ending> {
        let key = decision_key(&self.seats[seat]);
        let before = self.host.log().len();
        self.brain_calls += 1;
        if held.is_empty() && hold::is_quiet(&self.seats[seat].last_view, seat as u8) {
            self.brain_calls_while_quiet += 1;
        }
        if !held.is_empty() {
            self.held_wakes += 1;
        }
        let situation = self.situation(seat, label, held);
        for line in &situation.held {
            self.trace.line(format!("  held: {line}"));
        }
        self.trace.line(format!(
            "  ai request: {}",
            brain.breakdown(&situation).render()
        ));
        let mut chatter: Vec<String> = Vec::new();
        let outcome = {
            let mut exec = |line: &str| -> String {
                if line.starts_with("say ") {
                    return "said".to_string();
                }
                if self.failure.is_none() {
                    if let Err(ending) = self.command(seat, line) {
                        self.failure = Some(ending);
                    }
                }
                if let Some(ending) = &self.failure {
                    return format!("the table stopped: {}", ending.detail());
                }
                let mut lines = snapshot(&mut self.seats[seat]);
                if actionable(&self.seats[seat].last_view).is_empty() {
                    lines.push(kai::ai::brain::DECISION_OVER.to_string());
                }
                lines.join("\n")
            };
            let mut log = |text: &str| chatter.push(text.to_string());
            brain.decide(&situation, &mut exec, &mut log)
        };
        for line in chatter {
            self.trace.line(format!("  {line}"));
        }
        self.trace.line(format!(
            "  ai finished: {} commands, {}",
            outcome.commands.len(),
            outcome.usage_line()
        ));
        if let Some(ending) = self.failure.take() {
            return Err(ending);
        }
        self.view(seat);
        if decision_key(&self.seats[seat]) == key && brain.hold_requested().is_none() {
            self.nudge(seat)?;
        }
        let folded = self.host.log().len() - before;
        if folded > 0 {
            Ok(Applied::Folded(folded))
        } else if brain.hold_requested().is_some() {
            Ok(Applied::Refused(String::new()))
        } else {
            self.refusals += 1;
            self.last_refusal = format!(
                "the AI seat left the table unchanged ({})",
                self.last_refusal
            );
            if self.refusals >= MAX_REFUSALS {
                return Err(Ending::Stuck(format!(
                    "{MAX_REFUSALS} decisions in a row folded nothing, last {}",
                    self.last_refusal
                )));
            }
            Ok(Applied::Refused(self.last_refusal.clone()))
        }
    }

    fn status_of(&self, seat: usize, view: &PluginView) -> String {
        let mut lines = view.status.clone();
        if let Some(prompt) = &view.prompt {
            lines.push(format!("prompt for seat {}: {}", prompt.seat, prompt.why));
        }
        self.seats[seat].expand_notice(&lines.join(" | "))
    }
}

pub struct Runner {
    pub modules: Modules,
    pub players: [Player; 2],
    pub seed: u64,
    pub turn_cap: u32,
    pub model: String,
    pub trace: Option<String>,
    pub jobs: usize,
}

impl Runner {
    fn trace(&self) -> Out {
        match (self.trace.as_deref(), self.jobs) {
            (None, _) => Out::quiet(),
            (Some("-"), 1) => Out::open(None),
            (Some(path), 1) => Out::open(Some(path)),
            (Some(_), _) => Out::capture(),
        }
    }

    fn flush_trace(&self, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        static FLUSH: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
        let _held = FLUSH.lock();
        let mut out = match self.trace.as_deref() {
            Some("-") => Out::open(None),
            Some(path) => Out::open(Some(path)),
            None => return,
        };
        for line in lines {
            out.line(line);
        }
    }

    pub fn play(&self, game: u32) -> Result<GameRecord, String> {
        self.play_with(game, &mut |_, _| {})
    }

    pub fn play_with(
        &self,
        game: u32,
        probe: &mut dyn FnMut(&mut Table, u32),
    ) -> Result<GameRecord, String> {
        let (seats, first) = layout(game);
        let seed = random::derive_seed(self.seed, game);
        let labels = [
            self.players[seats[0]].label.clone(),
            self.players[seats[1]].label.clone(),
        ];
        let brains = [
            self.players[seats[0]].brain_label(&self.model),
            self.players[seats[1]].brain_label(&self.model),
        ];
        let mut minds = [
            self.players[seats[0]].mind(&self.model),
            self.players[seats[1]].mind(&self.model),
        ];
        let mut pilots = [Pilot::default(), Pilot::default()];
        let mut trace = self.trace();
        trace.line(format!(
            "== game {game}: {} (seat 0) vs {} (seat 1), seat {first} first, seed {seed}",
            labels[0], labels[1]
        ));
        let mut table = Table::open(
            &self.modules,
            [labels[0].as_str(), labels[1].as_str()],
            seed,
            trace,
        )?;
        let started = Instant::now();
        for (seat, player) in seats.iter().enumerate() {
            table.deal(seat, &self.players[*player].deck)?;
        }
        table.start(first)?;
        let mut record = GameRecord {
            game,
            seed: self.seed,
            game_seed: seed,
            turn_cap: self.turn_cap,
            engine: self.modules.engine_label().to_string(),
            plugin: self.modules.note.clone(),
            decks: labels,
            brains,
            first,
            winner: None,
            points: [0, 0],
            turns: 1,
            entries: 0,
            millis: 0,
            ending: Ending::TurnCap,
            quiet: 0,
            brain_calls: 0,
        };
        let mut steps: u64 = 0;
        let ending = loop {
            match self.step(&mut table, &mut minds, &mut pilots, &mut record, &mut steps) {
                Ok(Some(ending)) => break ending,
                Ok(None) => probe(&mut table, record.turns),
                Err(ending) => break ending,
            }
        };
        record.ending = ending;
        record.winner = table.winner();
        record.points = table.points();
        record.turns = table.turn();
        record.entries = table.host.log().len() as u64;
        record.millis = started.elapsed().as_millis() as u64;
        record.quiet = table.quiet_steps;
        record.brain_calls = table.brain_calls;
        for (seat, pilot) in pilots.iter_mut().enumerate() {
            if let Some(line) = pilot.stretch() {
                table.trace.line(format!("  seat {seat} {line}"));
            }
        }
        table.trace.line(format!(
            "== {}{}",
            record.line(),
            if record.ending.detail().is_empty() {
                String::new()
            } else {
                format!(" — {}", record.ending.detail())
            }
        ));
        self.flush_trace(table.trace.take());
        Ok(record)
    }

    fn step(
        &self,
        table: &mut Table,
        minds: &mut [Mind; 2],
        pilots: &mut [Pilot; 2],
        record: &mut GameRecord,
        steps: &mut u64,
    ) -> Result<Option<Ending>, Ending> {
        *steps += 1;
        let mut turn = record.turns;
        let mut idle: Vec<String> = Vec::new();
        let mut acting: Option<(usize, Vec<Choice>, Option<String>)> = None;
        let mut seat = table.last_actor;
        let mut seen = 0;
        while seen < 2 {
            seen += 1;
            let (view, revealed) = table.ready_view(seat)?;
            if revealed {
                return Ok(None);
            }
            if view.winner.is_some() {
                return Ok(Some(Ending::Winner));
            }
            turn = turn_of(&view.status).unwrap_or(turn);
            if turn > self.turn_cap {
                return Ok(Some(Ending::TurnCap));
            }
            if *steps > MAX_STEPS {
                return Ok(Some(Ending::Stuck(format!(
                    "{MAX_STEPS} steps without a winner inside turn {turn}"
                ))));
            }
            let options = random::options(&view, seat as u8);
            let asked_elsewhere = view
                .prompt
                .as_ref()
                .is_some_and(|prompt| usize::from(prompt.seat) != seat);
            let asked = view
                .prompt
                .as_ref()
                .filter(|prompt| usize::from(prompt.seat) == seat)
                .map(|prompt| table.seats[seat].expand_notice(&prompt.why));
            if !options.is_empty() && (!asked_elsewhere || seen == 2) {
                acting = Some((seat, options, asked));
                break;
            }
            if !options.is_empty() {
                acting = Some((seat, options, asked));
            } else {
                idle.push(format!("seat {seat}: {}", table.status_of(seat, &view)));
            }
            seat = 1 - seat;
        }
        let Some((pick, options, asked)) = acting else {
            return Ok(Some(Ending::Stuck(format!(
                "no legal affordance for either seat in turn {turn}; {}",
                idle.join("; ")
            ))));
        };
        table.last_actor = pick;
        if hold::is_quiet(&table.seats[pick].last_view, pick as u8) {
            table.quiet_steps += 1;
        }
        let applied = match &mut minds[pick] {
            Mind::Random | Mind::Auto => {
                let auto = match &minds[pick] {
                    Mind::Auto => auto_choice(&table.seats[pick].last_view, pick as u8),
                    _ => None,
                };
                let forced = auto
                    .as_ref()
                    .is_some_and(|(choice, _)| options.as_slice() == [choice.clone()]);
                if let Some((choice, true)) = auto.filter(|_| !forced) {
                    let open: Vec<String> = options
                        .iter()
                        .map(|option| option.describe(&|zone| table.zone_name(zone)))
                        .collect();
                    return Err(Ending::Stuck(format!(
                        "seat {pick}'s auto-pass would send {} in turn {turn} while the seat could also {}",
                        choice.describe(&|zone| table.zone_name(zone)),
                        open.join(" / ")
                    )));
                }
                let choice = random::pick(&mut table.rng, &options).expect("options exist");
                let what = choice.describe(&|zone| table.zone_name(zone));
                let question = asked
                    .as_deref()
                    .map(|why| format!(" — {why}"))
                    .unwrap_or_default();
                let verb = if forced {
                    table.auto_fired += 1;
                    "auto-sends"
                } else {
                    "picks"
                };
                table
                    .trace
                    .line(format!("turn {turn}: seat {pick} {verb} {what}{question}"));
                table.choose(pick, choice)?
            }
            Mind::Llm(brain) => {
                let view = table.seats[pick].last_view.clone();
                let board = table.seats[pick].board_cards();
                match pilots[pick].step(&view, pick as u8, &board, false) {
                    Step::Idle => table.refuse(pick, "the pilot sees nothing to do".into())?,
                    Step::Press { index, label, .. } => {
                        table.auto_fired += 1;
                        table.trace.line(format!(
                            "turn {turn}: seat {pick} auto-sends do {} ({label})",
                            index + 1
                        ));
                        table.press(pick, index)?
                    }
                    Step::Model { held } => {
                        if let Some(line) = pilots[pick].stretch() {
                            table.trace.line(format!("  seat {pick} {line}"));
                        }
                        table.trace.line(format!(
                            "turn {turn}: seat {pick} thinks ({})",
                            brain.model()
                        ));
                        let label = record.decks[pick].clone();
                        let applied = table.think(pick, brain, &label, &held)?;
                        let Some(until) = brain.take_hold() else {
                            return finish(table, record, turn, applied);
                        };
                        let said = until.describe();
                        let view = table.view(pick);
                        let board = table.seats[pick].board_cards();
                        match pilots[pick].hold(until, &view, pick as u8, &board) {
                            Ok(()) => {
                                table.holds += 1;
                                table.trace.line(format!(
                                    "  seat {pick} holds until {}",
                                    pilots[pick]
                                        .holding()
                                        .map(|until| until.describe())
                                        .unwrap_or_default()
                                ));
                                applied
                            }
                            Err(dropped) => {
                                table.trace.line(format!(
                                    "  seat {pick} hold dropped: {}",
                                    dropped.reason
                                ));
                                hold_dropped(&mut table.seats[pick], brain, &said, &dropped);
                                if dropped.again {
                                    table.nudge(pick)?
                                } else {
                                    applied
                                }
                            }
                        }
                    }
                }
            }
        };
        finish(table, record, turn, applied)
    }

    pub fn run(&self, start: u32, games: u32) -> Vec<Result<GameRecord, (u32, String)>> {
        let next = std::sync::atomic::AtomicU32::new(start);
        let end = start + games;
        let results = parking_lot::Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..self.jobs.max(1) {
                scope.spawn(|| loop {
                    let game = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if game >= end {
                        break;
                    }
                    let outcome = self.play(game).map_err(|error| (game, error));
                    if let Ok(record) = &outcome {
                        println!("{}", record.line());
                    }
                    results.lock().push(outcome);
                });
            }
        });
        let mut results = results.into_inner();
        results.sort_by_key(|outcome| match outcome {
            Ok(record) => record.game,
            Err((game, _)) => *game,
        });
        results
    }
}

fn finish(
    table: &mut Table,
    record: &mut GameRecord,
    turn: u32,
    applied: Applied,
) -> Result<Option<Ending>, Ending> {
    match applied {
        Applied::Folded(count) => table.trace.line(format!("  folded {count} entries")),
        Applied::Refused(reason) if !reason.is_empty() => {
            table.trace.line(format!("  no fold: {reason}"));
        }
        Applied::Refused(_) => {}
    }
    record.turns = turn;
    Ok(None)
}

pub fn write_record(out: &mut std::fs::File, record: &GameRecord) -> Result<(), String> {
    let json = serde_json::to_string(record).map_err(|error| error.to_string())?;
    writeln!(out, "{json}").map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())
}

pub fn main(args: &[String]) -> ! {
    let args = parse_args(args);
    set_catalog_dir(
        args.catalog
            .as_ref()
            .map(PathBuf::from)
            .or_else(kai::os::paths::store_dir),
    );
    let modules = match Modules::resolve(args.engine) {
        Ok(modules) => modules,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let mut players = Vec::new();
    for (source, brain) in args.decks.iter().zip(args.brains) {
        match player(source, brain) {
            Ok(player) => players.push(player),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }
    let Ok(players): Result<[Player; 2], _> = players.try_into() else {
        std::process::exit(2);
    };
    println!(
        "soak: {} ({}) vs {} ({}), {} games from game {}, seed {}, turn cap {}, {} job(s), {} engine, {}",
        players[0].label,
        players[0].brain_label(&args.model),
        players[1].label,
        players[1].brain_label(&args.model),
        args.games,
        args.start,
        args.seed,
        args.turn_cap,
        args.jobs,
        modules.engine_label(),
        modules.note
    );
    let mut out = match std::fs::File::create(&args.out) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("cannot write {}: {error}", args.out.display());
            std::process::exit(2);
        }
    };
    if let Some(path) = args.trace.as_deref().filter(|path| *path != "-") {
        let _ = std::fs::remove_file(path);
    }
    let runner = Runner {
        modules,
        players,
        seed: args.seed,
        turn_cap: args.turn_cap,
        model: args.model.clone(),
        trace: args.trace.clone(),
        jobs: args.jobs,
    };
    let mut records = Vec::new();
    let mut broken = Vec::new();
    for outcome in runner.run(args.start, args.games) {
        match outcome {
            Ok(record) => {
                if let Err(error) = write_record(&mut out, &record) {
                    eprintln!("{}: {error}", args.out.display());
                }
                records.push(record);
            }
            Err((game, error)) => {
                eprintln!("game {game} could not be set up: {error}");
                broken.push((game, error));
            }
        }
    }
    let summary = Summary::of_outcomes(&records, &broken);
    println!("{}", summary.render());
    println!("records: {}", args.out.display());
    std::process::exit(if summary.failed() { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_alternates_seats_and_first_player() {
        assert_eq!(layout(1), ([0, 1], 0));
        assert_eq!(layout(2), ([1, 0], 0));
        assert_eq!(layout(3), ([0, 1], 1));
        assert_eq!(layout(4), ([1, 0], 1));
        assert_eq!(layout(5), ([0, 1], 0));
        let first_deck = |game: u32| {
            let (seats, first) = layout(game);
            seats[usize::from(first)]
        };
        assert_eq!(
            (1..=4).map(first_deck).collect::<Vec<_>>(),
            [0, 1, 1, 0],
            "each deck goes first twice over four games"
        );
        assert_eq!(
            turn_of(&["turn 12 · {seat 1} · action phase · rules enforced".into()]),
            Some(12)
        );
        assert_eq!(turn_of(&["roll for first player".into()]), None);
    }

    #[test]
    fn the_arguments_default_to_twenty_random_games_of_the_two_saved_decks() {
        let args = parse_args(&[]);
        assert_eq!(args.games, DEFAULT_GAMES);
        assert_eq!(args.decks, ["lillia", "irelia"]);
        assert_eq!(args.brains, [MindKind::Random, MindKind::Random]);
        assert_eq!(args.turn_cap, DEFAULT_TURN_CAP);
        let custom: Vec<String> = [
            "--games",
            "3",
            "--start",
            "7",
            "--seed",
            "99",
            "--brain-b",
            "nanogpt",
            "--engine",
            "wasm",
            "--deck-a",
            "pool:lillia",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let args = parse_args(&custom);
        assert_eq!((args.games, args.start, args.seed), (3, 7, 99));
        assert_eq!(args.brains[1], MindKind::Llm);
        assert_eq!(args.engine, EngineChoice::Wasm);
        assert_eq!(args.decks[0], "pool:lillia");
    }

    #[test]
    fn two_random_games_run_to_an_end_headless() {
        let modules = Modules::resolve(EngineChoice::Native)
            .expect("the riftbound plugin is built (plugin-build) or named by AGNI_RIFTBOUND_WASM");
        let lillia = player("pool:lillia", MindKind::Random).unwrap();
        let irelia = player("pool:irelia", MindKind::Random).unwrap();
        assert_eq!(lillia.label, "Lillia (house)");
        assert_eq!(irelia.label, "Irelia (house)");
        assert!(
            player("pool:nasus", MindKind::Random).is_ok(),
            "every pool deck answers to its slug prefix"
        );
        assert_eq!(
            player("pool:zed", MindKind::Random).err(),
            Some("the pool has no deck starting with zed".into())
        );
        let runner = Runner {
            modules,
            players: [lillia, irelia],
            seed: 7,
            turn_cap: DEFAULT_TURN_CAP,
            model: String::new(),
            trace: None,
            jobs: 2,
        };
        let records: Vec<GameRecord> = runner
            .run(1, 2)
            .into_iter()
            .map(|outcome| outcome.expect("the game sets up"))
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].decks, ["Lillia (house)", "Irelia (house)"]);
        assert_eq!(records[1].decks, ["Irelia (house)", "Lillia (house)"]);
        assert_eq!((records[0].first, records[1].first), (0, 0));
        for record in &records {
            assert!(
                !record.ending.is_failure(),
                "game {} failed: {}",
                record.game,
                record.ending.detail()
            );
            assert!(record.turns >= 1);
            assert!(record.entries > 10);
            assert_eq!(record.turn_cap, DEFAULT_TURN_CAP);
            assert_eq!(record.engine, "native");
            assert!(!record.plugin.is_empty());
            assert_eq!(record.game_seed, random::derive_seed(7, record.game));
            assert!(
                record.quiet > 0 && record.brain_calls == 0,
                "game {}: {} quiet steps went by without a brain call ({} calls)",
                record.game,
                record.quiet,
                record.brain_calls
            );
            if record.ending == Ending::Winner {
                let winner = record.winner.expect("a winner");
                assert_eq!(record.points[usize::from(winner)], 8);
            }
        }
        assert!(
            records.iter().any(|record| record.ending == Ending::Winner),
            "a random game reaches a winner inside the turn cap: {:?}",
            records
                .iter()
                .map(|record| record.line())
                .collect::<Vec<_>>()
        );
        let summary = Summary::of(&records);
        assert!(!summary.failed(), "{}", summary.render());
        assert!(summary
            .render()
            .contains("soak passed: every game ended by a winner or the turn cap"));
    }

    #[test]
    fn one_round_of_a_decision_for_either_seat_of_a_long_random_game_stays_under_the_budget_and_flat(
    ) {
        let modules = Modules::resolve(EngineChoice::Native)
            .expect("the riftbound plugin is built (plugin-build) or named by AGNI_RIFTBOUND_WASM");
        let runner = Runner {
            modules,
            players: [
                player("pool:lillia", MindKind::Random).unwrap(),
                player("pool:irelia", MindKind::Random).unwrap(),
            ],
            seed: 23,
            turn_cap: DEFAULT_TURN_CAP,
            model: String::new(),
            trace: None,
            jobs: 1,
        };
        let mut brains = [
            kai::ai::brain::Brain::new(
                kai::ai::nanogpt::Client::new(kai::ai::nanogpt::DEFAULT_MODEL),
                kai::ai::cards::CardTexts::pool(),
                None,
            ),
            kai::ai::brain::Brain::new(
                kai::ai::nanogpt::Client::new(kai::ai::nanogpt::DEFAULT_MODEL),
                kai::ai::cards::CardTexts::pool(),
                None,
            ),
        ];
        let mut samples: Vec<(u32, kai::ai::brain::Breakdown)> = Vec::new();
        let mut with_legal_list = 0;
        let record = runner
            .play_with(1, &mut |table, turn| {
                for (seat, brain) in brains.iter_mut().enumerate() {
                    let label = format!("deck {seat}");
                    let situation = table.situation(seat, &label, &[]);
                    if situation
                        .state
                        .iter()
                        .any(|line| line.starts_with(kai::ai::brain::LEGAL_TAG))
                    {
                        with_legal_list += 1;
                    }
                    let breakdown = brain.breakdown(&situation);
                    let messages = brain.request(&situation);
                    let sent: usize = messages
                        .iter()
                        .map(|message| message["content"].as_str().map_or(0, str::len))
                        .sum();
                    assert!(
                        sent <= breakdown.total()
                            && breakdown.total() <= sent + breakdown.tools + 64,
                        "the breakdown accounts for what is sent: {sent} sent, {}",
                        breakdown.render()
                    );
                    samples.push((turn, breakdown));
                }
            })
            .expect("the game sets up");
        assert!(!record.ending.is_failure(), "{}", record.ending.detail());
        assert!(
            record.quiet > 0 && record.brain_calls == 0,
            "the random seats never call a brain, quiet stretches included ({} quiet steps)",
            record.quiet
        );
        assert!(record.turns >= 8, "a long game: {} turns", record.turns);
        assert!(samples.len() >= 80, "{} requests sampled", samples.len());
        assert!(
            with_legal_list >= 20,
            "the seat about to act, legal list and all, is among the samples: {with_legal_list}"
        );
        let worst = samples
            .iter()
            .max_by_key(|(_, breakdown)| breakdown.total())
            .unwrap();
        eprintln!("worst decision (turn {}): {}", worst.0, worst.1.render());
        assert!(
            worst.1.tokens() <= kai::ai::brain::REQUEST_TOKEN_BUDGET,
            "turn {}: {} tokens over the budget of {} — {}",
            worst.0,
            worst.1.tokens(),
            kai::ai::brain::REQUEST_TOKEN_BUDGET,
            worst.1.render()
        );
        let early: Vec<&kai::ai::brain::Breakdown> = samples
            .iter()
            .filter(|(turn, _)| *turn <= 2)
            .map(|(_, breakdown)| breakdown)
            .collect();
        let late: Vec<&kai::ai::brain::Breakdown> = samples
            .iter()
            .filter(|(turn, _)| *turn + 2 >= record.turns)
            .map(|(_, breakdown)| breakdown)
            .collect();
        let average = |set: &[&kai::ai::brain::Breakdown]| {
            set.iter().map(|breakdown| breakdown.total()).sum::<usize>() / set.len().max(1)
        };
        let (first, last) = (average(&early), average(&late));
        eprintln!("average request: {first} bytes in the first two turns, {last} in the last two");
        assert!(
            last <= first + first / 2,
            "the request grows across the game: {first} → {last} bytes"
        );
        for (turn, breakdown) in &samples {
            assert!(
                breakdown.stable() >= breakdown.volatile(),
                "turn {turn}: the volatile part outgrew the cached prefix — {}",
                breakdown.render()
            );
        }
    }

    #[test]
    fn the_seed_replays_a_game_move_for_move() {
        let modules = Modules::resolve(EngineChoice::Native)
            .expect("the riftbound plugin is built (plugin-build) or named by AGNI_RIFTBOUND_WASM");
        let runner = Runner {
            modules,
            players: [
                player("pool:lillia", MindKind::Random).unwrap(),
                player("pool:irelia", MindKind::Random).unwrap(),
            ],
            seed: 11,
            turn_cap: 3,
            model: String::new(),
            trace: None,
            jobs: 1,
        };
        let mut first = runner.play(3).unwrap();
        let mut again = runner.play(3).unwrap();
        first.millis = 0;
        again.millis = 0;
        assert_eq!(first, again);
        assert_eq!(first.ending, Ending::TurnCap);
        assert_eq!(first.first, 1);
        assert!(first.turns >= 3);
        let other = runner.play(4).unwrap();
        assert_ne!(
            other.entries, first.entries,
            "another game deals and plays differently"
        );
    }

    #[test]
    fn the_auto_passing_seat_plays_the_same_game_as_the_manual_seat_on_the_same_seed() {
        let runner = |brain: MindKind| Runner {
            modules: Modules::resolve(EngineChoice::Native).expect(
                "the riftbound plugin is built (plugin-build) or named by AGNI_RIFTBOUND_WASM",
            ),
            players: [
                player("pool:lillia", brain).unwrap(),
                player("pool:irelia", MindKind::Random).unwrap(),
            ],
            seed: 11,
            turn_cap: 4,
            model: String::new(),
            trace: None,
            jobs: 1,
        };
        let manual = runner(MindKind::Random);
        let desktop = runner(MindKind::Auto);
        assert_eq!(desktop.players[0].brain_label(""), "random+auto");
        for game in 1..=2 {
            let mut manual_log = Vec::new();
            let mut manual_record = manual
                .play_with(game, &mut |table, _| manual_log = table.host.log().to_vec())
                .unwrap();
            let mut auto_log = Vec::new();
            let mut fired = 0;
            let mut auto_record = desktop
                .play_with(game, &mut |table, _| {
                    auto_log = table.host.log().to_vec();
                    fired = table.auto_fired;
                })
                .unwrap();
            assert!(
                fired > 0,
                "game {game}: the desktop seat auto-passed at least once"
            );
            assert_eq!(
                auto_record.brains[usize::from(game % 2 == 0)],
                "random+auto"
            );
            manual_record.millis = 0;
            auto_record.millis = 0;
            auto_record.brains = manual_record.brains.clone();
            assert_eq!(auto_record, manual_record, "game {game} is the same game");
            assert!(
                !auto_record.ending.is_failure(),
                "{}",
                auto_record.ending.detail()
            );
            let shared = manual_log.len().min(auto_log.len());
            assert!(shared > 10);
            assert_eq!(manual_log[..shared], auto_log[..shared]);
        }
    }

    fn tool_call(id: usize, name: &str, arguments: serde_json::Value) -> Reply {
        Reply {
            tool_calls: vec![kai::ai::nanogpt::ToolCall {
                id: format!("c{id}"),
                name: name.into(),
                arguments: arguments.to_string(),
            }],
            ..Reply::default()
        }
    }

    #[test]
    fn a_canned_llm_seat_is_woken_only_for_real_choices_and_its_holds_end_with_a_note() {
        let mut script = Vec::new();
        for round in 0..40 {
            script.push(tool_call(
                round * 2,
                "hold",
                serde_json::json!({ "until": "any_of", "any_of": ["my_turn", "opponent_played"] }),
            ));
            script.push(tool_call(
                round * 2 + 1,
                "done",
                serde_json::json!({ "reason": "nothing worth doing" }),
            ));
        }
        for round in 80..400 {
            script.push(tool_call(
                round,
                "done",
                serde_json::json!({ "reason": "let it be" }),
            ));
        }
        let mut lillia = player("pool:lillia", MindKind::Llm).unwrap();
        lillia.script = Some(script);
        let runner = Runner {
            modules: Modules::resolve(EngineChoice::Native).expect(
                "the riftbound plugin is built (plugin-build) or named by AGNI_RIFTBOUND_WASM",
            ),
            players: [lillia, player("pool:irelia", MindKind::Random).unwrap()],
            seed: 5,
            turn_cap: 6,
            model: "canned".into(),
            trace: Some("-".into()),
            jobs: 1,
        };
        let mut counts = (0, 0, 0, 0, 0);
        let record = runner
            .play_with(1, &mut |table, _| {
                counts = (
                    table.brain_calls,
                    table.brain_calls_while_quiet,
                    table.auto_fired,
                    table.holds,
                    table.held_wakes,
                );
            })
            .expect("the game sets up");
        let (brain_calls, while_quiet, auto_fired, holds, held_wakes) = counts;
        assert!(
            !record.ending.is_failure(),
            "{}: {}",
            record.ending.label(),
            record.ending.detail()
        );
        assert_eq!(record.brains[0], "nanogpt:canned");
        assert_eq!(record.brain_calls, brain_calls);
        assert!(brain_calls > 0, "the model was consulted");
        assert_eq!(
            while_quiet, 0,
            "no brain call happened while the seat had nothing to do"
        );
        assert!(
            auto_fired > 0 && record.quiet > 0,
            "the quiet stretches were sent by the pilot ({auto_fired} presses over {} quiet steps)",
            record.quiet
        );
        assert!(holds > 0, "the canned brain held at least once");
        assert!(
            held_wakes > 0,
            "a hold ended and the model was woken with the note"
        );
        assert!(
            record.turns >= 3,
            "the seat that only holds and lets things be still plays its turns: {}",
            record.turns
        );
    }
}
