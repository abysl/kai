use agni_sim::engine::{Engine, PluginModule};
pub use agni_sim::pins::hash_hex;
use agni_sim::pins::{genesis_engine_pin, genesis_plugin_pin, pin_hash};
use agni_sim::wire::ZoneDecl;
use bevy::prelude::*;
use bevy_egui::egui;
use spirit_sdk::modules::Role;

const REFRESH_SECS: f32 = 1.0;

pub const RIFTBOUND_REF: &str = "riftbound";
pub const MTG_REF: &str = "mtg";

pub struct ActivePlugin {
    pub module: Option<Box<dyn PluginModule>>,
    pub bytes: Option<Vec<u8>>,
    pub note: Option<String>,
    pub zones: Option<Vec<ZoneDecl>>,
    pub counters: Option<Vec<agni_sim::wire::CounterDecl>>,
    pub tokens: Vec<agni_sim::wire::TokenDecl>,
    pub despawn_any: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRow {
    pub name: String,
    pub role: Role,
    pub version: String,
    pub hash_hex: String,
    pub signer: Option<String>,
    pub source: &'static str,
    pub held: bool,
}

pub enum JoinModules {
    Ready {
        engine: Box<dyn Engine>,
        plugin: Option<Box<dyn PluginModule>>,
        note: String,
    },
    Pending {
        status: String,
    },
    Refused {
        error: String,
    },
}

pub fn short_hex(hex: &str) -> String {
    hex.chars().take(8).collect()
}

pub enum PinState {
    Bytes { bytes: Vec<u8>, source: String },
    Pending(String),
    Failed(String),
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) mod fetch {
    use super::{hash_hex, short_hex, PinState};
    use agni_net::session::{module_hash, HostMsg, ModuleInbox};
    use std::collections::BTreeMap;

    pub const RETRY_POLLS: u64 = 60;
    pub const RETRY_CAP_POLLS: u64 = 1024;

    pub enum GatewayOutcome {
        Bytes(Vec<u8>),
        NotFound,
        Failed(String),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Ask {
        Gateway,
        Host,
    }

    enum Stage {
        Gateway {
            backoff: u64,
            refused: Option<String>,
        },
        Retry {
            at: u64,
            delay: u64,
            error: String,
            refused: Option<String>,
        },
        Host {
            via_gateway: bool,
        },
        HostRefused {
            reason: String,
        },
        Got {
            bytes: Vec<u8>,
            source: String,
        },
        Failed(String),
    }

    pub struct Fetches {
        polls: u64,
        stages: BTreeMap<[u8; 32], Stage>,
        inbox: ModuleInbox,
    }

    pub fn host_refusal(reason: &str) -> String {
        format!("the host refused to serve it: {reason}")
    }

    pub fn wrong_bytes(source: &str, hash: [u8; 32]) -> String {
        format!(
            "{source} served wrong bytes for pinned module {} — refusing",
            short_hex(&hash_hex(&hash))
        )
    }

    fn refused_everywhere(reason: &str) -> String {
        format!("{} and the gateway has no such blob", host_refusal(reason))
    }

    impl Fetches {
        pub const fn new() -> Self {
            Self {
                polls: 0,
                stages: BTreeMap::new(),
                inbox: ModuleInbox::new(),
            }
        }

        pub fn poll(&mut self, hash: [u8; 32], gateway: bool) -> (PinState, Option<Ask>) {
            self.polls += 1;
            let polls = self.polls;
            let Some(stage) = self.stages.get_mut(&hash) else {
                let (stage, ask) = if gateway {
                    (
                        Stage::Gateway {
                            backoff: 0,
                            refused: None,
                        },
                        Ask::Gateway,
                    )
                } else {
                    (Stage::Host { via_gateway: false }, Ask::Host)
                };
                self.stages.insert(hash, stage);
                return (PinState::Pending(source_of(ask)), Some(ask));
            };
            match stage {
                Stage::Gateway { .. } => (PinState::Pending(source_of(Ask::Gateway)), None),
                Stage::Host { .. } => (
                    PinState::Pending(match self.inbox.progress(hash) {
                        Some((received, total)) => {
                            format!("the host ({received} of {total} bytes)")
                        }
                        None => source_of(Ask::Host),
                    }),
                    None,
                ),
                Stage::HostRefused { reason } if gateway => {
                    *stage = Stage::Gateway {
                        backoff: 0,
                        refused: Some(reason.clone()),
                    };
                    (
                        PinState::Pending(source_of(Ask::Gateway)),
                        Some(Ask::Gateway),
                    )
                }
                Stage::HostRefused { reason } => {
                    let error = host_refusal(reason);
                    *stage = Stage::Failed(error.clone());
                    (PinState::Failed(error), None)
                }
                Stage::Retry { at, error, .. } if polls < *at => (
                    PinState::Pending(format!("the gateway (retrying after: {error})")),
                    None,
                ),
                Stage::Retry { delay, refused, .. } => {
                    *stage = Stage::Gateway {
                        backoff: *delay,
                        refused: refused.take(),
                    };
                    (
                        PinState::Pending(source_of(Ask::Gateway)),
                        Some(Ask::Gateway),
                    )
                }
                Stage::Got { bytes, source } => (
                    PinState::Bytes {
                        bytes: bytes.clone(),
                        source: source.clone(),
                    },
                    None,
                ),
                Stage::Failed(error) => (PinState::Failed(error.clone()), None),
            }
        }

        pub fn gateway_done(&mut self, hash: [u8; 32], outcome: GatewayOutcome) -> Option<Ask> {
            let (previous, refused) = match self.stages.get_mut(&hash) {
                Some(Stage::Gateway { backoff, refused }) => (*backoff, refused.take()),
                Some(Stage::Retry { delay, refused, .. }) => (*delay, refused.take()),
                _ => return None,
            };
            let stage = match outcome {
                GatewayOutcome::Bytes(bytes) if module_hash(&bytes) == hash => Stage::Got {
                    bytes,
                    source: "the gateway".into(),
                },
                GatewayOutcome::Bytes(_) => Stage::Failed(wrong_bytes("the gateway", hash)),
                GatewayOutcome::NotFound => match refused {
                    Some(reason) => Stage::Failed(refused_everywhere(&reason)),
                    None => Stage::Host { via_gateway: true },
                },
                GatewayOutcome::Failed(error) => {
                    let delay = if previous == 0 {
                        RETRY_POLLS
                    } else {
                        (previous * 2).min(RETRY_CAP_POLLS)
                    };
                    Stage::Retry {
                        at: self.polls + delay,
                        delay,
                        error,
                        refused,
                    }
                }
            };
            let ask = matches!(stage, Stage::Host { .. }).then_some(Ask::Host);
            self.stages.insert(hash, stage);
            ask
        }

        fn awaiting_host(&self, hash: [u8; 32]) -> bool {
            matches!(self.stages.get(&hash), Some(Stage::Host { .. }))
        }

        pub fn host_frame(&mut self, msg: &HostMsg) -> Option<([u8; 32], Vec<u8>)> {
            match msg {
                HostMsg::Module { hash, .. } if self.awaiting_host(*hash) => {
                    match self.inbox.receive(msg) {
                        Ok(Some(done)) => {
                            let bytes = self.inbox.take(done)?;
                            self.stages.insert(
                                done,
                                Stage::Got {
                                    bytes: bytes.clone(),
                                    source: "the host".into(),
                                },
                            );
                            Some((done, bytes))
                        }
                        Ok(None) => None,
                        Err(error) => {
                            self.stages
                                .insert(*hash, Stage::Failed(host_refusal(&error.to_string())));
                            None
                        }
                    }
                }
                HostMsg::NoModule { hash, reason } if self.awaiting_host(*hash) => {
                    let stage = match self.stages.get(hash) {
                        Some(Stage::Host { via_gateway: false }) => Stage::HostRefused {
                            reason: reason.clone(),
                        },
                        _ => Stage::Failed(host_refusal(reason)),
                    };
                    self.stages.insert(*hash, stage);
                    None
                }
                _ => None,
            }
        }

        pub fn reset(&mut self) {
            self.stages
                .retain(|_, stage| matches!(stage, Stage::Got { .. }));
            self.inbox.clear_open();
        }
    }

    fn source_of(ask: Ask) -> String {
        match ask {
            Ask::Gateway => "the gateway".into(),
            Ask::Host => "the host".into(),
        }
    }
}

static FETCHES: parking_lot::Mutex<fetch::Fetches> = parking_lot::Mutex::new(fetch::Fetches::new());

pub fn module_frame(msg: &agni_net::session::HostMsg) {
    let landed = FETCHES.lock().host_frame(msg);
    if let Some((hash, bytes)) = landed {
        platform::keep_fetched(hash, &bytes);
    }
}

pub fn reset_fetches() {
    FETCHES.lock().reset();
}

fn ask(hash: [u8; 32], who: fetch::Ask) {
    match who {
        fetch::Ask::Host => {
            agni_net::bridge::send_to_host(agni_net::session::ClientMsg::NeedModule { hash });
            platform::ask_mesh(hash);
        }
        fetch::Ask::Gateway => platform::ask_gateway(hash),
    }
}

fn fetched_state(hash: [u8; 32], gateway: bool) -> PinState {
    let (state, who) = FETCHES.lock().poll(hash, gateway);
    if let Some(who) = who {
        ask(hash, who);
    }
    state
}

struct PinnedState {
    kind: &'static str,
    short: String,
    state: PinState,
}

fn fetch_pin(
    kind: &'static str,
    pin: &str,
    fetch: impl FnOnce([u8; 32]) -> PinState,
) -> Result<PinnedState, JoinModules> {
    let Some(hash) = pin_hash(pin) else {
        return Err(JoinModules::Refused {
            error: format!("unparsable module pin {pin}"),
        });
    };
    Ok(PinnedState {
        kind,
        short: short_hex(&hash_hex(&hash)),
        state: fetch(hash),
    })
}

fn unresolved(pinned: &PinnedState) -> Option<JoinModules> {
    let PinnedState { kind, short, state } = pinned;
    match state {
        PinState::Bytes { .. } => None,
        PinState::Pending(from) => Some(JoinModules::Pending {
            status: format!("fetching pinned {kind} {short} from {from}…"),
        }),
        PinState::Failed(error) => Some(JoinModules::Refused {
            error: format!("pinned {kind} {short} unavailable — {error}"),
        }),
    }
}

fn settled(pins: &[&PinnedState]) -> Result<(), JoinModules> {
    let mut pending = None;
    for pin in pins {
        match unresolved(pin) {
            Some(refused @ JoinModules::Refused { .. }) => return Err(refused),
            Some(outcome) => {
                pending.get_or_insert(outcome);
            }
            None => {}
        }
    }
    pending.map_or(Ok(()), Err)
}

fn load_pinned<T>(
    pinned: PinnedState,
    load: impl FnOnce(&[u8]) -> Result<T, String>,
    notes: &mut Vec<String>,
) -> Result<T, JoinModules> {
    let PinnedState { kind, short, state } = pinned;
    let PinState::Bytes { bytes, source } = state else {
        return Err(
            unresolved(&PinnedState { kind, short, state }).expect("only settled bytes are loaded")
        );
    };
    match load(&bytes) {
        Ok(loaded) => {
            notes.push(format!("{kind} pinned @ {short} from {source}"));
            Ok(loaded)
        }
        Err(error) => Err(JoinModules::Refused {
            error: format!("pinned {kind} refused: {error}"),
        }),
    }
}

#[cfg(target_arch = "wasm32")]
fn resolve_pin<T>(
    kind: &'static str,
    pin: &str,
    fetch: impl FnOnce([u8; 32]) -> PinState,
    load: impl FnOnce(&[u8]) -> Result<T, String>,
    notes: &mut Vec<String>,
) -> Result<T, JoinModules> {
    let pinned = fetch_pin(kind, pin, fetch)?;
    settled(&[&pinned])?;
    load_pinned(pinned, load, notes)
}

fn join_modules(
    log: &[agni_sim::log::LogEntry],
    fetch: impl Fn([u8; 32]) -> PinState,
    load_engine: impl FnOnce(&[u8]) -> Result<Box<dyn Engine>, String>,
    load_plugin: impl FnOnce(&[u8]) -> Result<Box<dyn PluginModule>, String>,
    fallback_engine: impl FnOnce() -> (Box<dyn Engine>, Option<String>),
) -> JoinModules {
    let mut notes: Vec<String> = Vec::new();
    let engine_pin = match genesis_engine_pin(log) {
        None => None,
        Some(pin) => match fetch_pin("engine", &pin, &fetch) {
            Ok(pinned) => Some(pinned),
            Err(outcome) => return outcome,
        },
    };
    let plugin_pin = match genesis_plugin_pin(log) {
        None => None,
        Some(pin) => match fetch_pin("plugin", &pin, &fetch) {
            Ok(pinned) => Some(pinned),
            Err(outcome) => return outcome,
        },
    };
    let pins: Vec<&PinnedState> = [engine_pin.as_ref(), plugin_pin.as_ref()]
        .into_iter()
        .flatten()
        .collect();
    if let Err(outcome) = settled(&pins) {
        return outcome;
    }
    let engine = match engine_pin {
        None => {
            let (engine, note) = fallback_engine();
            notes.extend(note);
            engine
        }
        Some(pinned) => match load_pinned(pinned, load_engine, &mut notes) {
            Ok(engine) => engine,
            Err(outcome) => return outcome,
        },
    };
    let plugin = match plugin_pin {
        None => None,
        Some(pinned) => match load_pinned(pinned, load_plugin, &mut notes) {
            Ok(plugin) => Some(plugin),
            Err(outcome) => return outcome,
        },
    };
    JoinModules::Ready {
        engine,
        plugin,
        note: notes.join("; "),
    }
}

#[cfg(test)]
mod fetch_tests {
    use super::fetch::{Ask, Fetches, GatewayOutcome, RETRY_CAP_POLLS, RETRY_POLLS};
    use super::{join_modules, JoinModules, PinState};
    use agni_net::session::{engine_blob_ref, module_chunks, module_hash, HostMsg};
    use agni_sim::log::{LogAction, LogEntry, TableConfig};

    fn pending_from(state: &PinState) -> &str {
        match state {
            PinState::Pending(from) => from,
            PinState::Bytes { source, .. } => panic!("unexpected bytes from {source}"),
            PinState::Failed(error) => panic!("unexpected failure: {error}"),
        }
    }

    fn pinned_log(engine: [u8; 32], plugin: Option<[u8; 32]>) -> Vec<LogEntry> {
        vec![LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: Some(engine_blob_ref(engine)),
                    plugin: plugin.map(engine_blob_ref),
                    zones: Vec::new(),
                    options: None,
                    counters: Vec::new(),
                    despawn_any: false,
                },
            },
        )]
    }

    #[test]
    fn the_web_asks_the_gateway_first_and_the_host_only_after_a_404() {
        let bytes = b"the dev engine".to_vec();
        let hash = module_hash(&bytes);
        let mut fetches = Fetches::new();
        let (state, ask) = fetches.poll(hash, true);
        assert_eq!(pending_from(&state), "the gateway");
        assert_eq!(ask, Some(Ask::Gateway));
        let (state, ask) = fetches.poll(hash, true);
        assert_eq!(pending_from(&state), "the gateway");
        assert_eq!(ask, None, "one fetch in flight, not one per frame");

        assert_eq!(
            fetches.gateway_done(hash, GatewayOutcome::NotFound),
            Some(Ask::Host)
        );
        let (state, ask) = fetches.poll(hash, true);
        assert_eq!(pending_from(&state), "the host");
        assert_eq!(ask, None, "the host was asked when the 404 landed");

        let frames = module_chunks(hash, &bytes);
        assert_eq!(fetches.host_frame(&frames[0]), Some((hash, bytes.clone())));
        let (state, ask) = fetches.poll(hash, true);
        assert_eq!(ask, None);
        let PinState::Bytes { bytes: got, source } = state else {
            panic!("the host's bytes are ready");
        };
        assert_eq!(got, bytes);
        assert_eq!(source, "the host");
    }

    #[test]
    fn a_transient_gateway_error_retries_with_backoff_not_every_frame() {
        let hash = [3; 32];
        let mut fetches = Fetches::new();
        assert_eq!(fetches.poll(hash, true).1, Some(Ask::Gateway));
        assert_eq!(
            fetches.gateway_done(hash, GatewayOutcome::Failed("timed out".into())),
            None
        );
        for _ in 0..(RETRY_POLLS - 1) {
            let (state, ask) = fetches.poll(hash, true);
            assert_eq!(ask, None);
            assert!(pending_from(&state).contains("retrying after: timed out"));
        }
        assert_eq!(fetches.poll(hash, true).1, Some(Ask::Gateway));
        assert_eq!(
            fetches.gateway_done(hash, GatewayOutcome::Failed("timed out".into())),
            None
        );
        for _ in 0..(RETRY_POLLS * 2 - 1) {
            assert_eq!(fetches.poll(hash, true).1, None);
        }
        assert_eq!(fetches.poll(hash, true).1, Some(Ask::Gateway));
        let mut delay = RETRY_POLLS * 2;
        while delay < RETRY_CAP_POLLS {
            fetches.gateway_done(hash, GatewayOutcome::Failed("still down".into()));
            delay = (delay * 2).min(RETRY_CAP_POLLS);
            for _ in 0..(delay - 1) {
                assert_eq!(fetches.poll(hash, true).1, None);
            }
            assert_eq!(fetches.poll(hash, true).1, Some(Ask::Gateway));
        }
        fetches.gateway_done(hash, GatewayOutcome::Failed("still down".into()));
        for _ in 0..(RETRY_CAP_POLLS - 1) {
            assert_eq!(fetches.poll(hash, true).1, None);
        }
        assert_eq!(fetches.poll(hash, true).1, Some(Ask::Gateway));
    }

    #[test]
    fn without_a_gateway_the_host_is_asked_at_once_and_only_once() {
        let hash = [4; 32];
        let mut fetches = Fetches::new();
        let (state, ask) = fetches.poll(hash, false);
        assert_eq!(pending_from(&state), "the host");
        assert_eq!(ask, Some(Ask::Host));
        assert_eq!(fetches.poll(hash, false).1, None);
    }

    #[test]
    fn the_gateways_wrong_bytes_and_the_hosts_refusal_both_end_the_join_by_name() {
        let bytes = b"the dev engine".to_vec();
        let hash = module_hash(&bytes);
        let mut fetches = Fetches::new();
        fetches.poll(hash, true);
        fetches.gateway_done(hash, GatewayOutcome::Bytes(b"not those bytes".to_vec()));
        let PinState::Failed(error) = fetches.poll(hash, true).0 else {
            panic!("wrong bytes fail the pin");
        };
        assert!(error.contains("the gateway served wrong bytes"), "{error}");

        let mut fetches = Fetches::new();
        fetches.poll(hash, false);
        assert_eq!(
            fetches.host_frame(&HostMsg::NoModule {
                hash,
                reason: "this table pinned no module abc".into(),
            }),
            None
        );
        let PinState::Failed(error) = fetches.poll(hash, false).0 else {
            panic!("a refusal fails the pin");
        };
        assert_eq!(
            error,
            "the host refused to serve it: this table pinned no module abc"
        );

        let mut fetches = Fetches::new();
        fetches.poll(hash, false);
        let wrong = module_chunks(hash, b"tampered in flight");
        assert_eq!(fetches.host_frame(&wrong[0]), None);
        let PinState::Failed(error) = fetches.poll(hash, false).0 else {
            panic!("a mismatch fails the pin");
        };
        assert!(error.contains("refusing them"), "{error}");
    }

    #[test]
    fn a_refused_module_names_its_kind_hash_and_reason_in_the_join_status() {
        let engine = [
            0x21, 0xc5, 0x5f, 0x3a, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ];
        let plugin = [0xab; 32];
        let log = pinned_log(engine, Some(plugin));
        let asked = std::cell::RefCell::new(Vec::new());
        let outcome = join_modules(
            &log,
            |hash| {
                asked.borrow_mut().push(hash);
                if hash == engine {
                    PinState::Failed(
                        "the host refused to serve it: this table pinned no module 21c5".into(),
                    )
                } else {
                    PinState::Pending("the host".into())
                }
            },
            |_| Err("unused".into()),
            |_| Err("unused".into()),
            || panic!("a pinned engine never falls back"),
        );
        assert_eq!(
            asked.into_inner(),
            vec![engine, plugin],
            "both pins are asked for up front"
        );
        let JoinModules::Refused { error } = outcome else {
            panic!("a refusal ends the join");
        };
        assert_eq!(
            error,
            "pinned engine 21c55f3a unavailable — the host refused to serve it: this table pinned no module 21c5"
        );

        let outcome = join_modules(
            &log,
            |hash| {
                if hash == engine {
                    PinState::Bytes {
                        bytes: Vec::new(),
                        source: "the host".into(),
                    }
                } else {
                    PinState::Pending("the host (3 of 9 bytes)".into())
                }
            },
            |_| panic!("the engine is not compiled while the plugin is still crossing"),
            |_| Err("unused".into()),
            || panic!("a pinned engine never falls back"),
        );
        let JoinModules::Pending { status } = outcome else {
            panic!("the plugin is still crossing");
        };
        assert_eq!(
            status,
            "fetching pinned plugin abababab from the host (3 of 9 bytes)…"
        );

        let outcome = join_modules(
            &log,
            |hash| {
                if hash == engine {
                    PinState::Pending("the host".into())
                } else {
                    PinState::Failed("the host refused to serve it: no".into())
                }
            },
            |_| Err("unused".into()),
            |_| Err("unused".into()),
            || panic!("a pinned engine never falls back"),
        );
        assert!(
            matches!(outcome, JoinModules::Refused { ref error } if error.starts_with("pinned plugin abababab unavailable")),
            "a refused plugin ends the join even while the engine is pending"
        );
    }

    #[test]
    fn frames_for_a_hash_nobody_asked_the_host_for_are_ignored() {
        let bytes = b"unsolicited".to_vec();
        let hash = module_hash(&bytes);
        let mut fetches = Fetches::new();
        assert_eq!(fetches.host_frame(&module_chunks(hash, &bytes)[0]), None);
        assert_eq!(
            fetches.host_frame(&HostMsg::NoModule {
                hash,
                reason: "poisoned".into(),
            }),
            None
        );
        let (state, ask) = fetches.poll(hash, false);
        assert_eq!(pending_from(&state), "the host");
        assert_eq!(
            ask,
            Some(Ask::Host),
            "nothing was recorded for the stray frames"
        );

        let asked = [8; 32];
        let mut fetches = Fetches::new();
        fetches.poll(asked, true);
        assert_eq!(fetches.host_frame(&module_chunks(asked, b"early")[0]), None);
        assert_eq!(
            pending_from(&fetches.poll(asked, true).0),
            "the gateway",
            "a host frame does not count while the gateway is being asked"
        );
    }

    #[test]
    fn a_host_refusal_before_the_gateway_was_up_tries_the_gateway_once() {
        let bytes = b"published later".to_vec();
        let hash = module_hash(&bytes);
        let mut fetches = Fetches::new();
        assert_eq!(fetches.poll(hash, false).1, Some(Ask::Host));
        fetches.host_frame(&HostMsg::NoModule {
            hash,
            reason: "this table pinned no module".into(),
        });
        let (state, ask) = fetches.poll(hash, true);
        assert_eq!(pending_from(&state), "the gateway");
        assert_eq!(ask, Some(Ask::Gateway));
        assert_eq!(fetches.gateway_done(hash, GatewayOutcome::NotFound), None);
        let PinState::Failed(error) = fetches.poll(hash, true).0 else {
            panic!("refused by both");
        };
        assert_eq!(
            error,
            "the host refused to serve it: this table pinned no module and the gateway has no such blob"
        );

        let mut fetches = Fetches::new();
        fetches.poll(hash, false);
        fetches.host_frame(&HostMsg::NoModule {
            hash,
            reason: "no".into(),
        });
        assert_eq!(fetches.poll(hash, true).1, Some(Ask::Gateway));
        assert_eq!(
            fetches.gateway_done(hash, GatewayOutcome::Bytes(bytes.clone())),
            None
        );
        assert!(matches!(
            fetches.poll(hash, true).0,
            PinState::Bytes { ref source, .. } if source == "the gateway"
        ));

        let mut fetches = Fetches::new();
        fetches.poll(hash, true);
        fetches.gateway_done(hash, GatewayOutcome::NotFound);
        fetches.host_frame(&HostMsg::NoModule {
            hash,
            reason: "no".into(),
        });
        assert!(
            matches!(fetches.poll(hash, true).0, PinState::Failed(_)),
            "a host refusal after the gateway already missed is final"
        );
    }

    #[test]
    fn a_new_join_forgets_stalled_asks_and_refusals_but_keeps_verified_bytes() {
        let bytes = b"kept".to_vec();
        let kept = module_hash(&bytes);
        let stalled = [5; 32];
        let refused = [6; 32];
        let mut fetches = Fetches::new();
        fetches.poll(kept, false);
        fetches.host_frame(&module_chunks(kept, &bytes)[0]);
        fetches.poll(stalled, false);
        fetches.poll(refused, false);
        fetches.host_frame(&HostMsg::NoModule {
            hash: refused,
            reason: "no".into(),
        });
        fetches.reset();
        assert!(matches!(
            fetches.poll(kept, false).0,
            PinState::Bytes { .. }
        ));
        assert_eq!(fetches.poll(stalled, false).1, Some(Ask::Host));
        assert_eq!(fetches.poll(refused, true).1, Some(Ask::Gateway));
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::{hash_hex, join_modules, short_hex, JoinModules, ModuleRow, PinState};
    use agni_engine_host::{
        load_engine, load_plugin, ModuleBytes, ModuleSource, StoreSource, ENGINE_GAS_BUDGET,
        PLUGIN_GAS_BUDGET,
    };
    use agni_sim::abi::ENGINE_ABI_VERSION;
    use agni_sim::engine::{Engine, PluginModule};
    use agni_sim::log::LogEntry;
    use parking_lot::Mutex;
    use spirit_sdk::modules::{self, Module, Role};
    use spirit_sdk::record::Tdr;
    use spirit_sdk::{identity, BlobHash, BlobStore, Identity, StoreError};
    use std::path::PathBuf;

    pub const ENGINE_REF: &str = "engine";
    use super::{MTG_REF, RIFTBOUND_REF};

    struct State {
        selected_engine: Option<String>,
        selected_plugin: Option<String>,
        engine: Option<(Option<ModuleBytes>, String)>,
        status: String,
    }

    static STATE: Mutex<State> = Mutex::new(State {
        selected_engine: None,
        selected_plugin: None,
        engine: None,
        status: String::new(),
    });

    static TOKENS: Mutex<Vec<agni_sim::wire::TokenDecl>> = Mutex::new(Vec::new());

    static PROVIDER: Mutex<Option<String>> = Mutex::new(None);

    pub fn tokens() -> Vec<agni_sim::wire::TokenDecl> {
        TOKENS.lock().clone()
    }

    fn remember_tokens(manifest: Option<&agni_sim::wire::PluginManifest>) {
        *TOKENS.lock() = manifest
            .map(|manifest| manifest.tokens.clone())
            .unwrap_or_default();
    }

    fn open_store() -> Option<BlobStore> {
        BlobStore::open(crate::os::paths::store_dir()?).ok()
    }

    fn bundle_td() -> Tdr {
        Tdr::new(
            "kai-bundle",
            &(
                "kai",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
            ),
        )
        .expect("the bundle transform encodes")
    }

    fn asset_candidates(relative: &str, env_var: &str) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Ok(path) = std::env::var(env_var) {
            candidates.push(PathBuf::from(path));
        }
        if let Ok(root) = std::env::var("BEVY_ASSET_ROOT") {
            candidates.push(PathBuf::from(root).join(relative));
        }
        candidates.push(PathBuf::from(relative));
        #[cfg(target_os = "android")]
        if let Some(dir) = crate::os::paths::store_dir() {
            if let Some(file) = PathBuf::from(relative).file_name() {
                candidates.push(dir.join(file));
            }
        }
        candidates
    }

    pub fn bundled_asset(relative: &str) -> Option<Vec<u8>> {
        let path = asset_candidates(relative, "")
            .into_iter()
            .find(|path| path.is_file())?;
        std::fs::read(path).ok()
    }

    fn bundled_bytes(relative: &str, env_var: &str) -> Option<(ModuleBytes, String)> {
        let path = asset_candidates(relative, env_var)
            .into_iter()
            .find(|path| path.is_file())?;
        let bytes = std::fs::read(&path).ok()?;
        Some((ModuleBytes::new(bytes), path.display().to_string()))
    }

    fn bundled_engine() -> Option<(ModuleBytes, String)> {
        bundled_bytes("assets/engine/engine.wasm", "AGNI_ENGINE_WASM")
    }

    fn bundled_riftbound() -> Option<(ModuleBytes, String)> {
        bundled_bytes("assets/plugins/riftbound.wasm", "AGNI_RIFTBOUND_WASM")
    }

    fn bundled_mtg() -> Option<(ModuleBytes, String)> {
        bundled_bytes("assets/plugins/mtg.wasm", "AGNI_MTG_WASM")
    }

    fn engine_from_store(
        store: &BlobStore,
        name: &str,
        selected: bool,
    ) -> (Option<ModuleBytes>, String) {
        let source = StoreSource::new(store.root(), name);
        match source.engine_module() {
            Ok(module) => {
                let hash = hash_hex(&module.hash);
                let picked = if selected { " (selected)" } else { "" };
                let note = format!(
                    "engine from store modules/{name} @ {}{picked}",
                    short_hex(&hash)
                );
                (Some(module), note)
            }
            Err(error) => (None, format!("engine modules/{name} failed: {error}")),
        }
    }

    fn resolve_engine(
        store: Option<&BlobStore>,
        selected: Option<&str>,
        bundled: Option<(ModuleBytes, String)>,
    ) -> (Option<ModuleBytes>, String) {
        if let Some(name) = selected {
            return match store {
                Some(store) => engine_from_store(store, name, true),
                None => (
                    None,
                    format!("engine modules/{name} selected but no store is available"),
                ),
            };
        }
        if let Some(store) = store {
            let (module, note) = engine_from_store(store, ENGINE_REF, false);
            if module.is_some() {
                return (module, note);
            }
        }
        if let Some((module, label)) = bundled {
            let hash = hash_hex(&module.hash);
            let note = format!(
                "engine bundled from {label} @ {} — not serving from the store",
                short_hex(&hash)
            );
            return (Some(module), note);
        }
        (None, "no engine module anywhere — folding natively".into())
    }

    pub fn engine_module() -> (Option<ModuleBytes>, String) {
        let mut state = STATE.lock();
        if state.engine.is_none() {
            let selected = state.selected_engine.clone();
            let resolved =
                resolve_engine(open_store().as_ref(), selected.as_deref(), bundled_engine());
            state.engine = Some(resolved);
        }
        let (module, note) = state.engine.as_ref().expect("engine resolution cached");
        (module.clone(), note.clone())
    }

    pub fn engine_note() -> String {
        engine_module().1
    }

    pub(super) fn engine_bytes() -> Option<Vec<u8>> {
        engine_module().0.map(|module| module.bytes)
    }

    pub fn reload() {
        let mut state = STATE.lock();
        state.engine = None;
        state.status = "modules re-resolved from the store".into();
    }

    pub fn select_engine(name: Option<String>) {
        let mut state = STATE.lock();
        state.selected_engine = name;
        state.engine = None;
    }

    pub fn select_plugin(name: Option<String>) {
        STATE.lock().selected_plugin = name;
    }

    pub fn selected_engine() -> Option<String> {
        STATE.lock().selected_engine.clone()
    }

    pub fn selected_plugin() -> Option<String> {
        STATE.lock().selected_plugin.clone()
    }

    pub fn status() -> String {
        STATE.lock().status.clone()
    }

    pub fn active_plugin() -> super::ActivePlugin {
        let refused = |note: String| super::ActivePlugin {
            module: None,
            bytes: None,
            note: Some(note),
            zones: None,
            counters: None,
            tokens: Vec::new(),
            despawn_any: false,
        };
        let Some(name) = selected_plugin() else {
            return super::ActivePlugin {
                module: None,
                bytes: None,
                note: None,
                zones: None,
                counters: None,
                tokens: Vec::new(),
                despawn_any: false,
            };
        };
        let Some(store) = open_store() else {
            return refused(format!("plugin modules/{name} unavailable — no store"));
        };
        match StoreSource::new(store.root(), &name).load() {
            Ok((version, module)) if version.module.role == Role::Plugin => {
                match load_plugin(&module.bytes, PLUGIN_GAS_BUDGET) {
                    Ok(mut plugin) => {
                        let manifest = plugin
                            .manifest_bytes()
                            .ok()
                            .and_then(|bytes| agni_sim::wire::decode_plugin_manifest(&bytes));
                        let zones = manifest.as_ref().map(|manifest| manifest.zones.clone());
                        let counters = manifest
                            .as_ref()
                            .filter(|manifest| !manifest.counters.is_empty())
                            .map(|manifest| manifest.counters.clone());
                        let note = format!(
                            "plugin modules/{name} {} @ {}",
                            version.module.version,
                            short_hex(&hash_hex(&module.hash))
                        );
                        remember_tokens(manifest.as_ref());
                        let tokens = manifest
                            .as_ref()
                            .map(|manifest| manifest.tokens.clone())
                            .unwrap_or_default();
                        let despawn_any = manifest
                            .as_ref()
                            .is_some_and(|manifest| manifest.despawn_any);
                        super::ActivePlugin {
                            module: Some(Box::new(plugin)),
                            bytes: Some(module.bytes),
                            note: Some(note),
                            zones,
                            counters,
                            tokens,
                            despawn_any,
                        }
                    }
                    Err(error) => refused(format!("plugin modules/{name} refused: {error}")),
                }
            }
            Ok(_) => refused(format!("modules/{name} is not a plugin")),
            Err(error) => refused(format!("plugin modules/{name} failed: {error}")),
        }
    }

    pub fn seed_bundled(dir: &std::path::Path) {
        let Ok(store) = BlobStore::open(dir) else {
            return;
        };
        let identity = match identity::load_or_create(dir) {
            Ok(identity) => identity,
            Err(error) => {
                STATE.lock().status = format!("no store identity to sign modules with: {error}");
                return;
            }
        };
        let mut notes = Vec::new();
        let bundles = [
            (ENGINE_REF, Role::Engine, bundled_engine()),
            (RIFTBOUND_REF, Role::Plugin, bundled_riftbound()),
            (MTG_REF, Role::Plugin, bundled_mtg()),
        ];
        for (name, role, bundle) in bundles {
            let Some((module, _)) = bundle else {
                notes.push(format!(
                    "no bundled modules/{name} — run engine-build and plugin-build in the kai shell"
                ));
                continue;
            };
            match seed_one(&store, &identity, name, role, &module.bytes) {
                Ok(note) => notes.extend(note),
                Err(error) => notes.push(format!("seeding modules/{name} failed: {error}")),
            }
        }
        if !notes.is_empty() {
            STATE.lock().status = notes.join("; ");
        }
    }

    fn seed_one(
        store: &BlobStore,
        identity: &Identity,
        name: &str,
        role: Role,
        bytes: &[u8],
    ) -> Result<Option<String>, String> {
        let module = Module::new(name, role, env!("CARGO_PKG_VERSION"), ENGINE_ABI_VERSION);
        let ci = module.ci().map_err(|e| e.to_string())?;
        let blob = BlobHash::of(bytes);
        let held = modules::versions(store, name);
        let trust = StoreSource::trust(store.root());
        if held.iter().any(|version| {
            version.ci == ci
                && version.blob == Some(blob)
                && version.held
                && version.trusted(&trust, spirit_sdk::TrustLevel::Cache)
        }) {
            return Ok(None);
        }
        modules::publish(store, identity, &module, &bundle_td(), bytes)?;
        let carried = held.len();
        Ok(Some(format!(
            "seeded modules/{name} {} @ {} beside {carried} version(s)",
            module.version,
            short_hex(&blob.to_string())
        )))
    }

    pub fn rows() -> Vec<ModuleRow> {
        let Some(store) = open_store() else {
            return Vec::new();
        };
        modules::list(&store)
            .into_iter()
            .flat_map(|(_, versions)| versions)
            .map(|version| ModuleRow {
                name: version.module.name.clone(),
                role: version.module.role,
                version: version.module.version.clone(),
                hash_hex: version
                    .blob
                    .map(|blob| blob.to_string())
                    .unwrap_or_default(),
                signer: version.signer.map(|dgid| dgid.short()),
                source: if version.legacy {
                    "legacy ref"
                } else {
                    "store"
                },
                held: version.held,
            })
            .collect()
    }

    fn pinned_bytes_from(
        store: Option<&BlobStore>,
        bundled: &[(ModuleBytes, String)],
        hash: [u8; 32],
    ) -> Option<PinState> {
        if let Some(store) = store {
            match store.get(BlobHash::from_bytes(hash)) {
                Ok(bytes) => {
                    return Some(PinState::Bytes {
                        bytes,
                        source: "store".into(),
                    })
                }
                Err(StoreError::Missing(_)) => {}
                Err(error) => {
                    return Some(PinState::Failed(format!("the store refused it: {error}")));
                }
            }
        }
        bundled
            .iter()
            .find(|(module, _)| module.hash == hash)
            .map(|(module, label)| PinState::Bytes {
                bytes: module.bytes.clone(),
                source: format!("bundle {label}"),
            })
    }

    fn pinned_bytes(hash: [u8; 32]) -> PinState {
        let bundled: Vec<(ModuleBytes, String)> =
            [bundled_engine(), bundled_riftbound(), bundled_mtg()]
                .into_iter()
                .flatten()
                .collect();
        match pinned_bytes_from(open_store().as_ref(), &bundled, hash) {
            Some(state) => state,
            None => super::fetched_state(hash, false),
        }
    }

    pub(super) fn ask_mesh(hash: [u8; 32]) {
        if let Some(node) = crate::net::node::get() {
            let provider = PROVIDER.lock().clone();
            node.mesh
                .request_blob(BlobHash::from_bytes(hash), provider.as_deref());
        }
    }

    pub(super) fn ask_gateway(_hash: [u8; 32]) {}

    fn keep_in(store: &BlobStore, hash: [u8; 32], bytes: &[u8]) -> Result<(), String> {
        let stored = store.put(bytes).map_err(|error| error.to_string())?;
        if stored != BlobHash::from_bytes(hash) {
            return Err(format!("the store filed them under {stored}"));
        }
        Ok(())
    }

    pub(super) fn keep_fetched(hash: [u8; 32], bytes: &[u8]) {
        let Some(store) = open_store() else {
            return;
        };
        if let Err(error) = keep_in(&store, hash, bytes) {
            STATE.lock().status = format!(
                "could not keep the host's module {} in the store: {error}",
                short_hex(&hash_hex(&hash))
            );
        }
    }

    pub fn prepare_join(log: &[LogEntry], host: Option<&str>) -> JoinModules {
        *PROVIDER.lock() = host.map(str::to_string);
        join_modules(
            log,
            pinned_bytes,
            |bytes| {
                load_engine(bytes, ENGINE_GAS_BUDGET)
                    .map(|engine| Box::new(engine) as Box<dyn Engine>)
                    .map_err(|fault| fault.to_string())
            },
            |bytes| {
                load_plugin(bytes, PLUGIN_GAS_BUDGET)
                    .map(|mut plugin| {
                        let manifest = plugin
                            .manifest_bytes()
                            .ok()
                            .and_then(|bytes| agni_sim::wire::decode_plugin_manifest(&bytes));
                        remember_tokens(manifest.as_ref());
                        Box::new(plugin) as Box<dyn PluginModule>
                    })
                    .map_err(|fault| fault.to_string())
            },
            crate::engine::session_engine,
        )
    }

    pub fn refresh_platform() {}

    #[cfg(test)]
    mod tests {
        use super::*;
        use agni_net::session::engine_blob_ref;

        fn scratch_store(tag: &str) -> (BlobStore, Identity) {
            let dir =
                std::env::temp_dir().join(format!("kai-modules-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let store = BlobStore::open(&dir).unwrap();
            let identity = identity::load_or_create(&dir).unwrap();
            (store, identity)
        }

        fn publish(
            store: &BlobStore,
            identity: &Identity,
            name: &str,
            role: Role,
            version: &str,
            bytes: &[u8],
        ) {
            modules::publish(
                store,
                identity,
                &Module::new(name, role, version, ENGINE_ABI_VERSION),
                &bundle_td(),
                bytes,
            )
            .unwrap();
        }

        fn bundle(bytes: &[u8]) -> (ModuleBytes, String) {
            (ModuleBytes::new(bytes.to_vec()), "assets/test.wasm".into())
        }

        fn hash_of(pin: &str) -> [u8; 32] {
            agni_sim::pins::pin_hash(pin).unwrap()
        }

        fn resolved(store: &BlobStore, name: &str) -> Vec<u8> {
            StoreSource::new(store.root(), name).load().unwrap().1.bytes
        }

        #[test]
        fn a_rebuilt_bundle_reseeds_and_a_newer_mesh_version_still_wins() {
            let (store, identity) = scratch_store("reseed");
            assert!(
                seed_one(&store, &identity, "riftbound", Role::Plugin, b"plugin v1")
                    .unwrap()
                    .is_some()
            );
            assert!(
                seed_one(&store, &identity, "riftbound", Role::Plugin, b"plugin v1")
                    .unwrap()
                    .is_none()
            );
            assert_eq!(resolved(&store, "riftbound"), b"plugin v1");

            seed_one(&store, &identity, "riftbound", Role::Plugin, b"plugin v2").unwrap();
            assert_eq!(resolved(&store, "riftbound"), b"plugin v2");

            let peer = Identity::from_secret([42; 32]);
            publish(
                &store,
                &peer,
                "riftbound",
                Role::Plugin,
                "99.0.0",
                b"mesh plugin",
            );
            let mut trust = StoreSource::trust(store.root());
            trust.set(peer.dgid(), spirit_sdk::TrustLevel::Cache);
            trust.save(store.root()).unwrap();

            assert_eq!(resolved(&store, "riftbound"), b"mesh plugin");
            seed_one(&store, &identity, "riftbound", Role::Plugin, b"plugin v3").unwrap();
            assert_eq!(resolved(&store, "riftbound"), b"mesh plugin");
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn an_untrusted_peer_version_is_listed_but_never_loaded() {
            let (store, identity) = scratch_store("untrusted");
            publish(
                &store,
                &identity,
                "riftbound",
                Role::Plugin,
                "0.1.0",
                b"mine",
            );
            let stranger = Identity::from_secret([7; 32]);
            publish(
                &store,
                &stranger,
                "riftbound",
                Role::Plugin,
                "99.0.0",
                b"hostile",
            );
            assert_eq!(resolved(&store, "riftbound"), b"mine");
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn an_explicit_selection_beats_the_default_store_ref() {
            let (store, identity) = scratch_store("selection");
            publish(
                &store,
                &identity,
                "engine",
                Role::Engine,
                "0.1.0",
                b"default engine",
            );
            publish(
                &store,
                &identity,
                "engine-next",
                Role::Engine,
                "0.1.0",
                b"selected engine",
            );
            let (module, note) = resolve_engine(
                Some(&store),
                Some("engine-next"),
                Some(bundle(b"bundled engine")),
            );
            assert_eq!(module.unwrap().bytes, b"selected engine");
            assert!(note.contains("modules/engine-next"));
            assert!(note.contains("(selected)"));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn the_store_ref_beats_the_bundled_module() {
            let (store, identity) = scratch_store("store-first");
            publish(
                &store,
                &identity,
                "engine",
                Role::Engine,
                "0.1.0",
                b"store engine",
            );
            let (module, note) =
                resolve_engine(Some(&store), None, Some(bundle(b"bundled engine")));
            assert_eq!(module.unwrap().bytes, b"store engine");
            assert!(note.contains("engine from store modules/engine"));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn an_empty_store_falls_back_to_the_bundle_loudly() {
            let (store, _identity) = scratch_store("bundle-fallback");
            let (module, note) =
                resolve_engine(Some(&store), None, Some(bundle(b"bundled engine")));
            assert_eq!(module.unwrap().bytes, b"bundled engine");
            assert!(note.contains("bundled"));
            let (module, note) = resolve_engine(Some(&store), None, None);
            assert!(module.is_none());
            assert!(note.contains("folding natively"));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn a_broken_selection_is_loud_not_silently_substituted() {
            let (store, identity) = scratch_store("broken-selection");
            publish(
                &store,
                &identity,
                "engine",
                Role::Engine,
                "0.1.0",
                b"default engine",
            );
            publish(
                &store,
                &identity,
                "riftbound",
                Role::Plugin,
                "0.1.0",
                b"plugin bytes",
            );
            let (module, note) = resolve_engine(
                Some(&store),
                Some("riftbound"),
                Some(bundle(b"bundled engine")),
            );
            assert!(module.is_none());
            assert!(note.contains("not an engine"));
            let (module, note) = resolve_engine(Some(&store), Some("absent"), None);
            assert!(module.is_none());
            assert!(note.contains("absent"));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn a_reload_resolves_freshly_published_bytes() {
            let (store, identity) = scratch_store("hot-reload");
            publish(
                &store,
                &identity,
                "engine",
                Role::Engine,
                "0.1.0",
                b"engine v1",
            );
            let (module, _) = resolve_engine(Some(&store), None, None);
            assert_eq!(module.unwrap().bytes, b"engine v1");
            publish(
                &store,
                &identity,
                "engine",
                Role::Engine,
                "0.2.0",
                b"engine v2",
            );
            let (module, note) = resolve_engine(Some(&store), None, None);
            let module = module.unwrap();
            assert_eq!(module.bytes, b"engine v2");
            assert!(note.contains(&short_hex(&hash_hex(&module.hash))));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn every_published_version_is_listed_with_its_signer() {
            let (store, identity) = scratch_store("rows");
            publish(&store, &identity, "mtg", Role::Plugin, "0.1.0", b"a");
            publish(&store, &identity, "mtg", Role::Plugin, "0.2.0", b"b");
            let listed = modules::list(&store);
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].1.len(), 2);
            assert!(listed[0].1.iter().all(|version| version.held));
            assert!(listed[0]
                .1
                .iter()
                .all(|version| version.signer == Some(identity.dgid())));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn pinned_bytes_come_from_the_store_by_hash() {
            let (store, _identity) = scratch_store("pin-store");
            let hash = store.put(b"pinned module").unwrap();
            let pin = format!("blob:{hash}");
            let Some(PinState::Bytes { bytes, source }) =
                pinned_bytes_from(Some(&store), &[], hash_of(&pin))
            else {
                panic!("the store holds the pinned bytes");
            };
            assert_eq!(bytes, b"pinned module");
            assert_eq!(source, "store");
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn pinned_bytes_fall_back_to_a_matching_bundle() {
            let (store, _identity) = scratch_store("pin-bundle");
            let bundle = bundle(b"bundled module");
            let pin = engine_blob_ref(bundle.0.hash);
            let Some(PinState::Bytes { bytes, source }) =
                pinned_bytes_from(Some(&store), &[bundle], hash_of(&pin))
            else {
                panic!("the bundle holds the pinned bytes");
            };
            assert_eq!(bytes, b"bundled module");
            assert!(source.contains("bundle"));
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn a_tampered_store_blob_is_refused_not_loaded() {
            let (store, _identity) = scratch_store("pin-tampered");
            let hash = store.put(b"the real module").unwrap();
            std::fs::write(store.root().join(hash.to_string()), b"tampered bytes").unwrap();
            let pin = format!("blob:{hash}");
            match pinned_bytes_from(Some(&store), &[], hash_of(&pin)) {
                Some(PinState::Failed(error)) => assert!(error.contains("corrupt")),
                _ => panic!("a corrupt blob must refuse, not load or fetch"),
            }
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn an_absent_pin_misses_locally_and_is_asked_of_the_host() {
            let (store, _identity) = scratch_store("pin-missing");
            let hash = *b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let pin = engine_blob_ref(hash);
            assert!(pinned_bytes_from(Some(&store), &[], hash_of(&pin)).is_none());
            let mut fetches = super::super::fetch::Fetches::new();
            let (state, ask) = fetches.poll(hash, false);
            assert!(matches!(state, PinState::Pending(ref from) if from == "the host"));
            assert_eq!(ask, Some(super::super::fetch::Ask::Host));
            let (_, again) = fetches.poll(hash, false);
            assert_eq!(again, None, "the host is asked once, not every frame");
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn a_module_the_host_served_is_kept_in_the_store_for_the_next_join() {
            let (store, _identity) = scratch_store("pin-kept");
            let bytes = b"served by the host".to_vec();
            let hash = agni_net::session::module_hash(&bytes);
            assert!(pinned_bytes_from(Some(&store), &[], hash).is_none());
            keep_in(&store, hash, &bytes).unwrap();
            assert!(keep_in(&store, [0; 32], &bytes).is_err());
            let Some(PinState::Bytes { bytes: held, .. }) =
                pinned_bytes_from(Some(&store), &[], hash)
            else {
                panic!("the store now holds the host's bytes");
            };
            assert_eq!(held, bytes);
            let _ = std::fs::remove_dir_all(store.root());
        }

        #[test]
        fn a_garbage_pin_is_refused_before_any_fetch() {
            let log = vec![LogEntry::new(
                0,
                0,
                agni_sim::log::LogAction::Genesis {
                    name: "rae".into(),
                    config: agni_sim::log::TableConfig {
                        engine: Some("garbage".into()),
                        plugin: None,
                        zones: Vec::new(),
                        options: None,
                        counters: Vec::new(),
                        despawn_any: false,
                    },
                },
            )];
            let outcome = join_modules(
                &log,
                |_| panic!("no fetch for an unparsable pin"),
                |_| Err("unused".into()),
                |_| Err("unused".into()),
                || (Box::new(agni_sim::engine::NativeEngine::new()), None),
            );
            assert!(
                matches!(outcome, JoinModules::Refused { error } if error.contains("unparsable"))
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::fetch::GatewayOutcome;
    use super::{
        hash_hex, join_modules, short_hex, JoinModules, ModuleRow, PinState, MTG_REF, RIFTBOUND_REF,
    };
    use crate::net::gateway::FetchError;
    use agni_sim::engine::{Engine, PluginModule};
    use agni_sim::log::LogEntry;
    use agni_sim::pins::genesis_engine_pin;
    use agni_sim::wire::{PluginManifest, TokenDecl};
    use spirit_sdk::modules::Role;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    thread_local! {
        static BUNDLED: RefCell<BTreeMap<String, ([u8; 32], Vec<u8>)>> =
            const { RefCell::new(BTreeMap::new()) };
        static PENDING_BUNDLES: RefCell<crate::engine::selection::PendingBundles> =
            RefCell::new(crate::engine::selection::PendingBundles::default());
        static ROWS: RefCell<Vec<ModuleRow>> = const { RefCell::new(Vec::new()) };
        static LISTING: RefCell<bool> = const { RefCell::new(false) };
        static STATUS: RefCell<String> = const { RefCell::new(String::new()) };
        static SELECTED_PLUGIN: RefCell<Option<String>> = const { RefCell::new(None) };
        static TOKENS: RefCell<Vec<TokenDecl>> = const { RefCell::new(Vec::new()) };
    }

    fn set_status(status: String) {
        STATUS.with(|slot| *slot.borrow_mut() = status);
    }

    pub fn status() -> String {
        STATUS.with(|slot| slot.borrow().clone())
    }

    pub fn engine_note() -> String {
        crate::engine::web::provenance()
    }

    pub(super) fn engine_bytes() -> Option<Vec<u8>> {
        crate::engine::web::engine_bytes()
    }

    pub fn selected_engine() -> Option<String> {
        None
    }

    pub fn selected_plugin() -> Option<String> {
        SELECTED_PLUGIN.with(|slot| slot.borrow().clone())
    }

    pub fn tokens() -> Vec<TokenDecl> {
        TOKENS.with(|slot| slot.borrow().clone())
    }

    fn remember_tokens(manifest: Option<&PluginManifest>) {
        TOKENS.with(|slot| {
            *slot.borrow_mut() = manifest
                .map(|manifest| manifest.tokens.clone())
                .unwrap_or_default()
        });
    }

    pub fn select_engine(_name: Option<String>) {}

    pub fn select_plugin(name: Option<String>) {
        SELECTED_PLUGIN.with(|slot| *slot.borrow_mut() = name);
    }

    fn load_plugin(bytes: &[u8]) -> Result<Box<dyn PluginModule>, String> {
        let (plugin, manifest) = crate::engine::web::plugin_with_manifest(bytes)?;
        remember_tokens(manifest.as_ref());
        Ok(plugin)
    }

    pub fn fetch_bundle() {
        for name in [RIFTBOUND_REF, MTG_REF] {
            PENDING_BUNDLES.with(|pending| pending.borrow_mut().start(name));
            wasm_bindgen_futures::spawn_local(async move {
                let url = format!("./assets/plugins/{name}.wasm");
                let fetched = n0_future::time::timeout(
                    std::time::Duration::from_secs(20),
                    crate::net::gateway::fetch_bytes(&url),
                )
                .await
                .unwrap_or_else(|_| Err(FetchError::Other("download timed out".into())));
                match fetched {
                    Ok(bytes) => {
                        let hash = *blake3::hash(&bytes).as_bytes();
                        BUNDLED.with(|slot| {
                            slot.borrow_mut().insert(name.to_string(), (hash, bytes));
                        });
                    }
                    Err(FetchError::NotFound) => {}
                    Err(FetchError::Other(error)) => {
                        set_status(format!("bundled plugin {name}: {error}"));
                    }
                }
                PENDING_BUNDLES.with(|pending| pending.borrow_mut().finish(name));
            });
        }
    }

    fn bundled_row(name: &str) -> Option<(ModuleRow, [u8; 32])> {
        BUNDLED.with(|slot| {
            slot.borrow().get(name).map(|(hash, _)| {
                (
                    ModuleRow {
                        name: name.to_string(),
                        role: Role::Plugin,
                        version: "bundled".into(),
                        hash_hex: hash_hex(hash),
                        signer: None,
                        source: "bundle",
                        held: true,
                    },
                    *hash,
                )
            })
        })
    }

    fn bundled_bytes(hash: [u8; 32]) -> Option<Vec<u8>> {
        BUNDLED.with(|slot| {
            slot.borrow()
                .values()
                .find(|(held, _)| *held == hash)
                .map(|(_, bytes)| bytes.clone())
        })
    }

    fn held_plugin(name: &str) -> Result<(ModuleRow, [u8; 32]), String> {
        if let Some(bundled) = bundled_row(name) {
            return Ok(bundled);
        }
        if PENDING_BUNDLES.with(|pending| pending.borrow().contains(name)) {
            return Err(format!("waiting for this build's bundled {name} rules…"));
        }
        let rows = rows();
        if rows.is_empty() {
            return Err(format!(
                "plugin modules/{name} is still loading — neither the bundle nor the gateway list has arrived"
            ));
        }
        let row = rows
            .iter()
            .rfind(|row| row.role == Role::Plugin && row.name == name && row.held)
            .ok_or_else(|| format!("no trusted modules/{name} held by the gateway"))?;
        let hash = spirit_sdk::BlobHash::parse(&row.hash_hex)
            .map(|hash| *hash.as_bytes())
            .ok_or_else(|| format!("modules/{name} names no blob on the gateway"))?;
        Ok((row.clone(), hash))
    }

    pub fn plugin_block(name: &str) -> Option<String> {
        let (_, hash) = match held_plugin(name) {
            Ok(held) => held,
            Err(reason) => return Some(reason),
        };
        match pinned_state(hash) {
            PinState::Bytes { .. } => None,
            PinState::Pending(_) => {
                Some(format!("fetching plugin modules/{name} from the gateway…"))
            }
            PinState::Failed(error) => Some(error),
        }
    }

    fn prefetch_plugins(rows: &[ModuleRow]) {
        for name in [RIFTBOUND_REF, MTG_REF] {
            let Some(row) = rows
                .iter()
                .rfind(|row| row.role == Role::Plugin && row.name == name && row.held)
            else {
                continue;
            };
            if let Some(hash) = spirit_sdk::BlobHash::parse(&row.hash_hex) {
                pinned_state(*hash.as_bytes());
            }
        }
    }

    pub fn active_plugin() -> super::ActivePlugin {
        let refused = |note: String| super::ActivePlugin {
            module: None,
            bytes: None,
            note: Some(note),
            zones: None,
            counters: None,
            tokens: Vec::new(),
            despawn_any: false,
        };
        let Some(name) = selected_plugin() else {
            return super::ActivePlugin {
                module: None,
                bytes: None,
                note: None,
                zones: None,
                counters: None,
                tokens: Vec::new(),
                despawn_any: false,
            };
        };
        let (row, hash) = match held_plugin(&name) {
            Ok(held) => held,
            Err(reason) => return refused(reason),
        };
        let bytes = match pinned_state(hash) {
            PinState::Bytes { bytes, .. } => bytes,
            PinState::Pending(_) => {
                return refused(format!("fetching plugin modules/{name} from the gateway…"))
            }
            PinState::Failed(error) => return refused(error),
        };
        match crate::engine::web::plugin_with_manifest(&bytes) {
            Ok((plugin, manifest)) => {
                remember_tokens(manifest.as_ref());
                let zones = manifest.as_ref().map(|manifest| manifest.zones.clone());
                let counters = manifest
                    .as_ref()
                    .filter(|manifest| !manifest.counters.is_empty())
                    .map(|manifest| manifest.counters.clone());
                let tokens = manifest
                    .as_ref()
                    .map(|manifest| manifest.tokens.clone())
                    .unwrap_or_default();
                let despawn_any = manifest
                    .as_ref()
                    .is_some_and(|manifest| manifest.despawn_any);
                super::ActivePlugin {
                    module: Some(plugin),
                    bytes: Some(bytes),
                    note: Some(format!(
                        "plugin modules/{name} {} @ {} from the {}",
                        row.version,
                        short_hex(&row.hash_hex),
                        row.source
                    )),
                    zones,
                    counters,
                    tokens,
                    despawn_any,
                }
            }
            Err(error) => refused(format!("plugin modules/{name} refused: {error}")),
        }
    }

    pub fn rows() -> Vec<ModuleRow> {
        let mut rows: Vec<ModuleRow> = [RIFTBOUND_REF, MTG_REF]
            .into_iter()
            .filter_map(|name| bundled_row(name).map(|(row, _)| row))
            .collect();
        rows.extend(ROWS.with(|rows| rows.borrow().clone()));
        rows
    }

    pub fn reload() {
        LISTING.with(|flag| *flag.borrow_mut() = false);
        set_status("refreshing modules from the gateway…".into());
    }

    pub fn refresh_platform() {
        let Some(base) = crate::net::gateway::gateway_base() else {
            return;
        };
        let already = LISTING.with(|flag| std::mem::replace(&mut *flag.borrow_mut(), true));
        if already {
            return;
        }
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_module_list(base).await {
                Ok(rows) => {
                    let count = rows.len();
                    maybe_adopt_store_engine(base, &rows);
                    prefetch_plugins(&rows);
                    ROWS.with(|slot| *slot.borrow_mut() = rows);
                    set_status(format!("{count} module version(s) served by the gateway"));
                }
                Err(error) => {
                    LISTING.with(|flag| *flag.borrow_mut() = false);
                    set_status(format!("module listing failed: {error}"));
                }
            }
        });
    }

    fn maybe_adopt_store_engine(base: &'static str, rows: &[ModuleRow]) {
        let Some(row) = rows
            .iter()
            .rfind(|row| row.role == Role::Engine && row.name == "engine" && row.held)
        else {
            return;
        };
        let Some(pin) = spirit_sdk::BlobHash::parse(&row.hash_hex).map(|hash| *hash.as_bytes())
        else {
            return;
        };
        if crate::engine::web::loaded_engine_hash() == Some(pin) {
            return;
        }
        let hash_hex = row.hash_hex.clone();
        let name = row.name.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let bytes =
                match crate::net::gateway::fetch_bytes(&format!("{base}/gateway/blob/{hash_hex}"))
                    .await
                {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        set_status(format!(
                            "fetching modules/{name} from the gateway failed: {error}"
                        ));
                        return;
                    }
                };
            if *blake3::hash(&bytes).as_bytes() != pin {
                set_status(format!(
                    "gateway served wrong bytes for modules/{name} — refusing"
                ));
                return;
            }
            let provenance = format!(
                "engine from gateway store modules/{name} @ {}",
                short_hex(&hash_hex)
            );
            match crate::engine::web::install_engine(&bytes, provenance.clone()) {
                Ok(true) => set_status(provenance),
                Ok(false) => {}
                Err(error) => set_status(format!("store engine refused: {error}")),
            }
        });
    }

    async fn fetch_module_list(base: &'static str) -> Result<Vec<ModuleRow>, String> {
        #[derive(serde::Deserialize)]
        struct GatewayModule {
            name: String,
            role: String,
            version: String,
            blob: Option<String>,
            signer: Option<String>,
            trusted: bool,
            held: bool,
            legacy: bool,
        }
        let text = crate::net::gateway::fetch_text(&format!("{base}/gateway/modules"), false)
            .await
            .map_err(|_| "gateway modules unreachable".to_string())?;
        let listed: Vec<GatewayModule> =
            serde_json::from_str(&text).map_err(|error| error.to_string())?;
        Ok(listed
            .into_iter()
            .filter(|module| module.trusted)
            .map(|module| ModuleRow {
                name: module.name,
                role: if module.role == "engine" {
                    Role::Engine
                } else {
                    Role::Plugin
                },
                version: module.version,
                hash_hex: module.blob.unwrap_or_default(),
                signer: module
                    .signer
                    .and_then(|signer| spirit_sdk::Dgid::parse(&signer))
                    .map(|dgid| dgid.short()),
                source: if module.legacy {
                    "gateway legacy ref"
                } else {
                    "gateway"
                },
                held: module.held,
            })
            .collect())
    }

    pub(super) fn ask_gateway(hash: [u8; 32]) {
        let Some(base) = crate::net::gateway::gateway_base() else {
            let ask = super::FETCHES
                .lock()
                .gateway_done(hash, GatewayOutcome::NotFound);
            if let Some(who) = ask {
                super::ask(hash, who);
            }
            return;
        };
        let hex = hash_hex(&hash);
        wasm_bindgen_futures::spawn_local(async move {
            let outcome =
                match crate::net::gateway::fetch_bytes(&format!("{base}/gateway/blob/{hex}")).await
                {
                    Ok(bytes) => GatewayOutcome::Bytes(bytes),
                    Err(FetchError::NotFound) => GatewayOutcome::NotFound,
                    Err(FetchError::Other(error)) => GatewayOutcome::Failed(error),
                };
            let ask = super::FETCHES.lock().gateway_done(hash, outcome);
            if let Some(who) = ask {
                super::ask(hash, who);
            }
        });
    }

    pub(super) fn ask_mesh(_hash: [u8; 32]) {}

    pub(super) fn keep_fetched(_hash: [u8; 32], _bytes: &[u8]) {}

    fn pinned_state(hash: [u8; 32]) -> PinState {
        if let Some(bytes) = bundled_bytes(hash) {
            return PinState::Bytes {
                bytes,
                source: "bundle".into(),
            };
        }
        super::fetched_state(hash, crate::net::gateway::gateway_base().is_some())
    }

    pub fn prepare_join(log: &[LogEntry], _host: Option<&str>) -> JoinModules {
        let loaded = crate::engine::web::loaded_engine_hash();
        let pinned = genesis_engine_pin(log).and_then(|pin| agni_sim::pins::pin_hash(&pin));
        if pinned.is_some() && pinned == loaded {
            match crate::engine::web::instantiate_loaded() {
                Ok(engine) => {
                    let mut notes = vec![format!(
                        "engine pinned @ {}",
                        short_hex(&hash_hex(&pinned.unwrap_or_default()))
                    )];
                    return join_plugin_only(log, engine, &mut notes);
                }
                Err(error) => {
                    return JoinModules::Refused {
                        error: format!("pinned engine refused: {error}"),
                    }
                }
            }
        }
        join_modules(
            log,
            pinned_state,
            crate::engine::web::engine_from_bytes,
            load_plugin,
            crate::engine::session_engine,
        )
    }

    fn join_plugin_only(
        log: &[LogEntry],
        engine: Box<dyn Engine>,
        notes: &mut Vec<String>,
    ) -> JoinModules {
        let plugin: Option<Box<dyn PluginModule>> = match agni_sim::pins::genesis_plugin_pin(log) {
            None => None,
            Some(pin) => match super::resolve_pin("plugin", &pin, pinned_state, load_plugin, notes)
            {
                Ok(plugin) => Some(plugin),
                Err(outcome) => return outcome,
            },
        };
        JoinModules::Ready {
            engine,
            plugin,
            note: notes.join("; "),
        }
    }
}

pub fn serve_into(
    session: &mut agni_net::session::HostSession,
    plugin: Option<Vec<u8>>,
) -> Vec<[u8; 32]> {
    [platform::engine_bytes(), plugin]
        .into_iter()
        .flatten()
        .filter_map(|bytes| session.serve_module(bytes))
        .collect()
}

pub use platform::{
    active_plugin, engine_note, prepare_join, refresh_platform, reload, rows, select_engine,
    select_plugin, selected_engine, selected_plugin, status, tokens,
};
#[cfg(not(target_arch = "wasm32"))]
pub use platform::{bundled_asset, engine_module, seed_bundled};
#[cfg(target_arch = "wasm32")]
pub use platform::{fetch_bundle, plugin_block};

#[derive(Resource, Default)]
pub struct ModulesPanel {
    rows: Vec<ModuleRow>,
    since_refresh: f32,
}

impl ModulesPanel {
    pub fn rows(&self) -> &[ModuleRow] {
        &self.rows
    }
}

pub fn refresh_modules(time: Res<Time>, mut panel: ResMut<ModulesPanel>) {
    panel.since_refresh += time.delta_secs();
    if panel.since_refresh < REFRESH_SECS {
        return;
    }
    panel.since_refresh = 0.0;
    refresh_platform();
    let rows = rows();
    if panel.rows != rows {
        panel.rows = rows;
    }
}

fn row_line(row: &ModuleRow) -> String {
    let held = if row.held { "" } else { " — blob missing" };
    let signer = match &row.signer {
        Some(signer) => format!("signed {signer}"),
        None => "unsigned".into(),
    };
    let bytes = if row.hash_hex.is_empty() {
        "no bytes".to_string()
    } else {
        format!("@ {}", short_hex(&row.hash_hex))
    };
    format!(
        "modules/{} — {} v{} {bytes} ({signer}, {}){held}",
        row.name,
        row.role.label(),
        row.version,
        row.source,
    )
}

pub fn modules_section(ui: &mut egui::Ui, panel: &ModulesPanel) {
    {
        ui.label(engine_note());
        ui.separator();
        if panel.rows.is_empty() {
            ui.label(egui::RichText::new("no modules in the store yet").weak());
        }
        let hosting_target = cfg!(not(target_arch = "wasm32"));
        for row in &panel.rows {
            ui.horizontal(|ui| {
                ui.label(row_line(row));
                if !hosting_target {
                    return;
                }
                match row.role {
                    Role::Engine => {
                        let active = selected_engine().as_deref() == Some(row.name.as_str())
                            || (selected_engine().is_none() && row.name == "engine");
                        if ui.selectable_label(active, "use").clicked() && !active {
                            select_engine(Some(row.name.clone()));
                        }
                    }
                    Role::Plugin => {
                        let active = selected_plugin().as_deref() == Some(row.name.as_str());
                        if ui.selectable_label(active, "use for new tables").clicked() {
                            select_plugin(if active { None } else { Some(row.name.clone()) });
                        }
                    }
                }
            });
        }
        ui.separator();
        if ui.button("reload modules").clicked() {
            reload();
        }
        let status = status();
        if !status.is_empty() {
            ui.label(status);
        }
    }
}
