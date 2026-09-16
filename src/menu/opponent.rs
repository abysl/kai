use super::lobby::{legend_id, thumbnail};
use super::{chip, segmented, Menu, TOUCH};
use super::{DeckSeat, Sheet};
use crate::deck::thumbs::Thumbs;
use crate::deck::{history, import, pool};
use crate::net::{self, identity, TableGame};
use crate::settings::{DeckParams, NetParams, TableParams};

use crate::table::{colors, MySeat, Recovery, SessionRole};
use crate::theme;
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    Ai,
    Friends,
    Join,
    Match,
}

impl Segment {
    pub fn label(self) -> &'static str {
        match self {
            Segment::Ai => "AI",
            Segment::Friends => "friends",
            Segment::Join => "join",
            Segment::Match => "find game",
        }
    }
}

pub fn segments() -> &'static [Segment] {
    &[Segment::Ai, Segment::Friends, Segment::Join, Segment::Match]
}

pub fn default_segment() -> Segment {
    segments()[0]
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AiChoice {
    #[default]
    Auto,
    Seated,
    Pool(String),
    Saved(spirit_sdk::CiHash),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinTarget {
    pub host: String,
    pub name: String,
}

#[derive(Resource, Default)]
pub struct Opponent {
    pub segment: Option<Segment>,
    pub ai: AiChoice,
    pub ai_battlefield: usize,
    pub pending_ai: bool,
    pub cached: Option<(AiChoice, import::ImportedDeck)>,
    pub ticket: String,
    pub selected: Option<JoinTarget>,
    pub status: String,
    pub invite_open: bool,
}

impl Opponent {
    pub fn segment(&self) -> Segment {
        self.segment.unwrap_or_else(default_segment)
    }

    pub fn join_target(&self) -> Option<JoinTarget> {
        self.selected.clone()
    }

    pub fn ai_deck(&mut self, seated: &import::SeatedDeck) -> Option<import::ImportedDeck> {
        match &self.ai {
            AiChoice::Auto => None,
            AiChoice::Seated => seated.0.as_ref().map(|record| record.deck.clone()),
            choice => {
                if let Some((cached, deck)) = &self.cached {
                    if cached == choice {
                        return Some(deck.clone());
                    }
                }
                let deck = match choice {
                    AiChoice::Pool(slug) => {
                        pool::deck(slug).ok().map(import::ImportedDeck::Riftbound)
                    }
                    AiChoice::Saved(ci) => history::store::recall(agni_riftbound::GAME, *ci),
                    AiChoice::Auto | AiChoice::Seated => None,
                }?;
                self.cached = Some((choice.clone(), deck.clone()));
                Some(deck)
            }
        }
    }

    pub fn ai_label(&mut self, seated: &import::SeatedDeck) -> String {
        match self.ai_deck(seated) {
            Some(deck) => history::label(&deck),
            None => match self.ai {
                AiChoice::Auto => "let the AI pick".into(),
                _ => "that deck is not available here".into(),
            },
        }
    }
}

pub fn ai_battlefield_name(deck: &import::ImportedDeck, index: usize) -> Option<String> {
    match deck {
        import::ImportedDeck::Riftbound(deck) => deck
            .battlefields
            .get(index)
            .map(|entry| entry.card.name.clone()),
        import::ImportedDeck::Mtg(_) => None,
    }
}

pub fn legends_by_seat(table: &agni_core::Table) -> BTreeMap<u8, String> {
    let zone = agni_core::Zone::Plugin(agni_riftbound::ZONE_LEGEND);
    table
        .cards()
        .iter()
        .filter(|card| card.zone == zone)
        .map(|card| (card.seat.0, card.face.name.clone()))
        .collect()
}

pub fn seat_tags(seat: &agni_net::session::SeatInfo, me: u8) -> String {
    let mut tags = Vec::new();
    if seat.seat == me {
        tags.push("you");
    }
    if seat.host {
        tags.push("host");
    }
    if !seat.connected {
        tags.push("gone");
    }
    tags.iter()
        .map(|tag| format!("({tag})"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn recovery_card(recovery: &Recovery) -> Option<(String, &'static str)> {
    match recovery {
        Recovery::Nothing => None,
        Recovery::Rejoin { host } => Some((
            format!("reconnect to {}", identity::short_id(host)),
            "the table you were at may still be open",
        )),
        Recovery::Rehost => Some((
            "re-host this table".into(),
            "keeps the log and the seats — players reconnect into the seats they had",
        )),
    }
}

pub fn ai_alive() -> bool {
    crate::ai::seat::status().is_some_and(|status| status.alive)
}

pub fn free_brain_deck(seated: &import::SeatedDeck) -> Option<(import::ImportedDeck, String)> {
    if let Some(record) = seated
        .0
        .as_ref()
        .filter(|record| matches!(record.deck, import::ImportedDeck::Mtg(_)))
    {
        return Some((record.deck.clone(), history::label(&record.deck)));
    }
    let mine = match seated.0.as_ref().map(|record| &record.deck) {
        Some(import::ImportedDeck::Riftbound(deck)) => deck
            .legend
            .as_ref()
            .map(|card| card.name.clone())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let pick = pool::another_than(&mine).or_else(|| pool::pinnable().into_iter().next())?;
    let deck = pool::deck(&pick.slug).ok()?;
    Some((import::ImportedDeck::Riftbound(deck), pick.label))
}

pub fn start_ai(
    opponent: &mut Opponent,
    seated: &import::SeatedDeck,
    lobby: &crate::ai::seat::AiLobby,
) -> String {
    let mut deck = opponent.ai_deck(seated);
    if deck.is_none() && opponent.ai != AiChoice::Auto {
        return "that deck is not available here — pick another for the AI".into();
    }
    let mut label = deck.as_ref().map(history::label);
    if deck.is_none() && (!lobby.kind.is_llm() || cfg!(target_arch = "wasm32")) {
        match free_brain_deck(seated) {
            Some((picked, picked_label)) => {
                deck = Some(picked);
                label = Some(picked_label);
            }
            None => return "no pool deck is available for the free brain".into(),
        }
    }
    let battlefield = opponent.ai_battlefield + 1;
    let chosen = deck.as_ref().zip(label.as_deref());
    match crate::ai::seat::start(chosen, battlefield, lobby) {
        Ok(()) => format!(
            "AI joining with {}…",
            label.unwrap_or_else(|| "a deck of its choosing".into())
        ),
        Err(error) => error,
    }
}

pub fn start_pending_ai(
    mut opponent: ResMut<Opponent>,
    info: Res<crate::table::SessionInfo>,
    seated: Res<import::SeatedDeck>,
    mut lobby: ResMut<crate::ai::seat::AiLobby>,
) {
    if !opponent.pending_ai {
        return;
    }
    match info.role {
        SessionRole::Starting => return,
        SessionRole::Host => {}
        _ => {
            opponent.pending_ai = false;
            return;
        }
    }
    opponent.pending_ai = false;
    lobby.status = start_ai(&mut opponent, &seated, &lobby);
}

#[allow(clippy::too_many_arguments)]
pub fn opponent_group(
    ui: &mut egui::Ui,
    game: TableGame,
    menu: &mut Menu,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) {
    ui.label(
        egui::RichText::new("opponent")
            .size(18.0)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
    ui.add_space(4.0);
    let options: Vec<(Segment, &str)> = segments()
        .iter()
        .map(|segment| (*segment, segment.label()))
        .collect();
    let mut segment = net.opponent.segment();
    ui.add_enabled_ui(!net.matchmaking.active(), |ui| {
        if segmented(ui, &mut segment, &options) {
            net.opponent.segment = Some(segment);
        }
    });
    ui.add_space(4.0);
    crate::os::profile::name_field(ui, &mut net.name, 180.0);
    ui.add_space(8.0);
    match segment {
        Segment::Ai => ai_segment(ui, game, menu, net, decks, thumbs),
        Segment::Friends => friends_segment(ui, menu, my_seat, table, net, decks),
        Segment::Join => join_segment(ui, net),
        Segment::Match => {
            ui.label(
                "Find one other player searching with the same game, rules and table settings.",
            );
            ui.label(if game == TableGame::FreeForm {
                "Keep this screen open while searching. You can add cards once you meet your opponent."
            } else {
                "Choose your deck before searching. Keep this screen open; your deck list is not advertised."
            });
            if !net.matchmaking.status.is_empty() {
                ui.add_space(8.0);
                ui.label(&net.matchmaking.status);
                if net.matchmaking.active() {
                    ui.spinner();
                }
            }
            if let Some(warning) = net::hosting_warning() {
                ui.small(warning);
            }
        }
    }
}

fn ai_segment(
    ui: &mut egui::Ui,
    game: TableGame,
    menu: &mut Menu,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) {
    let label = net.opponent.ai_label(&decks.seated);
    let deck = net.opponent.ai_deck(&decks.seated);
    let texture = deck
        .as_ref()
        .and_then(legend_id)
        .and_then(|id| thumbs.id(id));
    let mut change = false;
    ui.horizontal_top(|ui| {
        thumbnail(ui, texture);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width());
            ui.label(
                egui::RichText::new(&label)
                    .strong()
                    .color(theme::tokens(ui.ctx()).ink),
            );
            let _ = game;
            change |= ui.small_button(super::lobby::SWITCH_LINK).clicked();
        });
    });
    if change {
        menu.open_sheet(Sheet::DeckBox(DeckSeat::Ai));
    }
    ui.add_space(4.0);
    ui.label(if net.ai.configured {
        format!(
            "{} · {}",
            net.ai.credentials.provider.label(),
            net.ai.brain_label()
        )
    } else {
        "Set up an AI provider, key and model before playing".into()
    });
    if chip(ui, "AI settings", false).clicked() {
        super::ai_setup::open(menu, &mut net.ai, false);
    }

    if let Some(status) = crate::ai::seat::status() {
        if status.alive {
            ui.horizontal_wrapped(|ui| {
                ui.label(status.seat_line());
                if ui.small_button("stop AI").clicked() {
                    crate::ai::seat::stop();
                    net.ai.status = "AI player stopped".into();
                }
            });
            if let Some(fault) = status.fault_line() {
                ui.label(
                    egui::RichText::new(fault)
                        .color(theme::tokens(ui.ctx()).ink_weak)
                        .small(),
                );
            }
        } else {
            net.ai.status = format!("AI player exited ({})", status.exit.unwrap_or_default());
            crate::ai::seat::stop();
        }
    } else if net.info.role == SessionRole::Host
        && !net.opponent.pending_ai
        && chip(ui, "add AI to this table", false).clicked()
    {
        super::ai_setup::open(menu, &mut net.ai, true);
    }
    if net.opponent.pending_ai {
        ui.label(
            egui::RichText::new("the AI joins as soon as the table opens")
                .color(theme::tokens(ui.ctx()).ink_weak),
        );
    }
    if !net.ai.status.is_empty() {
        ui.label(
            egui::RichText::new(&net.ai.status)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
}

fn friends_segment(
    ui: &mut egui::Ui,
    menu: &mut Menu,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
) {
    if net.info.role == SessionRole::Solo || net.info.role == SessionRole::Ended {
        if let Some((verb, note)) = recovery_card(&net.info.recovery) {
            egui::Frame::new()
                .fill(theme::tokens(ui.ctx()).amber.gamma_multiply(0.2))
                .stroke(egui::Stroke::new(1.0, theme::tokens(ui.ctx()).amber))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if ui
                        .add(egui::Button::new(&verb).min_size(egui::vec2(0.0, TOUCH)))
                        .clicked()
                    {
                        match net.info.recovery.clone() {
                            Recovery::Rejoin { host } => net::rejoin(&host),
                            Recovery::Rehost => net::rehost(&mut net.info),
                            Recovery::Nothing => {}
                        }
                    }
                    ui.label(
                        egui::RichText::new(note)
                            .color(theme::tokens(ui.ctx()).ink_weak)
                            .small(),
                    );
                });
            ui.add_space(8.0);
        }
    }
    let live = matches!(
        net.info.role,
        SessionRole::Host | SessionRole::Client | SessionRole::Ended
    );
    let legends = legends_by_seat(&decks.game_table.0);
    let mine = decks
        .seated
        .0
        .as_ref()
        .map(|record| history::label(&record.deck));
    if live {
        for seat in net.info.roster.clone() {
            let (name, color, is_me) = colors::seat_label(
                &net.info.roster,
                &table.seat_colors,
                my_seat.0,
                agni_core::PlayerId(seat.seat),
            );
            let deck = if is_me {
                mine.clone()
            } else {
                legends.get(&seat.seat).cloned()
            };
            seat_row(
                ui,
                &name,
                color,
                table.tuning.colour_blind,
                deck.as_deref(),
                &seat_tags(&seat, my_seat.0 .0),
            );
        }
        let seats = net.info.roster.len();
        let open = net.info.options_in_play(seats.max(2)).battlefields.max(2) as usize;
        let _ = open;
    } else {
        let (name, color, _) = colors::seat_label(&[], &table.seat_colors, my_seat.0, my_seat.0);
        seat_row(
            ui,
            &name,
            color,
            table.tuning.colour_blind,
            mine.as_deref(),
            "(you)",
        );
    }
    ui.add_space(4.0);
    if net.info.role == SessionRole::Host || net.info.role == SessionRole::Solo {
        ui.horizontal_wrapped(|ui| {
            if chip(ui, "invite", net.opponent.invite_open).clicked() {
                net.opponent.invite_open = !net.opponent.invite_open;
                if net.opponent.invite_open {
                    if let Some(node) = net::node::get() {
                        net.opponent.status = match crate::os::clipboard::set_text(&node.ticket) {
                            Ok(()) => "ticket copied — share it or show the QR".into(),
                            Err(error) => format!("copy failed: {error}"),
                        };
                    }
                }
            }
            if !ai_alive() && !net.opponent.pending_ai && chip(ui, "add AI", false).clicked() {
                super::ai_setup::open(menu, &mut net.ai, true);
            }
        });
        if net.info.role == SessionRole::Solo {
            ui.label(
                egui::RichText::new("host a table, then friends join by ticket or from the mesh")
                    .color(theme::tokens(ui.ctx()).ink_weak)
                    .small(),
            );
        }
        if let Some(warning) = net::hosting_warning() {
            ui.label(
                egui::RichText::new(warning)
                    .color(theme::tokens(ui.ctx()).amber)
                    .small(),
            );
        }
        if net.opponent.invite_open {
            identity::identity_header(ui, &mut net.identity, &[]);
        }
    }
    if net.info.role == SessionRole::Ended {
        ui.label(
            egui::RichText::new("the session ended — the table is frozen")
                .color(theme::tokens(ui.ctx()).ink_weak),
        );
    }
    if !net.opponent.status.is_empty() {
        ui.label(
            egui::RichText::new(&net.opponent.status)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
}

fn seat_row(
    ui: &mut egui::Ui,
    name: &str,
    color: [u8; 3],
    colour_blind: bool,
    deck: Option<&str>,
    tags: &str,
) {
    ui.horizontal(|ui| {
        ui.set_min_height(TOUCH);
        colors::swatch(ui, color, 16.0, colour_blind);
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .strong()
                        .color(theme::tokens(ui.ctx()).ink),
                );
                if !tags.is_empty() {
                    ui.label(
                        egui::RichText::new(tags)
                            .color(theme::tokens(ui.ctx()).ink_weak)
                            .small(),
                    );
                }
            });
            ui.label(
                egui::RichText::new(deck.unwrap_or("no deck"))
                    .color(theme::tokens(ui.ctx()).ink_weak)
                    .small(),
            );
        });
    });
}

fn join_segment(ui: &mut egui::Ui, net: &mut NetParams) {
    let idle = net.info.role == SessionRole::Solo;
    let tables = net::open_tables();
    if tables.is_empty() {
        ui.label(
            egui::RichText::new("no tables yet — ask a friend to host, or paste their ticket")
                .color(theme::tokens(ui.ctx()).ink_weak),
        );
        if net.opponent.selected.is_some() {
            net.opponent.selected = None;
        }
    }
    for table in &tables {
        let selected = net
            .opponent
            .selected
            .as_ref()
            .is_some_and(|target| target.host == table.host);
        let line = format!(
            "{} · {} · {}",
            table.name,
            net.choice.game.label(),
            identity::short_id(&table.host)
        );
        let response = ui.add(
            egui::Button::selectable(selected, line)
                .min_size(egui::vec2(ui.available_width(), 64.0)),
        );
        if response.clicked() {
            net.opponent.selected = Some(JoinTarget {
                host: table.host.clone(),
                name: table.name.clone(),
            });
        }
    }
    if let Some(selected) = &net.opponent.selected {
        if !tables.iter().any(|table| table.host == selected.host) {
            net.opponent.selected = None;
        }
    }
    if net.opponent.selected.is_none() {
        if let Some(first) = tables.first() {
            net.opponent.selected = Some(JoinTarget {
                host: first.host.clone(),
                name: first.name.clone(),
            });
        }
    }
    ui.add_space(4.0);
    let mut clip_status = None;
    let mut join = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut net.opponent.ticket)
                .hint_text("paste their ticket")
                .desired_width((ui.available_width() - 140.0).max(80.0)),
        );
        clip_status = crate::os::clipboard::paste_button(
            ui,
            crate::os::clipboard::IDENTITY_TICKET,
            &mut net.opponent.ticket,
        );
        join = ui
            .add_enabled(
                idle && !net.opponent.ticket.trim().is_empty(),
                egui::Button::new("join"),
            )
            .clicked();
    });
    if let Some(status) = clip_status {
        net.opponent.status = status;
    }
    if join {
        let ticket = net.opponent.ticket.trim().to_string();
        net.opponent.status = match net::join_by_ticket(&ticket) {
            Ok(id) => {
                net.info.status = "joining…".into();
                net.opponent.ticket.clear();
                format!("joining {}…", identity::short_id(&id))
            }
            Err(error) => format!("that ticket did not take: {error}"),
        };
    }
    if !idle && !net.info.status.is_empty() {
        ui.label(
            egui::RichText::new(&net.info.status)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
    if !net.opponent.status.is_empty() {
        ui.label(
            egui::RichText::new(&net.opponent.status)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seat_tags_read_you_host_and_gone() {
        let seat = agni_net::session::SeatInfo {
            seat: 0,
            name: "rae".into(),
            host: true,
            connected: false,
            color: 0,
            playmat: None,
        };
        assert_eq!(seat_tags(&seat, 0), "(you) (host) (gone)");
        assert_eq!(seat_tags(&seat, 1), "(host) (gone)");
        let other = agni_net::session::SeatInfo {
            host: false,
            connected: true,
            ..seat
        };
        assert_eq!(seat_tags(&other, 1), "");
    }

    #[test]
    fn the_first_segment_is_the_ai_on_desktop_and_the_ai_deck_defaults_to_auto() {
        let opponent = Opponent::default();
        assert_eq!(opponent.segment(), Segment::Ai);
        assert_eq!(opponent.ai, AiChoice::Auto);
        assert_eq!(
            opponent.ai_battlefield, 0,
            "the first battlefield of its deck"
        );
        assert!(opponent.join_target().is_none());
        assert_eq!(segments().len(), 4);
    }

    #[test]
    fn the_ai_takes_any_pool_deck_and_its_first_battlefield() {
        let mut opponent = Opponent {
            ai: AiChoice::Pool("nasus-thundertrees".into()),
            ..Default::default()
        };
        let seated = import::SeatedDeck(None);
        assert_eq!(opponent.ai_label(&seated), "Nasus (ThunderTrees)");
        let deck = opponent.ai_deck(&seated).expect("the pool deck");
        assert!(ai_battlefield_name(&deck, 0).is_some());
        assert!(opponent.cached.is_some(), "the pool deck is resolved once");
        opponent.ai = AiChoice::Auto;
        assert_eq!(opponent.ai_label(&seated), "let the AI pick");
    }

    #[test]
    fn the_free_brain_takes_a_pool_deck_other_than_mine() {
        let (deck, label) = free_brain_deck(&import::SeatedDeck(None)).expect("a pool deck");
        assert!(label.starts_with("Lillia") || label.starts_with("Irelia"));
        let lillia = crate::ai::soak::pool_deck("lillia").unwrap();
        let seated = import::SeatedDeck(Some(import::SeatedDeckRecord {
            seat: agni_core::PlayerId(0),
            faces: import::face_map(&import::ImportedDeck::Riftbound(lillia.clone())),
            deck: import::ImportedDeck::Riftbound(lillia),
            battlefield: Some(0),
            battlefield_played: false,
        }));
        let (other, other_label) = free_brain_deck(&seated).unwrap();
        assert_ne!(history::label(&other), "Lillia (house)");
        assert!(other_label.starts_with("Irelia"), "{other_label}");
        assert!(ai_battlefield_name(&deck, 0).is_some());
    }

    #[test]
    fn the_recovery_card_only_appears_when_there_is_something_to_recover() {
        assert!(recovery_card(&Recovery::Nothing).is_none());
        let (verb, _) = recovery_card(&Recovery::Rehost).unwrap();
        assert_eq!(verb, "re-host this table");
        let (verb, _) = recovery_card(&Recovery::Rejoin {
            host: "abcdef0123456789".into(),
        })
        .unwrap();
        assert_eq!(verb, "reconnect to abcdef012345");
    }

    #[test]
    fn the_legends_on_the_table_are_read_per_seat() {
        let mut table = agni_core::Table::default();
        let legend = agni_core::Zone::Plugin(agni_riftbound::ZONE_LEGEND);
        table.add(
            agni_core::PlayerId(1),
            legend,
            "Irelia - Blade Dancer",
            [0; 3],
        );
        table.add(
            agni_core::PlayerId(0),
            agni_core::Zone::Hand,
            "Defy",
            [0; 3],
        );
        let legends = legends_by_seat(&table);
        assert_eq!(
            legends.get(&1).map(String::as_str),
            Some("Irelia - Blade Dancer")
        );
        assert!(!legends.contains_key(&0));
    }
}
