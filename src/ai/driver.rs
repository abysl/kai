use crate::ai::brain::{Brain, Situation, DECISION_OVER};
use crate::ai::hold::{Dropped, Pilot, Step};
use crate::ai::random::{self, Choice, Rng};
use crate::deck::import::{deal_plan_for, face_map, ImportedDeck, SeatedDeckRecord};
use crate::engine::modules::{prepare_join, JoinModules};
use crate::net::PendingWelcome;
use crate::table::auto::{self, Offer};
use crate::table::plugin_ui::{
    action_bytes, answer_wording, card_label, expand, prompt_line, RollSecrets,
};
use agni_core::{CardFace, CardId, PlayerId, Table, Zone};
use agni_net::bridge::{self, NetToGame};
use agni_net::session::{
    engine_blob_ref, verify_engine_pin, verify_plugin_pin, ClientMsg, ClientSession, HostMsg,
    SeatInfo, WireIntent, WIRE_VERSION,
};
use agni_sim::log::{LogAction, LogState};
use agni_sim::wire::{
    Affordance, AffordanceKind, Arrow, ArrowKind, Legal, LegalKind, Origin, PluginView, TargetRef,
    ZoneDecl, ZoneKind, ZoneOwner,
};
use parking_lot::Mutex;
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TICK: Duration = Duration::from_millis(100);
pub const SETTLE_TICKS: usize = 12;
pub const FOLD_WAIT: Duration = Duration::from_secs(5);
pub const PANIC_BUTTON: &str = "free table";
pub const FALLBACK_LABELS: [&str; 6] = ["pass", "end turn", "keep", "done", "no", "your base"];

pub trait Link {
    fn send(&mut self, msg: ClientMsg);
    fn poll(&mut self) -> Vec<NetToGame>;
    fn closed(&self) -> bool {
        false
    }
}

pub struct BridgeLink;

impl Link for BridgeLink {
    fn send(&mut self, msg: ClientMsg) {
        bridge::send_to_host(msg);
    }

    fn poll(&mut self) -> Vec<NetToGame> {
        bridge::drain_events()
    }
}

static CATALOG_DIR: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

pub fn set_catalog_dir(dir: Option<PathBuf>) {
    *CATALOG_DIR.lock() = Some(dir);
}

pub fn catalog_dir() -> Option<PathBuf> {
    CATALOG_DIR
        .lock()
        .get_or_insert_with(crate::os::paths::store_dir)
        .clone()
}

pub struct Out {
    sink: Box<dyn Write + Send>,
    captured: Option<Vec<String>>,
}

impl Out {
    pub fn open(path: Option<&str>) -> Self {
        let sink: Box<dyn Write + Send> = match path {
            Some(path) => Box::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap_or_else(|error| {
                        eprintln!("cannot open log {path}: {error}");
                        std::process::exit(1)
                    }),
            ),
            None => Box::new(std::io::stdout()),
        };
        Self {
            sink,
            captured: None,
        }
    }

    pub fn file(path: &std::path::Path) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(Self {
            sink: Box::new(file),
            captured: None,
        })
    }

    pub fn quiet() -> Self {
        Self {
            sink: Box::new(std::io::sink()),
            captured: None,
        }
    }

    pub fn capture() -> Self {
        Self {
            sink: Box::new(std::io::sink()),
            captured: Some(Vec::new()),
        }
    }

    pub fn take(&mut self) -> Vec<String> {
        self.captured.take().unwrap_or_default()
    }

    pub fn line(&mut self, text: impl AsRef<str>) {
        if let Some(captured) = self.captured.as_mut() {
            captured.push(text.as_ref().to_string());
            return;
        }
        let _ = writeln!(self.sink, "{}", text.as_ref());
        let _ = self.sink.flush();
    }
}

pub struct Seat {
    pub session: Option<ClientSession>,
    pub pending: Option<PendingWelcome>,
    pub module_status: Option<String>,
    pub roster: Vec<SeatInfo>,
    pub seat: u8,
    pub deck: Option<SeatedDeckRecord>,
    pub secrets: RollSecrets,
    pub last_view: PluginView,
    pub last_seq: u64,
    pub dealt: bool,
    pub resumed: bool,
    pub new_game: bool,
    pub ended: Option<String>,
    pub fault: Option<String>,
    pub notices: Vec<String>,
    pub refusals: u64,
    pub own_folds: u64,
    pub undo: agni_net::session::UndoStatus,
    pub undo_voted: Option<u64>,
    pub rolled_back: bool,
}

impl Default for Seat {
    fn default() -> Self {
        Self::new()
    }
}

impl Seat {
    pub fn new() -> Self {
        Self {
            session: None,
            pending: None,
            module_status: None,
            roster: Vec::new(),
            seat: 0,
            deck: None,
            secrets: RollSecrets::default(),
            last_view: PluginView::default(),
            last_seq: u64::MAX,
            dealt: false,
            resumed: false,
            new_game: false,
            ended: None,
            fault: None,
            notices: Vec::new(),
            refusals: 0,
            own_folds: 0,
            undo: Default::default(),
            undo_voted: None,
            rolled_back: false,
        }
    }

    pub fn deck_on_table(&self) -> bool {
        let Some(table) = self.table() else {
            return false;
        };
        self.zones()
            .iter()
            .filter(|decl| decl.kind == ZoneKind::Deck && decl.owner == ZoneOwner::PerSeat)
            .any(|decl| {
                table
                    .in_area(PlayerId(self.seat), Zone::Plugin(decl.id))
                    .next()
                    .is_some()
            })
    }

    pub fn zones(&self) -> Vec<ZoneDecl> {
        self.session
            .as_ref()
            .map(|session| session.view().zones.clone())
            .unwrap_or_default()
    }

    pub fn state(&self) -> Option<&LogState> {
        self.session.as_ref().map(|session| session.state())
    }

    pub fn zone_named(&self, wanted: &str) -> Option<ZoneDecl> {
        let wanted = wanted.to_ascii_lowercase();
        let wanted = match wanted.strip_prefix("bf") {
            Some(n) if n.chars().all(|c| c.is_ascii_digit()) => format!("battlefield-{n}"),
            _ => wanted,
        };
        self.zones().into_iter().find(|decl| {
            decl.name.eq_ignore_ascii_case(&wanted) || decl.label.eq_ignore_ascii_case(&wanted)
        })
    }

    pub fn zone_of_kind(&self, kind: ZoneKind, prefer: Option<&str>) -> Option<ZoneDecl> {
        let zones = self.zones();
        if let Some(name) = prefer {
            if let Some(decl) = zones
                .iter()
                .find(|decl| decl.kind == kind && decl.name.contains(name))
            {
                return Some(decl.clone());
            }
        }
        zones.into_iter().find(|decl| decl.kind == kind)
    }

    pub fn table(&self) -> Option<Table> {
        self.session.as_ref().map(|session| session.table())
    }

    pub fn card(&self, id: u32) -> Option<agni_core::Card> {
        self.table()?.get(CardId(id)).cloned()
    }

    pub fn board_cards(&self) -> Vec<(u32, String)> {
        let Some(session) = self.session.as_ref() else {
            return Vec::new();
        };
        let zones = &session.view().zones;
        session
            .table()
            .cards()
            .iter()
            .filter(|card| {
                !card.face.name.is_empty()
                    && agni_sim::wire::zone_kind(zones, card.zone) == Some(ZoneKind::Battlefield)
                    && agni_sim::wire::zone_visibility(zones, card.zone)
                        == Some(agni_sim::wire::ZoneVisibility::All)
            })
            .map(|card| (card.id.0, card.face.name.clone()))
            .collect()
    }

    pub fn seat_name(&self, seat: u8) -> String {
        self.roster
            .iter()
            .find(|held| held.seat == seat)
            .map(|held| held.name.clone())
            .unwrap_or_else(|| format!("seat {seat}"))
    }

    pub fn zone_label(&self, zone: Zone) -> String {
        match zone {
            Zone::Plugin(id) => self
                .zones()
                .into_iter()
                .find(|decl| decl.id == id)
                .map(|decl| decl.name)
                .unwrap_or_else(|| format!("zone-{id}")),
            Zone::Hand => "hand".into(),
            Zone::Board => "board".into(),
        }
    }

    pub fn expand_notice(&self, text: &str) -> String {
        let seat_name = |seat: u8| self.seat_name(seat);
        let zone_name = |zone: u16| self.zone_label(Zone::Plugin(zone));
        let card_name = |card: u32| match self.session.as_ref() {
            Some(session) => {
                card_label(&session.table(), session.view(), PlayerId(self.seat), card)
            }
            None => format!("card {card}"),
        };
        expand(text, &seat_name, &zone_name, &card_name)
    }

    pub fn describe(&mut self, out: &mut Out) {
        let mine = self.seat;
        let plugin_view = match self.session.as_mut() {
            Some(session) => session.plugin_view(mine),
            None => {
                out.line("== no table yet");
                return;
            }
        };
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let table = session.table();
        let view = session.view();
        out.line(format!(
            "== table seq {} · you are seat {} ({}) · {} seats",
            view.next_seq,
            self.seat,
            self.seat_name(self.seat),
            view.seats.len()
        ));
        if session.state().options.is_some() {
            out.line(format!(
                "== table options · {}",
                agni_riftbound::TableOptions::in_play(
                    session.state().options.as_ref(),
                    u8::try_from(view.seats.len()).unwrap_or(u8::MAX)
                )
                .label()
            ));
        }
        let seat_name = |seat: u8| self.seat_name(seat);
        let zone_name = |zone: u16| self.zone_label(Zone::Plugin(zone));
        let card_name = |card: u32| card_label(&table, view, PlayerId(mine), card);
        let question = plugin_view
            .prompt
            .as_ref()
            .map(|summary| summary.why.clone());
        for line in &plugin_view.status {
            if question.as_deref() == Some(line.as_str()) {
                continue;
            }
            out.line(format!(
                "turn: {}",
                expand(line, &seat_name, &zone_name, &card_name)
            ));
        }
        if let Some(summary) = &plugin_view.prompt {
            out.line(format!(
                "prompt: {}",
                expand(
                    &prompt_line(summary, mine),
                    &seat_name,
                    &zone_name,
                    &card_name
                )
            ));
        }
        if let Some(winner) = plugin_view.winner {
            out.line(format!("== game over: {} won", self.seat_name(winner)));
        }
        for (index, affordance) in plugin_view.shown() {
            let mut line = action_line(index, affordance, &|text| {
                expand(text, &seat_name, &zone_name, &card_name)
            });
            if let Some(wording) = answer_wording(&plugin_view, mine, index) {
                line.push_str(&format!(" ({wording})"));
            }
            out.line(line);
        }
        for row in &plugin_view.legal {
            out.line(legal_line(row, &card_name, &zone_name));
        }
        for arrow in &plugin_view.arrows {
            out.line(arrow_line(arrow, &card_name, &zone_name, &seat_name));
        }
        self.last_view = plugin_view;
        let mut by_zone: BTreeMap<(String, u8), Vec<String>> = BTreeMap::new();
        for card in table.cards() {
            let name = if card.face.name.is_empty() {
                "?".to_string()
            } else {
                card.face.name.clone()
            };
            let label = self.zone_label(card.zone);
            let key = (label, card.seat.0);
            let mut tag = format!("#{} {name}", card.id.0);
            let stats: Vec<String> = [
                card.face.energy.map(|value| format!("{value}e")),
                card.face.power.map(|value| format!("{value}p")),
                card.face.might.map(|value| format!("{value}m")),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !stats.is_empty() {
                tag.push_str(&format!(" [{}]", stats.join(" ")));
            }
            if session
                .view()
                .card(card.id.0)
                .is_some_and(|held| held.badges.iter().any(|badge| badge.key == "exhausted"))
            {
                tag.push_str(" (exhausted)");
            }
            let public = agni_sim::wire::zone_visibility(&view.zones, card.zone)
                == Some(agni_sim::wire::ZoneVisibility::All);
            if public && !card.face.name.is_empty() && !view.revealed.contains(&card.id.0) {
                tag.push_str(" (hidden)");
            }
            by_zone.entry(key).or_default().push(tag);
        }
        for ((zone, seat), cards) in by_zone {
            let owner = if self
                .zones()
                .iter()
                .any(|decl| (decl.name == zone) && decl.owner == ZoneOwner::Shared)
            {
                String::new()
            } else {
                format!(
                    " ({})",
                    if seat == self.seat {
                        "mine".to_string()
                    } else {
                        self.seat_name(seat)
                    }
                )
            };
            out.line(format!("{zone}{owner}: {}", cards.join(", ")));
        }
        for counter in &view.counters {
            let Some(decl) = view.counter_decl(counter.counter) else {
                continue;
            };
            if counter.value == decl.start {
                continue;
            }
            out.line(format!(
                "counter {:?} {} = {}",
                counter.target, decl.name, counter.value
            ));
        }
        self.last_seq = view.next_seq;
    }
}

pub fn action_line(
    index: usize,
    affordance: &Affordance,
    expand: &dyn Fn(&str) -> String,
) -> String {
    let key = affordance
        .hotkey
        .as_deref()
        .map(|key| format!(" [{key}]"))
        .unwrap_or_default();
    let card = affordance
        .card
        .map(|card| format!(" (#{card})"))
        .unwrap_or_default();
    let kind = match affordance.kind {
        AffordanceKind::Plain => "",
        AffordanceKind::Commit { .. } => " (commit)",
        AffordanceKind::Reveal { .. } => " (reveal, automatic)",
    };
    let state = if affordance.enabled {
        ""
    } else {
        " (unavailable)"
    };
    format!(
        "action {}: {}{card}{key}{kind}{state}",
        index + 1,
        expand(&affordance.label)
    )
}

pub fn legal_kind_word(kind: LegalKind) -> String {
    match kind {
        LegalKind::Play { accelerate: true } => "play (accelerate)".to_string(),
        LegalKind::Play { .. } => "play".to_string(),
        LegalKind::March => "march".to_string(),
        LegalKind::Activate { .. } => "activate".to_string(),
        LegalKind::React => "react".to_string(),
        LegalKind::Answer => "answer".to_string(),
        LegalKind::Hide => "hide".to_string(),
    }
}

pub fn legal_line(
    row: &Legal,
    card_name: &dyn Fn(u32) -> String,
    zone_name: &dyn Fn(u16) -> String,
) -> String {
    let mut kinds: Vec<String> = Vec::new();
    for word in row.kinds.iter().copied().map(legal_kind_word) {
        if !kinds.contains(&word) {
            kinds.push(word);
        }
    }
    let kinds = if kinds.is_empty() {
        "nothing".to_string()
    } else {
        kinds.join(", ")
    };
    let zones: Vec<String> = row.zones.iter().copied().map(zone_name).collect();
    let hidden: Vec<String> = row.hidden.iter().copied().map(zone_name).collect();
    let card = card_name(row.card);
    let mut line = format!("legal: #{} {card} — {kinds}", row.card);
    if !zones.is_empty() {
        line.push_str(&format!(" → {}", zones.join(", ")));
    }
    if !hidden.is_empty() {
        line.push_str(&format!(" · hide at {}", hidden.join(", ")));
    }
    line
}

pub fn arrow_kind_word(kind: ArrowKind) -> &'static str {
    match kind {
        ArrowKind::Spell => "spell",
        ArrowKind::Ability => "ability",
        ArrowKind::Attack => "attack",
        ArrowKind::Counter => "counter",
        ArrowKind::Combat => "combat",
    }
}

pub fn arrow_line(
    arrow: &Arrow,
    card_name: &dyn Fn(u32) -> String,
    zone_name: &dyn Fn(u16) -> String,
    seat_name: &dyn Fn(u8) -> String,
) -> String {
    let from = match arrow.from {
        Origin::Card(card) => format!("#{card} {}", card_name(card)),
        Origin::Item(item) => format!("chain item {item}"),
    };
    let to = match arrow.to {
        TargetRef::Card(card) => format!("#{card} {}", card_name(card)),
        TargetRef::Seat(seat) => seat_name(seat),
        TargetRef::Zone(zone) => zone_name(zone),
        TargetRef::Item(item) => format!("chain item {item}"),
    };
    format!("arrow: {} {from} → {to}", arrow_kind_word(arrow.kind))
}

pub fn saved_deck(source: &str) -> Option<ImportedDeck> {
    if source.starts_with("http://")
        || source.starts_with("https://")
        || std::path::Path::new(source).exists()
    {
        return None;
    }
    let dir = catalog_dir()?;
    let wanted = source.trim().to_ascii_lowercase();
    let rows = crate::deck::history::store::rows_in(&dir, agni_riftbound::GAME);
    let row = rows
        .iter()
        .find(|row| row.label.to_ascii_lowercase() == wanted)
        .or_else(|| {
            rows.iter()
                .find(|row| row.label.to_ascii_lowercase().contains(&wanted))
        })?;
    crate::deck::history::store::recall_in(&dir, agni_riftbound::GAME, row.ci)
}

pub fn finish_join(seat: &mut Seat, out: &mut Out) {
    let Some(pending) = seat.pending.as_ref() else {
        return;
    };
    match prepare_join(pending.log(), None) {
        JoinModules::Pending { status } => {
            if seat.module_status.as_deref() != Some(status.as_str()) {
                out.line(format!("modules: {status}"));
                seat.module_status = Some(status);
            }
        }
        JoinModules::Refused { error } => {
            seat.pending = None;
            seat.module_status = None;
            out.line(format!("join refused: {error}"));
        }
        JoinModules::Ready {
            engine,
            plugin,
            note,
        } => {
            let pending = seat.pending.take().expect("pending welcome");
            seat.module_status = None;
            let engine_pin = engine.engine_hash().map(engine_blob_ref);
            if let Err(error) = verify_engine_pin(pending.log(), engine_pin.as_deref()) {
                out.line(format!("engine pin: {error}"));
                return;
            }
            let plugin_pin = plugin
                .as_ref()
                .and_then(|plugin| plugin.module_hash())
                .map(engine_blob_ref);
            if let Err(error) = verify_plugin_pin(pending.log(), plugin_pin.as_deref()) {
                out.line(format!("plugin pin: {error}"));
                return;
            }
            let mine = pending.seat();
            let caught_up = pending.queued();
            match pending.seat_replica(engine, plugin) {
                Ok((session, roster)) => {
                    seat.seat = mine;
                    seat.roster = roster;
                    seat.session = Some(session);
                    if caught_up > 0 {
                        out.line(format!(
                            "caught up on {caught_up} frame(s) the host sent while the modules crossed"
                        ));
                    }
                    out.line(format!(
                        "seated as player {} {}",
                        mine + 1,
                        if note.is_empty() {
                            String::new()
                        } else {
                            format!("({note})")
                        }
                    ));
                    seat.describe(out);
                }
                Err(fault) => out.line(format!("replica failed: {fault}")),
            }
        }
    }
}

pub fn handle_event(seat: &mut Seat, event: NetToGame, out: &mut Out) {
    match event {
        NetToGame::Connected => out.line("connected — waiting for a seat"),
        NetToGame::FromHost { msg } => match absorb_while_pending(seat, msg) {
            None => {}
            Some(HostMsg::Undo { status }) => seat.undo = status,
            Some(HostMsg::RolledBack { next_seq, faces }) => {
                if let Some(session) = seat.session.as_mut() {
                    match session.rollback(next_seq, faces) {
                        Ok(()) => {
                            seat.rolled_back = true;
                            seat.last_seq = u64::MAX;
                            out.line("rollback accepted");
                        }
                        Err(fault) => {
                            seat.ended = Some(format!("rollback failed: {fault}"));
                        }
                    }
                }
            }
            Some(HostMsg::Welcome {
                version,
                seat: mine,
                roster,
                log,
            }) => {
                if version != WIRE_VERSION {
                    out.line(format!(
                        "host speaks wire {version}, this build wire {WIRE_VERSION} — update one of them"
                    ));
                    return;
                }
                seat.pending = Some(PendingWelcome::new(mine, roster, log));
                seat.undo = Default::default();
                seat.undo_voted = None;
                finish_join(seat, out);
            }
            Some(HostMsg::Roster { roster }) => {
                if let Some(session) = seat.session.as_mut() {
                    session.set_roster(roster.clone());
                }
                seat.roster = roster;
            }
            Some(HostMsg::Entry { entry }) => {
                if let Some(session) = seat.session.as_mut() {
                    let own = entry.seat == seat.seat
                        && WireIntent::try_from(entry.action.clone()).is_ok();
                    let reset = matches!(entry.action, LogAction::Reset);
                    match session.apply(entry) {
                        Ok(true) => {
                            if own {
                                seat.own_folds += 1;
                            }
                            if reset {
                                seat.dealt = false;
                                seat.resumed = false;
                                seat.new_game = true;
                                if let Some(record) = seat.deck.as_mut() {
                                    record.battlefield_played = false;
                                }
                                out.line("host started a new game");
                            }
                        }
                        Ok(false) => out.line("host refused an entry we expected to fold"),
                        Err(fault) => out.line(format!("replica failed: {fault}")),
                    }
                }
            }
            Some(HostMsg::Faces { faces }) => {
                if let Some(session) = seat.session.as_mut() {
                    session.add_faces(faces);
                }
            }
            Some(HostMsg::End { reason }) => {
                seat.ended = Some(reason.clone());
                out.line(format!("host ended the session: {reason}"));
            }
            Some(msg @ (HostMsg::Module { .. } | HostMsg::NoModule { .. })) => {
                crate::engine::modules::module_frame(&msg);
            }
            Some(HostMsg::Notice { text }) => {
                let text = seat.expand_notice(&text);
                let line = if text.starts_with("refused: ") {
                    seat.refusals += 1;
                    text
                } else {
                    format!("host: {text}")
                };
                out.line(&line);
                seat.notices.push(line);
            }
        },
        NetToGame::Dropped { reason } => {
            out.line(format!("dropped: {reason}"));
            if seat.session.is_some() {
                seat.ended = Some(reason);
            }
        }
        NetToGame::HostReady
        | NetToGame::HostFailed { .. }
        | NetToGame::HostClosed
        | NetToGame::HostLost { .. }
        | NetToGame::PeerJoined { .. }
        | NetToGame::PeerFrame { .. }
        | NetToGame::PeerLeft { .. } => {}
    }
}

fn absorb_while_pending(seat: &mut Seat, msg: HostMsg) -> Option<HostMsg> {
    match (seat.session.is_none(), seat.pending.as_mut()) {
        (true, Some(pending)) => {
            let handed_back = pending.absorb(msg);
            if handed_back.is_none() {
                seat.roster = pending.roster().to_vec();
            }
            handed_back
        }
        _ => Some(msg),
    }
}

pub fn send(link: &mut dyn Link, intent: WireIntent, out: &mut Out, what: &str) {
    link.send(ClientMsg::Intent { intent });
    out.line(format!("sent {what}"));
}

pub fn seat_deck(seat: &mut Seat, deck: ImportedDeck, battlefield: Option<usize>) {
    let faces = face_map(&deck);
    seat.deck = Some(SeatedDeckRecord {
        seat: PlayerId(seat.seat),
        deck,
        faces,
        battlefield,
        battlefield_played: false,
    });
    seat.dealt = false;
}

pub fn load_deck(seat: &mut Seat, path: &str, out: &mut Out) {
    use agni_importers::riftbound::query::DeckQuery;
    if path.ends_with(".snapshot.json") {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                out.line(format!("cannot read {path}: {error}"));
                return;
            }
        };
        let snapshot: agni_deck::Snapshot = match serde_json::from_str(&text) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                out.line(format!("{path} is not a deck snapshot: {error}"));
                return;
            }
        };
        let Some(mut deck) = crate::deck::history::imported(&snapshot) else {
            out.line(format!(
                "{path} holds a deck for a game this build does not know"
            ));
            return;
        };
        if let Some(dir) = catalog_dir() {
            crate::deck::history::store::backfill(&mut deck, &dir);
        }
        let count = face_map(&deck).len();
        seat_deck(seat, deck, None);
        out.line(format!("deck ready from snapshot: {count} faces"));
        return;
    }
    let resolved = if path.starts_with("http://") || path.starts_with("https://") {
        out.line(format!("fetching {path}…"));
        crate::deck::import::run_riftbound_query(&DeckQuery::Url(path.to_string()))
    } else {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                out.line(format!("cannot read {path}: {error}"));
                return;
            }
        };
        if agni_importers::riftbound::parse_any(&text).is_ok() {
            crate::deck::import::run_riftbound_query(&DeckQuery::Text(text))
        } else {
            crate::deck::import::run_mtg_query(&text)
        }
    };
    match resolved {
        Ok(resolved) => {
            let count = face_map(&resolved.deck).len();
            seat_deck(seat, resolved.deck, None);
            out.line(format!(
                "deck ready: {count} faces, {} unresolved",
                resolved.unresolved.len()
            ));
            if let Some(ImportedDeck::Riftbound(deck)) =
                seat.deck.as_ref().map(|record| &record.deck)
            {
                for (index, entry) in deck.battlefields.iter().enumerate() {
                    out.line(format!("battlefield {}: {}", index + 1, entry.card.name));
                }
            }
        }
        Err(error) => out.line(format!("deck failed: {error}")),
    }
}

pub fn deal(seat: &mut Seat, link: &mut dyn Link, out: &mut Out) {
    let Some(record) = seat.deck.as_mut() else {
        out.line("no deck loaded — `deck <file>` first");
        return;
    };
    record.seat = PlayerId(seat.seat);
    if crate::deck::battlefield::needs_choice(record) {
        out.line("choose a battlefield first: `battlefield <n>`");
        return;
    }
    let groups = deal_plan_for(record);
    if seat.deck_on_table() {
        seat.dealt = true;
        seat.resumed = true;
        out.line(format!(
            "player {}'s deck is already on the table — resuming with it",
            seat.seat + 1
        ));
        return;
    }
    link.send(ClientMsg::DealDeck { groups });
    seat.dealt = true;
    out.line("deal requested");
}

pub fn spawn_args(rest: &[&str]) -> Option<(String, String, Option<u8>)> {
    if rest.len() < 2 {
        return None;
    }
    let might = rest.last().and_then(|word| word.parse::<u8>().ok());
    let words = if might.is_some() && rest.len() >= 3 {
        &rest[..rest.len() - 1]
    } else {
        rest
    };
    let (zone, token) = words.split_last()?;
    if token.is_empty() {
        return None;
    }
    let token = token
        .iter()
        .flat_map(|word| word.split('_'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some((token, (*zone).to_string(), might))
}

pub fn move_intent(
    seat: &Seat,
    card: u32,
    zone: &ZoneDecl,
    index: Option<u32>,
    hidden: bool,
) -> WireIntent {
    let target_seat = if zone.owner == ZoneOwner::Shared {
        0
    } else {
        seat.seat
    };
    let index = index.unwrap_or_else(|| {
        seat.table()
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

pub fn press(seat: &mut Seat, link: &mut dyn Link, out: &mut Out, index: usize) {
    let Some(affordance) = seat.last_view.affordances.get(index).cloned() else {
        out.line("no such action — `state` lists them");
        return;
    };
    match action_bytes(&affordance, &mut seat.secrets, &crate::os::entropy::secret) {
        Some(data) => send(
            link,
            WireIntent::Game {
                data: ByteBuf::from(data),
            },
            out,
            &affordance.label,
        ),
        None => out.line("that action is not available right now"),
    }
}

pub fn choose(seat: &mut Seat, link: &mut dyn Link, out: &mut Out, choice: Choice) {
    match choice {
        Choice::Action { index, .. } => press(seat, link, out, index),
        Choice::Move { card, zone, hidden } => {
            let Some(decl) = seat.zones().into_iter().find(|decl| decl.id == zone) else {
                out.line(format!("no zone {zone}"));
                return;
            };
            let intent = move_intent(seat, card, &decl, None, hidden);
            let what = format!(
                "{} #{card} {} → {}",
                if hidden { "hidden play" } else { "move" },
                seat.card(card)
                    .map(|held| held.face.name)
                    .unwrap_or_default(),
                decl.name
            );
            send(link, intent, out, &what);
        }
    }
}

pub fn run_command(seat: &mut Seat, link: &mut dyn Link, line: &str, out: &mut Out) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((&verb, rest)) = words.split_first() else {
        return true;
    };
    match verb {
        "quit" | "exit" => return false,
        "help" => {
            out.line(
                "peers · seed <ticket> · tables · join <host> · state · view · do <n> (act <n>) · move <card> <zone> [i] · play <card> · draw [rune] · exhaust <card> · trash <card> · counter <card|seat> <name> <delta> · deck <file> · battlefield <n> · deal · quit",
            );
            out.line(
                "modes: the roll winner switches with `switch to rules enforced` before `go first`; under rules enforced answer `prompt:` questions with `do <n>` (a mulligan also takes `move <card> main-deck`), plays and marches are moves to base, a held battlefield or the chain, everything else is automatic and `refused: …` names why",
            );
            out.line(
                "under rules enforced `legal:` lines list every card you may act on right now, your own hand included, how (play, march, activate, react, answer) and the zones it may be moved to — a card that is absent is refused; an `activate` card is pressed through the numbered action that names it, not by its ability number; `arrow:` lines name what each pending or chain item, attacker and combat pairing is aimed at",
            );
        }
        "state" | "view" => seat.describe(out),
        "do" | "act" => {
            let Some(index) = rest.first().and_then(|n| n.parse::<usize>().ok()) else {
                out.line(format!("{verb} <action number>"));
                return true;
            };
            press(seat, link, out, index.wrapping_sub(1));
        }
        "move" | "play" | "trash" | "draw" => {
            let (card, to_zone) = match verb {
                "move" => {
                    let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                        out.line("move <card id> <zone> [index]");
                        return true;
                    };
                    let Some(zone) = rest.get(1).and_then(|z| seat.zone_named(z)) else {
                        out.line(
                            "unknown zone — zones: ".to_string()
                                + &seat
                                    .zones()
                                    .iter()
                                    .map(|d| d.name.clone())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                        );
                        return true;
                    };
                    (Some(card), zone)
                }
                "play" => {
                    let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                        out.line("play <card id>");
                        return true;
                    };
                    let Some(zone) = seat.zone_of_kind(ZoneKind::Stack, None) else {
                        out.line("this table has no chain zone");
                        return true;
                    };
                    (Some(card), zone)
                }
                "trash" => {
                    let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                        out.line("trash <card id>");
                        return true;
                    };
                    let Some(zone) = seat.zone_of_kind(ZoneKind::Discard, None) else {
                        out.line("this table has no discard zone");
                        return true;
                    };
                    (Some(card), zone)
                }
                _ => {
                    let prefer = Some(rest.first().copied().unwrap_or("main"));
                    let Some(deck) = seat.zone_of_kind(ZoneKind::Deck, prefer) else {
                        out.line("this table has no deck zone");
                        return true;
                    };
                    let Some(hand) = seat.zone_of_kind(ZoneKind::Hand, None) else {
                        out.line("this table has no hand zone");
                        return true;
                    };
                    let top = seat.table().and_then(|table| {
                        table
                            .in_area(PlayerId(seat.seat), Zone::Plugin(deck.id))
                            .last()
                            .map(|card| card.id.0)
                    });
                    match top {
                        Some(card) => (Some(card), hand),
                        None => {
                            out.line(format!("{} is empty", deck.name));
                            return true;
                        }
                    }
                }
            };
            let Some(card) = card else { return true };
            let Some(held) = seat.card(card) else {
                out.line(format!("no card #{card} on the table"));
                return true;
            };
            let index = rest.get(2).and_then(|i| i.parse::<u32>().ok());
            let what = format!(
                "move #{card} {} → {}",
                if held.face.name.is_empty() {
                    "?"
                } else {
                    &held.face.name
                },
                to_zone.name
            );
            let intent = move_intent(seat, card, &to_zone, index, false);
            send(link, intent, out, &what);
        }
        "recycle" => {
            let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                out.line("recycle <card id>");
                return true;
            };
            let Some(deck) = seat.zone_of_kind(ZoneKind::Deck, Some("rune")) else {
                out.line("this table has no rune deck");
                return true;
            };
            send(
                link,
                WireIntent::Move {
                    card,
                    to: Zone::Plugin(deck.id),
                    seat: seat.seat,
                    index: 0,
                },
                out,
                &format!("recycle #{card} → {}", deck.name),
            );
        }
        "hide" => {
            let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                out.line("hide <card id> [zone]");
                return true;
            };
            let zone = match rest.get(1) {
                Some(name) => seat.zone_named(name),
                None => {
                    let zones = seat.zones();
                    zones
                        .iter()
                        .find(|decl| {
                            decl.kind == ZoneKind::Battlefield && decl.owner == ZoneOwner::Shared
                        })
                        .or_else(|| {
                            zones.iter().find(|decl| {
                                decl.kind == ZoneKind::Battlefield
                                    && decl.owner == ZoneOwner::PerSeat
                            })
                        })
                        .cloned()
                }
            };
            let Some(zone) = zone else {
                out.line("unknown zone");
                return true;
            };
            let intent = move_intent(seat, card, &zone, None, true);
            send(
                link,
                intent,
                out,
                &format!("hidden play #{card} → {}", zone.name),
            );
        }
        "reveal" => {
            let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                out.line("reveal <card id>");
                return true;
            };
            if seat
                .card(card)
                .is_some_and(|held| held.owner.0 != seat.seat)
            {
                out.line("that is not your card to reveal");
                return true;
            }
            send(
                link,
                WireIntent::Reveal { card },
                out,
                &format!("reveal #{card}"),
            );
        }
        "spawn" => {
            let Some((token, zone_name, might)) = spawn_args(rest) else {
                out.line("spawn <token|name> <zone> [might]");
                return true;
            };
            let Some(zone) = seat.zone_named(&zone_name) else {
                out.line("unknown zone");
                return true;
            };
            let known = agni_riftbound::token_table();
            let mut face = match known
                .iter()
                .find(|decl| decl.name.eq_ignore_ascii_case(&token))
            {
                Some(decl) => decl.face(),
                None => CardFace::named(token).with_kind(agni_riftbound::KIND_UNIT),
            };
            if let Some(might) = might {
                face.might = Some(might);
            }
            let target_seat = if zone.owner == ZoneOwner::Shared {
                0
            } else {
                seat.seat
            };
            let what = format!("spawn {} → {}", face.name, zone.name);
            send(
                link,
                WireIntent::Spawn {
                    face,
                    to: Zone::Plugin(zone.id),
                    seat: target_seat,
                },
                out,
                &what,
            );
        }
        "playmat" => {
            let Some(choice) = rest.first() else {
                out.line("playmat <image link|card:<battlefield name>|felt>");
                return true;
            };
            let playmat = (*choice != "felt").then(|| choice.to_string());
            link.send(ClientMsg::PickPlaymat { playmat });
            out.line(format!("playmat → {choice}"));
        }
        "exhaust" => {
            let Some(card) = rest.first().and_then(|c| c.parse::<u32>().ok()) else {
                out.line("exhaust <card id>");
                return true;
            };
            let on = seat
                .session
                .as_ref()
                .and_then(|session| {
                    session
                        .view()
                        .card(card)
                        .map(|held| held.badges.iter().any(|b| b.key == "exhausted"))
                })
                .unwrap_or(false);
            send(
                link,
                WireIntent::Annotate {
                    card,
                    key: "exhausted".into(),
                    value: (!on).then(|| ByteBuf::from(vec![0xf5])),
                },
                out,
                &format!("exhaust #{card} → {}", !on),
            );
        }
        "counter" => {
            let (Some(target), Some(name), Some(delta)) = (
                rest.first(),
                rest.get(1),
                rest.get(2).and_then(|d| d.parse::<i32>().ok()),
            ) else {
                out.line("counter <card id|seat> <name> <delta>");
                return true;
            };
            let Some(decl) = seat.state().and_then(|state| {
                state
                    .counter_table
                    .iter()
                    .find(|decl| decl.name.eq_ignore_ascii_case(name))
                    .cloned()
            }) else {
                out.line("unknown counter");
                return true;
            };
            let target = if *target == "seat" {
                agni_sim::wire::CounterTarget::Seat(seat.seat)
            } else if let Ok(card) = target.parse::<u32>() {
                agni_sim::wire::CounterTarget::Card(card)
            } else {
                agni_sim::wire::CounterTarget::Table
            };
            send(
                link,
                WireIntent::Counter {
                    target,
                    counter: decl.id,
                    delta,
                },
                out,
                &format!("{name} {delta:+}"),
            );
        }
        "decks" => {
            let rows = catalog_dir()
                .map(|dir| crate::deck::history::store::rows_in(&dir, agni_riftbound::GAME))
                .unwrap_or_default();
            if rows.is_empty() {
                out.line("no saved decks in the catalogue store");
            }
            for row in rows {
                let cards = catalog_dir()
                    .and_then(|dir| {
                        crate::deck::history::store::recall_in(&dir, agni_riftbound::GAME, row.ci)
                    })
                    .map(|deck| {
                        deck.cards()
                            .iter()
                            .map(|card| format!("{} x{}", card.name, 1))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                out.line(format!("saved deck: {} ({} cards)", row.label, cards.len()));
                if rest.first() == Some(&"full") {
                    out.line(format!("  {}", cards.join(" | ")));
                }
            }
        }
        "deck" => match rest.first() {
            Some(_) => {
                let source = rest.join(" ");
                match saved_deck(&source) {
                    Some(deck) => {
                        let count = face_map(&deck).len();
                        seat_deck(seat, deck, None);
                        out.line(format!("deck ready from history: {source} ({count} faces)"));
                    }
                    None => load_deck(seat, &source, out),
                }
            }
            None => out.line("deck <decklist file|url|saved deck label>"),
        },
        "battlefield" => {
            let Some(index) = rest.first().and_then(|n| n.parse::<usize>().ok()) else {
                out.line("battlefield <n>");
                return true;
            };
            match seat.deck.as_mut() {
                Some(record) => {
                    record.battlefield = Some(index.saturating_sub(1));
                    out.line(format!(
                        "battlefield {}: {}",
                        index,
                        crate::deck::battlefield::chosen_name(record).unwrap_or("?")
                    ));
                }
                None => out.line("no deck loaded"),
            }
        }
        "deal" => deal(seat, link, out),
        other => out.line(format!("unknown command {other} — try help")),
    }
    true
}

pub struct Chat {
    path: Option<PathBuf>,
    seen: usize,
    pub messages: Vec<String>,
}

impl Chat {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            seen: 0,
            messages: Vec::new(),
        }
    }

    pub fn poll(&mut self, brain: &mut Brain, out: &mut Out) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() < self.seen {
            self.seen = 0;
        }
        let mut fresh = false;
        for line in &lines[self.seen..] {
            if let Some(message) = line.strip_prefix("you: ") {
                self.messages.push(message.to_string());
                fresh = true;
            } else if let Some(model) = line.strip_prefix("model: ") {
                brain.set_model(model.trim());
                out.line(format!("ai model → {}", model.trim()));
            }
        }
        self.seen = lines.len();
        if self.messages.len() > 12 {
            let drop = self.messages.len() - 12;
            self.messages.drain(..drop);
        }
        fresh
    }

    pub fn say(&mut self, text: &str) {
        let Some(path) = &self.path else {
            return;
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "bot: {text}");
        }
    }
}

pub fn over(seat: &Seat, link: &dyn Link, halted: &dyn Fn() -> bool) -> bool {
    seat.ended.is_some() || link.closed() || halted()
}

pub fn settle(
    seat: &mut Seat,
    link: &mut dyn Link,
    out: &mut Out,
    ticks: usize,
    halted: &dyn Fn() -> bool,
) {
    for _ in 0..ticks {
        if over(seat, link, halted) {
            return;
        }
        std::thread::sleep(TICK);
        for event in link.poll() {
            handle_event(seat, event, out);
        }
    }
}

pub fn snapshot(seat: &mut Seat) -> Vec<String> {
    let mut capture = Out::capture();
    seat.describe(&mut capture);
    let mut lines = capture.take();
    lines.append(&mut seat.notices);
    lines
}

pub fn actionable(view: &PluginView) -> Vec<String> {
    view.shown()
        .map(|(_, affordance)| affordance)
        .filter(|affordance| {
            affordance.enabled
                && !matches!(affordance.kind, AffordanceKind::Reveal { .. })
                && affordance.label != PANIC_BUTTON
        })
        .map(|affordance| affordance.label.clone())
        .collect()
}

pub fn decision_key(seat: &Seat) -> Option<(u64, Vec<String>)> {
    let session = seat.session.as_ref()?;
    Some((session.view().next_seq, actionable(&seat.last_view)))
}

pub fn fallback(view: &PluginView, me: u8) -> Option<usize> {
    let usable = |(index, affordance): &(usize, &Affordance)| {
        affordance.enabled
            && matches!(affordance.kind, AffordanceKind::Plain)
            && affordance.label != PANIC_BUTTON
            && !view.is_hidden(*index)
    };
    let candidates: Vec<(usize, &Affordance)> =
        view.affordances.iter().enumerate().filter(usable).collect();
    let untouched_optional = view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == me && summary.optional && summary.picked == 0);
    if untouched_optional {
        if let Some((index, _)) = candidates
            .iter()
            .find(|(_, affordance)| ["skip", "cancel"].contains(&affordance.label.as_str()))
        {
            return Some(*index);
        }
    }
    for label in FALLBACK_LABELS {
        if let Some((index, _)) = candidates
            .iter()
            .find(|(_, affordance)| affordance.label == label)
        {
            return Some(*index);
        }
    }
    if view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == me)
    {
        return candidates
            .iter()
            .find(|(_, affordance)| affordance.hotkey.as_deref() == Some("x"))
            .or_else(|| candidates.first())
            .map(|(index, _)| *index);
    }
    None
}

pub fn auto_choice(view: &PluginView, seat: u8) -> Option<(Choice, bool)> {
    let (index, pass) = match auto::offer(view, seat) {
        Offer::Pass(index) | Offer::EndTurn(index) => (index, true),
        Offer::Forced(index) => (index, false),
        Offer::Nothing | Offer::Theirs | Offer::Choice => return None,
    };
    let label = view.affordances.get(index)?.label.clone();
    Some((Choice::Action { index, label }, pass))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindKind {
    Random,
    Auto,
    Llm,
}

impl MindKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "random" => Some(Self::Random),
            "auto" | "desktop" => Some(Self::Auto),
            "nanogpt" | "ai" | "llm" => Some(Self::Llm),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Auto => "random+auto",
            Self::Llm => "nanogpt",
        }
    }

    pub fn is_llm(self) -> bool {
        self == Self::Llm
    }
}

pub enum Mind {
    Random,
    Auto,
    Llm(Box<Brain>),
}

impl Mind {
    pub fn kind(&self) -> MindKind {
        match self {
            Self::Random => MindKind::Random,
            Self::Auto => MindKind::Auto,
            Self::Llm(_) => MindKind::Llm,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Llm(brain) => format!("nanogpt:{}", brain.model()),
            other => other.kind().label().to_string(),
        }
    }
}

pub fn llm_mind(model: &str, notes: Option<PathBuf>, out: &mut Out) -> Mind {
    let mut cards = catalog_dir()
        .as_deref()
        .map(crate::ai::cards::CardTexts::load)
        .unwrap_or_default();
    let from_store = cards.len();
    cards.absorb(crate::ai::cards::CardTexts::pool());
    out.line(format!(
        "ai mode: {model}, {from_store} card texts from the store, {} with the pool",
        cards.len()
    ));
    Mind::Llm(Box::new(Brain::new(
        crate::ai::nanogpt::Client::new(model),
        cards,
        notes,
    )))
}

struct Sent {
    folds: u64,
    refusals: u64,
    at: Instant,
}

pub struct Driver {
    pub seat: Seat,
    pub out: Out,
    mind: Option<Mind>,
    chat: Chat,
    last_decision: Option<(u64, Vec<String>)>,
    auto_deal: bool,
    playmat_wanted: Option<String>,
    last_reveal_seq: u64,
    rng: Rng,
    pace: Duration,
    sent: Option<Sent>,
    verbose: bool,
    halt: Option<Arc<AtomicBool>>,
    pub decisions: u64,
    pub pilot: Pilot,
}

impl Driver {
    pub fn new(out: Out) -> Self {
        Self {
            seat: Seat::new(),
            out,
            mind: None,
            chat: Chat::new(None),
            last_decision: None,
            auto_deal: false,
            playmat_wanted: None,
            last_reveal_seq: u64::MAX,
            rng: Rng::seeded(
                crate::os::entropy::secret()
                    .iter()
                    .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte)),
            ),
            pace: Duration::ZERO,
            sent: None,
            verbose: true,
            halt: None,
            decisions: 0,
            pilot: Pilot::default(),
        }
    }

    pub fn with_halt(mut self, flag: Arc<AtomicBool>) -> Self {
        self.halt = Some(flag);
        self
    }

    pub fn halted(&self) -> bool {
        self.halt
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    pub fn with_mind(mut self, mind: Mind) -> Self {
        self.verbose = matches!(mind, Mind::Llm(_));
        self.mind = Some(mind);
        self
    }

    fn refresh_view(&mut self) {
        if self.verbose {
            self.seat.describe(&mut self.out);
        } else {
            self.seat.describe(&mut Out::quiet());
        }
    }

    pub fn with_chat(mut self, path: Option<PathBuf>) -> Self {
        self.chat = Chat::new(path);
        self
    }

    pub fn with_deck(mut self, deck: ImportedDeck, battlefield: Option<usize>) -> Self {
        seat_deck(&mut self.seat, deck, battlefield);
        self.auto_deal = true;
        self
    }

    pub fn with_deck_file(mut self, path: &str, battlefield: Option<usize>) -> Self {
        load_deck(&mut self.seat, path, &mut self.out);
        if let Some(record) = self.seat.deck.as_mut() {
            if let Some(index) = battlefield {
                record.battlefield = Some(index.saturating_sub(1));
            }
            self.auto_deal = true;
        }
        self
    }

    pub fn with_playmat(mut self, playmat: Option<String>) -> Self {
        self.playmat_wanted = playmat;
        self
    }

    pub fn with_pace(mut self, pace: Duration) -> Self {
        self.pace = pace;
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = Rng::seeded(seed);
        self
    }

    pub fn mind(&self) -> Option<&Mind> {
        self.mind.as_ref()
    }

    pub fn deck_label(&self) -> Option<String> {
        self.seat
            .deck
            .as_ref()
            .map(|record| crate::deck::history::label(&record.deck))
    }

    pub fn command(&mut self, link: &mut dyn Link, line: &str) -> bool {
        run_command(&mut self.seat, link, line, &mut self.out)
    }

    pub fn tick(&mut self, link: &mut dyn Link) -> bool {
        for event in link.poll() {
            handle_event(&mut self.seat, event, &mut self.out);
        }
        if self.seat.pending.is_some() {
            finish_join(&mut self.seat, &mut self.out);
        }
        if std::mem::take(&mut self.seat.rolled_back) {
            self.seat.secrets.rearm();
            self.last_reveal_seq = u64::MAX;
            self.last_decision = None;
            self.sent = None;
            self.pilot.release();
            if let Some(Mind::Llm(brain)) = self.mind.as_mut() {
                brain.forget_game();
            }
        }
        if let Some(proposal) = &self.seat.undo.proposal {
            if proposal.waiting.contains(&self.seat.seat)
                && self.seat.undo_voted != Some(proposal.id)
            {
                link.send(ClientMsg::VoteUndo {
                    id: proposal.id,
                    accept: true,
                });
                self.seat.undo_voted = Some(proposal.id);
            }
            return self.seat.ended.is_none() && !link.closed();
        }
        if std::mem::take(&mut self.seat.new_game) {
            self.auto_deal = self.seat.deck.is_some();
            self.last_decision = None;
            self.sent = None;
            self.pilot.release();
            if let Some(Mind::Llm(brain)) = self.mind.as_mut() {
                brain.forget_game();
            }
        }
        if let Some(session) = self.seat.session.as_ref() {
            let seq = session.view().next_seq;
            if seq != self.seat.last_seq {
                self.refresh_view();
            }
            if self.auto_deal && self.seat.deck.is_some() && !self.seat.dealt {
                deal(&mut self.seat, link, &mut self.out);
                self.auto_deal = false;
            }
            if let Some(playmat) = self.playmat_wanted.take() {
                link.send(ClientMsg::PickPlaymat {
                    playmat: Some(playmat.clone()),
                });
                self.out.line(format!("playmat → {playmat}"));
            }
            if seq != self.last_reveal_seq {
                self.last_reveal_seq = seq;
                for (index, affordance) in
                    self.seat.last_view.affordances.clone().iter().enumerate()
                {
                    if matches!(affordance.kind, AffordanceKind::Reveal { .. })
                        && affordance.enabled
                    {
                        press(&mut self.seat, link, &mut self.out, index);
                    }
                }
            }
        }
        match self.mind.take() {
            Some(Mind::Llm(mut brain)) => {
                self.steer(link, &mut brain);
                self.mind = Some(Mind::Llm(brain));
            }
            Some(mind @ (Mind::Random | Mind::Auto)) => {
                self.mind = Some(mind);
                self.roll(link);
            }
            None => {}
        }
        self.seat.ended.is_none() && !link.closed()
    }

    fn in_flight(&self) -> bool {
        let Some(sent) = &self.sent else {
            return false;
        };
        let answered = sent.folds != self.seat.own_folds || sent.refusals != self.seat.refusals;
        (!answered && sent.at.elapsed() < FOLD_WAIT) || sent.at.elapsed() < self.pace
    }

    fn steer(&mut self, link: &mut dyn Link, brain: &mut Brain) {
        let fresh = self.chat.poll(brain, &mut self.out);
        let in_game =
            self.seat.session.is_some() && self.seat.dealt && self.seat.last_view.winner.is_none();
        let step = if in_game {
            if !fresh && self.in_flight() {
                return;
            }
            let board = self.seat.board_cards();
            self.pilot
                .step(&self.seat.last_view, self.seat.seat, &board, fresh)
        } else {
            self.pilot.release();
            Step::Model { held: Vec::new() }
        };
        match step {
            Step::Idle => {}
            Step::Press { index, .. } => {
                press(&mut self.seat, link, &mut Out::quiet(), index);
                self.sent = Some(Sent {
                    folds: self.seat.own_folds,
                    refusals: self.seat.refusals,
                    at: Instant::now(),
                });
            }
            Step::Model { held } => {
                if let Some(line) = self.pilot.stretch() {
                    self.out.line(line);
                }
                let halt = self.halt.clone();
                let halted = move || {
                    halt.as_ref()
                        .is_some_and(|flag| flag.load(Ordering::Relaxed))
                };
                if think(
                    brain,
                    &mut self.seat,
                    link,
                    &mut self.out,
                    &mut self.last_decision,
                    &mut self.chat,
                    fresh,
                    held,
                    &halted,
                ) {
                    self.decisions += 1;
                }
                if let Some(until) = brain.take_hold().filter(|_| self.seat.dealt) {
                    let said = until.describe();
                    let board = self.seat.board_cards();
                    if let Err(dropped) =
                        self.pilot
                            .hold(until, &self.seat.last_view, self.seat.seat, &board)
                    {
                        self.out
                            .line(format!("ai hold dropped: {}", dropped.reason));
                        hold_dropped(&mut self.seat, brain, &said, &dropped);
                        if !dropped.again {
                            self.last_decision = None;
                        } else if nudge(&mut self.seat, link, &mut self.out, &halted) {
                            self.last_decision = decision_key(&self.seat);
                        }
                    }
                }
                self.sent = None;
            }
        }
    }

    fn roll(&mut self, link: &mut dyn Link) {
        if self.seat.session.is_none() {
            return;
        }
        let view = self.seat.last_view.clone();
        if view.winner.is_some() {
            return;
        }
        let options = random::options(&view, self.seat.seat);
        if options.is_empty() || self.in_flight() {
            return;
        }
        let auto = match self.mind {
            Some(Mind::Auto) => auto_choice(&view, self.seat.seat).map(|(choice, _)| choice),
            _ => None,
        };
        let Some(choice) = auto.or_else(|| random::pick(&mut self.rng, &options)) else {
            return;
        };
        let zone_name = |zone: u16| self.seat.zone_label(Zone::Plugin(zone));
        self.out
            .line(format!("ai picks {}", choice.describe(&zone_name)));
        choose(&mut self.seat, link, &mut self.out, choice);
        self.decisions += 1;
        self.sent = Some(Sent {
            folds: self.seat.own_folds,
            refusals: self.seat.refusals,
            at: Instant::now(),
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn think(
    brain: &mut Brain,
    seat: &mut Seat,
    link: &mut dyn Link,
    out: &mut Out,
    last_decision: &mut Option<(u64, Vec<String>)>,
    chat: &mut Chat,
    fresh_chat: bool,
    held: Vec<String>,
    halted: &dyn Fn() -> bool,
) -> bool {
    let Some(key) = decision_key(seat) else {
        return false;
    };
    let needs_deck = seat.deck.is_none() || !seat.dealt;
    let new_situation = !key.1.is_empty() && last_decision.as_ref() != Some(&key);
    let woken = !held.is_empty();
    if !new_situation && !fresh_chat && !woken && !(needs_deck && last_decision.is_none()) {
        return false;
    }
    *last_decision = Some(key.clone());
    if !seat.dealt {
        brain.forget_game();
    }
    let state = snapshot(seat);
    let card_names: Vec<String> = seat
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
    let zone_names: Vec<String> = seat.zones().iter().map(|decl| decl.name.clone()).collect();
    let decks: Vec<String> = catalog_dir()
        .map(|dir| crate::deck::history::store::rows_in(&dir, agni_riftbound::GAME))
        .unwrap_or_default()
        .into_iter()
        .map(|row| row.label)
        .collect();
    let situation = Situation {
        seat_name: seat.seat_name(seat.seat),
        state,
        card_names,
        zone_names,
        messages: chat.messages.clone(),
        decks,
        deck_loaded: seat
            .deck
            .as_ref()
            .map(|record| crate::deck::history::label(&record.deck)),
        dealt: seat.dealt,
        held: held.iter().map(|line| seat.expand_notice(line)).collect(),
    };
    out.line(format!("ai deciding (seq {})…", key.0));
    let mut fault = None;
    let pending: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let outcome = {
        let mut exec = |line: &str| -> String {
            for text in pending.borrow_mut().drain(..) {
                out.line(text);
            }
            if over(seat, link, halted) {
                return DECISION_OVER.to_string();
            }
            out.line(format!("ai> {line}"));
            if let Some(text) = line.strip_prefix("say ") {
                chat.say(text);
                return "said".to_string();
            }
            run_command(seat, link, line, out);
            settle(seat, link, out, SETTLE_TICKS, halted);
            let mut lines = snapshot(seat);
            if actionable(&seat.last_view).is_empty() {
                lines.push(DECISION_OVER.to_string());
            }
            lines.join("\n")
        };
        let mut log = |text: &str| {
            if let Some(error) = text.strip_prefix("ai: ") {
                if error != crate::ai::brain::HALTED {
                    fault = Some(error.to_string());
                }
            }
            if !text.starts_with("ai> ") {
                pending.borrow_mut().push(text.to_string());
            }
        };
        brain.decide_until(&situation, &mut exec, &mut log, halted)
    };
    for text in pending.into_inner() {
        out.line(text);
    }
    seat.fault = fault;
    out.line(format!(
        "ai finished: {} commands, {}{}",
        outcome.commands.len(),
        outcome.usage_line(),
        if outcome.notes_changed {
            ", notes updated"
        } else {
            ""
        }
    ));
    if over(seat, link, halted) || decision_key(seat).as_ref() != Some(&key) {
        return true;
    }
    if let Some(until) = brain.hold_requested() {
        out.line(format!("ai holds until {}", until.describe()));
        return true;
    }
    if nudge(seat, link, out, halted) {
        *last_decision = decision_key(seat);
    }
    true
}

pub fn hold_dropped(seat: &mut Seat, brain: &mut Brain, until: &str, dropped: &Dropped) {
    let line = format!("your hold until {until} was dropped: {}", dropped.reason);
    brain.hold_dropped(&line);
    seat.notices.push(line);
}

pub fn nudge(
    seat: &mut Seat,
    link: &mut dyn Link,
    out: &mut Out,
    halted: &dyn Fn() -> bool,
) -> bool {
    let Some(index) = fallback(&seat.last_view, seat.seat) else {
        return false;
    };
    let label = seat.last_view.affordances[index].label.clone();
    out.line(format!(
        "ai idle with {label} still offered: pressing it so the table moves on"
    ));
    press(seat, link, out, index);
    settle(seat, link, out, SETTLE_TICKS, halted);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_ai_votes_once_and_waits_for_the_host() {
        #[derive(Default)]
        struct TestLink(Vec<ClientMsg>);
        impl Link for TestLink {
            fn send(&mut self, msg: ClientMsg) {
                self.0.push(msg);
            }
            fn poll(&mut self) -> Vec<NetToGame> {
                Vec::new()
            }
        }
        let mut driver = Driver::new(Out::quiet());
        driver.seat.seat = 1;
        driver.seat.undo.proposal = Some(agni_net::session::UndoProposal {
            id: 7,
            requester: 0,
            actions: 2,
            waiting: vec![1],
        });
        let mut link = TestLink::default();
        assert!(driver.tick(&mut link));
        assert!(driver.tick(&mut link));
        assert_eq!(
            link.0,
            vec![ClientMsg::VoteUndo {
                id: 7,
                accept: true
            }]
        );
        driver.seat.undo.proposal = None;
        assert!(driver.tick(&mut link));
        assert_eq!(link.0.len(), 1);
    }

    #[test]
    fn spawn_takes_a_multi_word_token_name_before_the_zone() {
        assert_eq!(
            spawn_args(&["Sand", "Soldier", "battlefield-1"]),
            Some(("Sand Soldier".into(), "battlefield-1".into(), None))
        );
        assert_eq!(
            spawn_args(&["Shadow_Clone", "base", "3"]),
            Some(("Shadow Clone".into(), "base".into(), Some(3)))
        );
        assert_eq!(
            spawn_args(&["Tentacle", "base"]),
            Some(("Tentacle".into(), "base".into(), None))
        );
        assert_eq!(spawn_args(&["Tentacle"]), None);
        let known = agni_riftbound::token_table();
        let (token, _, _) = spawn_args(&["sand", "soldier", "bf1"]).unwrap();
        assert!(known
            .iter()
            .any(|decl| decl.name.eq_ignore_ascii_case(&token)));
    }

    #[test]
    fn a_host_notice_is_printed_once_and_reaches_the_next_snapshot_once() {
        let mut seat = Seat::new();
        let mut out = Out::capture();
        handle_event(
            &mut seat,
            NetToGame::FromHost {
                msg: HostMsg::Notice {
                    text: "refused: it is not your turn".into(),
                },
            },
            &mut out,
        );
        handle_event(
            &mut seat,
            NetToGame::FromHost {
                msg: HostMsg::Notice {
                    text: "welcome".into(),
                },
            },
            &mut out,
        );
        assert_eq!(
            out.take(),
            ["refused: it is not your turn", "host: welcome"]
        );
        assert_eq!(
            seat.refusals, 1,
            "a refusal is counted, a plain notice is not"
        );
        let lines = snapshot(&mut seat);
        assert!(lines.contains(&"refused: it is not your turn".to_string()));
        assert!(lines.contains(&"host: welcome".to_string()));
        let again = snapshot(&mut seat);
        assert!(!again.iter().any(|line| line.starts_with("refused")));
        assert!(!again.iter().any(|line| line.starts_with("host:")));
    }

    fn plain(label: &str, hotkey: Option<&str>) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(String::from),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![13]),
            card: None,
        }
    }

    fn strip(labels: &[(&str, Option<&str>)], prompt_for: Option<u8>) -> PluginView {
        PluginView {
            status: Vec::new(),
            affordances: labels
                .iter()
                .map(|(label, hotkey)| plain(label, *hotkey))
                .collect(),
            winner: None,
            prompt: prompt_for.map(|seat| agni_sim::wire::PromptSummary {
                seat,
                why: "where does it enter?".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            ..Default::default()
        }
    }

    fn named(id: u32) -> String {
        match id {
            50 => "Vi".to_string(),
            77 => "Jinx".to_string(),
            116 => "Punching Poro".to_string(),
            _ => format!("card {id}"),
        }
    }

    fn zoned(id: u16) -> String {
        match id {
            1 => "base".to_string(),
            2 => "battlefield-1".to_string(),
            3 => "chain".to_string(),
            _ => format!("zone-{id}"),
        }
    }

    fn seated(seat: u8) -> String {
        format!("seat-{seat}")
    }

    #[test]
    fn legal_rows_print_their_kinds_and_their_destinations() {
        let row = Legal {
            card: 116,
            kinds: vec![LegalKind::Play { accelerate: true }],
            zones: vec![1, 2],
            hidden: Vec::new(),
        };
        assert_eq!(
            legal_line(&row, &named, &zoned),
            "legal: #116 Punching Poro — play (accelerate) → base, battlefield-1"
        );
        let march = Legal {
            card: 50,
            kinds: vec![
                LegalKind::March,
                LegalKind::Activate { ability: 0 },
                LegalKind::Activate { ability: 2 },
            ],
            zones: vec![2],
            hidden: Vec::new(),
        };
        assert_eq!(
            legal_line(&march, &named, &zoned),
            "legal: #50 Vi — march, activate → battlefield-1",
            "two payable abilities are one word: the ability index is not an action number"
        );
        let answer = Legal {
            card: 77,
            kinds: vec![LegalKind::Answer, LegalKind::React],
            zones: Vec::new(),
            hidden: Vec::new(),
        };
        assert_eq!(
            legal_line(&answer, &named, &zoned),
            "legal: #77 Jinx — answer, react"
        );
        let hide = Legal {
            card: 116,
            kinds: vec![LegalKind::Hide],
            zones: Vec::new(),
            hidden: vec![2],
        };
        assert_eq!(
            legal_line(&hide, &named, &zoned),
            "legal: #116 Punching Poro — hide · hide at battlefield-1",
            "the AI reads hiding through this one word"
        );
        let both = Legal {
            card: 116,
            kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
            zones: vec![1],
            hidden: vec![2],
        };
        assert_eq!(
            legal_line(&both, &named, &zoned),
            "legal: #116 Punching Poro — play, hide → base · hide at battlefield-1",
            "a card that is both playable and hideable names both, and each destination list is its own"
        );
        let bare = Legal {
            card: 9,
            kinds: Vec::new(),
            zones: Vec::new(),
            hidden: Vec::new(),
        };
        assert_eq!(
            legal_line(&bare, &named, &zoned),
            "legal: #9 card 9 — nothing"
        );
    }

    #[test]
    fn arrows_print_their_kind_and_both_endpoints() {
        let spell = Arrow {
            from: Origin::Card(116),
            to: TargetRef::Card(50),
            kind: ArrowKind::Spell,
        };
        assert_eq!(
            arrow_line(&spell, &named, &zoned, &seated),
            "arrow: spell #116 Punching Poro → #50 Vi"
        );
        let ability = Arrow {
            from: Origin::Item(3),
            to: TargetRef::Seat(1),
            kind: ArrowKind::Ability,
        };
        assert_eq!(
            arrow_line(&ability, &named, &zoned, &seated),
            "arrow: ability chain item 3 → seat-1"
        );
        let counter = Arrow {
            from: Origin::Card(77),
            to: TargetRef::Item(2),
            kind: ArrowKind::Counter,
        };
        assert_eq!(
            arrow_line(&counter, &named, &zoned, &seated),
            "arrow: counter #77 Jinx → chain item 2"
        );
        let attack = Arrow {
            from: Origin::Card(50),
            to: TargetRef::Zone(2),
            kind: ArrowKind::Attack,
        };
        assert_eq!(
            arrow_line(&attack, &named, &zoned, &seated),
            "arrow: attack #50 Vi → battlefield-1"
        );
        let combat = Arrow {
            from: Origin::Card(50),
            to: TargetRef::Card(77),
            kind: ArrowKind::Combat,
        };
        assert_eq!(
            arrow_line(&combat, &named, &zoned, &seated),
            "arrow: combat #50 Vi → #77 Jinx"
        );
    }

    #[test]
    fn the_legal_and_arrow_lines_carry_the_tags_the_brain_reads() {
        let row = Legal {
            card: 116,
            kinds: vec![LegalKind::March],
            zones: vec![2],
            hidden: Vec::new(),
        };
        let arrow = Arrow {
            from: Origin::Card(116),
            to: TargetRef::Zone(2),
            kind: ArrowKind::Attack,
        };
        let legal = legal_line(&row, &named, &zoned);
        let aimed = arrow_line(&arrow, &named, &zoned, &seated);
        assert!(legal.starts_with(crate::ai::brain::LEGAL_TAG));
        assert!(aimed.starts_with(crate::ai::brain::ARROW_TAG));
        let state = vec![
            "turn: turn 2 · rae · action phase · rules enforced".to_string(),
            legal.clone(),
            aimed.clone(),
        ];
        assert_eq!(
            crate::ai::brain::tagged(&state, crate::ai::brain::LEGAL_TAG),
            vec![legal]
        );
        assert_eq!(
            crate::ai::brain::tagged(&state, crate::ai::brain::ARROW_TAG),
            vec![aimed]
        );
    }

    #[test]
    fn the_panic_button_never_wakes_the_ai_and_an_idle_decision_falls_back_to_a_safe_press() {
        let waiting = strip(&[("free table", None)], None);
        assert!(actionable(&waiting).is_empty());
        assert_eq!(fallback(&waiting, 0), None);
        let attacking = strip(&[("pass", Some("w")), ("free table", None)], None);
        assert_eq!(actionable(&attacking), ["pass"]);
        assert_eq!(fallback(&attacking, 0), Some(0));
        let quiet = strip(&[("end turn", Some("space")), ("free table", None)], None);
        assert_eq!(fallback(&quiet, 0), Some(0));
        let mulligan = strip(
            &[
                ("set aside {card 1}", None),
                ("set aside {card 2}", None),
                ("keep", None),
                ("free table", None),
            ],
            Some(0),
        );
        assert_eq!(fallback(&mulligan, 0), Some(2));
        let location = strip(
            &[
                ("your base", None),
                ("{zone 9}", None),
                ("cancel", Some("x")),
                ("free table", None),
            ],
            Some(0),
        );
        assert_eq!(fallback(&location, 0), Some(0));
        let mut optional = strip(
            &[
                ("{card 50}", None),
                ("{card 60}", None),
                ("done", None),
                ("skip", None),
                ("cancel", Some("x")),
                ("free table", None),
            ],
            Some(0),
        );
        let summary = optional.prompt.as_mut().unwrap();
        summary.min = 0;
        summary.max = 2;
        summary.optional = true;
        assert_eq!(
            fallback(&optional, 0),
            Some(3),
            "an untouched optional prompt is skipped, not finished for nothing"
        );
        optional.prompt.as_mut().unwrap().picked = 1;
        assert_eq!(fallback(&optional, 0), Some(2));
        let staged = strip(&[("{zone 9}", None), ("{zone 10}", None)], Some(0));
        assert_eq!(fallback(&staged, 0), Some(0));
        let theirs = strip(&[("{zone 9}", None), ("{zone 10}", None)], Some(1));
        assert_eq!(fallback(&theirs, 0), None);
        let mut disabled = strip(&[("pass", Some("w"))], None);
        disabled.affordances[0].enabled = false;
        assert!(actionable(&disabled).is_empty());
        assert_eq!(fallback(&disabled, 0), None);
        let mut hidden = strip(&[("concede", None), ("confirm free table", None)], None);
        hidden.hidden = vec![0, 1];
        assert!(
            actionable(&hidden).is_empty(),
            "table-menu verbs never reach the AI's list"
        );
        assert_eq!(fallback(&hidden, 0), None);
    }

    #[test]
    fn a_notice_names_the_seats_it_mentions() {
        let mut seat = Seat::new();
        seat.roster = ["rae", "ada"]
            .iter()
            .enumerate()
            .map(|(index, name)| SeatInfo {
                seat: index as u8,
                name: (*name).into(),
                host: index == 0,
                connected: true,
                color: index as u8,
                playmat: None,
            })
            .collect();
        let mut out = Out::capture();
        handle_event(
            &mut seat,
            NetToGame::FromHost {
                msg: HostMsg::Notice {
                    text: "refused: the question is for {seat 1}".into(),
                },
            },
            &mut out,
        );
        assert_eq!(out.take(), ["refused: the question is for ada"]);
        assert_eq!(
            seat.expand_notice("the game is over: {seat 0} won · {card 7}"),
            "the game is over: rae won · card 7"
        );
    }

    #[test]
    fn prompt_options_read_with_their_card_hotkey_and_availability() {
        let expand = |text: &str| text.replace("{card 36}", "Punching Poro");
        let mut option = Affordance {
            label: "set aside {card 36}".into(),
            hotkey: None,
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![10, 1, 0, 0, 0]),
            card: Some(36),
        };
        assert_eq!(
            action_line(0, &option, &expand),
            "action 1: set aside Punching Poro (#36)"
        );
        option.card = None;
        option.label = "cancel".into();
        option.hotkey = Some("x".into());
        assert_eq!(action_line(2, &option, &expand), "action 3: cancel [x]");
        option.enabled = false;
        option.hotkey = None;
        option.kind = AffordanceKind::Commit { roll: 2 };
        option.label = "roll".into();
        assert_eq!(
            action_line(0, &option, &expand),
            "action 1: roll (commit) (unavailable)"
        );
    }

    #[test]
    fn the_xp_tail_and_the_ransom_prompt_print_in_the_presenters_words() {
        let mut seat = Seat::new();
        seat.roster = ["rae", "ada"]
            .iter()
            .enumerate()
            .map(|(index, name)| SeatInfo {
                seat: index as u8,
                name: (*name).into(),
                host: index == 0,
                connected: true,
                color: index as u8,
                playmat: None,
            })
            .collect();
        assert_eq!(
            seat.expand_notice("points · {seat 0} 2 · {seat 1} 0 · xp {seat 0} 1 · {seat 1} 3"),
            "points · rae 2 · ada 0 · xp rae 1 · ada 3"
        );
        let view = PluginView {
            prompt: Some(agni_sim::wire::PromptSummary {
                seat: 0,
                why: "pay 2 energy to keep {card 71}?".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            affordances: vec![
                Affordance {
                    label: "yes".into(),
                    hotkey: None,
                    enabled: true,
                    kind: AffordanceKind::Plain,
                    data: ByteBuf::from(vec![1]),
                    card: None,
                },
                Affordance {
                    label: "no".into(),
                    hotkey: Some("x".into()),
                    enabled: true,
                    kind: AffordanceKind::Plain,
                    data: ByteBuf::from(vec![2]),
                    card: None,
                },
            ],
            ..Default::default()
        };
        let expand = |text: &str| seat.expand_notice(text);
        assert_eq!(
            expand(&prompt_line(view.prompt.as_ref().unwrap(), 0)),
            "pay 2 energy to keep card 71? · pick one"
        );
        let lines: Vec<String> = (0..2)
            .map(|index| {
                let mut line = action_line(index, &view.affordances[index], &expand);
                if let Some(wording) = answer_wording(&view, 0, index) {
                    line.push_str(&format!(" ({wording})"));
                }
                line
            })
            .collect();
        assert_eq!(
            lines,
            [
                "action 1: yes (pay 2 energy)",
                "action 2: no [x] (let it resolve)"
            ]
        );
        assert_eq!(fallback(&view, 0), Some(1), "an idle brain lets it resolve");
    }

    #[test]
    fn the_auto_choice_is_the_desktops_decision_over_the_seats_view() {
        let pass = PluginView {
            status: vec!["turn 2 · {seat 1} · action phase · rules enforced".into()],
            affordances: vec![plain("pass", Some("w"))],
            ..Default::default()
        };
        let (choice, is_pass) = auto_choice(&pass, 0).expect("nothing to respond with");
        assert!(is_pass);
        assert_eq!(
            choice,
            Choice::Action {
                index: 0,
                label: "pass".into()
            }
        );
        assert_eq!(random::options(&pass, 0), vec![choice]);
        let mut response = pass.clone();
        response.legal = vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![13],
            ..Default::default()
        }];
        assert_eq!(auto_choice(&response, 0), None);
        assert_eq!(random::options(&response, 0).len(), 2);
    }

    #[test]
    fn a_mind_kind_parses_the_soak_and_cli_spellings() {
        assert_eq!(MindKind::parse("random"), Some(MindKind::Random));
        assert_eq!(MindKind::parse("auto"), Some(MindKind::Auto));
        assert_eq!(MindKind::parse("desktop"), Some(MindKind::Auto));
        assert_eq!(MindKind::parse("nanogpt"), Some(MindKind::Llm));
        assert_eq!(MindKind::parse("llm"), Some(MindKind::Llm));
        assert_eq!(MindKind::parse("clever"), None);
        assert_eq!(Mind::Random.label(), "random");
        assert_eq!(Mind::Auto.label(), "random+auto");
        assert!(MindKind::Llm.is_llm());
        assert!(!MindKind::Random.is_llm());
    }
}
