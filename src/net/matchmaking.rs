use super::{bridge, node, ClientState, HostState, TableChoice, TableGame};
use crate::menu::{Menu, Screen};
use crate::table::{SessionInfo, SessionRole};
use agni_net::matchmaking::{fingerprint, Phase, Ticket};
use agni_sim::log::{LogAction, LogEntry, TableConfig};
use bevy::prelude::*;
use parking_lot::Mutex;
use spirit_node::gossip::TableAdvert;
use std::time::Duration;
use web_time::Instant;

static EXPECTED: Mutex<Option<(TableGame, [u8; 32])>> = Mutex::new(None);

#[derive(Default, Debug, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Opening,
    Waiting,
    Closing(String),
    Joining,
    Cancelling,
}

#[derive(Resource, Default)]
pub struct Queue {
    stage: Stage,
    game: TableGame,
    last_scan: Option<Instant>,
    joining_since: Option<Instant>,
    cursor: usize,
    advertised: bool,
    pub status: String,
}

impl Queue {
    pub fn active(&self) -> bool {
        self.stage != Stage::Idle
    }

    pub fn start(&mut self, info: &mut SessionInfo, choice: &TableChoice) {
        if self.active() || info.role != SessionRole::Solo {
            return;
        }
        let Some(node) = node::get() else {
            self.status = "network is still starting — try again shortly".into();
            return;
        };
        node.matchmaking.prepare();
        self.game = choice.game;
        self.stage = Stage::Opening;
        self.last_scan = None;
        self.advertised = false;
        self.status = "opening a two-player search…".into();
        super::host_table(info);
    }

    pub fn cancel(
        &mut self,
        info: &mut ResMut<SessionInfo>,
        host: &mut ResMut<HostState>,
        client: &mut ResMut<ClientState>,
    ) {
        if !self.active() {
            return;
        }
        if let Some(node) = node::get() {
            node.matchmaking.prepare();
            if node.table.is_open() {
                super::close_table();
            }
        }
        bridge::cancel_join();
        *EXPECTED.lock() = None;
        host.clear();
        client.session = None;
        client.pending = None;
        info.role = SessionRole::Solo;
        info.recovery = crate::table::Recovery::Nothing;
        info.roster.clear();
        self.stage = Stage::Cancelling;
        self.status = "search cancelled".into();
        info.status = self.status.clone();
    }
}

pub fn key(game: TableGame, config: &TableConfig) -> [u8; 32] {
    let mut config = config.clone();
    if game == TableGame::Riftbound {
        let options = agni_riftbound::TableOptions::of_config(config.options.as_deref()).clamped();
        let enforced = agni_riftbound::TableOptions::enforced_in(config.options.as_deref());
        let bytes = agni_riftbound::TableOptions::genesis(Some(options), enforced).unwrap();
        let canonical: std::collections::BTreeMap<String, i64> =
            agni_sim::abi::decode(&bytes).unwrap();
        let existing = match config.options.as_deref() {
            None => Some(std::collections::BTreeMap::<String, i64>::new()),
            Some(bytes) => agni_sim::abi::decode(bytes),
        };
        if let Some(mut values) = existing {
            values.remove(agni_riftbound::OPTION_RULES_ENFORCED);
            values.extend(canonical);
            config.options = Some(agni_sim::abi::encode(&values).into());
        }
    }
    fingerprint(&agni_sim::abi::encode(&(
        1u8,
        agni_net::session::WIRE_VERSION,
        game.label(),
        2u8,
        config,
    )))
}

fn log_key(game: TableGame, log: &[LogEntry]) -> Option<[u8; 32]> {
    match &log.first()?.action {
        LogAction::Genesis { config, .. } => Some(key(game, config)),
        _ => None,
    }
}

pub fn accepts_welcome(log: &[LogEntry], seat: u8, players: usize) -> bool {
    EXPECTED
        .lock()
        .as_ref()
        .is_none_or(|(game, expected)| welcome_matches(*game, *expected, log, seat, players))
}

fn welcome_matches(
    game: TableGame,
    expected: [u8; 32],
    log: &[LogEntry],
    seat: u8,
    players: usize,
) -> bool {
    seat == 1 && players == 2 && log_key(game, log) == Some(expected)
}

pub fn tick(
    mut queue: ResMut<Queue>,
    mut info: ResMut<SessionInfo>,
    mut host: ResMut<HostState>,
    mut client: ResMut<ClientState>,
    choice: Res<TableChoice>,
    mut menu: ResMut<Menu>,
    mut import: ResMut<crate::deck::import::ImportPanel>,
) {
    if !queue.active() {
        return;
    }
    if queue.stage == Stage::Cancelling {
        if let Some(node) = node::get() {
            node.matchmaking.cancel();
        }
        queue.stage = Stage::Idle;
        return;
    }
    let Some(node) = node::get() else {
        queue.cancel(&mut info, &mut host, &mut client);
        return;
    };
    if queue.stage == Stage::Joining && info.role == SessionRole::Client {
        info!("matchmaking paired {} as client", queue.game.label());
        queue.stage = Stage::Idle;
        queue.status = "match found".into();
        import.auto_deal = true;
        *EXPECTED.lock() = None;
        menu.screen = Screen::Table;
        return;
    }
    if menu.screen != Screen::Lobby(queue.game) {
        queue.cancel(&mut info, &mut host, &mut client);
        return;
    }
    if queue.stage == Stage::Waiting && !node.table.is_open() {
        queue.cancel(&mut info, &mut host, &mut client);
        queue.status = "search stopped — keep the app open while searching".into();
        return;
    }
    match &queue.stage {
        Stage::Opening if info.role == SessionRole::Host => {
            let Some(key) = host
                .session
                .as_ref()
                .and_then(|session| log_key(queue.game, session.log()))
            else {
                queue.cancel(&mut info, &mut host, &mut client);
                queue.status = "could not read the table settings".into();
                return;
            };
            match node.matchmaking.begin(key) {
                Ok(ticket) => {
                    node.mesh.set_table(Some(TableAdvert {
                        name: ticket.advert(),
                    }));
                    queue.advertised = true;
                    queue.stage = Stage::Waiting;
                    queue.status = "searching for a player with the same settings…".into();
                }
                Err(error) => {
                    queue.cancel(&mut info, &mut host, &mut client);
                    queue.status = format!("could not start a search: {error}");
                }
            }
        }
        Stage::Opening if info.role != SessionRole::Starting => {
            let reason = info.status.clone();
            queue.cancel(&mut info, &mut host, &mut client);
            queue.status = reason;
        }
        Stage::Waiting => match node.matchmaking.phase() {
            Phase::Matched(_) if info.role == SessionRole::Host && info.roster.len() == 2 => {
                info!("matchmaking paired {} as host", queue.game.label());
                node.mesh.set_table(None);
                queue.stage = Stage::Idle;
                queue.status = "match found".into();
                import.auto_deal = true;
                menu.screen = Screen::Table;
            }
            Phase::Joining(peer) => {
                *EXPECTED.lock() = node
                    .matchmaking
                    .ticket()
                    .map(|ticket| (queue.game, ticket.key));
                super::close_table();
                queue.stage = Stage::Closing(peer);
                queue.status = "match found — connecting…".into();
            }
            Phase::Reserved(_) => {
                if queue.advertised {
                    node.mesh.set_table(None);
                    queue.advertised = false;
                }
                queue.status = "match found — waiting for your opponent to connect…".into();
            }
            Phase::Searching if info.role == SessionRole::Host => {
                if let Some(ticket) = node.matchmaking.ticket() {
                    if !queue.advertised {
                        node.mesh.set_table(Some(TableAdvert {
                            name: ticket.advert(),
                        }));
                        queue.advertised = true;
                    }
                    queue.status = "searching for a player with the same settings…".into();
                    scan(&mut queue, &node, ticket);
                }
            }
            Phase::Offering(_) => {}
            _ => {
                queue.cancel(&mut info, &mut host, &mut client);
                queue.status = "search stopped — the table is no longer available".into();
            }
        },
        Stage::Closing(peer) if info.role == SessionRole::Solo => {
            bridge::request_join(peer.clone());
            queue.joining_since = Some(Instant::now());
            queue.stage = Stage::Joining;
        }
        Stage::Joining => {
            let timed_out = queue
                .joining_since
                .is_some_and(|at| at.elapsed() > Duration::from_secs(45));
            if timed_out || matches!(info.role, SessionRole::Ended | SessionRole::Solo) {
                queue.cancel(&mut info, &mut host, &mut client);
                node.matchmaking.cancel();
                queue.stage = Stage::Idle;
                queue.start(&mut info, &choice);
                queue.status = "connection interrupted — searching again…".into();
            }
        }
        _ => {}
    }
}

fn scan(queue: &mut Queue, node: &node::Node, mine: Ticket) {
    if queue
        .last_scan
        .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
    {
        return;
    }
    queue.last_scan = Some(Instant::now());
    let candidates: Vec<_> = node
        .mesh
        .open_tables()
        .into_iter()
        .filter_map(|table| {
            let ticket = Ticket::parse(&table.name)?;
            (ticket.key == mine.key && table.host < node.node_id).then_some((table.host, ticket))
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    let (peer, ticket) = &candidates[queue.cursor % candidates.len()];
    queue.cursor = queue.cursor.wrapping_add(1);
    let Some(addr) = bridge::host_addr(&node.mesh, peer) else {
        return;
    };
    let worker = node.clone();
    let ticket = *ticket;
    node.spawn(async move {
        worker
            .matchmaking
            .offer(&worker.endpoint, addr, ticket, mine)
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_must_match_the_settings_and_exactly_two_seats() {
        let host = agni_net::session::HostSession::new("host");
        let expected = log_key(TableGame::FreeForm, host.log()).unwrap();
        assert!(welcome_matches(
            TableGame::FreeForm,
            expected,
            host.log(),
            1,
            2
        ));
        assert!(!welcome_matches(
            TableGame::FreeForm,
            expected,
            host.log(),
            1,
            3
        ));
        assert!(!welcome_matches(
            TableGame::FreeForm,
            expected,
            host.log(),
            0,
            2
        ));
        assert!(!welcome_matches(TableGame::Mtg, expected, host.log(), 1, 2));
        assert!(!welcome_matches(TableGame::FreeForm, expected, &[], 1, 2));
    }

    #[test]
    fn future_options_do_not_disappear_from_the_fingerprint() {
        let config = TableConfig::default();
        let options = std::collections::BTreeMap::from([("future_rule", 1i64)]);
        let future = TableConfig {
            options: Some(agni_sim::abi::encode(&options).into()),
            ..config.clone()
        };
        assert_ne!(
            key(TableGame::Riftbound, &config),
            key(TableGame::Riftbound, &future)
        );
    }

    #[test]
    fn effective_two_player_defaults_match_explicit_defaults() {
        let default = TableConfig::default();
        let explicit = TableConfig {
            options: agni_riftbound::TableOptions::genesis(
                Some(agni_riftbound::TableOptions::for_players(2)),
                false,
            )
            .map(Into::into),
            ..default.clone()
        };
        assert_eq!(
            key(TableGame::Riftbound, &default),
            key(TableGame::Riftbound, &explicit)
        );
    }

    #[test]
    fn rules_settings_modules_and_game_are_part_of_the_match() {
        let config = TableConfig::default();
        let expected = key(TableGame::Riftbound, &config);
        for changed in [
            TableConfig {
                engine: Some("engine:other".into()),
                ..config.clone()
            },
            TableConfig {
                plugin: Some("plugin:other".into()),
                ..config.clone()
            },
            TableConfig {
                options: agni_riftbound::TableOptions::genesis(None, true).map(Into::into),
                ..config.clone()
            },
            TableConfig {
                options: agni_riftbound::TableOptions::genesis(
                    Some(agni_riftbound::TableOptions {
                        victory_score: 12,
                        battlefields: 3,
                    }),
                    false,
                )
                .map(Into::into),
                ..config.clone()
            },
        ] {
            assert_ne!(expected, key(TableGame::Riftbound, &changed));
        }
        assert_ne!(expected, key(TableGame::Mtg, &config));
        assert_ne!(
            key(TableGame::FreeForm, &config),
            key(TableGame::Mtg, &config)
        );
    }
}
