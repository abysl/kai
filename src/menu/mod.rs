pub mod browser;
pub mod deckbox;
pub mod decks;
pub mod editor;
pub mod home;
pub mod lobby;
pub mod opponent;

use crate::deck::pinned;
use crate::deck::thumbs::Thumbs;
use crate::net::TableGame;
use crate::settings::{self, DeckParams, NetParams, Settings, TableParams};
use crate::table::hud;
use crate::table::{MySeat, SessionInfo, SessionRole};
use crate::theme;
use crate::viewport::{back_pressed, BackKey, Viewport};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

pub const MARGIN: f32 = 24.0;
pub const PHONE_MARGIN: f32 = 16.0;
pub const HOME_MAX_W: f32 = 720.0;
pub const LOBBY_MAX_W: f32 = 960.0;
pub const EDITOR_MAX_W: f32 = 1400.0;
pub const PRIMARY_H: f32 = 56.0;
pub const FOOTER_H: f32 = 72.0;
pub const ICON: f32 = 40.0;
pub const TOUCH: f32 = 48.0;

pub use home::{tile_columns, tile_for, tile_row_width, tile_size, HOME_ORDER};
pub use lobby::{lobby_sections, primary_verb, DeckState, LobbyState, Section, Verb};
pub use opponent::{Opponent, Segment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Games,
    Lobby(TableGame),
    Table,
    Decks,
    DeckEditor,
}

impl Screen {
    pub fn in_decks(self) -> bool {
        matches!(self, Self::Decks | Self::DeckEditor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckSeat {
    Mine,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    DeckBox(DeckSeat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    CloseSheet,
    OpenTableMenu,
    CloseTableMenu,
    Home,
    Up,
    Stay,
}

#[derive(Resource, Debug, Default)]
pub struct Menu {
    pub screen: Screen,
    pub last_game: Option<TableGame>,
    pub sheet: Option<Sheet>,
    pub decks_from: Screen,
}

impl Menu {
    pub fn at_table(&self) -> bool {
        self.screen == Screen::Table
    }

    pub fn in_lobby(&self) -> bool {
        matches!(self.screen, Screen::Lobby(_))
    }

    pub fn open_lobby(&mut self, game: TableGame) {
        self.last_game = Some(game);
        self.screen = Screen::Lobby(game);
        self.sheet = None;
    }

    pub fn go_home(&mut self) {
        self.screen = Screen::Games;
        self.sheet = None;
    }

    pub fn open_sheet(&mut self, sheet: Sheet) {
        self.sheet = Some(sheet);
    }

    fn remember_decks_origin(&mut self) {
        if !self.screen.in_decks() {
            self.decks_from = self.screen;
        }
    }

    pub fn open_decks(&mut self) {
        self.remember_decks_origin();
        self.screen = Screen::Decks;
        self.sheet = None;
    }

    pub fn open_editor(&mut self) {
        self.remember_decks_origin();
        self.screen = Screen::DeckEditor;
        self.sheet = None;
    }

    pub fn leave_decks(&mut self) {
        self.sheet = None;
        self.screen = match self.decks_from {
            Screen::Decks | Screen::DeckEditor => Screen::Games,
            from => from,
        };
    }

    pub fn back(&mut self) {
        self.sheet = None;
        self.screen = match self.screen {
            Screen::Games | Screen::Lobby(_) => Screen::Games,
            Screen::Table => match self.last_game {
                Some(game) => Screen::Lobby(game),
                None => Screen::Games,
            },
            Screen::DeckEditor => Screen::Decks,
            Screen::Decks => {
                self.leave_decks();
                self.screen
            }
        };
    }

    pub fn ladder(&mut self, table_menu_open: bool) -> Rung {
        if self.sheet.take().is_some() {
            return Rung::CloseSheet;
        }
        match self.screen {
            Screen::Table => {
                if table_menu_open {
                    Rung::CloseTableMenu
                } else {
                    Rung::OpenTableMenu
                }
            }
            Screen::Lobby(_) => {
                self.screen = Screen::Games;
                Rung::Home
            }
            Screen::DeckEditor => {
                self.screen = Screen::Decks;
                Rung::Up
            }
            Screen::Decks => {
                self.leave_decks();
                Rung::Up
            }
            Screen::Games => Rung::Stay,
        }
    }
}

pub fn follow_session(role: SessionRole, was: SessionRole, screen: Screen) -> Screen {
    if role == SessionRole::Client && was != SessionRole::Client {
        Screen::Table
    } else {
        screen
    }
}

pub fn tagline(game: TableGame) -> &'static str {
    match game {
        TableGame::Riftbound => "legends, battlefields, showdowns — rules enforced or a free table",
        TableGame::Mtg => "commander and constructed, library and command zone",
        TableGame::FreeForm => "a bare table: hand and board, no rules, deal anything",
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.init_resource::<crate::ai::seat::AiLobby>()
            .add_systems(Update, opponent::start_pending_ai);
        crate::os::profile::register(app);
        app.init_resource::<Menu>()
            .init_resource::<Opponent>()
            .init_resource::<decks::LibraryState>()
            .init_resource::<crate::deck::editor::DeckEditor>()
            .init_resource::<editor::EditorSheet>()
            .init_resource::<pinned::PinnedDeck>()
            .add_systems(
                Update,
                pinned::pin_decks.before(crate::deck::battlefield::watch_seating),
            )
            .add_systems(Update, (watch_session, menu_keys))
            .add_systems(
                EguiPrimaryContextPass,
                menu_ui.before(settings::settings_ui),
            );
    }
}

fn watch_session(info: Res<SessionInfo>, mut menu: ResMut<Menu>, mut was: Local<SessionRole>) {
    let next = follow_session(info.role, *was, menu.screen);
    if next != menu.screen {
        menu.screen = next;
        menu.sheet = None;
    }
    *was = info.role;
}

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    back: Res<BackKey>,
    mut contexts: EguiContexts,
    mut settings: ResMut<Settings>,
    mut menu: ResMut<Menu>,
    mut table_menu: ResMut<hud::TableMenu>,
    mut editor: ResMut<crate::deck::editor::DeckEditor>,
) {
    if !back_pressed(&keys, &back) {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    if settings.open {
        settings.open = false;
        return;
    }
    if editor_steps_back(&menu, &mut editor) {
        return;
    }
    match menu.ladder(table_menu.open) {
        Rung::OpenTableMenu => table_menu.open = true,
        Rung::CloseTableMenu => table_menu.open = false,
        Rung::CloseSheet | Rung::Home | Rung::Up | Rung::Stay => {}
    }
}

pub fn editor_steps_back(menu: &Menu, editor: &mut crate::deck::editor::DeckEditor) -> bool {
    menu.screen == Screen::DeckEditor && editor.draft.as_mut().is_some_and(|draft| draft.back())
}

pub fn step_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(text)
            .size(16.0)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
    ui.add_space(4.0);
}

pub fn margin(class: crate::viewport::ViewportClass) -> f32 {
    if class.is_phone() {
        PHONE_MARGIN
    } else {
        MARGIN
    }
}

pub fn content_width(class: crate::viewport::ViewportClass, screen_w: f32, max_w: f32) -> f32 {
    (screen_w - 2.0 * margin(class))
        .min(max_w)
        .max(settings::MIN_INNER)
}

pub fn screen_frame(context: &egui::Context, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    let screen = context.content_rect();
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(screen.min)
        .order(egui::Order::Middle)
        .show(context, |ui| {
            ui.set_min_size(screen.size());
            ui.set_max_size(screen.size());
            ui.set_clip_rect(screen);
            let tokens = theme::dress(ui);
            ui.painter().rect_filled(screen, 0.0, tokens.surface);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            body(ui);
        });
}

pub fn gear_button(ui: &mut egui::Ui) -> bool {
    ui.add(
        egui::Button::new(
            egui::RichText::new("⚙")
                .size(22.0)
                .color(theme::tokens(ui.ctx()).ink),
        )
        .min_size(egui::vec2(ICON, ICON)),
    )
    .on_hover_text("settings")
    .clicked()
}

pub fn back_button(ui: &mut egui::Ui) -> bool {
    ui.add(
        egui::Button::new(
            egui::RichText::new("‹")
                .size(26.0)
                .color(theme::tokens(ui.ctx()).ink),
        )
        .min_size(egui::vec2(ICON, ICON)),
    )
    .on_hover_text("back (esc)")
    .clicked()
}

pub fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .selected(selected)
            .corner_radius(egui::CornerRadius::same(12))
            .min_size(egui::vec2(TOUCH, 32.0)),
    )
}

pub fn float_note(
    context: &egui::Context,
    id: &str,
    at: egui::Pos2,
    text: &str,
    color: egui::Color32,
) {
    let tokens = theme::tokens(context);
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(tokens.surface_opaque())
                .stroke(egui::Stroke::new(1.0, tokens.hairline))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(color).small());
                });
        });
    context.request_repaint_after(std::time::Duration::from_millis(200));
}

pub fn link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let height = ui.spacing().interact_size.y.max(32.0);
    ui.add(
        egui::Button::new(egui::RichText::new(text).underline())
            .frame(false)
            .min_size(egui::vec2(TOUCH, height)),
    )
}

pub fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    options: &[(T, &str)],
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (value, label) in options {
            if chip(ui, label, *current == *value).clicked() && *current != *value {
                *current = *value;
                changed = true;
            }
        }
    });
    changed
}

#[allow(clippy::too_many_arguments)]
fn menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<Menu>,
    mut settings: ResMut<Settings>,
    my_seat: Res<MySeat>,
    mut table: TableParams,
    mut net: NetParams,
    mut decks: DeckParams,
    viewport: Res<Viewport>,
    input: Res<crate::viewport::InputKind>,
    mut thumbs: Local<Thumbs>,
) -> Result {
    if menu.at_table() {
        thumbs.drop_all();
        return Ok(());
    }
    if menu.in_lobby() {
        deckbox::stage_thumbs(&mut contexts, &mut thumbs, &mut decks, &net.opponent);
    } else {
        thumbs.drop_all();
    }
    if menu.screen == Screen::DeckEditor {
        editor::stage(&mut contexts, &mut decks);
    }
    let context = contexts.ctx_mut()?.clone();
    let class = viewport.class;
    let screen = menu.screen;
    let covered = class.is_phone() && (menu.sheet.is_some() || settings.open);
    screen_frame(&context, "menu screen", |ui| match screen {
        _ if covered => {}
        Screen::Games => home::home_screen(
            ui,
            class,
            &mut menu,
            &mut settings,
            &my_seat,
            &net,
            &table,
            &decks,
        ),
        Screen::Lobby(game) => lobby::lobby_screen(
            ui,
            class,
            game,
            &mut menu,
            &mut settings,
            &my_seat,
            &mut table,
            &mut net,
            &mut decks,
            &thumbs,
        ),
        Screen::Decks => decks::library_screen(ui, class, &mut menu, &mut settings, &mut decks),
        Screen::DeckEditor => editor::editor_screen(
            ui,
            class,
            *input,
            &mut menu,
            &mut settings,
            &my_seat,
            &mut table,
            &mut net,
            &mut decks,
        ),
        Screen::Table => {}
    });
    match screen {
        Screen::Lobby(game) => {
            deckbox::deckbox_ui(
                &context, class, game, &mut menu, &my_seat, &mut table, &mut net, &mut decks,
                &thumbs,
            );
            crate::deck::exchange::qr_modal(&context, &mut decks.import.qr);
        }
        Screen::Decks => crate::deck::exchange::qr_modal(&context, &mut decks.import.qr),
        Screen::DeckEditor => crate::deck::exchange::qr_modal(&context, &mut decks.sheet.qr),
        Screen::Games | Screen::Table => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_joiner_landing_in_its_seat_is_taken_to_the_table() {
        use SessionRole::*;
        assert_eq!(
            follow_session(Client, Joining, Screen::Games),
            Screen::Table
        );
        assert_eq!(
            follow_session(Client, Solo, Screen::Lobby(TableGame::Mtg)),
            Screen::Table
        );
        assert_eq!(
            follow_session(Client, Client, Screen::Lobby(TableGame::Mtg)),
            Screen::Lobby(TableGame::Mtg)
        );
        let lobby = Screen::Lobby(TableGame::Riftbound);
        assert_eq!(follow_session(Starting, Solo, lobby), lobby);
        assert_eq!(
            follow_session(Host, Starting, lobby),
            lobby,
            "a host stays in the lobby"
        );
        assert_eq!(follow_session(Host, Host, lobby), lobby);
        assert_eq!(follow_session(Joining, Solo, lobby), lobby);
        assert_eq!(follow_session(Ended, Client, Screen::Table), Screen::Table);
        assert_eq!(follow_session(Solo, Host, Screen::Table), Screen::Table);
        assert_eq!(
            follow_session(Client, Ended, Screen::Games),
            Screen::Table,
            "a reconnecting joiner lands at the table again"
        );
    }

    #[test]
    fn the_back_ladder_closes_a_sheet_then_toggles_the_table_menu_then_goes_home() {
        let mut menu = Menu::default();
        menu.open_lobby(TableGame::Riftbound);
        menu.open_sheet(Sheet::DeckBox(DeckSeat::Mine));
        assert_eq!(menu.ladder(false), Rung::CloseSheet);
        assert_eq!(menu.sheet, None);
        assert_eq!(menu.screen, Screen::Lobby(TableGame::Riftbound));
        assert_eq!(menu.ladder(false), Rung::Home);
        assert_eq!(menu.screen, Screen::Games);
        assert_eq!(menu.ladder(false), Rung::Stay);
        assert_eq!(menu.screen, Screen::Games);
        assert_eq!(menu.last_game, Some(TableGame::Riftbound));
        menu.screen = Screen::Table;
        assert_eq!(menu.ladder(false), Rung::OpenTableMenu);
        assert_eq!(
            menu.screen,
            Screen::Table,
            "the ladder never leaves the table"
        );
        assert_eq!(menu.ladder(true), Rung::CloseTableMenu);
        assert_eq!(menu.screen, Screen::Table);
        menu.open_sheet(Sheet::DeckBox(DeckSeat::Ai));
        assert_eq!(menu.ladder(true), Rung::CloseSheet, "a sheet closes first");
    }

    #[test]
    fn the_editor_screen_steps_back_through_its_panes_then_to_the_library_then_home() {
        use crate::deck::editor::{DeckEditor, Draft, Origin, Pane};
        let mut menu = Menu::default();
        menu.open_lobby(TableGame::Riftbound);
        let mut editor = DeckEditor::default();
        assert!(
            !editor_steps_back(&menu, &mut editor),
            "no editor, nothing to step"
        );
        let mut draft = Draft::new("t", Origin::New);
        draft.pane = Pane::Cards;
        crate::deck::editor::open(&mut editor, &mut menu, draft);
        assert_eq!(menu.screen, Screen::DeckEditor);
        assert_eq!(menu.decks_from, Screen::Lobby(TableGame::Riftbound));
        assert!(editor_steps_back(&menu, &mut editor));
        assert_eq!(editor.draft.as_ref().unwrap().pane, Pane::List);
        assert_eq!(menu.screen, Screen::DeckEditor, "the screen stays");
        assert!(!editor_steps_back(&menu, &mut editor));
        assert_eq!(menu.ladder(false), Rung::Up);
        assert_eq!(menu.screen, Screen::Decks);
        assert!(
            editor.draft.is_some(),
            "leaving the editor never loses a draft"
        );
        assert_eq!(menu.ladder(false), Rung::Up);
        assert_eq!(
            menu.screen,
            Screen::Lobby(TableGame::Riftbound),
            "the library returns to the screen that opened it"
        );
        menu.go_home();
        menu.open_decks();
        assert_eq!(menu.decks_from, Screen::Games);
        menu.open_editor();
        assert_eq!(
            menu.decks_from,
            Screen::Games,
            "moving between the two deck screens keeps the origin"
        );
        menu.back();
        assert_eq!(menu.screen, Screen::Decks);
        menu.back();
        assert_eq!(menu.screen, Screen::Games);
        let mut odd = Menu {
            screen: Screen::Decks,
            decks_from: Screen::DeckEditor,
            ..Menu::default()
        };
        odd.leave_decks();
        assert_eq!(
            odd.screen,
            Screen::Games,
            "a circular origin falls back home"
        );
    }

    #[test]
    fn the_explicit_back_walks_table_to_lobby_to_home_and_remembers_the_game() {
        let mut menu = Menu::default();
        menu.open_lobby(TableGame::Riftbound);
        menu.screen = Screen::Table;
        menu.back();
        assert_eq!(menu.screen, Screen::Lobby(TableGame::Riftbound));
        menu.back();
        assert_eq!(menu.screen, Screen::Games);
        menu.back();
        assert_eq!(menu.screen, Screen::Games);
        let mut fresh = Menu {
            screen: Screen::Table,
            ..Menu::default()
        };
        fresh.back();
        assert_eq!(fresh.screen, Screen::Games);
    }

    #[test]
    fn the_content_width_respects_the_margin_and_the_cap() {
        use crate::viewport::ViewportClass;
        assert_eq!(
            content_width(ViewportClass::Desktop, 1280.0, HOME_MAX_W),
            720.0
        );
        assert_eq!(
            content_width(ViewportClass::Tablet, 1024.0, LOBBY_MAX_W),
            960.0
        );
        assert_eq!(
            content_width(ViewportClass::PhonePortrait, 360.0, HOME_MAX_W),
            328.0
        );
        assert_eq!(
            content_width(ViewportClass::PhoneLandscape, 800.0, HOME_MAX_W),
            720.0
        );
        assert_eq!(
            content_width(ViewportClass::PhonePortrait, 40.0, HOME_MAX_W),
            settings::MIN_INNER
        );
    }
}
