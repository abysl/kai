use super::brain::DECISION_OVER;
use super::brain::{Brain, Situation};
use super::driver::{self, Driver, Link, Mind, MindKind, Out};
use super::hold::{Pilot, Step};
use super::nanogpt::Client;
use super::provider::Credentials;
use crate::deck::import::ImportedDeck;
use agni_net::bridge::NetToGame;
use agni_net::session::{ClientMsg, HostMsg, WIRE_VERSION};
use std::cell::RefCell;
use std::path::PathBuf;
use web_time::{Duration, Instant};

pub const CHAT_FILE: &str = "chat.log";
pub const CONN_BASE: u64 = 1 << 62;

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
    static NEXT: RefCell<u64> = const { RefCell::new(CONN_BASE) };
    static FAREWELLS: RefCell<Vec<NetToGame>> = const { RefCell::new(Vec::new()) };
    static CHAT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
struct BrowserLink {
    incoming: Vec<HostMsg>,
    outgoing: Vec<ClientMsg>,
}

impl Link for BrowserLink {
    fn send(&mut self, msg: ClientMsg) {
        self.outgoing.push(msg);
    }
    fn poll(&mut self) -> Vec<NetToGame> {
        self.incoming
            .drain(..)
            .map(|msg| NetToGame::FromHost { msg })
            .collect()
    }
}

pub struct Config {
    pub kind: MindKind,
    pub model: String,
    pub credentials: Credentials,
    deck: Option<(ImportedDeck, String, usize)>,
}

impl Config {
    pub fn new(_dir: PathBuf, kind: MindKind, model: &str) -> Self {
        Self {
            kind,
            model: model.into(),
            credentials: Credentials::default(),
            deck: None,
        }
    }

    pub fn with_deck(mut self, deck: ImportedDeck, label: &str, battlefield: usize) -> Self {
        self.deck = Some((deck, label.into(), battlefield));
        self
    }
}

pub struct Status {
    pub alive: bool,
    pub kind: MindKind,
    pub model: String,
    pub log: PathBuf,
    pub exit: Option<String>,
    pub fault: Option<String>,
    pub resumed: bool,
    pub deck: String,
}

impl Status {
    pub fn seat_line(&self) -> String {
        format!("bot is at the table with {}", self.deck)
    }
    pub fn fault_line(&self) -> Option<String> {
        self.fault
            .as_ref()
            .map(|fault| format!("AI paused: {fault}. Stop the AI and check its settings."))
    }
}

struct Live {
    conn: u64,
    epoch: u64,
    announced: bool,
    left: bool,
    driver: Driver,
    link: BrowserLink,
    brain: Option<Brain>,
    credentials: Credentials,
    kind: MindKind,
    model: String,
    pilot: Pilot,
    key: Option<(u64, Vec<String>)>,
    thinking: bool,
    fresh_chat: bool,
    next_action: Instant,
}

pub fn start(config: Config) -> Result<(), String> {
    if config.kind.is_llm() {
        config.credentials.validate(&config.model)?;
    }
    stop();
    clear_chat();
    let conn = NEXT.with(|next| {
        let mut next = next.borrow_mut();
        *next += 1;
        *next
    });
    let mut driver = Driver::new(Out::quiet()).with_pace(Duration::from_millis(700));
    if let Some((deck, _, battlefield)) = config.deck {
        driver = driver.with_deck(deck, Some(battlefield));
    }
    if !config.kind.is_llm() {
        driver = driver.with_mind(Mind::Random);
    }
    let mut link = BrowserLink::default();
    link.send(ClientMsg::Join {
        name: super::seat::DEFAULT_NAME.into(),
        version: WIRE_VERSION,
    });
    let brain = config
        .kind
        .is_llm()
        .then(|| new_brain(&config.model, &config.credentials));
    LIVE.with(|slot| {
        *slot.borrow_mut() = Some(Live {
            conn,
            epoch: 0,
            announced: false,
            left: false,
            driver,
            link,
            brain,
            credentials: config.credentials,
            kind: config.kind,
            model: config.model,
            pilot: Pilot::default(),
            key: None,
            thinking: false,
            fresh_chat: false,
            next_action: Instant::now(),
        })
    });
    Ok(())
}

fn new_brain(model: &str, credentials: &Credentials) -> Brain {
    Brain::new(
        Client::configured(model, credentials.clone()),
        super::cards::CardTexts::pool(),
        None,
    )
}

pub fn status() -> Option<Status> {
    LIVE.with(|slot| {
        slot.borrow().as_ref().map(|live| Status {
            alive: live.driver.seat.ended.is_none(),
            kind: live.kind,
            model: live.model.clone(),
            log: PathBuf::new(),
            exit: live.driver.seat.ended.clone(),
            fault: live.driver.seat.fault.clone(),
            resumed: live.driver.seat.resumed,
            deck: live
                .driver
                .deck_label()
                .unwrap_or_else(|| "a chosen deck".into()),
        })
    })
}

pub fn stop() {
    end("the AI was stopped");
}

pub fn end(reason: &str) {
    if let Some(live) = LIVE.with(|slot| slot.borrow_mut().take()) {
        if live.left {
            return;
        }
        FAREWELLS.with(|events| {
            events.borrow_mut().push(NetToGame::PeerLeft {
                conn: live.conn,
                reason: reason.into(),
            })
        });
    }
}

pub fn deliver(conn: u64, msg: HostMsg) -> Option<HostMsg> {
    if conn < CONN_BASE {
        return Some(msg);
    }
    LIVE.with(|slot| {
        if let Some(live) = slot.borrow_mut().as_mut().filter(|live| live.conn == conn) {
            if let HostMsg::Undo { status } = &msg {
                if status.proposal.is_some() {
                    invalidate(live);
                }
                live.driver.seat.undo = status.clone();
            }
            if let HostMsg::End { reason } = &msg {
                live.driver.seat.ended = Some(reason.clone());
                live.link.outgoing.clear();
            }
            if matches!(
                &msg,
                HostMsg::RolledBack { .. }
                    | HostMsg::Entry {
                        entry: agni_sim::log::LogEntry {
                            action: agni_sim::log::LogAction::Reset,
                            ..
                        }
                    }
            ) {
                invalidate(live);
            }
            live.link.incoming.push(msg);
        }
    });
    None
}

fn invalidate(live: &mut Live) {
    live.epoch += 1;
    live.link
        .outgoing
        .retain(|msg| !matches!(msg, ClientMsg::Intent { .. }));
    live.thinking = false;
    live.key = None;
    live.pilot.release();
    live.driver.seat.fault = None;
    live.brain = live
        .kind
        .is_llm()
        .then(|| new_brain(&live.model, &live.credentials));
}

pub fn events() -> Vec<NetToGame> {
    let mut events = FAREWELLS.with(|held| held.take());
    LIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(live) = slot.as_mut() else {
            return;
        };
        if !live.announced {
            live.announced = true;
            events.push(NetToGame::PeerJoined {
                conn: live.conn,
                peer: "local:ai".into(),
            });
        }
        live.driver.tick(&mut live.link);
        if live.driver.seat.ended.is_none() {
            tick_model(live);
        } else if !live.left {
            live.left = true;
            events.push(NetToGame::PeerLeft {
                conn: live.conn,
                reason: live.driver.seat.ended.clone().unwrap_or_default(),
            });
        }
        events.extend(
            live.link
                .outgoing
                .drain(..)
                .map(|msg| NetToGame::PeerFrame {
                    conn: live.conn,
                    msg,
                }),
        );
    });
    events
}

fn tick_model(live: &mut Live) {
    if !live.kind.is_llm()
        || live.thinking
        || live.driver.seat.fault.is_some()
        || live.driver.seat.undo.proposal.is_some()
        || Instant::now() < live.next_action
    {
        return;
    }
    let Some(key) = driver::decision_key(&live.driver.seat) else {
        return;
    };
    let fresh =
        std::mem::take(&mut live.fresh_chat) | std::mem::take(&mut live.driver.seat.fresh_chat);
    if fresh {
        live.key = None;
    }
    if key.1.is_empty() && live.driver.seat.dealt && !fresh {
        return;
    }
    let board = live.driver.seat.board_cards();
    let step = live.pilot.step(
        &live.driver.seat.last_view,
        live.driver.seat.seat,
        &board,
        fresh,
    );
    let held = match step {
        Step::Idle => return,
        Step::Press { index, .. } => {
            driver::press(
                &mut live.driver.seat,
                &mut live.link,
                &mut live.driver.out,
                index,
            );
            live.next_action = Instant::now() + Duration::from_millis(700);
            return;
        }
        Step::Model { held } => held,
    };
    if live.key.as_ref() == Some(&key) {
        return;
    }
    live.key = Some(key);
    let Some(brain) = live.brain.take() else {
        return;
    };
    let seat = &mut live.driver.seat;
    let state = driver::snapshot(seat);
    let situation = Situation {
        seat_name: seat.seat_name(seat.seat),
        state,
        card_names: seat
            .table()
            .map(|table| {
                table
                    .cards()
                    .iter()
                    .filter(|card| !card.face.name.is_empty())
                    .map(|card| card.face.name.clone())
                    .collect()
            })
            .unwrap_or_default(),
        zone_names: seat.zones().iter().map(|zone| zone.name.clone()).collect(),
        messages: seat
            .messages
            .iter()
            .cloned()
            .chain(
                chat_lines()
                    .into_iter()
                    .filter_map(|line| line.strip_prefix("you: ").map(str::to_string)),
            )
            .collect(),
        decks: Vec::new(),
        deck_loaded: seat
            .deck
            .as_ref()
            .map(|record| crate::deck::history::label(&record.deck)),
        dealt: seat.dealt,
        held,
    };
    let (conn, epoch) = (live.conn, live.epoch);
    live.thinking = true;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (brain, situation, conn, epoch);
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let mut brain = brain;
        let result = brain.decide_async(&situation, conn, epoch).await;
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(live) = slot
                .as_mut()
                .filter(|live| live.conn == conn && live.epoch == epoch)
            else {
                return;
            };
            if let Err(error) = result {
                if error != super::brain::HALTED {
                    live.driver.seat.fault = Some(error);
                }
            }
            if let Some(until) = brain.take_hold() {
                let board = live.driver.seat.board_cards();
                if let Err(dropped) = live.pilot.hold(
                    until,
                    &live.driver.seat.last_view,
                    live.driver.seat.seat,
                    &board,
                ) {
                    brain.hold_dropped(&dropped.reason);
                    if !dropped.again {
                        live.key = None;
                    }
                }
            }
            live.brain = Some(brain);
            live.thinking = false;
            live.next_action = Instant::now() + Duration::from_millis(700);
        });
    });
}

pub fn decision_current(conn: u64, epoch: u64) -> bool {
    LIVE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|live| {
            live.conn == conn
                && live.epoch == epoch
                && live.driver.seat.ended.is_none()
                && live.driver.seat.undo.proposal.is_none()
        })
    })
}

pub async fn execute(conn: u64, epoch: u64, command: &str) -> String {
    if !decision_current(conn, epoch) {
        return DECISION_OVER.into();
    }
    if let Some(text) = command.strip_prefix("deck-action ") {
        let args: serde_json::Value = match serde_json::from_str(text) {
            Ok(args) => args,
            Err(_) => return "Invalid deck action JSON".into(),
        };
        let remote = if let Some(request) = crate::deck::actions::remote(&args) {
            #[cfg(target_arch = "wasm32")]
            let reply = crate::deck::service::perform(&request).await;
            #[cfg(not(target_arch = "wasm32"))]
            let reply = crate::deck::service::perform(&request);
            Some(reply)
        } else {
            None
        };
        if !decision_current(conn, epoch) {
            return DECISION_OVER.into();
        }
        return LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let live = slot.as_mut().expect("current decision");
            let result = match remote {
                Some(Ok(reply)) => {
                    crate::deck::actions::imported(&mut live.driver.seat.draft, reply)
                }
                Some(Err(error)) => Err(error),
                None => super::decks::local(&mut live.driver.seat, &mut live.link, &args),
            };
            match result {
                Ok(value) => value.to_string(),
                Err(error) => serde_json::json!({"error":error}).to_string(),
            }
        });
    }
    if let Some(text) = command.strip_prefix("say ") {
        append_chat(format!("bot: {text}"));
        LIVE.with(|slot| {
            if let Some(live) = slot.borrow_mut().as_mut() {
                live.driver.command(&mut live.link, command);
            }
        });
        return "said".into();
    }
    let before = LIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let live = slot.as_mut().expect("current decision");
        let before = (live.driver.seat.own_folds, live.driver.seat.refusals);
        live.driver.command(&mut live.link, command);
        before
    });
    for _ in 0..50 {
        #[cfg(target_arch = "wasm32")]
        super::web_http::pause(100).await;
        #[cfg(not(target_arch = "wasm32"))]
        bevy::tasks::futures_lite::future::yield_now().await;
        if !decision_current(conn, epoch) {
            return DECISION_OVER.into();
        }
        let answered = LIVE.with(|slot| {
            slot.borrow().as_ref().is_some_and(|live| {
                (live.driver.seat.own_folds, live.driver.seat.refusals) != before
            })
        });
        if answered {
            break;
        }
    }
    LIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(live) = slot.as_mut() else {
            return DECISION_OVER.into();
        };
        let mut state = driver::snapshot(&mut live.driver.seat);
        if driver::actionable(&live.driver.seat.last_view).is_empty() {
            state.push(DECISION_OVER.into());
        }
        state.join("\n")
    })
}

fn append_chat(line: String) {
    CHAT.with(|lines| {
        let mut lines = lines.borrow_mut();
        lines.push(line);
        if lines.len() > 100 {
            lines.remove(0);
        }
    });
}

pub fn say(text: &str) {
    append_chat(format!("you: {}", text.trim()));
    LIVE.with(|slot| {
        if let Some(live) = slot.borrow_mut().as_mut() {
            live.key = None;
            live.fresh_chat = true;
        }
    });
}

pub fn switch_model(model: &str) {
    LIVE.with(|slot| {
        if let Some(live) = slot.borrow_mut().as_mut() {
            live.model = model.trim().into();
            invalidate(live);
        }
    });
}

pub fn chat_lines() -> Vec<String> {
    CHAT.with(|lines| lines.borrow().clone())
}
pub fn clear_chat() {
    CHAT.with(|lines| lines.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn begin() -> (u64, u64) {
        stop();
        FAREWELLS.with(|events| events.borrow_mut().clear());
        start(Config::new(PathBuf::new(), MindKind::Random, "")).unwrap();
        LIVE.with(|slot| {
            let slot = slot.borrow();
            let live = slot.as_ref().unwrap();
            (live.conn, live.epoch)
        })
    }

    #[test]
    fn browser_seat_joins_through_messages_and_stops_once() {
        let (conn, epoch) = begin();
        let first = events();
        assert!(matches!(first[0], NetToGame::PeerJoined { conn: id, .. } if id == conn));
        assert!(matches!(
            first[1],
            NetToGame::PeerFrame {
                msg: ClientMsg::Join {
                    version: WIRE_VERSION,
                    ..
                },
                ..
            }
        ));
        assert!(events().is_empty());
        assert!(decision_current(conn, epoch));
        stop();
        assert!(!decision_current(conn, epoch));
        assert!(
            matches!(events().as_slice(), [NetToGame::PeerLeft { conn: id, .. }] if *id == conn)
        );
        assert!(events().is_empty());
    }

    #[test]
    fn reset_rollback_replacement_and_undo_invalidate_old_decisions() {
        let (conn, epoch) = begin();
        deliver(
            conn,
            HostMsg::RolledBack {
                next_seq: 0,
                faces: Vec::new(),
            },
        );
        assert!(!decision_current(conn, epoch));
        let next_epoch = LIVE.with(|slot| slot.borrow().as_ref().unwrap().epoch);
        deliver(
            conn,
            HostMsg::Entry {
                entry: agni_sim::log::LogEntry::new(0, 0, agni_sim::log::LogAction::Reset),
            },
        );
        assert!(!decision_current(conn, next_epoch));
        let (replacement, epoch) = begin();
        assert_ne!(conn, replacement);
        assert!(!decision_current(conn, epoch));
        LIVE.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .unwrap()
                .driver
                .seat
                .undo
                .proposal = Some(agni_net::session::UndoProposal {
                id: 1,
                requester: 0,
                actions: 1,
                waiting: vec![1],
            })
        });
        assert!(!decision_current(replacement, epoch));
        stop();
    }

    #[test]
    fn browser_keys_are_required_only_for_model_players() {
        stop();
        assert!(start(Config::new(PathBuf::new(), MindKind::Llm, "model")).is_err());
        assert!(status().is_none());
        begin();
        say("hello");
        assert_eq!(chat_lines(), ["you: hello"]);
        clear_chat();
        assert!(chat_lines().is_empty());
        stop();
    }

    #[test]
    fn browser_context_uses_only_its_own_private_faces() {
        use agni_net::session::{ClientSession, HostSession};
        let (conn, _) = begin();
        let mut host = HostSession::new("human");
        let (seat, _) = host.join("bot").unwrap();
        let _ = host
            .deal(0, vec![agni_core::CardFace::named("Human secret")])
            .unwrap();
        let (_, faces) = host
            .deal(seat, vec![agni_core::CardFace::named("AI private card")])
            .unwrap();
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let live = slot.as_mut().unwrap();
            live.driver.seat.seat = seat;
            live.driver.seat.session = Some(ClientSession::from_welcome(
                seat,
                host.roster(),
                host.log().to_vec(),
            ));
        });
        deliver(conn, HostMsg::Faces { faces });
        events();
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let live = slot.as_mut().unwrap();
            let text = driver::snapshot(&mut live.driver.seat).join("\n");
            assert!(text.contains("AI private card"));
            assert!(!text.contains("Human secret"));
        });
        deliver(
            conn,
            HostMsg::End {
                reason: "test ended".into(),
            },
        );
        let ended = events();
        assert!(ended
            .iter()
            .any(|event| matches!(event, NetToGame::PeerLeft { .. })));
        stop();
        assert!(events().is_empty());
    }

    #[test]
    fn an_async_model_move_waits_for_the_host_and_a_stopped_decision_does_nothing() {
        use super::super::nanogpt::{Reply, ToolCall};
        use agni_net::session::{ClientSession, HostSession};
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        let (conn, epoch) = begin();
        let mut host = HostSession::with_config(
            "human",
            agni_sim::log::TableConfig {
                zones: agni_riftbound::zone_table(),
                ..Default::default()
            },
        );
        let (seat, _) = host.join("bot").unwrap();
        let (_, faces) = host
            .deal_to(
                seat,
                vec![agni_core::CardFace::named("Synthetic card")],
                agni_core::Zone::Plugin(agni_riftbound::ZONE_HAND),
            )
            .unwrap();
        let card = faces[0].0;
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let live = slot.as_mut().unwrap();
            live.driver = Driver::new(Out::quiet());
            live.driver.seat.seat = seat;
            live.driver.seat.dealt = true;
            live.driver.seat.session = Some(ClientSession::from_welcome(
                seat,
                host.roster(),
                host.log().to_vec(),
            ));
        });
        deliver(conn, HostMsg::Faces { faces });
        events();
        let situation = Situation {
            seat_name: "bot".into(),
            state: Vec::new(),
            card_names: Vec::new(),
            zone_names: vec!["base".into()],
            messages: Vec::new(),
            decks: Vec::new(),
            deck_loaded: Some("test".into()),
            dealt: true,
            held: Vec::new(),
        };
        let client = Client::canned(
            "test",
            vec![Reply {
                tool_calls: vec![ToolCall {
                    id: "move-one".into(),
                    name: "move".into(),
                    arguments: serde_json::json!({"card": card, "zone": "base"}).to_string(),
                }],
                ..Reply::default()
            }],
        );
        let mut brain = Brain::new(
            client.clone(),
            super::super::cards::CardTexts::default(),
            None,
        );
        let mut task = Box::pin(brain.decide_async(&situation, conn, epoch));
        let mut context = Context::from_waker(Waker::noop());
        let mut completed = false;
        let mut requests = 0;
        for _ in 0..100 {
            if let Poll::Ready(result) = task.as_mut().poll(&mut context) {
                result.unwrap();
                completed = true;
                break;
            }
            for event in events() {
                if let NetToGame::PeerFrame {
                    msg: ClientMsg::Intent { intent },
                    ..
                } = event
                {
                    requests += 1;
                    for entry in host.intent(seat, intent).unwrap() {
                        deliver(conn, HostMsg::Entry { entry });
                    }
                }
            }
        }
        assert!(completed);
        assert_eq!(requests, 1);
        assert_eq!(
            host.table().get(agni_core::CardId(card)).unwrap().zone,
            agni_core::Zone::Plugin(agni_riftbound::ZONE_BASE)
        );
        drop(task);
        stop();
        let result = bevy::tasks::block_on(brain.decide_async(&situation, conn, epoch));
        assert_eq!(result.unwrap_err(), super::super::brain::HALTED);
        assert_eq!(client.canned_left(), 0);
    }
}
