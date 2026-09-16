pub mod defaults;
#[cfg(target_arch = "wasm32")]
pub mod gateway;
pub mod identity;
#[cfg(target_arch = "wasm32")]
pub mod js;
pub mod node;
#[cfg(target_arch = "wasm32")]
pub mod page;
pub mod peers;
pub mod undo;

use crate::table::{
    CardDropped, DealGeneration, ExhaustToggled, GameTable, Mirror, MySeat, PlayerCount, Recovery,
    Redeal, SessionInfo, SessionRole, ViewSeat, EXHAUSTED,
};
use crate::{app, Tuning};
use agni_core::{Intent, PlayerId, Zone};
use agni_net::bridge::{self, NetToGame};
use agni_net::session::{
    engine_blob_ref, verify_engine_pin, verify_plugin_pin, version_mismatch, ClientMsg,
    ClientSession, HostMsg, HostSession, SeatInfo, SessionError, WireIntent, WIRE_VERSION,
};
use agni_sim::engine::EngineFault;
use agni_sim::log::{LogAction, LogEntry, TableConfig};
use agni_sim::view::TableView;
use agni_sim::wire::{ZoneDecl, ZoneKind, ZoneOwner};
use bevy::prelude::*;
use serde_bytes::ByteBuf;
use spirit_node::mesh::OpenTable;
use std::collections::{BTreeMap, BTreeSet};

static PLAYER_NAME: parking_lot::Mutex<String> = parking_lot::Mutex::new(String::new());

pub fn set_player_name(name: &str) {
    *PLAYER_NAME.lock() = name.trim().to_string();
}

pub fn player_name() -> String {
    if let Some(scripted) = crate::autoplay::player_name() {
        return scripted;
    }
    let chosen = PLAYER_NAME.lock().clone();
    if chosen.is_empty() {
        default_player_name()
    } else {
        chosen
    }
}

#[cfg(target_arch = "wasm32")]
pub fn default_player_name() -> String {
    "browser".into()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn default_player_name() -> String {
    std::env::var("USER")
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "player".into())
}

#[cfg(target_os = "android")]
fn keep_awake(on: bool) {
    if let Err(error) = crate::os::android::set_keep_awake(on) {
        warn!("keep-awake toggle failed: {error}");
    }
}

#[cfg(target_arch = "wasm32")]
fn keep_awake(on: bool) {
    page::keep_awake(on);
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn keep_awake(_on: bool) {}

#[cfg(target_arch = "wasm32")]
pub fn withdraw_table() {
    if let Some(node) = node::get() {
        node.table.close();
        node.mesh.set_table(None);
    }
}

fn start_host() {
    let Some(node) = node::get() else {
        bridge::push(NetToGame::HostFailed {
            error: "spirit node not ready yet".into(),
        });
        return;
    };
    let spawner = node.clone();
    bridge::start_host(
        node.mesh.clone(),
        &node.table,
        node.endpoint.clone(),
        player_name(),
        move |future| spawner.spawn(future),
    );
    keep_awake(true);
}

fn close_table() {
    let node = node::get();
    bridge::close_host(node.as_ref().map(|node| (&*node.mesh, &node.table)));
    keep_awake(false);
}

fn refuse_hosting(info: &mut SessionInfo, reason: String) {
    close_table();
    info.role = SessionRole::Solo;
    info.recovery = Recovery::Nothing;
    info.status = reason;
}

fn host_closed(info: &mut SessionInfo, host: &mut HostState) {
    let was_open = host.session.is_some() || info.role != SessionRole::Solo;
    host.clear();
    info.role = SessionRole::Solo;
    info.recovery = Recovery::Nothing;
    info.roster.clear();
    if was_open {
        info.status = "table closed".into();
    }
}

pub const SELF_JOIN: &str =
    "that table is this node's own — join it from another browser, profile, or device";

fn start_join(host_id: String) {
    let Some(node) = node::get() else {
        bridge::push(NetToGame::Dropped {
            reason: "spirit node not ready yet".into(),
        });
        return;
    };
    let Some(addr) = bridge::host_addr(&node.mesh, &host_id) else {
        bridge::push(NetToGame::Dropped {
            reason: format!("no address for host {host_id}"),
        });
        return;
    };
    if addr.id.to_string() == node.node_id {
        bridge::push(NetToGame::Dropped {
            reason: SELF_JOIN.into(),
        });
        return;
    }
    let endpoint = node.endpoint.clone();
    let mesh = node.mesh.clone();
    node.spawn(bridge::run_join(
        endpoint,
        mesh,
        addr,
        host_id,
        player_name(),
    ));
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TableGame {
    #[default]
    FreeForm,
    Mtg,
    Riftbound,
}

impl TableGame {
    pub const ALL: [Self; 3] = [Self::FreeForm, Self::Mtg, Self::Riftbound];

    pub fn label(self) -> &'static str {
        match self {
            Self::FreeForm => "free-form",
            Self::Mtg => "MTG",
            Self::Riftbound => "Riftbound",
        }
    }

    pub fn id(self) -> Option<&'static str> {
        match self {
            Self::FreeForm => None,
            Self::Mtg => Some(agni_mtg::GAME),
            Self::Riftbound => Some(agni_riftbound::GAME),
        }
    }

    pub fn plugin_ref(self) -> Option<&'static str> {
        match self {
            Self::FreeForm => None,
            Self::Mtg => Some(crate::engine::modules::MTG_REF),
            Self::Riftbound => Some(crate::engine::modules::RIFTBOUND_REF),
        }
    }

    pub fn compiled_zones(self) -> Option<Vec<ZoneDecl>> {
        match self {
            Self::FreeForm => None,
            Self::Mtg => Some(agni_mtg::zone_table()),
            Self::Riftbound => Some(agni_riftbound::zone_table()),
        }
    }

    pub fn compiled_counters(self) -> Option<Vec<agni_sim::wire::CounterDecl>> {
        match self {
            Self::FreeForm => None,
            Self::Mtg => Some(agni_mtg::counter_table()),
            Self::Riftbound => Some(agni_riftbound::counter_table()),
        }
    }

    pub fn art_game(self) -> Option<crate::render::art::ArtGame> {
        match self {
            Self::FreeForm => None,
            Self::Mtg => Some(crate::render::art::ArtGame::Mtg),
            Self::Riftbound => Some(crate::render::art::ArtGame::Riftbound),
        }
    }
}

pub fn game_of_zones(zones: &[ZoneDecl]) -> TableGame {
    for game in [TableGame::Riftbound, TableGame::Mtg] {
        if game.compiled_zones().as_deref() == Some(zones) {
            return game;
        }
    }
    let named = |name: &str| zones.iter().any(|decl| decl.name == name);
    if named(agni_riftbound::ZONE_NAME_MAIN_DECK) && named(agni_riftbound::ZONE_NAME_RUNE_DECK) {
        TableGame::Riftbound
    } else if named(agni_mtg::ZONE_NAME_LIBRARY) {
        TableGame::Mtg
    } else {
        TableGame::FreeForm
    }
}

#[derive(Resource, Default)]
pub struct TableChoice {
    pub game: TableGame,
    pub options: Option<agni_riftbound::TableOptions>,
    pub enforced: bool,
}

impl TableChoice {
    pub fn config_options(&self) -> Option<ByteBuf> {
        match self.game {
            TableGame::Riftbound => agni_riftbound::TableOptions::genesis(
                self.options.map(agni_riftbound::TableOptions::clamped),
                self.enforced,
            )
            .map(ByteBuf::from),
            TableGame::Mtg | TableGame::FreeForm => None,
        }
    }
}

pub fn enforced_without_plugin(enforced: bool, loaded: bool, note: Option<&str>) -> Option<String> {
    if !enforced || loaded {
        return None;
    }
    Some(match note {
        Some(note) => format!("hosting refused — rules enforced needs the plugin: {note}"),
        None => "hosting refused — rules enforced needs the plugin, and none is selected".into(),
    })
}

pub fn rules_enforced(info: &SessionInfo, choice: &TableChoice) -> bool {
    match info.role {
        SessionRole::Solo | SessionRole::Starting => {
            choice.game == TableGame::Riftbound && choice.enforced
        }
        SessionRole::Joining | SessionRole::Host | SessionRole::Client | SessionRole::Ended => {
            agni_riftbound::TableOptions::enforced_in(info.options.as_deref())
        }
    }
}

fn send_to(conn: u64, msg: HostMsg) {
    #[cfg(not(target_arch = "wasm32"))]
    let Some(msg) = crate::ai::local::deliver(conn, msg) else {
        return;
    };
    bridge::send_to(conn, msg);
}

#[derive(Default)]
struct Conns {
    seats: BTreeMap<u64, u8>,
    asked: BTreeMap<u64, BTreeSet<[u8; 32]>>,
}

#[derive(Resource, Default)]
pub struct HostState {
    session: Option<HostSession>,
    conns: Conns,
    conn_nodes: BTreeMap<u64, String>,
    new_game_asked: Option<u8>,
    undo_sent: Option<agni_net::session::UndoStatus>,
}

impl HostState {
    pub fn plugin_view(&mut self, seat: u8) -> Option<agni_sim::wire::PluginView> {
        self.session
            .as_mut()
            .map(|session| session.plugin_view(seat))
    }
}

impl HostState {
    fn clear(&mut self) {
        self.undo_sent = None;
        self.session = None;
        self.conns.clear();
        self.conn_nodes.clear();
    }

    fn refresh_into(&self, table: &mut ResMut<GameTable>, mirror: &mut ResMut<Mirror>) {
        if let Some(session) = self.session.as_ref() {
            refresh(table, mirror, session.table(), session.view());
        }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn hosted(session: HostSession) -> Self {
        Self {
            session: Some(session),
            ..Self::default()
        }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn session(&self) -> &HostSession {
        self.session.as_ref().expect("a hosted table")
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn new_game(&mut self) -> Result<(), SessionError> {
        let session = self.session.as_mut().expect("a hosted table");
        let (entries, _) = session.reset(Vec::new())?;
        self.conns.broadcast(&entries);
        Ok(())
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn deck_dealt(&self, seat: u8) -> bool {
        deck_already_dealt(self.session(), seat)
    }

    pub(crate) fn peer_joined(&mut self, conn: u64, peer: String) {
        self.conn_nodes.insert(conn, peer);
    }

    pub(crate) fn peer_left(&mut self, info: &mut SessionInfo, conn: u64, reason: &str) {
        self.conn_nodes.remove(&conn);
        if let Some(seat) = self.conns.forget(conn) {
            if let Some(session) = self.session.as_mut() {
                session.disconnect(seat);
                let roster = session.roster();
                self.conns.broadcast_roster(&roster);
                info.roster = roster;
                info.status = format!("player {} disconnected ({reason})", seat + 1);
            }
        }
    }

    pub(crate) fn own_intent(&mut self, seat: u8, intent: WireIntent) -> Result<(), SessionError> {
        let Some(session) = self.session.as_mut() else {
            return Ok(());
        };
        match session.intent(seat, intent) {
            Ok(entries) => {
                self.conns.deliver(session, &entries);
                Ok(())
            }
            Err(error) => {
                self.conns.deliver(session, error.applied());
                Err(error)
            }
        }
    }

    pub(crate) fn own_deal(
        &mut self,
        seat: u8,
        groups: Vec<agni_sim::wire::DealGroup>,
    ) -> Result<(), SessionError> {
        let Some(session) = self.session.as_mut() else {
            return Ok(());
        };
        let (entries, _) = session.deal_groups(seat, groups)?;
        self.conns.broadcast(&entries);
        Ok(())
    }

    pub(crate) fn peer_message(
        &mut self,
        info: &mut SessionInfo,
        conn: u64,
        msg: ClientMsg,
    ) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        match msg {
            ClientMsg::RequestUndo { actions, revision } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                let result = session.request_undo(seat, actions, revision);
                undo::publish_result(session, &self.conns, info, result, Some(conn))
            }
            ClientMsg::VoteUndo { id, accept } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                let result = session.vote_undo(seat, id, accept);
                undo::publish_result(session, &self.conns, info, result, Some(conn))
            }
            ClientMsg::Join { name, version } => {
                self.undo_sent = None;
                if let Some(reason) = version_mismatch(version) {
                    send_to(conn, HostMsg::End { reason });
                    return false;
                }
                let node = self
                    .conn_nodes
                    .get(&conn)
                    .cloned()
                    .unwrap_or_else(|| format!("conn:{conn}"));
                let (seat, join_entry) = match session.join_as(&node, &name) {
                    Ok(seated) => seated,
                    Err(error) => {
                        send_to(
                            conn,
                            HostMsg::End {
                                reason: format!("the host could not seat you: {error}"),
                            },
                        );
                        session_failed(info, "seating a joiner", &error);
                        return false;
                    }
                };
                let reclaimed = join_entry.is_none();
                let entries: Vec<LogEntry> = join_entry.into_iter().collect();
                let roster = session.roster();
                self.conns.broadcast(&entries);
                self.conns.broadcast_roster(&roster);
                self.conns.seats.insert(conn, seat);
                send_to(
                    conn,
                    HostMsg::Welcome {
                        version: WIRE_VERSION,
                        seat,
                        roster: roster.clone(),
                        log: session.log().to_vec(),
                    },
                );
                let owed = session.faces_owed_to(seat);
                if !owed.is_empty() {
                    send_to(conn, HostMsg::Faces { faces: owed });
                }
                info.status = if reclaimed {
                    format!("{name} reconnected to player {}", seat + 1)
                } else {
                    format!("{name} joined as player {}", seat + 1)
                };
                info.roster = roster;
                true
            }
            ClientMsg::Intent { intent } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                match session.intent(seat, intent) {
                    Ok(entries) => self.conns.deliver(session, &entries),
                    Err(error) => {
                        self.conns.deliver(session, error.applied());
                        if let Some(text) = refusal_notice(&error) {
                            send_to(conn, HostMsg::Notice { text });
                        }
                        session_failed(info, &format!("player {}'s move", seat + 1), &error);
                    }
                }
                true
            }
            ClientMsg::DealDeck { groups } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                let battlefields = crate::deck::import::battlefield_only(&groups);
                if let Some(reason) = deal_refusal(session, seat, battlefields) {
                    send_to(
                        conn,
                        HostMsg::Notice {
                            text: reason.into(),
                        },
                    );
                    return false;
                }
                match session.deal_groups(seat, groups) {
                    Ok((entries, owner_faces)) => {
                        self.conns.broadcast(&entries);
                        if !owner_faces.is_empty() {
                            send_to(conn, HostMsg::Faces { faces: owner_faces });
                        }
                        info.status = format!("player {} dealt a deck", seat + 1);
                    }
                    Err(error) => {
                        session_failed(info, &format!("player {}'s deal", seat + 1), &error);
                    }
                }
                true
            }
            ClientMsg::ReloadDeck { groups } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                match session.reload_groups(seat, groups) {
                    Ok((entries, owner_faces)) => {
                        self.conns.broadcast(&entries);
                        if !owner_faces.is_empty() {
                            send_to(conn, HostMsg::Faces { faces: owner_faces });
                        }
                        info.status = format!("player {} reloaded their deck", seat + 1);
                    }
                    Err(error) => {
                        session_failed(info, &format!("player {}'s reload", seat + 1), &error);
                    }
                }
                true
            }
            ClientMsg::NewGame => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                self.new_game_asked = Some(seat);
                info.status = format!("player {} asked for a new game", seat + 1);
                false
            }
            ClientMsg::PickPlaymat { playmat } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                if session.pick_playmat(seat, playmat) {
                    let roster = session.roster();
                    self.conns.broadcast_roster(&roster);
                    info.roster = roster;
                }
                false
            }
            ClientMsg::NeedModule { hash } => {
                for msg in self.conns.module_frames(session, conn, hash) {
                    send_to(conn, msg);
                }
                false
            }
            ClientMsg::PickColor { color } => {
                let Some(&seat) = self.conns.seats.get(&conn) else {
                    return false;
                };
                let took = session.pick_color(seat, color);
                let roster = session.roster();
                self.conns.broadcast_roster(&roster);
                info.roster = roster;
                if took {
                    info.status = format!(
                        "player {} took {}",
                        seat + 1,
                        crate::table::colors::seat_color_name(color)
                    );
                }
                false
            }
        }
    }
}

impl Conns {
    fn clear(&mut self) {
        self.seats.clear();
        self.asked.clear();
    }

    fn forget(&mut self, conn: u64) -> Option<u8> {
        self.asked.remove(&conn);
        self.seats.remove(&conn)
    }

    fn module_frames(&mut self, session: &HostSession, conn: u64, hash: [u8; 32]) -> Vec<HostMsg> {
        if !self.seats.contains_key(&conn) {
            return Vec::new();
        }
        if !self.asked.entry(conn).or_default().insert(hash) {
            return Vec::new();
        }
        session.module_frames(hash)
    }

    fn broadcast(&self, entries: &[LogEntry]) {
        for entry in entries {
            for &conn in self.seats.keys() {
                send_to(
                    conn,
                    HostMsg::Entry {
                        entry: entry.clone(),
                    },
                );
            }
        }
    }

    fn broadcast_roster(&self, roster: &[SeatInfo]) {
        for &conn in self.seats.keys() {
            send_to(
                conn,
                HostMsg::Roster {
                    roster: roster.to_vec(),
                },
            );
        }
    }

    fn conn_for_seat(&self, seat: u8) -> Option<u64> {
        self.seats
            .iter()
            .find(|(_, &conn_seat)| conn_seat == seat)
            .map(|(&conn, _)| conn)
    }

    fn private_faces(&self, session: &mut HostSession) -> Vec<(u64, HostMsg)> {
        session
            .owed_faces()
            .into_iter()
            .filter(|(seat, _)| *seat != 0)
            .filter_map(|(seat, face)| {
                let conn = self.conn_for_seat(seat)?;
                Some((conn, HostMsg::Faces { faces: vec![face] }))
            })
            .collect()
    }

    fn send_private_faces(&self, session: &mut HostSession) {
        for (conn, msg) in self.private_faces(session) {
            send_to(conn, msg);
        }
    }

    fn deliver(&self, session: &mut HostSession, entries: &[LogEntry]) {
        self.broadcast(entries);
        self.send_private_faces(session);
    }
}

fn refresh(
    table: &mut ResMut<GameTable>,
    mirror: &mut ResMut<Mirror>,
    next: agni_core::Table,
    view: &TableView,
) {
    if table.0 != next {
        table.0 = next;
    }
    if mirror.view != *view {
        mirror.view = view.clone();
    }
}

fn free_form_table(session: &HostSession) -> bool {
    !session.state().has_zone_table()
}

fn deck_already_dealt(session: &HostSession, seat: u8) -> bool {
    session
        .state()
        .zones
        .iter()
        .filter(|decl| decl.kind == ZoneKind::Deck && decl.owner == ZoneOwner::PerSeat)
        .any(|decl| {
            session
                .state()
                .table
                .in_area(PlayerId(seat), Zone::Plugin(decl.id))
                .next()
                .is_some()
        })
}

fn session_failed(info: &mut SessionInfo, what: &str, error: &SessionError) {
    if error.is_engine_fault() {
        info.role = SessionRole::Ended;
        info.recovery = Recovery::Nothing;
        info.status = format!("{what} broke the engine — table frozen: {error}");
    } else {
        info.status = format!("{what} refused: {error}");
    }
}

fn my_intent_failed(info: &mut SessionInfo, what: &str, error: &SessionError) {
    session_failed(info, what, error);
    if !error.is_engine_fault() {
        info.notices.push(error.to_string());
    }
}

pub const NOTICE_REFUSED: &str = "refused: ";

pub fn notice_text(text: &str) -> String {
    text.strip_prefix(NOTICE_REFUSED)
        .unwrap_or(text)
        .to_string()
}

fn refusal_notice(error: &SessionError) -> Option<String> {
    (!error.is_engine_fault()).then(|| format!("refused: {error}"))
}

fn replica_failed(info: &mut SessionInfo, fault: &EngineFault) {
    info.role = SessionRole::Ended;
    info.recovery = Recovery::Nothing;
    info.status = format!("this replica's engine broke — table frozen: {fault}");
}

pub struct PendingWelcome {
    seat: u8,
    roster: Vec<SeatInfo>,
    log: Vec<LogEntry>,
    queued: Vec<HostMsg>,
}

impl PendingWelcome {
    pub fn new(seat: u8, roster: Vec<SeatInfo>, log: Vec<LogEntry>) -> Self {
        Self {
            seat,
            roster,
            log,
            queued: Vec::new(),
        }
    }

    pub fn seat(&self) -> u8 {
        self.seat
    }

    pub fn roster(&self) -> &[SeatInfo] {
        &self.roster
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    pub fn queued(&self) -> usize {
        self.queued.len()
    }

    pub fn absorb(&mut self, msg: HostMsg) -> Option<HostMsg> {
        match msg {
            HostMsg::Entry { .. } | HostMsg::Faces { .. } | HostMsg::RolledBack { .. } => {
                self.queued.push(msg);
                None
            }
            HostMsg::Roster { roster } => {
                self.roster = roster;
                None
            }
            other => Some(other),
        }
    }

    pub fn seat_replica(
        self,
        engine: Box<dyn agni_sim::engine::Engine>,
        plugin: Option<Box<dyn agni_sim::engine::PluginModule>>,
    ) -> Result<(ClientSession, Vec<SeatInfo>), EngineFault> {
        let Self {
            seat,
            roster,
            log,
            queued,
        } = self;
        let mut session =
            ClientSession::from_welcome_with(seat, roster.clone(), log, engine, plugin)?;
        for msg in queued {
            match msg {
                HostMsg::Entry { entry } => {
                    session.apply(entry)?;
                }
                HostMsg::Faces { faces } => session.add_faces(faces),
                HostMsg::RolledBack { next_seq, faces } => session.rollback(next_seq, faces)?,
                _ => {}
            }
        }
        Ok((session, roster))
    }
}

#[derive(Resource, Default)]
pub struct ClientState {
    session: Option<ClientSession>,
    pending: Option<PendingWelcome>,
}

impl ClientState {
    pub fn plugin_view(&mut self, seat: u8) -> Option<agni_sim::wire::PluginView> {
        self.session
            .as_mut()
            .map(|session| session.plugin_view(seat))
    }
}

pub fn drain_net(
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut generation: ResMut<DealGeneration>,
    mut info: ResMut<SessionInfo>,
    mut my_seat: ResMut<MySeat>,
    mut view: ResMut<ViewSeat>,
    mut players: ResMut<PlayerCount>,
    mut host: ResMut<HostState>,
    mut client: ResMut<ClientState>,
    choice: Res<TableChoice>,
) {
    let requests: Vec<String> = bridge::take_join_requests();
    for host_id in requests {
        if matches!(info.role, SessionRole::Solo | SessionRole::Ended) {
            let returning = matches!(&info.recovery, Recovery::Rejoin { host } if host == &host_id);
            client.session = None;
            client.pending = None;
            crate::engine::modules::reset_fetches();
            info.role = SessionRole::Joining;
            info.recovery = Recovery::Rejoin {
                host: host_id.clone(),
            };
            info.status = if returning {
                "reconnecting to the table over iroh…".into()
            } else {
                "joining table over iroh…".into()
            };
            start_join(host_id);
        }
    }
    #[allow(unused_mut)]
    let mut events: Vec<NetToGame> = bridge::drain_events();
    #[cfg(not(target_arch = "wasm32"))]
    events.extend(crate::ai::local::events());
    for event in events {
        match event {
            NetToGame::HostReady if host.session.is_some() => {
                let host = &mut *host;
                let session = host.session.as_ref().expect("kept host session");
                info.role = SessionRole::Host;
                info.recovery = Recovery::Nothing;
                info.roster = session.roster();
                info.options = session.state().options.as_ref().map(|bytes| bytes.to_vec());
                info.status = "hosting again — the same table, your players can reconnect".into();
                refresh(&mut table, &mut mirror, session.table(), session.view());
                players.0 = session.seat_count().max(2);
                my_seat.0 = PlayerId(0);
                view.0 = PlayerId(0);
            }
            NetToGame::HostReady => {
                let (engine, engine_note) = match crate::engine::hosting_engine() {
                    Ok(engine) => engine,
                    Err(reason) => {
                        refuse_hosting(&mut info, format!("hosting refused — {reason}"));
                        continue;
                    }
                };
                if let Some(plugin_ref) = choice.game.plugin_ref() {
                    if crate::engine::modules::selected_plugin().is_none() {
                        crate::engine::modules::select_plugin(Some(plugin_ref.into()));
                    }
                }
                let active = crate::engine::modules::active_plugin();
                if let Some(reason) = enforced_without_plugin(
                    rules_enforced(&info, &choice),
                    active.module.is_some(),
                    active.note.as_deref(),
                ) {
                    refuse_hosting(&mut info, reason);
                    continue;
                }
                let mut zone_note = None;
                let config = match choice.game.compiled_zones() {
                    None => TableConfig::default(),
                    Some(compiled) => {
                        let zones = match active.zones {
                            Some(zones) => {
                                zone_note = Some("zones from the plugin manifest".to_string());
                                zones
                            }
                            None => {
                                zone_note = Some("zones from the compiled table".to_string());
                                compiled
                            }
                        };
                        TableConfig {
                            engine: None,
                            plugin: None,
                            zones,
                            options: choice.config_options(),
                            counters: active
                                .counters
                                .clone()
                                .or_else(|| choice.game.compiled_counters())
                                .unwrap_or_default(),
                            despawn_any: active.despawn_any,
                        }
                    }
                };
                let mut session = match HostSession::host_from_with(
                    &player_name(),
                    &table.0,
                    config,
                    engine,
                    active.module,
                ) {
                    Ok(session) => session,
                    Err(error) => {
                        refuse_hosting(&mut info, format!("hosting failed: {error}"));
                        continue;
                    }
                };
                crate::engine::modules::serve_into(&mut session, active.bytes);
                let notes: Vec<String> = [engine_note, active.note, zone_note]
                    .into_iter()
                    .flatten()
                    .collect();
                info.role = SessionRole::Host;
                info.recovery = Recovery::Nothing;
                info.options = session.state().options.as_ref().map(|bytes| bytes.to_vec());
                info.status = if notes.is_empty() {
                    "hosting — table visible to your peers".into()
                } else {
                    format!(
                        "hosting — table visible to your peers ({})",
                        notes.join("; ")
                    )
                };
                info.roster = session.roster();
                refresh(&mut table, &mut mirror, session.table(), session.view());
                host.session = Some(session);
                my_seat.0 = PlayerId(0);
                view.0 = PlayerId(0);
                players.0 = 2;
            }
            NetToGame::HostFailed { error } => {
                info.role = if host.session.is_some() {
                    SessionRole::Ended
                } else {
                    SessionRole::Solo
                };
                info.status = format!("hosting failed: {error}");
            }
            NetToGame::HostClosed => {
                #[cfg(not(target_arch = "wasm32"))]
                crate::ai::local::end("the table closed");
                host_closed(&mut info, &mut host);
            }
            NetToGame::HostLost { reason } => {
                #[cfg(not(target_arch = "wasm32"))]
                crate::ai::local::end(&reason);
                host.conns.clear();
                host.conn_nodes.clear();
                if let Some(session) = host.session.as_mut() {
                    session.disconnect_all_guests();
                    info.roster = session.roster();
                }
                keep_awake(false);
                info.role = SessionRole::Ended;
                info.recovery = Recovery::Rehost;
                info.status = format!("hosting stopped: {reason}");
            }
            NetToGame::PeerJoined { conn, peer } => {
                host.peer_joined(conn, peer);
            }
            NetToGame::PeerFrame { conn, msg } => {
                handle_peer(
                    &mut table,
                    &mut mirror,
                    &mut info,
                    &mut players,
                    &mut host,
                    conn,
                    msg,
                );
            }
            NetToGame::PeerLeft { conn, reason } => {
                host.peer_left(&mut info, conn, &reason);
            }
            NetToGame::Connected => {
                info.status = "connected — waiting for a seat".into();
            }
            NetToGame::FromHost { msg } => {
                handle_host_msg(
                    &mut table,
                    &mut mirror,
                    &mut generation,
                    &mut info,
                    &mut my_seat,
                    &mut view,
                    &mut players,
                    &mut client,
                    msg,
                );
            }
            NetToGame::Dropped { reason } => {
                if matches!(info.role, SessionRole::Client | SessionRole::Joining) {
                    client.pending = None;
                    crate::engine::modules::reset_fetches();
                    if client.session.is_some() {
                        info.role = SessionRole::Ended;
                        info.status = format!("session over: {reason}");
                    } else {
                        info.role = SessionRole::Solo;
                        info.status = format!("join failed: {reason}");
                    }
                }
            }
        }
    }
    if client.pending.is_some() {
        try_finish_join(
            &mut table,
            &mut mirror,
            &mut generation,
            &mut info,
            &mut my_seat,
            &mut view,
            &mut players,
            &mut client,
        );
    }
}

fn handle_peer(
    table: &mut ResMut<GameTable>,
    mirror: &mut ResMut<Mirror>,
    info: &mut ResMut<SessionInfo>,
    players: &mut ResMut<PlayerCount>,
    host: &mut ResMut<HostState>,
    conn: u64,
    msg: ClientMsg,
) {
    let host = &mut **host;
    if host.peer_message(info, conn, msg) {
        if let Some(session) = host.session.as_ref() {
            refresh(table, mirror, session.table(), session.view());
            players.0 = session.seat_count().max(2);
        }
    }
}

fn try_finish_join(
    table: &mut ResMut<GameTable>,
    mirror: &mut ResMut<Mirror>,
    generation: &mut ResMut<DealGeneration>,
    info: &mut ResMut<SessionInfo>,
    my_seat: &mut ResMut<MySeat>,
    view: &mut ResMut<ViewSeat>,
    players: &mut ResMut<PlayerCount>,
    client: &mut ResMut<ClientState>,
) {
    let Some(pending) = client.pending.as_ref() else {
        return;
    };
    let host = match &info.recovery {
        Recovery::Rejoin { host } => Some(host.as_str()),
        _ => None,
    };
    let outcome = crate::engine::modules::prepare_join(pending.log(), host);
    match outcome {
        crate::engine::modules::JoinModules::Pending { status } => {
            info.status = status;
        }
        crate::engine::modules::JoinModules::Refused { error } => {
            client.pending = None;
            info.role = SessionRole::Ended;
            info.status = error;
        }
        crate::engine::modules::JoinModules::Ready {
            engine,
            plugin,
            note,
        } => {
            let pending = client.pending.take().expect("pending welcome present");
            let loaded_engine = engine.engine_hash().map(engine_blob_ref);
            if let Err(error) = verify_engine_pin(pending.log(), loaded_engine.as_deref()) {
                info.role = SessionRole::Ended;
                info.status = error;
                return;
            }
            let loaded_plugin = plugin
                .as_ref()
                .and_then(|plugin| plugin.module_hash())
                .map(engine_blob_ref);
            if let Err(error) = verify_plugin_pin(pending.log(), loaded_plugin.as_deref()) {
                info.role = SessionRole::Ended;
                info.status = error;
                return;
            }
            let seat = pending.seat();
            let reclaimed = my_seat.0 == PlayerId(seat)
                && matches!(info.recovery, Recovery::Rejoin { .. })
                && seat != 0;
            let (session, roster) = match pending.seat_replica(engine, plugin) {
                Ok(seated) => seated,
                Err(fault) => {
                    replica_failed(info, &fault);
                    return;
                }
            };
            refresh(table, mirror, session.table(), session.view());
            generation.0 += 1;
            info.options = session.state().options.as_ref().map(|bytes| bytes.to_vec());
            client.session = Some(session);
            info.role = SessionRole::Client;
            info.roster = roster;
            let seated = if reclaimed {
                format!("back in your seat as player {}", seat + 1)
            } else {
                format!("seated as player {}", seat + 1)
            };
            info.status = if note.is_empty() {
                seated
            } else {
                format!("{seated} ({note})")
            };
            my_seat.0 = PlayerId(seat);
            view.0 = PlayerId(seat);
            players.0 = info.roster.len().max(2);
        }
    }
}

fn handle_host_msg(
    table: &mut ResMut<GameTable>,
    mirror: &mut ResMut<Mirror>,
    generation: &mut ResMut<DealGeneration>,
    info: &mut ResMut<SessionInfo>,
    my_seat: &mut ResMut<MySeat>,
    view: &mut ResMut<ViewSeat>,
    players: &mut ResMut<PlayerCount>,
    client: &mut ResMut<ClientState>,
    msg: HostMsg,
) {
    let msg = match (client.session.is_none(), client.pending.as_mut()) {
        (true, Some(pending)) => match pending.absorb(msg) {
            Some(msg) => msg,
            None => {
                players.0 = pending.roster().len().max(2);
                if info.roster != pending.roster() {
                    info.roster = pending.roster().to_vec();
                }
                return;
            }
        },
        _ => msg,
    };
    match msg {
        HostMsg::Undo { status } => info.undo = status,
        HostMsg::RolledBack { next_seq, faces } => {
            if let Some(session) = client.session.as_mut() {
                match session.rollback(next_seq, faces) {
                    Ok(()) => {
                        refresh(table, mirror, session.table(), session.view());
                        info.status = "rollback accepted".into();
                        info.undo_generation += 1;
                    }
                    Err(fault) => replica_failed(info, &fault),
                }
            }
        }
        HostMsg::Welcome {
            version,
            seat,
            roster,
            log,
        } => {
            if let Some(reason) = version_mismatch(version) {
                client.pending = None;
                info.role = SessionRole::Ended;
                info.status = reason;
                return;
            }
            client.pending = Some(PendingWelcome::new(seat, roster, log));
            info.undo = Default::default();
            try_finish_join(
                table, mirror, generation, info, my_seat, view, players, client,
            );
        }
        HostMsg::Roster { roster } => {
            if let Some(session) = client.session.as_mut() {
                session.set_roster(roster.clone());
            }
            players.0 = roster.len().max(2);
            if info.roster != roster {
                info.roster = roster;
            }
        }
        HostMsg::Entry { entry } => {
            let Some(session) = client.session.as_mut() else {
                return;
            };
            let is_reset = matches!(entry.action, LogAction::Reset | LogAction::Clear { .. });
            match session.apply(entry) {
                Ok(true) => {
                    refresh(table, mirror, session.table(), session.view());
                    if is_reset {
                        generation.0 += 1;
                    }
                }
                Ok(false) => {}
                Err(fault) => replica_failed(info, &fault),
            }
        }
        HostMsg::Faces { faces } => {
            if let Some(session) = client.session.as_mut() {
                session.add_faces(faces);
                refresh(table, mirror, session.table(), session.view());
            }
        }
        HostMsg::End { reason } => {
            info.role = SessionRole::Ended;
            info.status = format!("host ended the session: {reason}");
        }
        HostMsg::Module { .. } | HostMsg::NoModule { .. } => {
            crate::engine::modules::module_frame(&msg);
        }
        HostMsg::Notice { text } => {
            let seat_name = |seat: u8| {
                info.roster
                    .iter()
                    .find(|held| held.seat == seat)
                    .map(|held| held.name.clone())
                    .unwrap_or_else(|| format!("seat {seat}"))
            };
            let zone_name = |zone: u16| {
                mirror
                    .view
                    .zones
                    .iter()
                    .find(|decl| decl.id == zone)
                    .map(|decl| decl.label.clone())
                    .unwrap_or_else(|| format!("zone {zone}"))
            };
            let card_name = |card: u32| {
                crate::table::plugin_ui::card_label(&table.0, &mirror.view, my_seat.0, card)
            };
            info.status =
                crate::table::plugin_ui::expand(&text, &seat_name, &zone_name, &card_name);
            let notice = notice_text(&info.status);
            info.notices.push(notice);
        }
    }
}

pub fn route_drops(
    mut dropped: MessageReader<CardDropped>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut client: ResMut<ClientState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for drop in dropped.read() {
        let intent = if drop.hidden {
            WireIntent::MoveHidden {
                card: drop.card.0,
                to: drop.to,
                seat: drop.seat.0,
                index: drop.index as u32,
            }
        } else {
            WireIntent::Move {
                card: drop.card.0,
                to: drop.to,
                seat: drop.seat.0,
                index: drop.index as u32,
            }
        };
        match info.role {
            SessionRole::Host => {
                if let Err(error) = host.own_intent(my_seat.0 .0, intent) {
                    my_intent_failed(&mut info, "your move", &error);
                }
                host.refresh_into(&mut table, &mut mirror);
            }
            SessionRole::Client => {
                if let Some(session) = client.session.as_mut() {
                    session.optimistic(intent.clone());
                }
                table.apply(Intent::MoveCard {
                    card: drop.card,
                    to: drop.to,
                    seat: drop.seat,
                    index: drop.index,
                });
                bridge::send_to_host(ClientMsg::Intent { intent });
            }
            SessionRole::Solo => {
                table.apply(Intent::MoveCard {
                    card: drop.card,
                    to: drop.to,
                    seat: drop.seat,
                    index: drop.index,
                });
            }
            SessionRole::Starting | SessionRole::Joining | SessionRole::Ended => {}
        }
    }
}

pub fn route_redeal(
    mut requests: MessageReader<Redeal>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut generation: ResMut<DealGeneration>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    mut art: ResMut<crate::render::art::ArtCache>,
    tuning: Res<Tuning>,
) {
    let asked = host.new_game_asked.take().is_some();
    if requests.read().next().is_none() && !asked {
        return;
    }
    if info.role != SessionRole::Host {
        return;
    }
    let host = &mut *host;
    let Some(session) = host.session.as_mut() else {
        return;
    };
    let hands = if free_form_table(session) {
        session
            .roster()
            .iter()
            .map(|seat| (seat.seat, app::sample_faces(tuning.foil_chance, &mut art)))
            .collect()
    } else {
        Vec::new()
    };
    match session.reset(hands) {
        Ok((entries, faces_by_seat)) => {
            host.conns.broadcast(&entries);
            for (seat, faces) in faces_by_seat {
                if seat == 0 {
                    continue;
                }
                if let Some(conn) = host.conns.conn_for_seat(seat) {
                    send_to(conn, HostMsg::Faces { faces });
                }
            }
        }
        Err(error) => {
            host.conns.broadcast(error.applied());
            my_intent_failed(&mut info, "the redeal", &error);
        }
    }
    refresh(&mut table, &mut mirror, session.table(), session.view());
    generation.0 += 1;
}

#[derive(Resource, Default)]
pub struct NewGameWatch {
    pub seen: u32,
}

pub fn redeal_after_new_game(
    info: Res<SessionInfo>,
    seated: Res<crate::deck::import::SeatedDeck>,
    generation: Res<DealGeneration>,
    mut watch: ResMut<NewGameWatch>,
    mut panel: ResMut<crate::deck::import::ImportPanel>,
    mut deals: MessageWriter<crate::deck::import::DealDeckRequested>,
) {
    if watch.seen == generation.0 {
        return;
    }
    watch.seen = generation.0;
    if seated.0.is_none() || !info.active() {
        return;
    }
    let _ = &mut panel;
    deals.write(crate::deck::import::DealDeckRequested);
}

pub fn deal_setup(
    info: &SessionInfo,
    my_seat: &MySeat,
    generation: &DealGeneration,
) -> Option<crate::deck::import::DealSetup> {
    if !info.active() {
        return None;
    }
    let players = info.roster.len() as u8;
    let first_player = info
        .roster
        .iter()
        .find(|seat| seat.host)
        .map(|seat| seat.seat)
        .unwrap_or(0);
    Some(crate::deck::import::DealSetup {
        players,
        seat: my_seat.0 .0,
        first_player,
        generation: generation.0,
        battlefields: info.options_in_play(usize::from(players)).battlefields,
    })
}

pub fn route_deck_deals(
    mut requests: MessageReader<crate::deck::import::DealDeckRequested>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    seated: Res<crate::deck::import::SeatedDeck>,
    my_seat: Res<MySeat>,
    generation: Res<DealGeneration>,
) {
    if requests.read().next().is_none() {
        return;
    }
    let Some(record) = &seated.0 else {
        return;
    };
    let groups = crate::deck::import::body_plan(record, deal_setup(&info, &my_seat, &generation));
    send_deal(
        groups,
        &mut table,
        &mut mirror,
        &mut host,
        &mut info,
        &my_seat,
    );
}

pub fn route_battlefield_placements(
    mut requests: MessageReader<crate::deck::import::PlaceBattlefieldRequested>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    seated: Res<crate::deck::import::SeatedDeck>,
    my_seat: Res<MySeat>,
    generation: Res<DealGeneration>,
) {
    if requests.read().next().is_none() {
        return;
    }
    let Some(record) = &seated.0 else {
        return;
    };
    let groups =
        crate::deck::import::battlefield_plan(record, deal_setup(&info, &my_seat, &generation));
    send_deal(
        groups,
        &mut table,
        &mut mirror,
        &mut host,
        &mut info,
        &my_seat,
    );
}

fn send_deal(
    groups: Vec<agni_sim::wire::DealGroup>,
    table: &mut ResMut<GameTable>,
    mirror: &mut ResMut<Mirror>,
    host: &mut ResMut<HostState>,
    info: &mut ResMut<SessionInfo>,
    my_seat: &MySeat,
) {
    if groups.is_empty() {
        return;
    }
    let battlefields = crate::deck::import::battlefield_only(&groups);
    match info.role {
        SessionRole::Host => {
            let Some(session) = host.session.as_ref() else {
                return;
            };
            if let Some(reason) = deal_refusal(session, my_seat.0 .0, battlefields) {
                info.status = reason.into();
                return;
            }
            let what = if battlefields {
                "your battlefield"
            } else {
                "your deal"
            };
            match host.own_deal(my_seat.0 .0, groups) {
                Ok(()) if battlefields => info.status = "battlefield placed".into(),
                Ok(()) => info.status = "deck dealt".into(),
                Err(error) => my_intent_failed(info, what, &error),
            }
            host.refresh_into(table, mirror);
        }
        SessionRole::Client => {
            bridge::send_to_host(ClientMsg::DealDeck { groups });
            info.status = if battlefields {
                "battlefield sent to the host".into()
            } else {
                "deal requested from the host".into()
            };
        }
        SessionRole::Solo | SessionRole::Starting | SessionRole::Joining | SessionRole::Ended => {}
    }
}

fn deal_refusal(session: &HostSession, seat: u8, battlefields: bool) -> Option<&'static str> {
    if battlefields {
        return battlefield_already_placed(session, seat)
            .then_some("your battlefield is already on the table");
    }
    deck_already_dealt(session, seat).then_some("your deck is already on the table")
}

fn battlefield_already_placed(session: &HostSession, seat: u8) -> bool {
    crate::deck::battlefield::placed_on_table(
        &session.state().table,
        &session.state().zones,
        PlayerId(seat),
    )
}

pub fn route_deck_reloads(
    mut requests: MessageReader<crate::deck::sideboard::ReloadDeckRequested>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut generation: ResMut<DealGeneration>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    seated: Res<crate::deck::import::SeatedDeck>,
    my_seat: Res<MySeat>,
) {
    if requests.read().next().is_none() {
        return;
    }
    let Some(record) = &seated.0 else {
        return;
    };
    let groups = crate::deck::import::deal_plan_for(record);
    if groups.is_empty() {
        return;
    }
    match info.role {
        SessionRole::Host => {
            let host = &mut *host;
            let Some(session) = host.session.as_mut() else {
                return;
            };
            match session.reload_groups(my_seat.0 .0, groups) {
                Ok((entries, _)) => {
                    host.conns.broadcast(&entries);
                    generation.0 += 1;
                    info.status = "deck reloaded from your list".into();
                }
                Err(error) => my_intent_failed(&mut info, "your reload", &error),
            }
            refresh(&mut table, &mut mirror, session.table(), session.view());
        }
        SessionRole::Client => {
            bridge::send_to_host(ClientMsg::ReloadDeck { groups });
            info.status = "reload requested from the host".into();
        }
        SessionRole::Solo | SessionRole::Starting | SessionRole::Joining | SessionRole::Ended => {}
    }
}

fn send_intent(
    intent: WireIntent,
    what: &str,
    table: &mut ResMut<GameTable>,
    mirror: &mut ResMut<Mirror>,
    host: &mut HostState,
    client: &mut ClientState,
    info: &mut SessionInfo,
    my_seat: PlayerId,
) {
    match info.role {
        SessionRole::Host => {
            if let Err(error) = host.own_intent(my_seat.0, intent) {
                my_intent_failed(info, what, &error);
            }
            host.refresh_into(table, mirror);
        }
        SessionRole::Client => {
            if let Some(session) = client.session.as_mut() {
                session.optimistic(intent.clone());
            }
            bridge::send_to_host(ClientMsg::Intent { intent });
        }
        SessionRole::Solo => {
            if let WireIntent::Spawn { face, to, seat } = intent {
                table.add_face(PlayerId(seat), to, face);
            }
        }
        SessionRole::Starting | SessionRole::Joining | SessionRole::Ended => {}
    }
}

pub fn route_reveals(
    mut reveals: MessageReader<crate::table::RevealRequested>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut client: ResMut<ClientState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for reveal in reveals.read() {
        send_intent(
            WireIntent::Reveal { card: reveal.0 .0 },
            "your reveal",
            &mut table,
            &mut mirror,
            &mut host,
            &mut client,
            &mut info,
            my_seat.0,
        );
    }
}

pub fn route_manual(
    mut requests: MessageReader<crate::table::manual::Requested>,
    panel: Res<crate::table::plugin_ui::PluginPanel>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut client: ResMut<ClientState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for request in requests.read() {
        if !crate::table::manual::available(&panel.view, info.role) {
            continue;
        }
        send_intent(
            request.0.clone(),
            "your manual change",
            &mut table,
            &mut mirror,
            &mut host,
            &mut client,
            &mut info,
            my_seat.0,
        );
    }
}

pub fn route_spawns(
    mut spawns: MessageReader<crate::table::TokenSpawn>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut client: ResMut<ClientState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for spawn in spawns.read() {
        send_intent(
            WireIntent::Spawn {
                face: spawn.face.clone(),
                to: spawn.to,
                seat: spawn.seat.0,
            },
            "your token",
            &mut table,
            &mut mirror,
            &mut host,
            &mut client,
            &mut info,
            my_seat.0,
        );
    }
}

pub fn route_playmat_picks(
    tuning: Res<crate::table::Tuning>,
    library: Res<crate::table::playmat::PlaymatLibrary>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
    mut announced: Local<Option<(SessionRole, String)>>,
) {
    let wanted = crate::table::playmat::shareable(&tuning.playmat, &library);
    let signature = (info.role, tuning.playmat.clone());
    if announced.as_ref() == Some(&signature) {
        return;
    }
    match info.role {
        SessionRole::Host => {
            let host = &mut *host;
            let Some(session) = host.session.as_mut() else {
                return;
            };
            if session.pick_playmat(my_seat.0 .0, wanted) {
                let roster = session.roster();
                host.conns.broadcast_roster(&roster);
                info.roster = roster;
            }
        }
        SessionRole::Client => {
            if client_seated(&info) {
                bridge::send_to_host(ClientMsg::PickPlaymat { playmat: wanted });
            } else {
                return;
            }
        }
        SessionRole::Solo => {}
        SessionRole::Starting | SessionRole::Joining | SessionRole::Ended => return,
    }
    *announced = Some(signature);
}

fn client_seated(info: &SessionInfo) -> bool {
    !info.roster.is_empty()
}

pub fn route_color_picks(
    mut picks: MessageReader<crate::table::colors::ColorPicked>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for pick in picks.read() {
        match info.role {
            SessionRole::Host => {
                let host = &mut *host;
                let Some(session) = host.session.as_mut() else {
                    continue;
                };
                session.pick_color(my_seat.0 .0, pick.0);
                let roster = session.roster();
                host.conns.broadcast_roster(&roster);
                info.roster = roster;
            }
            SessionRole::Client => {
                bridge::send_to_host(ClientMsg::PickColor { color: pick.0 });
            }
            SessionRole::Solo
            | SessionRole::Starting
            | SessionRole::Joining
            | SessionRole::Ended => {}
        }
    }
}

pub fn route_plugin_actions(
    mut requests: MessageReader<crate::table::plugin_ui::PluginActionRequested>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for request in requests.read() {
        let intent = WireIntent::Game {
            data: ByteBuf::from(request.0.clone()),
        };
        match info.role {
            SessionRole::Host => {
                if let Err(error) = host.own_intent(my_seat.0 .0, intent) {
                    my_intent_failed(&mut info, "the game action", &error);
                }
                host.refresh_into(&mut table, &mut mirror);
            }
            SessionRole::Client => {
                bridge::send_to_host(ClientMsg::Intent { intent });
            }
            SessionRole::Solo
            | SessionRole::Starting
            | SessionRole::Joining
            | SessionRole::Ended => {}
        }
    }
}

pub fn route_counters(
    mut nudges: MessageReader<crate::table::counters::CounterNudged>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for nudge in nudges.read() {
        let intent = WireIntent::Counter {
            target: nudge.target,
            counter: nudge.counter,
            delta: nudge.delta,
        };
        match info.role {
            SessionRole::Host => {
                if let Err(error) = host.own_intent(my_seat.0 .0, intent) {
                    my_intent_failed(&mut info, "the counter change", &error);
                }
                host.refresh_into(&mut table, &mut mirror);
            }
            SessionRole::Client => {
                bridge::send_to_host(ClientMsg::Intent { intent });
            }
            SessionRole::Solo
            | SessionRole::Starting
            | SessionRole::Joining
            | SessionRole::Ended => {}
        }
    }
}

pub fn route_annotations(
    mut toggles: MessageReader<ExhaustToggled>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    my_seat: Res<MySeat>,
) {
    for toggle in toggles.read() {
        let value = toggle.on.then(|| ByteBuf::from(vec![0xf5]));
        let intent = WireIntent::Annotate {
            card: toggle.card.0,
            key: EXHAUSTED.into(),
            value,
        };
        match info.role {
            SessionRole::Host => {
                if let Err(error) = host.own_intent(my_seat.0 .0, intent) {
                    my_intent_failed(&mut info, "the exhaust toggle", &error);
                }
                host.refresh_into(&mut table, &mut mirror);
            }
            SessionRole::Client => {
                bridge::send_to_host(ClientMsg::Intent { intent });
            }
            SessionRole::Solo => {
                if toggle.on {
                    mirror.solo_exhausted.insert(toggle.card.0);
                } else {
                    mirror.solo_exhausted.remove(&toggle.card.0);
                }
            }
            SessionRole::Starting | SessionRole::Joining | SessionRole::Ended => {}
        }
    }
}

pub fn open_tables() -> Vec<OpenTable> {
    let Some(node) = node::get() else {
        return Vec::new();
    };
    let self_id = node.node_id.clone();
    node.mesh
        .open_tables()
        .into_iter()
        .filter(|table| table.host != self_id)
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub fn host_block(choice: &TableChoice) -> Option<String> {
    if let Some(reason) = crate::engine::web::engine_block() {
        return Some(reason);
    }
    if choice.game == TableGame::Riftbound && choice.enforced {
        return crate::engine::modules::plugin_block(crate::engine::modules::RIFTBOUND_REF)
            .map(|reason| format!("rules enforced needs the plugin: {reason}"));
    }
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn host_block(_choice: &TableChoice) -> Option<String> {
    None
}

pub const HIDDEN_TAB_WARNING: &str = "keep this tab in front — the table pauses for everyone while it is hidden; the screen stays awake while you host, but switching apps on a phone hides it";

pub fn hosting_warning() -> Option<&'static str> {
    cfg!(target_arch = "wasm32").then_some(HIDDEN_TAB_WARNING)
}

pub fn host_table(info: &mut SessionInfo) {
    info.role = SessionRole::Starting;
    info.status = "opening table…".into();
    start_host();
}

pub fn join_table(host_id: String) {
    bridge::request_join(host_id);
}

pub fn join_by_ticket(ticket: &str) -> Result<String, String> {
    let id = node::add_peer(ticket.trim())?;
    bridge::request_join(id.clone());
    Ok(id)
}

pub fn rejoin(host: &str) {
    bridge::request_join(host.to_string());
}

pub fn rehost(info: &mut SessionInfo) {
    info.role = SessionRole::Starting;
    info.status = "re-opening the table…".into();
    start_host();
}

pub(crate) fn ask_new_game() {
    bridge::send_to_host(ClientMsg::NewGame);
}

pub(crate) fn leave_session(
    info: &mut ResMut<SessionInfo>,
    host: &mut ResMut<HostState>,
    client: &mut ResMut<ClientState>,
) {
    if host.session.is_some() {
        close_table();
    }
    #[cfg(not(target_arch = "wasm32"))]
    crate::ai::local::end("you left the table");
    host.clear();
    client.session = None;
    client.pending = None;
    info.role = SessionRole::Solo;
    info.recovery = Recovery::Nothing;
    info.roster.clear();
    info.status = "left the table".into();
}

pub fn new_game_hint(info: &SessionInfo) -> String {
    let players = info.roster.len() as u8;
    let options = info.options_in_play(info.roster.len());
    let sanctioned = agni_riftbound::default_mode(players)
        .filter(|mode| agni_riftbound::TableOptions::of_mode(mode) == options)
        .map(|mode| mode.name);
    match sanctioned {
        Some(name) => format!("{players} players — {name} · {}", options.label()),
        None => format!("{players} player(s) — house rules · {}", options.label()),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use agni_core::{CardFace, Table};
    use agni_sim::engine::NativeEngine;
    use agni_sim::log::encode_log;

    #[test]
    fn an_enforced_lobby_never_hosts_a_free_table_when_the_plugin_is_missing() {
        assert_eq!(enforced_without_plugin(false, false, None), None);
        assert_eq!(enforced_without_plugin(true, true, None), None);
        let refused = enforced_without_plugin(true, false, Some("no trusted modules/riftbound"));
        assert_eq!(
            refused.as_deref(),
            Some("hosting refused — rules enforced needs the plugin: no trusted modules/riftbound")
        );
        assert!(enforced_without_plugin(true, false, None)
            .unwrap()
            .starts_with("hosting refused"));
    }

    fn solo_with_legacy_hand() -> Table {
        let mut solo = Table::new();
        for i in 0..7 {
            solo.add_face(
                PlayerId(0),
                Zone::Hand,
                CardFace::named(format!("Lightning Bolt {i}")),
            );
        }
        solo
    }

    fn zoned_session(zones: Vec<ZoneDecl>, solo: &Table) -> HostSession {
        HostSession::host_from_with(
            "rae",
            solo,
            TableConfig {
                engine: None,
                plugin: None,
                zones,
                options: None,
                counters: Vec::new(),
                despawn_any: false,
            },
            Box::new(NativeEngine::new()),
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_zoned_table_hosted_over_a_solo_hand_starts_empty() {
        for zones in [agni_riftbound::zone_table(), agni_mtg::zone_table()] {
            let solo = solo_with_legacy_hand();
            let mut session = zoned_session(zones, &solo);
            assert!(!free_form_table(&session));
            assert!(session.table().is_empty());
            session.join("ada").unwrap();
            let log_bytes = encode_log(session.log());
            assert!(!log_bytes
                .windows(b"Lightning Bolt".len())
                .any(|window| window == b"Lightning Bolt"));
        }
    }

    #[test]
    fn a_free_form_table_carries_only_what_the_solo_table_already_held() {
        let session = HostSession::host_from_with(
            "rae",
            &Table::new(),
            TableConfig::default(),
            Box::new(NativeEngine::new()),
            None,
        )
        .unwrap();
        assert!(free_form_table(&session));
        assert!(session.table().is_empty());
        let solo = solo_with_legacy_hand();
        let session = HostSession::host_from_with(
            "rae",
            &solo,
            TableConfig::default(),
            Box::new(NativeEngine::new()),
            None,
        )
        .unwrap();
        assert_eq!(session.table(), solo);
    }

    #[test]
    fn a_four_seat_table_hosted_with_untouched_options_still_plays_three_battlefields() {
        let choice = TableChoice {
            game: TableGame::Riftbound,
            ..TableChoice::default()
        };
        assert!(choice.options.is_none());
        assert!(choice.config_options().is_none());
        let mut session = zoned_session(agni_riftbound::zone_table(), &Table::new());
        for name in ["ada", "bea", "cy"] {
            session.join(name).unwrap();
        }
        assert!(session.state().options.is_none());
        let mut info = SessionInfo {
            role: SessionRole::Host,
            options: session.state().options.as_ref().map(|bytes| bytes.to_vec()),
            roster: session.roster(),
            ..SessionInfo::default()
        };
        assert_eq!(info.roster.len(), 4);
        assert_eq!(
            info.options_in_play(info.roster.len()),
            agni_riftbound::TableOptions {
                victory_score: 8,
                battlefields: 3
            }
        );
        assert_eq!(info.battlefields_in_play(4), 3);
        assert!(new_game_hint(&info).starts_with("4 players — FFA4 (War) ·"));
        let placed: usize = (0..4u8)
            .map(|seat| {
                deal_setup(&info, &MySeat(PlayerId(seat)), &DealGeneration(1))
                    .expect("an active table deals")
                    .placed()
            })
            .sum();
        assert_eq!(
            placed, 3,
            "one battlefield per seat but the first player, as War plays"
        );
        info.roster.truncate(2);
        assert_eq!(info.battlefields_in_play(2), 2);
        assert!(new_game_hint(&info).starts_with("2 players — 1v1 (Duel) ·"));
        let chosen = TableChoice {
            game: TableGame::Riftbound,
            options: Some(agni_riftbound::TableOptions {
                victory_score: 2,
                battlefields: 3,
            }),
            enforced: false,
        };
        let bytes = chosen.config_options().expect("chosen options are written");
        info.options = Some(bytes.to_vec());
        assert_eq!(info.battlefields_in_play(2), 3);
        assert!(new_game_hint(&info)
            .starts_with("2 player(s) — house rules · first to 2 · 3 battlefields"));
        let placed: usize = (0..2u8)
            .map(|seat| {
                deal_setup(&info, &MySeat(PlayerId(seat)), &DealGeneration(1))
                    .expect("an active table deals")
                    .placed()
            })
            .sum();
        assert_eq!(placed, 3, "a duel on three battlefields places all three");
        let mtg = TableChoice {
            game: TableGame::Mtg,
            options: Some(agni_riftbound::TableOptions::default()),
            enforced: true,
        };
        assert!(mtg.config_options().is_none());
    }

    #[test]
    fn the_enforced_toggle_is_written_at_genesis_and_read_back_by_both_seats() {
        let choice = TableChoice {
            game: TableGame::Riftbound,
            options: None,
            enforced: true,
        };
        let bytes = choice.config_options().expect("the flag alone is written");
        let solo = SessionInfo::default();
        assert!(!rules_enforced(&solo, &TableChoice::default()));
        assert!(rules_enforced(&solo, &choice));
        assert!(!rules_enforced(
            &solo,
            &TableChoice {
                game: TableGame::Mtg,
                ..choice
            }
        ));
        let mut session = zoned_session(agni_riftbound::zone_table(), &Table::new());
        session.join("ada").unwrap();
        let host = SessionInfo {
            role: SessionRole::Host,
            options: Some(bytes.to_vec()),
            roster: session.roster(),
            ..SessionInfo::default()
        };
        assert!(rules_enforced(&host, &TableChoice::default()));
        assert_eq!(
            host.options_in_play(2),
            agni_riftbound::TableOptions::default(),
            "the flag alone leaves the score and battlefields by seat count"
        );
        let joiner = SessionInfo {
            role: SessionRole::Client,
            options: Some(bytes.to_vec()),
            ..SessionInfo::default()
        };
        assert!(rules_enforced(&joiner, &TableChoice::default()));
        let free = SessionInfo {
            role: SessionRole::Client,
            options: None,
            ..SessionInfo::default()
        };
        assert!(
            !rules_enforced(&free, &choice),
            "a joiner reads the table, not its own toggle"
        );
        let with_score = TableChoice {
            options: Some(agni_riftbound::TableOptions {
                victory_score: 5,
                battlefields: 3,
            }),
            ..choice
        };
        let both = with_score.config_options().expect("written");
        let info = SessionInfo {
            role: SessionRole::Host,
            options: Some(both.to_vec()),
            ..SessionInfo::default()
        };
        assert!(rules_enforced(&info, &TableChoice::default()));
        assert_eq!(info.battlefields_in_play(2), 3);
        assert_eq!(info.options_in_play(2).victory_score, 5);
    }

    #[test]
    fn a_joiner_is_seated_without_any_cards_being_dealt() {
        let mut session = HostSession::host_from_with(
            "rae",
            &Table::new(),
            TableConfig::default(),
            Box::new(NativeEngine::new()),
            None,
        )
        .unwrap();
        session.join("ada").unwrap();
        assert!(session.table().is_empty());
        assert_eq!(session.roster().len(), 2);
    }

    #[test]
    fn every_build_hosts_and_only_the_browser_is_warned_about_hidden_tabs() {
        assert_eq!(host_block(&TableChoice::default()), None);
        assert_eq!(
            host_block(&TableChoice {
                game: TableGame::Riftbound,
                options: None,
                enforced: true,
            }),
            None
        );
        assert_eq!(hosting_warning(), None);
        assert!(HIDDEN_TAB_WARNING.starts_with("keep this tab in front"));
    }

    #[test]
    fn a_refused_host_keeps_its_reason_when_the_table_close_drains() {
        let mut info = SessionInfo::default();
        let mut host = HostState::default();
        refuse_hosting(&mut info, "hosting refused — no engine".into());
        assert_eq!(info.role, SessionRole::Solo);
        assert!(bridge::drain_events()
            .iter()
            .any(|event| matches!(event, NetToGame::HostClosed)));
        host_closed(&mut info, &mut host);
        assert_eq!(info.status, "hosting refused — no engine");
        info.role = SessionRole::Host;
        info.status = "hosting — table visible to your peers".into();
        host_closed(&mut info, &mut host);
        assert_eq!(info.role, SessionRole::Solo);
        assert_eq!(info.status, "table closed");
    }

    #[test]
    fn the_zone_table_names_the_game_a_joiner_landed_in() {
        assert_eq!(game_of_zones(&[]), TableGame::FreeForm);
        assert_eq!(game_of_zones(&agni_mtg::zone_table()), TableGame::Mtg);
        assert_eq!(
            game_of_zones(&agni_riftbound::zone_table()),
            TableGame::Riftbound
        );
    }

    #[test]
    fn each_game_names_its_plugin_and_art_source() {
        assert_eq!(TableGame::FreeForm.plugin_ref(), None);
        assert!(TableGame::FreeForm.compiled_zones().is_none());
        assert!(TableGame::FreeForm.art_game().is_none());
        assert_eq!(TableGame::FreeForm.id(), None);
        assert_eq!(TableGame::Mtg.plugin_ref(), Some("mtg"));
        assert_eq!(TableGame::Mtg.id(), Some("mtg"));
        assert_eq!(TableGame::Riftbound.plugin_ref(), Some("riftbound"));
        for game in [TableGame::Mtg, TableGame::Riftbound] {
            let zones = game.compiled_zones().unwrap();
            assert_eq!(game_of_zones(&zones), game);
            assert!(zones.iter().any(|decl| decl.kind == ZoneKind::Deck));
        }
    }

    #[test]
    fn a_deck_deals_once_per_seat_on_either_games_table() {
        for game in [TableGame::Mtg, TableGame::Riftbound] {
            let zones = game.compiled_zones().unwrap();
            let mut session = zoned_session(zones.clone(), &Table::new());
            assert!(!deck_already_dealt(&session, 0));
            let deck = zones
                .iter()
                .find(|decl| decl.kind == ZoneKind::Deck)
                .unwrap();
            session
                .deal_to(0, vec![CardFace::named("top")], Zone::Plugin(deck.id))
                .unwrap();
            assert!(deck_already_dealt(&session, 0));
            assert!(!deck_already_dealt(&session, 1));
        }
        let free_form = HostSession::host_from_with(
            "rae",
            &Table::new(),
            TableConfig::default(),
            Box::new(NativeEngine::new()),
            None,
        )
        .unwrap();
        assert!(!deck_already_dealt(&free_form, 0));
    }

    struct ScriptedPlugin {
        next: std::sync::Arc<parking_lot::Mutex<Vec<agni_sim::log::Effect>>>,
    }

    impl agni_sim::engine::PluginModule for ScriptedPlugin {
        fn decide(&mut self, _request: &[u8]) -> Result<agni_sim::log::Verdict, EngineFault> {
            let effects = std::mem::take(&mut *self.next.lock());
            Ok(agni_sim::log::Verdict::accept().with_effects(effects))
        }
    }

    #[test]
    fn a_host_action_that_peeks_or_draws_for_the_joiner_owes_that_seat_its_faces_at_once() {
        use agni_sim::log::Effect;
        let next = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mut session = HostSession::with_engine(
            "rae",
            TableConfig {
                engine: None,
                plugin: None,
                zones: agni_riftbound::zone_table(),
                options: None,
                counters: Vec::new(),
                despawn_any: false,
            },
            Box::new(NativeEngine::new()),
            Some(Box::new(ScriptedPlugin { next: next.clone() })),
        )
        .unwrap();
        let (ada, _) = session.join("ada").unwrap();
        let deck = Zone::Plugin(agni_riftbound::ZONE_MAIN_DECK);
        session
            .deal_to(
                ada,
                vec![CardFace::named("Wisp"), CardFace::named("Sprite")],
                deck,
            )
            .unwrap();
        let dealt: Vec<u32> = session
            .state()
            .table
            .in_area(PlayerId(ada), deck)
            .map(|card| card.id.0)
            .collect();
        let (top, second) = (dealt[1], dealt[0]);
        let mut conns = Conns::default();
        conns.seats.insert(7, ada);
        assert!(
            conns.private_faces(&mut session).is_empty(),
            "a face-down deck owes nobody a face"
        );
        *next.lock() = vec![Effect::Peek {
            card: top,
            seat: ada,
        }];
        let entries = session
            .intent(
                0,
                WireIntent::Game {
                    data: ByteBuf::from(vec![0]),
                },
            )
            .unwrap();
        assert_eq!(entries.len(), 1, "the host's own action appends one entry");
        let owed = conns.private_faces(&mut session);
        assert!(
            matches!(
                owed.as_slice(),
                [(7, HostMsg::Faces { faces })] if faces == &[session.face_of(top).unwrap()]
            ),
            "the peeked face goes to the joiner in the same frame: {owed:?}"
        );
        assert!(conns.private_faces(&mut session).is_empty(), "sent once");
        *next.lock() = vec![Effect::Move {
            card: second,
            to: Zone::Plugin(agni_riftbound::ZONE_HAND),
            seat: ada,
            index: 0,
        }];
        session
            .intent(
                0,
                WireIntent::Game {
                    data: ByteBuf::from(vec![1]),
                },
            )
            .unwrap();
        let owed = conns.private_faces(&mut session);
        assert!(
            matches!(
                owed.as_slice(),
                [(7, HostMsg::Faces { faces })] if faces == &[session.face_of(second).unwrap()]
            ),
            "a draw inside the host's action owes the drawn face too: {owed:?}"
        );
    }

    #[test]
    fn a_refusal_updates_the_status_and_an_engine_fault_ends_the_session() {
        let mut info = SessionInfo {
            role: SessionRole::Host,
            ..Default::default()
        };
        session_failed(
            &mut info,
            "your move",
            &SessionError::Refused(agni_sim::log::FoldError::NoOp),
        );
        assert_eq!(info.role, SessionRole::Host);
        assert!(info.status.contains("changes nothing"));
        let reasoned = SessionError::Refused(agni_sim::log::FoldError::Rejected {
            reason: Some("the chain resolves itself".into()),
        });
        session_failed(&mut info, "your move", &reasoned);
        assert_eq!(info.status, "your move refused: the chain resolves itself");
        assert!(info.notices.is_empty());
        my_intent_failed(&mut info, "your move", &reasoned);
        assert_eq!(info.notices, ["the chain resolves itself"]);
        assert_eq!(
            refusal_notice(&reasoned).as_deref(),
            Some("refused: the chain resolves itself")
        );
        assert_eq!(
            notice_text(&refusal_notice(&reasoned).unwrap()),
            "the chain resolves itself"
        );
        assert_eq!(
            notice_text("your deck is already on the table"),
            "your deck is already on the table"
        );
        info.notices.clear();
        my_intent_failed(
            &mut info,
            "your move",
            &SessionError::Engine(EngineFault("trapped".into())),
        );
        assert!(info.notices.is_empty(), "a frozen table is not a toast");
        info.role = SessionRole::Host;
        assert_eq!(
            refusal_notice(&SessionError::Engine(EngineFault("trapped".into()))),
            None
        );
        session_failed(
            &mut info,
            "your move",
            &SessionError::Engine(EngineFault("trapped".into())),
        );
        assert_eq!(info.role, SessionRole::Ended);
        assert!(info.status.contains("frozen"));
    }

    struct PinnedEngine {
        inner: NativeEngine,
        hash: [u8; 32],
    }

    impl agni_sim::engine::Engine for PinnedEngine {
        fn fold_entry(
            &mut self,
            entry: &LogEntry,
            verdict: Option<agni_sim::log::Verdict>,
            mode: agni_sim::engine::FoldMode,
            viewer: u8,
        ) -> Result<agni_sim::engine::FoldOutcome, EngineFault> {
            self.inner.fold_entry(entry, verdict, mode, viewer)
        }

        fn fold_log(
            &mut self,
            entries: &[LogEntry],
            viewer: u8,
        ) -> Result<agni_sim::engine::FoldLogOutcome, EngineFault> {
            self.inner.fold_log(entries, viewer)
        }

        fn decide_request(&mut self, entry: &LogEntry) -> Result<Option<Vec<u8>>, EngineFault> {
            self.inner.decide_request(entry)
        }

        fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault> {
            self.inner.snapshot()
        }

        fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault> {
            self.inner.restore(bytes)
        }

        fn view(&mut self, viewer: u8) -> Result<TableView, EngineFault> {
            self.inner.view(viewer)
        }

        fn engine_hash(&self) -> Option<[u8; 32]> {
            Some(self.hash)
        }
    }

    #[test]
    fn a_seat_deals_its_body_first_and_its_battlefield_as_a_second_deal() {
        use crate::deck::battlefield;
        use crate::deck::import::{
            battlefield_only, battlefield_plan, body_plan, DealSetup, ImportedDeck,
            SeatedDeckRecord,
        };
        let mut host = HostSession::with_engine(
            "rae",
            TableConfig {
                engine: None,
                plugin: None,
                zones: agni_riftbound::zone_table(),
                options: None,
                counters: Vec::new(),
                despawn_any: false,
            },
            Box::new(NativeEngine::new()),
            None,
        )
        .unwrap();
        let (ada, _) = host.join("ada").unwrap();
        let record = |seat: u8| SeatedDeckRecord {
            seat: PlayerId(seat),
            deck: ImportedDeck::Riftbound(crate::deck::pool::deck("lillia-house").unwrap()),
            faces: Default::default(),
            battlefield: Some(1),
            battlefield_played: false,
        };
        let setup = |seat: u8| {
            Some(DealSetup {
                players: 2,
                seat,
                first_player: 0,
                generation: 0,
                battlefields: 2,
            })
        };
        for seat in [0, ada] {
            let mine = record(seat);
            let body = body_plan(&mine, setup(seat));
            assert!(!battlefield_only(&body));
            assert!(deal_refusal(&host, seat, false).is_none());
            host.deal_groups(seat, body).unwrap();
            assert!(
                battlefield::legend_on_table(&host.state().table, PlayerId(seat)),
                "seat {seat}'s legend is out before any battlefield"
            );
            assert!(!battlefield::placed_on_table(
                &host.state().table,
                &host.state().zones,
                PlayerId(seat)
            ));
            assert!(battlefield::placement_pending(
                &mine,
                &host.state().table,
                &host.state().zones,
                PlayerId(seat)
            ));
            assert_eq!(
                deal_refusal(&host, seat, false),
                Some("your deck is already on the table")
            );
            let placement = battlefield_plan(&mine, setup(seat));
            assert!(battlefield_only(&placement), "{placement:?}");
            assert!(deal_refusal(&host, seat, true).is_none());
            host.deal_groups(seat, placement).unwrap();
            assert!(
                battlefield::placed_on_table(
                    &host.state().table,
                    &host.state().zones,
                    PlayerId(seat)
                ),
                "seat {seat}'s battlefield is on the band"
            );
            assert!(!battlefield::placement_pending(
                &mine,
                &host.state().table,
                &host.state().zones,
                PlayerId(seat)
            ));
            assert_eq!(
                deal_refusal(&host, seat, true),
                Some("your battlefield is already on the table")
            );
        }
        let bands = battlefield::battlefield_zones(&host.state().zones);
        let placed: Vec<(u8, Zone)> = host
            .state()
            .table
            .cards()
            .iter()
            .filter(|card| bands.contains(&card.zone))
            .map(|card| (card.seat.0, card.zone))
            .collect();
        assert_eq!(placed.len(), 2, "{placed:?}");
        assert_ne!(placed[0].1, placed[1].1, "two seats, two battlefields");
    }

    #[test]
    fn entries_and_faces_sent_while_the_modules_cross_fold_once_the_joiner_seats() {
        use agni_net::session::{module_chunks, module_hash};
        let engine_bytes: Vec<u8> = (0..(agni_net::session::MODULE_CHUNK_BYTES + 3))
            .map(|i| (i % 253) as u8)
            .collect();
        let engine_hash = module_hash(&engine_bytes);
        let mut host = HostSession::with_engine(
            "rae",
            TableConfig {
                engine: None,
                plugin: None,
                zones: agni_riftbound::zone_table(),
                options: None,
                counters: Vec::new(),
                despawn_any: false,
            },
            Box::new(PinnedEngine {
                inner: NativeEngine::new(),
                hash: engine_hash,
            }),
            None,
        )
        .unwrap();
        assert_eq!(host.serve_module(engine_bytes.clone()), Some(engine_hash));
        let (ada, _) = host.join("ada").unwrap();
        let mut pending = PendingWelcome::new(ada, host.roster(), host.log().to_vec());
        let welcome_seq = host.state().next_seq;

        let deck = Zone::Plugin(agni_riftbound::ZONE_MAIN_DECK);
        let (dealt_to_host, _) = host
            .deal_to(0, vec![CardFace::named("Wisp")], deck)
            .unwrap();
        let (dealt_to_ada, owed) = host
            .deal_to(ada, vec![CardFace::named("Sprite")], Zone::Hand)
            .unwrap();
        assert!(!owed.is_empty(), "a hand deal owes its owner the face");
        let host_seq = host.state().next_seq;
        assert_eq!(host_seq, welcome_seq + 2);

        let chunks = module_chunks(engine_hash, &engine_bytes);
        assert_eq!(chunks.len(), 2);
        let mut fetches = super::super::engine::modules::fetch::Fetches::new();
        assert_eq!(
            fetches.poll(engine_hash, false).1,
            Some(super::super::engine::modules::fetch::Ask::Host)
        );
        assert_eq!(fetches.host_frame(&chunks[0]), None);
        assert_eq!(
            pending.absorb(HostMsg::Entry {
                entry: dealt_to_host
            }),
            None
        );
        assert_eq!(pending.absorb(HostMsg::Faces { faces: owed }), None);
        assert_eq!(
            pending.absorb(HostMsg::Roster {
                roster: host.roster()
            }),
            None
        );
        assert_eq!(
            pending.absorb(HostMsg::Entry {
                entry: dealt_to_ada
            }),
            None
        );
        assert!(matches!(
            pending.absorb(HostMsg::End {
                reason: "not mine to swallow".into()
            }),
            Some(HostMsg::End { .. })
        ));
        assert_eq!(pending.queued(), 3);
        let landed = fetches.host_frame(&chunks[1]).expect("the engine lands");
        assert_eq!(landed.1, engine_bytes);
        assert!(
            pending.log().len() as u64 == welcome_seq,
            "the welcome log itself is untouched"
        );

        let (session, roster) = pending
            .seat_replica(
                Box::new(PinnedEngine {
                    inner: NativeEngine::new(),
                    hash: engine_hash,
                }),
                None,
            )
            .unwrap();
        assert_eq!(session.next_seq(), host_seq);
        assert_eq!(roster, host.roster());
        let hand: Vec<String> = session
            .table()
            .in_area(PlayerId(ada), Zone::Hand)
            .map(|card| card.face.name.clone())
            .collect();
        assert_eq!(
            hand,
            vec!["Sprite".to_string()],
            "the queued face was applied"
        );
        assert_eq!(session.table().in_area(PlayerId(0), deck).count(), 1);
    }

    #[test]
    fn a_connection_is_served_each_pinned_module_once_and_only_while_seated() {
        use agni_net::session::module_hash;
        let engine_bytes = b"the dev engine".to_vec();
        let engine_hash = module_hash(&engine_bytes);
        let mut session = HostSession::with_engine(
            "rae",
            TableConfig::default(),
            Box::new(PinnedEngine {
                inner: NativeEngine::new(),
                hash: engine_hash,
            }),
            None,
        )
        .unwrap();
        assert_eq!(session.serve_module(engine_bytes), Some(engine_hash));
        let (ada, _) = session.join("ada").unwrap();
        let mut conns = Conns::default();
        assert!(
            conns.module_frames(&session, 7, engine_hash).is_empty(),
            "an unseated connection is not served"
        );
        conns.seats.insert(7, ada);
        let frames = conns.module_frames(&session, 7, engine_hash);
        assert!(matches!(frames.as_slice(), [HostMsg::Module { .. }]));
        assert!(
            conns.module_frames(&session, 7, engine_hash).is_empty(),
            "a repeat ask on the same connection is ignored"
        );
        let stranger = [0xaa; 32];
        assert!(matches!(
            conns.module_frames(&session, 7, stranger).as_slice(),
            [HostMsg::NoModule { .. }]
        ));
        assert!(conns.module_frames(&session, 7, stranger).is_empty());
        conns.seats.insert(9, 2);
        assert!(matches!(
            conns.module_frames(&session, 9, engine_hash).as_slice(),
            [HostMsg::Module { .. }]
        ));
        assert_eq!(conns.forget(7), Some(ada));
        conns.seats.insert(7, ada);
        assert!(
            matches!(
                conns.module_frames(&session, 7, engine_hash).as_slice(),
                [HostMsg::Module { .. }]
            ),
            "a fresh connection under the same id starts over"
        );
        conns.clear();
        assert!(conns.asked.is_empty());
    }
}

pub fn resume_rejoin(role: SessionRole, recovery: &Recovery, resumed: bool) -> Option<String> {
    if !resumed || role != SessionRole::Ended {
        return None;
    }
    match recovery {
        Recovery::Rejoin { host } => Some(host.clone()),
        _ => None,
    }
}

pub fn rejoin_on_resume(
    mut lifecycle: MessageReader<bevy::window::AppLifecycle>,
    mut info: ResMut<SessionInfo>,
) {
    let resumed = lifecycle
        .read()
        .any(|event| matches!(event, bevy::window::AppLifecycle::WillResume));
    if let Some(host) = resume_rejoin(info.role, &info.recovery, resumed) {
        info.status = "rejoining the table…".into();
        bridge::request_join(host);
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    #[test]
    fn a_resume_after_a_dropped_join_rejoins_the_same_host() {
        let rejoin = Recovery::Rejoin { host: "abc".into() };
        assert_eq!(
            resume_rejoin(SessionRole::Ended, &rejoin, true),
            Some("abc".to_string())
        );
        assert_eq!(resume_rejoin(SessionRole::Ended, &rejoin, false), None);
        assert_eq!(resume_rejoin(SessionRole::Client, &rejoin, true), None);
        assert_eq!(
            resume_rejoin(SessionRole::Ended, &Recovery::Rehost, true),
            None
        );
        assert_eq!(
            resume_rejoin(SessionRole::Ended, &Recovery::Nothing, true),
            None
        );
    }
}
