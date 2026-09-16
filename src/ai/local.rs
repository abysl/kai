use crate::ai::driver::{self, Driver, Link, Mind, MindKind, Out};
use crate::deck::import::ImportedDeck;
use agni_net::bridge::NetToGame;
use agni_net::session::{ClientMsg, HostMsg, WIRE_VERSION};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub const NODE: &str = "local:ai";
pub const CONN_BASE: u64 = 1 << 62;
pub const DEFAULT_PACE: Duration = Duration::from_millis(700);
pub const LOG_FILE: &str = "seat.log";
pub const CHAT_FILE: &str = "chat.log";
pub const NOTES_FILE: &str = "ai-notes.md";

static NEXT_CONN: AtomicU64 = AtomicU64::new(CONN_BASE);
static LIVE: Mutex<Option<Live>> = Mutex::new(None);
static FAREWELLS: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());

pub fn is_local_conn(conn: u64) -> bool {
    conn >= CONN_BASE
}

pub struct ChannelLink {
    rx: mpsc::Receiver<HostMsg>,
    tx: mpsc::Sender<ClientMsg>,
    closed: bool,
}

impl Link for ChannelLink {
    fn send(&mut self, msg: ClientMsg) {
        let _ = self.tx.send(msg);
    }

    fn poll(&mut self) -> Vec<NetToGame> {
        let mut events = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(msg) => events.push(NetToGame::FromHost { msg }),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.closed {
                        self.closed = true;
                        events.push(NetToGame::Dropped {
                            reason: "the table closed".into(),
                        });
                    }
                    break;
                }
            }
        }
        events
    }

    fn closed(&self) -> bool {
        self.closed
    }
}

pub struct Ports {
    to_seat: mpsc::Sender<HostMsg>,
    from_seat: mpsc::Receiver<ClientMsg>,
}

pub fn channel() -> (Ports, ChannelLink) {
    let (to_seat, rx) = mpsc::channel();
    let (tx, from_seat) = mpsc::channel();
    (
        Ports { to_seat, from_seat },
        ChannelLink {
            rx,
            tx,
            closed: false,
        },
    )
}

#[derive(Default)]
struct Shared {
    ended: Mutex<Option<String>>,
    fault: Mutex<Option<String>>,
    resumed: AtomicBool,
}

struct Live {
    conn: u64,
    ports: Ports,
    announced: bool,
    left: bool,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
    shared: Arc<Shared>,
    kind: MindKind,
    model: String,
    deck: String,
    log: PathBuf,
}

pub struct Config {
    pub name: String,
    pub kind: MindKind,
    pub model: String,
    pub deck: Option<(ImportedDeck, String)>,
    pub battlefield: Option<usize>,
    pub dir: PathBuf,
    pub pace: Duration,
    pub tick: Duration,
    pub seed: Option<u64>,
}

impl Config {
    pub fn new(dir: PathBuf, kind: MindKind, model: &str) -> Self {
        Self {
            name: crate::ai::seat::DEFAULT_NAME.into(),
            kind,
            model: model.into(),
            deck: None,
            battlefield: None,
            dir,
            pace: DEFAULT_PACE,
            tick: driver::TICK,
            seed: None,
        }
    }

    pub fn with_deck(mut self, deck: ImportedDeck, label: &str, battlefield: usize) -> Self {
        self.deck = Some((deck, label.into()));
        self.battlefield = Some(battlefield);
        self
    }

    pub fn deck_label(&self) -> String {
        self.deck
            .as_ref()
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| "a deck of its choosing".into())
    }

    pub fn brain_label(&self) -> String {
        match self.kind {
            MindKind::Llm => self.model.clone(),
            other => other.label().to_string(),
        }
    }
}

pub struct Status {
    pub alive: bool,
    pub kind: MindKind,
    pub model: String,
    pub deck: String,
    pub log: PathBuf,
    pub exit: Option<String>,
    pub fault: Option<String>,
    pub resumed: bool,
}

impl Status {
    pub fn seat_line(&self) -> String {
        if self.resumed {
            format!(
                "{} resumed its seat with the deck already on the table",
                crate::ai::seat::DEFAULT_NAME
            )
        } else {
            format!(
                "{} is at the table with {}",
                crate::ai::seat::DEFAULT_NAME,
                self.deck
            )
        }
    }

    pub fn fault_line(&self) -> Option<String> {
        self.fault
            .as_ref()
            .map(|fault| format!("the model is failing — {fault}"))
    }
}

pub fn status() -> Option<Status> {
    let slot = LIVE.lock();
    let live = slot.as_ref()?;
    let alive = !live.thread.is_finished();
    let exit = (!alive).then(|| {
        live.shared
            .ended
            .lock()
            .clone()
            .unwrap_or_else(|| "the seat thread stopped".into())
    });
    let fault = live.shared.fault.lock().clone();
    Some(Status {
        alive,
        kind: live.kind,
        model: live.model.clone(),
        deck: live.deck.clone(),
        log: live.log.clone(),
        exit,
        fault,
        resumed: live.shared.resumed.load(Ordering::Relaxed),
    })
}

pub fn start(config: Config) -> Result<(), String> {
    stop_with("replaced by a new seat");
    std::fs::create_dir_all(&config.dir)
        .map_err(|error| format!("{}: {error}", config.dir.display()))?;
    let log = config.dir.join(LOG_FILE);
    let _ = std::fs::remove_file(&log);
    let chat = config.dir.join(CHAT_FILE);
    let _ = std::fs::remove_file(&chat);
    let notes = config.dir.join(NOTES_FILE);
    let out = Out::file(&log)?;
    let conn = NEXT_CONN.fetch_add(1, Ordering::SeqCst);
    let (ports, mut link) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared::default());
    let kind = config.kind;
    let model = config.brain_label();
    let deck_label = config.deck_label();
    let worker_stop = stop.clone();
    let worker_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("kai-ai-seat".into())
        .spawn(move || {
            let Config {
                name,
                kind,
                model,
                deck,
                battlefield,
                pace,
                tick,
                seed,
                ..
            } = config;
            let mut out = out;
            let mind = match kind {
                MindKind::Llm => driver::llm_mind(&model, Some(notes), &mut out),
                MindKind::Random => Mind::Random,
                MindKind::Auto => Mind::Auto,
            };
            let mut driver = Driver::new(out)
                .with_mind(mind)
                .with_chat(Some(chat))
                .with_pace(pace)
                .with_halt(worker_stop.clone());
            if let Some(seed) = seed {
                driver = driver.with_seed(seed);
            }
            if let Some((deck, _)) = deck {
                driver = driver.with_deck(deck, battlefield);
            }
            driver
                .out
                .line(format!("joining the table in-process as {name}"));
            link.send(ClientMsg::Join {
                name,
                version: WIRE_VERSION,
            });
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                while !worker_stop.load(Ordering::Relaxed) {
                    let going = driver.tick(&mut link);
                    *worker_shared.fault.lock() = driver.seat.fault.clone();
                    worker_shared
                        .resumed
                        .store(driver.seat.resumed, Ordering::Relaxed);
                    if !going {
                        break;
                    }
                    std::thread::sleep(tick);
                }
            }));
            let fallback = match run {
                Ok(()) => driver
                    .seat
                    .ended
                    .clone()
                    .unwrap_or_else(|| "stopped".into()),
                Err(payload) => format!("the seat thread panicked: {}", panic_text(&payload)),
            };
            let reason = worker_shared
                .ended
                .lock()
                .get_or_insert_with(|| fallback)
                .clone();
            driver.out.line(format!("seat over: {reason}"));
        })
        .map_err(|error| format!("the AI seat thread did not start: {error}"))?;
    *LIVE.lock() = Some(Live {
        conn,
        ports,
        announced: false,
        left: false,
        stop,
        thread,
        shared,
        kind,
        model,
        deck: deck_label,
        log,
    });
    Ok(())
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
        })
        .unwrap_or_else(|| "no message".into())
}

pub(crate) fn stop_with(reason: &str) -> Option<JoinHandle<()>> {
    let live = LIVE.lock().take()?;
    live.shared
        .ended
        .lock()
        .get_or_insert_with(|| reason.to_string());
    live.stop.store(true, Ordering::Relaxed);
    if !live.left {
        FAREWELLS.lock().push((live.conn, reason.to_string()));
    }
    Some(live.thread)
}

pub fn stop() {
    stop_with("the AI seat was stopped from the lobby");
}

pub fn end(reason: &str) {
    stop_with(reason);
}

pub fn events() -> Vec<NetToGame> {
    let mut events: Vec<NetToGame> = FAREWELLS
        .lock()
        .drain(..)
        .map(|(conn, reason)| NetToGame::PeerLeft { conn, reason })
        .collect();
    let mut slot = LIVE.lock();
    let Some(live) = slot.as_mut() else {
        return events;
    };
    if !live.announced {
        live.announced = true;
        events.push(NetToGame::PeerJoined {
            conn: live.conn,
            peer: NODE.into(),
        });
    }
    while let Ok(msg) = live.ports.from_seat.try_recv() {
        events.push(NetToGame::PeerFrame {
            conn: live.conn,
            msg,
        });
    }
    if live.thread.is_finished() && !live.left {
        live.left = true;
        let reason = live
            .shared
            .ended
            .lock()
            .clone()
            .unwrap_or_else(|| "the seat thread stopped".into());
        events.push(NetToGame::PeerLeft {
            conn: live.conn,
            reason,
        });
    }
    events
}

pub fn deliver(conn: u64, msg: HostMsg) -> Option<HostMsg> {
    if !is_local_conn(conn) {
        return Some(msg);
    }
    let slot = LIVE.lock();
    match slot.as_ref() {
        Some(live) if live.conn == conn => {
            let _ = live.ports.to_seat.send(msg);
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::random::{self, Rng};
    use crate::net::HostState;
    use crate::table::plugin_ui::{action_bytes, RollSecrets};
    use crate::table::SessionInfo;
    use agni_engine_host::{load_plugin, PLUGIN_GAS_BUDGET};
    use agni_net::session::{HostSession, WireIntent};
    use agni_sim::engine::PluginModule;
    use agni_sim::log::TableConfig;
    use agni_sim::wire::AffordanceKind;
    use serde_bytes::ByteBuf;
    use std::time::Instant;

    static SERIAL: Mutex<()> = Mutex::new(());

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SeatEvent {
        Joined,
        Frame,
        Deal,
        Left,
        Other,
    }

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

    fn hosted() -> HostState {
        let (engine, _) = crate::engine::session_engine();
        let session = HostSession::with_engine(
            "rae",
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
        HostState::hosted(session)
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kai-local-seat-{tag}-{}-{}",
            std::process::id(),
            crate::os::entropy::secret()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn host_seat_plays(host: &mut HostState, rng: &mut Rng, secrets: &mut RollSecrets) -> bool {
        let Some(view) = host.plugin_view(0) else {
            return false;
        };
        if view.winner.is_some() {
            return false;
        }
        let reveal = view.affordances.iter().position(|affordance| {
            affordance.enabled && matches!(affordance.kind, AffordanceKind::Reveal { .. })
        });
        let choice = match reveal {
            Some(index) => random::Choice::Action {
                index,
                label: view.affordances[index].label.clone(),
            },
            None => match random::pick(rng, &random::options(&view, 0)) {
                Some(choice) => choice,
                None => return false,
            },
        };
        let intent = match choice {
            random::Choice::Action { index, .. } => {
                let secret = rng.secret();
                let fresh = move || secret;
                match action_bytes(&view.affordances[index], secrets, &fresh) {
                    Some(data) => WireIntent::Game {
                        data: ByteBuf::from(data),
                    },
                    None => return false,
                }
            }
            random::Choice::Move { card, zone, hidden } => {
                let zones = host.session().view().zones.clone();
                assert!(zones.iter().any(|decl| decl.id == zone), "the zone exists");
                let target = 0;
                let index = host
                    .session()
                    .table()
                    .in_area(agni_core::PlayerId(target), agni_core::Zone::Plugin(zone))
                    .count() as u32;
                if hidden {
                    WireIntent::MoveHidden {
                        card,
                        to: agni_core::Zone::Plugin(zone),
                        seat: target,
                        index,
                    }
                } else {
                    WireIntent::Move {
                        card,
                        to: agni_core::Zone::Plugin(zone),
                        seat: target,
                        index,
                    }
                }
            }
        };
        let _ = host.own_intent(0, intent);
        true
    }

    fn pump(host: &mut HostState, info: &mut SessionInfo) -> Vec<SeatEvent> {
        let mut seen = Vec::new();
        for event in events() {
            seen.push(match &event {
                NetToGame::PeerJoined { .. } => SeatEvent::Joined,
                NetToGame::PeerFrame {
                    msg: ClientMsg::DealDeck { .. },
                    ..
                } => SeatEvent::Deal,
                NetToGame::PeerFrame { .. } => SeatEvent::Frame,
                NetToGame::PeerLeft { .. } => SeatEvent::Left,
                _ => SeatEvent::Other,
            });
            match event {
                NetToGame::PeerJoined { conn, peer } => host.peer_joined(conn, peer),
                NetToGame::PeerFrame { conn, msg } => {
                    host.peer_message(info, conn, msg);
                }
                NetToGame::PeerLeft { conn, reason } => {
                    host.peer_left(info, conn, &reason);
                }
                _ => {}
            }
        }
        seen
    }

    #[test]
    fn a_local_random_seat_joins_the_hosted_table_in_process_and_a_game_runs_to_a_winner() {
        let _guard = SERIAL.lock();
        let dir = scratch_dir("game");
        let deck = crate::ai::soak::pool_deck("lillia").expect("the lillia pool deck");
        let mut winner = None;
        let mut turns = 0;
        for attempt in 0..3u64 {
            let mut host = hosted();
            let mut info = SessionInfo {
                role: crate::table::SessionRole::Host,
                roster: host.session().roster(),
                ..SessionInfo::default()
            };
            let mut config = Config::new(dir.clone(), MindKind::Random, "").with_deck(
                ImportedDeck::Riftbound(deck.clone()),
                "Lillia (house)",
                0,
            );
            config.pace = Duration::ZERO;
            config.tick = Duration::from_millis(1);
            config.seed = Some(11 + attempt);
            start(config).expect("the seat starts");
            let mut rng = Rng::seeded(99 + attempt);
            let mut secrets = RollSecrets::default();
            let started = Instant::now();
            let mut joined = false;
            let mut dealt_host = false;
            turns = 0;
            loop {
                let seen = pump(&mut host, &mut info);
                joined |= seen.contains(&SeatEvent::Joined);
                assert!(
                    !seen.contains(&SeatEvent::Left),
                    "the seat left early: {}",
                    status().and_then(|status| status.exit).unwrap_or_default()
                );
                if info.roster.len() == 2 && !dealt_host {
                    let irelia = crate::ai::soak::pool_deck("irelia").unwrap();
                    let record = crate::deck::import::SeatedDeckRecord {
                        seat: agni_core::PlayerId(0),
                        faces: crate::deck::import::face_map(&ImportedDeck::Riftbound(
                            irelia.clone(),
                        )),
                        deck: ImportedDeck::Riftbound(irelia),
                        battlefield: Some(0),
                        battlefield_played: false,
                    };
                    let groups = crate::deck::import::deal_plan_for(&record);
                    host.own_deal(0, groups).expect("the host's deck deals");
                    dealt_host = true;
                }
                if dealt_host {
                    host_seat_plays(&mut host, &mut rng, &mut secrets);
                    let view = host.plugin_view(0).unwrap();
                    turns = crate::ai::soak::turn_of(&view.status).unwrap_or(turns);
                    if let Some(seat) = view.winner {
                        winner = Some(seat);
                        break;
                    }
                    if turns > crate::ai::soak::DEFAULT_TURN_CAP {
                        break;
                    }
                    assert!(
                        started.elapsed() < Duration::from_secs(240),
                        "the game did not end in time (turn {turns}): {}",
                        view.status.join(" | ")
                    );
                }
                assert!(
                    started.elapsed() < Duration::from_secs(30) || dealt_host,
                    "the seat joins"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(joined, "the seat was announced to the host as a peer");
            assert_eq!(
                info.roster
                    .iter()
                    .map(|seat| seat.name.as_str())
                    .collect::<Vec<_>>(),
                ["rae", "bot"],
                "the AI sits in the roster under its own name"
            );
            let handle = stop_with("the test is over").expect("the seat was live");
            let waited = Instant::now();
            while !handle.is_finished() {
                assert!(waited.elapsed() < Duration::from_secs(10), "the seat stops");
                std::thread::sleep(Duration::from_millis(5));
            }
            let seen = pump(&mut host, &mut info);
            assert!(
                seen.contains(&SeatEvent::Left),
                "the host hears the seat leave"
            );
            assert!(!info.roster[1].connected, "the seat is marked gone");
            assert!(status().is_none(), "nothing is live after a stop");
            if winner.is_some() {
                break;
            }
        }
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("seated as player 2"), "{log}");
        assert!(log.contains("ai picks"), "{log}");
        assert!(
            winner.is_some(),
            "a random game reaches a winner inside the turn cap (last game: turn {turns})"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn host_deals(host: &mut HostState) {
        let irelia = crate::ai::soak::pool_deck("irelia").unwrap();
        let record = crate::deck::import::SeatedDeckRecord {
            seat: agni_core::PlayerId(0),
            faces: crate::deck::import::face_map(&ImportedDeck::Riftbound(irelia.clone())),
            deck: ImportedDeck::Riftbound(irelia),
            battlefield: Some(0),
            battlefield_played: false,
        };
        let groups = crate::deck::import::deal_plan_for(&record);
        host.own_deal(0, groups).expect("the host's deck deals");
    }

    fn pump_until(
        host: &mut HostState,
        info: &mut SessionInfo,
        deals: &mut usize,
        what: &str,
        done: impl Fn(&HostState, &SessionInfo, usize) -> bool,
    ) {
        let started = Instant::now();
        loop {
            let seen = pump(host, info);
            *deals += seen
                .iter()
                .filter(|event| **event == SeatEvent::Deal)
                .count();
            assert!(
                !seen.contains(&SeatEvent::Left),
                "the seat left early: {}",
                status().and_then(|status| status.exit).unwrap_or_default()
            );
            if done(host, info, *deals) {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(60), "{what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn the_seat_deals_its_deck_again_after_the_hosts_new_game() {
        let _guard = SERIAL.lock();
        let dir = scratch_dir("reset");
        let deck = crate::ai::soak::pool_deck("lillia").expect("the lillia pool deck");
        let mut host = hosted();
        let mut info = SessionInfo {
            role: crate::table::SessionRole::Host,
            roster: host.session().roster(),
            ..SessionInfo::default()
        };
        let mut config = Config::new(dir.clone(), MindKind::Random, "").with_deck(
            ImportedDeck::Riftbound(deck),
            "Lillia (house)",
            0,
        );
        config.pace = Duration::from_secs(60);
        config.tick = Duration::from_millis(1);
        config.seed = Some(5);
        start(config).expect("the seat starts");
        let mut deals = 0;
        pump_until(
            &mut host,
            &mut info,
            &mut deals,
            "the seat joins",
            |_, info, _| info.roster.len() == 2,
        );
        host_deals(&mut host);
        pump_until(
            &mut host,
            &mut info,
            &mut deals,
            "the seat deals its deck",
            |host, _, deals| deals == 1 && host.deck_dealt(1),
        );
        host.new_game().expect("the table resets");
        assert!(
            !host.deck_dealt(1),
            "the reset swept the seat's deck off the table"
        );
        pump_until(
            &mut host,
            &mut info,
            &mut deals,
            "the seat deals again after the new game",
            |host, _, deals| deals == 2 && host.deck_dealt(1),
        );
        let handle = stop_with("the test is over").expect("the seat was live");
        let waited = Instant::now();
        while !handle.is_finished() {
            assert!(waited.elapsed() < Duration::from_secs(10), "the seat stops");
            std::thread::sleep(Duration::from_millis(5));
        }
        pump(&mut host, &mut info);
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("host started a new game"), "{log}");
        assert_eq!(
            log.matches("deal requested").count(),
            2,
            "one deal per game: {log}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_seat_that_reclaims_its_place_keeps_the_deck_already_on_the_table() {
        let _guard = SERIAL.lock();
        let dir = scratch_dir("reclaim");
        let deck = crate::ai::soak::pool_deck("lillia").expect("the lillia pool deck");
        let mut host = hosted();
        let mut info = SessionInfo {
            role: crate::table::SessionRole::Host,
            roster: host.session().roster(),
            ..SessionInfo::default()
        };
        let seat_config = |label: &str| {
            let mut config = Config::new(dir.clone(), MindKind::Random, "").with_deck(
                ImportedDeck::Riftbound(deck.clone()),
                label,
                0,
            );
            config.pace = Duration::from_secs(60);
            config.tick = Duration::from_millis(1);
            config
        };
        start(seat_config("Lillia (house)")).expect("the seat starts");
        let mut deals = 0;
        pump_until(
            &mut host,
            &mut info,
            &mut deals,
            "the seat joins",
            |_, info, _| info.roster.len() == 2,
        );
        host_deals(&mut host);
        pump_until(
            &mut host,
            &mut info,
            &mut deals,
            "the seat deals its deck",
            |host, _, deals| deals == 1 && host.deck_dealt(1),
        );
        assert!(status().is_some_and(|status| !status.resumed));
        let handle = stop_with("stopped from the lobby").unwrap();
        while !handle.is_finished() {
            std::thread::sleep(Duration::from_millis(2));
        }
        pump(&mut host, &mut info);
        start(seat_config("Lillia (again)")).expect("the seat starts again");
        let started = Instant::now();
        loop {
            let seen = pump(&mut host, &mut info);
            assert!(
                !seen.contains(&SeatEvent::Deal),
                "a reclaimed seat does not deal a second deck"
            );
            if status().is_some_and(|status| status.resumed) {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "the seat resumes its place"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(info.roster.len(), 2, "the bot took its old seat back");
        assert!(info.roster[1].connected);
        let status = status().unwrap();
        assert!(status.seat_line().contains("resumed its seat"));
        assert!(status.fault_line().is_none());
        assert_eq!(status.kind, MindKind::Random);
        let handle = stop_with("the test is over").unwrap();
        while !handle.is_finished() {
            std::thread::sleep(Duration::from_millis(2));
        }
        pump(&mut host, &mut info);
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(
            log.contains("already on the table — resuming with it"),
            "{log}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_seat_stops_when_the_table_closes() {
        let _guard = SERIAL.lock();
        let dir = scratch_dir("close");
        let mut host = hosted();
        let mut info = SessionInfo {
            role: crate::table::SessionRole::Host,
            roster: host.session().roster(),
            ..SessionInfo::default()
        };
        let mut config = Config::new(dir.clone(), MindKind::Random, "");
        config.tick = Duration::from_millis(1);
        start(config).unwrap();
        let started = Instant::now();
        while info.roster.len() < 2 {
            pump(&mut host, &mut info);
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "the seat joins"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(status().is_some_and(|status| status.alive));
        let handle = stop_with("the table closed").unwrap();
        let waited = Instant::now();
        while !handle.is_finished() {
            assert!(waited.elapsed() < Duration::from_secs(10), "the seat stops");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(pump(&mut host, &mut info).contains(&SeatEvent::Left));
        assert!(deliver(CONN_BASE, HostMsg::End { reason: "x".into() }).is_none());
        assert_eq!(
            deliver(7, HostMsg::End { reason: "x".into() }),
            Some(HostMsg::End { reason: "x".into() }),
            "a network conn falls through to the bridge"
        );
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("seat over: the table closed"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
