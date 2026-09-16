use super::opponent::{self, JoinTarget, Segment};
use super::{
    back_button, chip, content_width, decks, gear_button, link, margin, segmented, DeckSeat, Menu,
    Screen, Sheet, FOOTER_H, LOBBY_MAX_W, PRIMARY_H,
};
use crate::deck::thumbs::Thumbs;
use crate::deck::{battlefield, history, import, pool};
use crate::net::{self, TableGame};
use crate::settings::{DeckParams, NetParams, Settings, TableParams};

use crate::table::{dim, MySeat, SessionRole};
use crate::theme;
use crate::viewport::ViewportClass;
use bevy_egui::egui;

pub const THUMB_W: f32 = 56.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Deck,
    Rules { locked: bool, fixed: bool },
    Opponent,
}

pub fn lobby_sections(game: TableGame, enforced: bool, role: SessionRole) -> Vec<Section> {
    let _ = enforced;
    let fixed = role != SessionRole::Solo;
    match game {
        TableGame::FreeForm => vec![
            Section::Rules {
                locked: true,
                fixed,
            },
            Section::Opponent,
        ],
        TableGame::Mtg => vec![
            Section::Deck,
            Section::Rules {
                locked: true,
                fixed,
            },
            Section::Opponent,
        ],
        TableGame::Riftbound => vec![
            Section::Deck,
            Section::Rules {
                locked: false,
                fixed,
            },
            Section::Opponent,
        ],
    }
}

pub fn default_enforced(game: TableGame) -> bool {
    game == TableGame::Riftbound
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckState {
    NotNeeded,
    Missing,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyState {
    pub game: TableGame,
    pub role: SessionRole,
    pub segment: Segment,
    pub deck: DeckState,
    pub join_target: Option<JoinTarget>,
    pub host_block: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verb {
    pub label: String,
    pub enabled: bool,
    pub reason: Option<String>,
}

impl Verb {
    fn on(label: &str) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            reason: None,
        }
    }

    fn off(label: &str, reason: &str) -> Self {
        Self {
            label: label.into(),
            enabled: false,
            reason: Some(reason.into()),
        }
    }
}

pub fn primary_verb(state: &LobbyState) -> Verb {
    match state.role {
        SessionRole::Starting => return Verb::off("host table", "opening the table…"),
        SessionRole::Joining => return Verb::off("join", "joining…"),
        SessionRole::Host | SessionRole::Client | SessionRole::Ended => {
            return Verb::on("go to the table")
        }
        SessionRole::Solo => {}
    }
    let verb = match state.segment {
        Segment::Ai => "play vs AI",
        Segment::Friends => "host table",
        Segment::Join => "join",
        Segment::Match => "find game",
    };
    match state.deck {
        DeckState::Missing => return Verb::off(verb, "choose a deck first"),
        DeckState::NotNeeded | DeckState::Ready => {}
    }
    match state.segment {
        Segment::Ai | Segment::Friends | Segment::Match => match &state.host_block {
            Some(reason) => Verb::off(verb, reason),
            None => Verb::on(verb),
        },
        Segment::Join => match &state.join_target {
            Some(target) => Verb::on(&format!("join {}", target.name)),
            None => Verb::off("join", "pick a table or paste a ticket"),
        },
    }
}

pub fn deck_state(game: TableGame, seated: &import::SeatedDeck) -> DeckState {
    if game == TableGame::FreeForm {
        return DeckState::NotNeeded;
    }
    match &seated.0 {
        None => DeckState::Missing,
        Some(record) if record.deck.game() != game => DeckState::Missing,
        Some(_) => DeckState::Ready,
    }
}

pub fn mode_line(options: Option<&agni_riftbound::TableOptions>) -> String {
    match options {
        Some(options) => format!("{} · first player by roll", options.label()),
        None => format!(
            "first to {} · one battlefield per seat, never fewer than two · first player by roll",
            agni_riftbound::DEFAULT_VICTORY_SCORE
        ),
    }
}

pub fn rules_line(enforced: bool) -> &'static str {
    if enforced {
        "the table refuses illegal plays and runs the turn"
    } else {
        "anything goes; you move the cards"
    }
}

pub fn table_line(options: &agni_riftbound::TableOptions, enforced: bool) -> String {
    let mode = if enforced {
        "rules enforced"
    } else {
        "free table"
    };
    format!("this table · {} · {mode}", options.label())
}

pub enum Action {
    PlayVsAi,
    HostTable,
    FindGame,
    Join(String),
    GoToTable,
}

pub fn action_for(state: &LobbyState) -> Option<Action> {
    if !primary_verb(state).enabled {
        return None;
    }
    Some(match state.role {
        SessionRole::Host | SessionRole::Client | SessionRole::Ended => Action::GoToTable,
        _ => match state.segment {
            Segment::Ai => Action::PlayVsAi,
            Segment::Friends => Action::HostTable,
            Segment::Match => Action::FindGame,
            Segment::Join => Action::Join(state.join_target.as_ref()?.host.clone()),
        },
    })
}

pub fn footer_reserve(phone: bool, has_reason: bool, small_line_h: f32, spacing_y: f32) -> f32 {
    if !phone {
        return 8.0;
    }
    FOOTER_H
        + if has_reason {
            small_line_h + 2.0 * spacing_y
        } else {
            0.0
        }
}

pub fn thumb_size() -> egui::Vec2 {
    egui::vec2(THUMB_W, THUMB_W * dim::CARD_H / dim::CARD_W)
}

pub fn thumbnail(ui: &mut egui::Ui, texture: Option<egui::TextureId>) {
    let size = thumb_size();
    match texture {
        Some(texture) => {
            ui.image(egui::load::SizedTexture::new(texture, size));
        }
        None => {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 4.0, egui::Color32::from_gray(58));
        }
    }
}

pub fn legend_id(deck: &import::ImportedDeck) -> Option<&str> {
    match deck {
        import::ImportedDeck::Riftbound(deck) => deck
            .legend
            .as_ref()
            .or(deck.chosen_champion.as_ref())
            .map(|card| card.riftbound_id.as_str()),
        import::ImportedDeck::Mtg(_) => None,
    }
}

pub fn coverage_text(deck: &import::ImportedDeck) -> Option<String> {
    match deck {
        import::ImportedDeck::Riftbound(deck) => Some(pool::coverage_chip(pool::deck_coverage(
            deck,
            pool::scripted_names(),
        ))),
        import::ImportedDeck::Mtg(_) => None,
    }
}

pub fn primary_button(ui: &mut egui::Ui, verb: &Verb, width: f32) -> bool {
    let fill = if verb.enabled {
        theme::tokens(ui.ctx()).green
    } else {
        theme::tokens(ui.ctx()).grey
    };
    let button = egui::Button::new(
        egui::RichText::new(&verb.label)
            .size(16.0)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    )
    .fill(fill)
    .corner_radius(egui::CornerRadius::same(28))
    .min_size(egui::vec2(width, PRIMARY_H));
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(width, PRIMARY_H),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| ui.add_enabled(verb.enabled, button),
        )
        .inner;
    if let Some(reason) = &verb.reason {
        response.clone().on_disabled_hover_text(reason);
    }
    response.clicked()
}

fn group_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(18.0)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
    ui.add_space(4.0);
}

pub fn deck_card(ui: &mut egui::Ui, menu: &mut Menu, decks: &mut DeckParams, thumbs: &Thumbs) {
    group_title(ui, "deck");
    let Some(record) = decks.seated.0.as_ref() else {
        let mut editor = false;
        ui.horizontal_wrapped(|ui| {
            if chip(ui, SWITCH_LINK, false).clicked() {
                menu.open_sheet(Sheet::DeckBox(DeckSeat::Mine));
            }
            editor = chip(ui, EDITOR_CHIP, false).clicked();
        });
        ui.label(
            egui::RichText::new(EMPTY_DECK_LINE)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
        if editor {
            decks::go(menu);
        }
        return;
    };
    let label = history::label(&record.deck);
    let texture = legend_id(&record.deck).and_then(|id| thumbs.id(id));
    let mut change = false;
    let mut editor = false;
    ui.horizontal_top(|ui| {
        thumbnail(ui, texture);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width());
            ui.label(
                egui::RichText::new(&label)
                    .strong()
                    .color(theme::tokens(ui.ctx()).ink),
            );
            if battlefield::asks(record) {
                ui.label(
                    egui::RichText::new(BATTLEFIELD_LINE)
                        .color(theme::tokens(ui.ctx()).ink_weak)
                        .small(),
                );
            }
            if let Some(note) = import::art_note(record, &decks.art) {
                ui.label(
                    egui::RichText::new(note)
                        .color(theme::tokens(ui.ctx()).ink_weak)
                        .small(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                change |= link(ui, SWITCH_LINK).clicked();
                editor |= link(ui, EDITOR_CHIP).clicked();
            });
        });
    });
    if let Some(error) = &decks.import.error {
        ui.colored_label(theme::tokens(ui.ctx()).danger, error);
    } else if let Some(flash) = decks.import.flash_text(ui.ctx()) {
        ui.label(
            egui::RichText::new(flash)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    } else if let Some(note) = decks.import.note.as_ref().filter(|_| !decks.import.busy) {
        ui.label(
            egui::RichText::new(note)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
    if change {
        menu.open_sheet(Sheet::DeckBox(DeckSeat::Mine));
    }
    if editor {
        decks::go(menu);
    }
}

pub const EDITOR_CHIP: &str = "deck editor";
pub const SWITCH_LINK: &str = "switch decks";
pub const BATTLEFIELD_LINE: &str =
    "you choose a battlefield at the table, once you see your opponent";
pub const EMPTY_DECK_LINE: &str =
    "pick a saved deck here — build, import and share decks in the deck editor";

pub fn rules_group(
    ui: &mut egui::Ui,
    game: TableGame,
    locked: bool,
    fixed: bool,
    net: &mut NetParams,
) {
    group_title(ui, "rules");
    if fixed {
        let players = net.info.roster.len();
        let enforced = net::rules_enforced(&net.info, &net.choice);
        if game == TableGame::Riftbound {
            ui.label(table_line(&net.info.options_in_play(players), enforced));
        } else {
            ui.label(format!("{} · free table", game.label()));
        }
        ui.label(
            egui::RichText::new("table options are fixed while the table is open")
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
        return;
    }
    if locked {
        let _ = chip(ui, "free table", true);
        ui.label(
            egui::RichText::new(match game {
                TableGame::Mtg => "anything goes; you move the cards — commander seats in the command zone, seven-card opening hand",
                _ => "anything goes; you move the cards — no zones, no plugin",
            })
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
        );
        return;
    }
    let mut enforced = net.choice.enforced;
    segmented(
        ui,
        &mut enforced,
        &[(true, "rules enforced"), (false, "free table")],
    );
    if enforced != net.choice.enforced {
        net.choice.enforced = enforced;
    }
    ui.label(
        egui::RichText::new(rules_line(enforced))
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
    ui.add_space(4.0);
    let mut next = None;
    let sanctioned = net.choice.options.as_ref().is_some_and(|options| {
        agni_riftbound::MODES
            .iter()
            .any(|mode| agni_riftbound::TableOptions::of_mode(mode) == *options)
    });
    ui.horizontal_wrapped(|ui| {
        if chip(ui, "by seats", net.choice.options.is_none()).clicked() {
            next = Some(None);
        }
        for mode in &agni_riftbound::MODES {
            let preset = agni_riftbound::TableOptions::of_mode(mode);
            let selected = net.choice.options == Some(preset);
            if chip(ui, mode.name, selected)
                .on_hover_text(format!("{} players · {}", mode.players, preset.label()))
                .clicked()
            {
                next = Some(Some(preset));
            }
        }
        let house = net.choice.options.is_some() && !sanctioned;
        if chip(ui, "house rules…", house).clicked() && !house {
            next = Some(Some(net.choice.options.unwrap_or_default()));
        }
    });
    if let Some(next) = next {
        net.choice.options = next;
    }
    if let Some(options) = net.choice.options.as_mut() {
        ui.horizontal_wrapped(|ui| {
            ui.label("first to");
            ui.add(
                egui::DragValue::new(&mut options.victory_score)
                    .range(agni_riftbound::VICTORY_SCORE_RANGE)
                    .speed(0.1),
            );
            ui.label("points ·");
            ui.add(
                egui::DragValue::new(&mut options.battlefields)
                    .range(agni_riftbound::BATTLEFIELDS_RANGE)
                    .speed(0.1),
            );
            ui.label("battlefields");
        });
    }
    ui.label(
        egui::RichText::new(mode_line(net.choice.options.as_ref()))
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn lobby_screen(
    ui: &mut egui::Ui,
    class: ViewportClass,
    game: TableGame,
    menu: &mut Menu,
    settings: &mut Settings,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) {
    if net.info.role == SessionRole::Solo && net.choice.game != game {
        net.choice.game = game;
        net.choice.enforced = default_enforced(game);
    }
    let phone = class.is_phone();
    let screen_w = ui.max_rect().width();
    let width = content_width(class, screen_w, LOBBY_MAX_W);
    let left = ui.max_rect().min.x + ((screen_w - width) / 2.0).max(margin(class));
    let rect = egui::Rect::from_min_size(
        egui::pos2(left, ui.max_rect().min.y + margin(class) / 2.0),
        egui::vec2(width, ui.max_rect().height() - margin(class)),
    );
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    column.set_max_width(width);
    column.set_min_width(width);
    let ui = &mut column;
    let state = LobbyState {
        game,
        role: net.info.role,
        segment: net.opponent.segment(),
        deck: deck_state(game, &decks.seated),
        join_target: net.opponent.join_target(),
        host_block: net::host_block(&net.choice),
    };
    let searching = net.matchmaking.active();
    let verb = if searching {
        Verb::on("cancel search")
    } else {
        primary_verb(&state)
    };
    let mut pressed = false;
    ui.horizontal(|ui| {
        if back_button(ui) {
            menu.go_home();
        }
        ui.label(
            egui::RichText::new(game.label())
                .size(20.0)
                .strong()
                .color(theme::tokens(ui.ctx()).ink),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if gear_button(ui) {
                settings.open = true;
            }
            if !phone {
                pressed = primary_button(ui, &verb, 200.0);
                if let Some(reason) = &verb.reason {
                    ui.label(
                        egui::RichText::new(reason)
                            .color(theme::tokens(ui.ctx()).ink_weak)
                            .small(),
                    );
                }
            }
        });
    });
    ui.separator();
    let footer = footer_reserve(
        phone,
        verb.reason.is_some(),
        ui.text_style_height(&egui::TextStyle::Small),
        ui.spacing().item_spacing.y,
    );
    let body_h = (rect.bottom() - ui.cursor().top() - footer).max(crate::settings::MIN_INNER);
    let sections = lobby_sections(game, net.choice.enforced, net.info.role);
    egui::ScrollArea::vertical()
        .id_salt("lobby body")
        .max_height(body_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if phone {
                for section in &sections {
                    section_ui(ui, *section, game, menu, my_seat, table, net, decks, thumbs);
                    ui.add_space(16.0);
                }
            } else {
                ui.columns(2, |columns| {
                    for section in &sections {
                        let column = match section {
                            Section::Opponent => 1,
                            _ => 0,
                        };
                        section_ui(
                            &mut columns[column],
                            *section,
                            game,
                            menu,
                            my_seat,
                            table,
                            net,
                            decks,
                            thumbs,
                        );
                        columns[column].add_space(16.0);
                    }
                });
            }
        });
    if phone {
        if let Some(reason) = &verb.reason {
            ui.label(
                egui::RichText::new(reason)
                    .color(theme::tokens(ui.ctx()).ink_weak)
                    .small(),
            );
        }
        ui.add_space(4.0);
        pressed = primary_button(ui, &verb, ui.available_width());
    }
    if pressed {
        if searching {
            net.matchmaking
                .cancel(&mut net.info, &mut net.host, &mut net.client);
            return;
        }
        match action_for(&state) {
            Some(Action::PlayVsAi) => {
                super::ai_setup::open(menu, &mut net.ai, true);
            }
            Some(Action::HostTable) => net::host_table(&mut net.info),
            Some(Action::FindGame) => {
                decks.pinned.side = None;
                decks.pinned.seating = None;
                net.matchmaking.start(&mut net.info, &net.choice);
            }
            Some(Action::Join(host)) => {
                net.info.status = "joining…".into();
                net::join_table(host);
            }
            Some(Action::GoToTable) => menu.screen = Screen::Table,
            None => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn section_ui(
    ui: &mut egui::Ui,
    section: Section,
    game: TableGame,
    menu: &mut Menu,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) {
    match section {
        Section::Deck => {
            ui.add_enabled_ui(!net.matchmaking.active(), |ui| {
                deck_card(ui, menu, decks, thumbs)
            });
        }
        Section::Rules { locked, fixed } => {
            rules_group(ui, game, locked, fixed || net.matchmaking.active(), net)
        }
        Section::Opponent => {
            opponent::opponent_group(ui, game, menu, my_seat, table, net, decks, thumbs)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn matchmaking_requires_a_deck_and_a_host_capable_build() {
        let mut state = state(SessionRole::Solo, Segment::Match, DeckState::Missing);
        assert!(!primary_verb(&state).enabled);
        state.deck = DeckState::Ready;
        assert_eq!(primary_verb(&state).label, "find game");
        assert!(matches!(action_for(&state), Some(Action::FindGame)));
        state.host_block = Some("engine is loading".into());
        assert!(!primary_verb(&state).enabled);
        assert!(action_for(&state).is_none());
    }
    #[test]
    fn the_phone_footer_reserves_the_reason_line_at_the_touch_style() {
        assert_eq!(super::footer_reserve(false, true, 14.0, 8.0), 8.0);
        assert_eq!(
            super::footer_reserve(true, false, 14.0, 8.0),
            super::FOOTER_H
        );
        let with_reason = super::footer_reserve(true, true, 14.0, 8.0);
        assert!(with_reason >= super::FOOTER_H + 14.0 + 8.0);
        assert!(super::footer_reserve(true, true, 18.0, 8.0) > with_reason);
    }

    use super::*;

    fn state(role: SessionRole, segment: Segment, deck: DeckState) -> LobbyState {
        LobbyState {
            game: TableGame::Riftbound,
            role,
            segment,
            deck,
            join_target: None,
            host_block: None,
        }
    }

    #[test]
    fn the_lobby_sections_follow_the_game_and_the_session() {
        assert_eq!(
            lobby_sections(TableGame::Riftbound, true, SessionRole::Solo),
            vec![
                Section::Deck,
                Section::Rules {
                    locked: false,
                    fixed: false
                },
                Section::Opponent
            ]
        );
        assert_eq!(
            lobby_sections(TableGame::Riftbound, false, SessionRole::Host),
            vec![
                Section::Deck,
                Section::Rules {
                    locked: false,
                    fixed: true
                },
                Section::Opponent
            ]
        );
        assert_eq!(
            lobby_sections(TableGame::Mtg, true, SessionRole::Solo),
            vec![
                Section::Deck,
                Section::Rules {
                    locked: true,
                    fixed: false
                },
                Section::Opponent
            ]
        );
        assert_eq!(
            lobby_sections(TableGame::FreeForm, true, SessionRole::Client),
            vec![
                Section::Rules {
                    locked: true,
                    fixed: true
                },
                Section::Opponent
            ]
        );
        assert!(default_enforced(TableGame::Riftbound));
        assert!(!default_enforced(TableGame::Mtg));
        assert!(!default_enforced(TableGame::FreeForm));
    }

    #[test]
    fn the_primary_verb_names_the_action_for_every_lobby_state() {
        let ready = |segment| state(SessionRole::Solo, segment, DeckState::Ready);
        assert_eq!(primary_verb(&ready(Segment::Ai)), Verb::on("play vs AI"));
        assert_eq!(
            primary_verb(&ready(Segment::Friends)),
            Verb::on("host table")
        );
        let join = primary_verb(&ready(Segment::Join));
        assert!(!join.enabled);
        assert_eq!(
            join.reason.as_deref(),
            Some("pick a table or paste a ticket")
        );
        let mut picked = ready(Segment::Join);
        picked.join_target = Some(JoinTarget {
            host: "abc123".into(),
            name: "claude".into(),
        });
        assert_eq!(primary_verb(&picked), Verb::on("join claude"));
        let missing = primary_verb(&state(SessionRole::Solo, Segment::Ai, DeckState::Missing));
        assert_eq!(missing.label, "play vs AI");
        assert!(!missing.enabled);
        assert_eq!(missing.reason.as_deref(), Some("choose a deck first"));
        let free_form = primary_verb(&LobbyState {
            game: TableGame::FreeForm,
            deck: DeckState::NotNeeded,
            ..ready(Segment::Friends)
        });
        assert!(free_form.enabled);
        for role in [SessionRole::Host, SessionRole::Client, SessionRole::Ended] {
            assert_eq!(
                primary_verb(&state(role, Segment::Join, DeckState::Missing)),
                Verb::on("go to the table")
            );
        }
        assert!(
            !primary_verb(&state(SessionRole::Starting, Segment::Ai, DeckState::Ready)).enabled
        );
        assert!(
            !primary_verb(&state(
                SessionRole::Joining,
                Segment::Join,
                DeckState::Ready
            ))
            .enabled
        );
        let browser = LobbyState {
            host_block: net::host_block(&crate::net::TableChoice {
                game: TableGame::Riftbound,
                options: None,
                enforced: true,
            }),
            ..ready(Segment::Friends)
        };
        assert_eq!(primary_verb(&browser), Verb::on("host table"));
        let blocked = LobbyState {
            host_block: Some("engine.wasm is still loading — try again in a moment".into()),
            ..ready(Segment::Friends)
        };
        let verb = primary_verb(&blocked);
        assert!(!verb.enabled);
        assert_eq!(verb.label, "host table");
        assert!(verb
            .reason
            .unwrap()
            .starts_with("engine.wasm is still loading"));
        assert!(matches!(
            action_for(&ready(Segment::Ai)),
            Some(Action::PlayVsAi)
        ));
        assert!(matches!(action_for(&picked), Some(Action::Join(host)) if host == "abc123"));
        assert!(action_for(&missing_state()).is_none());
    }

    fn missing_state() -> LobbyState {
        state(SessionRole::Solo, Segment::Ai, DeckState::Missing)
    }

    #[test]
    fn the_mode_note_is_one_sentence() {
        let duel = agni_riftbound::TableOptions::default();
        assert_eq!(
            mode_line(Some(&duel)),
            "first to 8 · 2 battlefields · first player by roll"
        );
        assert_eq!(
            mode_line(None),
            "first to 8 · one battlefield per seat, never fewer than two · first player by roll"
        );
        assert_eq!(
            table_line(&duel, true),
            "this table · first to 8 · 2 battlefields · rules enforced"
        );
        assert_eq!(
            table_line(&duel, false),
            "this table · first to 8 · 2 battlefields · free table"
        );
        assert_eq!(
            rules_line(true),
            "the table refuses illegal plays and runs the turn"
        );
        assert_eq!(rules_line(false), "anything goes; you move the cards");
    }

    #[test]
    fn the_deck_state_reads_the_seated_deck_and_never_waits_for_a_battlefield() {
        let none = import::SeatedDeck(None);
        assert_eq!(deck_state(TableGame::FreeForm, &none), DeckState::NotNeeded);
        assert_eq!(deck_state(TableGame::Riftbound, &none), DeckState::Missing);
        assert_eq!(deck_state(TableGame::Mtg, &none), DeckState::Missing);
        let deck = crate::deck::pool::deck("lillia-house").unwrap();
        let record = import::SeatedDeckRecord {
            seat: agni_core::PlayerId(0),
            deck: import::ImportedDeck::Riftbound(deck),
            faces: Default::default(),
            battlefield: None,
            battlefield_played: false,
        };
        assert!(battlefield::needs_choice(&record));
        let seated = import::SeatedDeck(Some(record));
        assert_eq!(
            deck_state(TableGame::Riftbound, &seated),
            DeckState::Ready,
            "the battlefield is chosen at the table"
        );
        assert_eq!(deck_state(TableGame::Mtg, &seated), DeckState::Missing);
    }
}
