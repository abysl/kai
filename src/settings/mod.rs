pub mod advanced;
pub mod look;
pub mod play;
pub mod you;

use crate::deck::import;
use crate::engine::modules;
use crate::net::{self, identity, peers};
use crate::render::art::ArtCache;
use crate::table::hud::{self, Side};
use crate::table::{colors, Mirror, MySeat, Redeal, SessionInfo, Tuning};
use crate::telemetry;
use crate::theme;
use crate::viewport::{Viewport, ViewportClass};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

pub const MARGIN: f32 = 24.0;
pub const MIN_INNER: f32 = 80.0;
const FIELD_W: f32 = 320.0;
const NAME_W: f32 = 140.0;
const PANEL_W: f32 = 560.0;
const LIST_H: f32 = 340.0;
const LIST_H_MIN: f32 = 120.0;
const LIST_CHROME_H: f32 = 200.0;
const CHOICE_W: f32 = 180.0;
pub const SHEET_PAD: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelMetrics {
    pub inner_w: f32,
    pub columns: usize,
    pub field_w: f32,
    pub name_w: f32,
    pub panel_w: f32,
    pub list_h: f32,
    pub choice_w: f32,
}

pub fn panel_metrics(class: ViewportClass, size: egui::Vec2) -> PanelMetrics {
    let inner_w = size.x.max(MIN_INNER);
    let phone = class.is_phone();
    PanelMetrics {
        inner_w,
        columns: if phone { 1 } else { 2 },
        field_w: FIELD_W.min(inner_w),
        name_w: NAME_W.min(inner_w),
        panel_w: PANEL_W.min(inner_w),
        list_h: (size.y - LIST_CHROME_H).clamp(LIST_H_MIN, LIST_H),
        choice_w: if phone {
            CHOICE_W.min((inner_w - 8.0) / 2.0).max(MIN_INNER)
        } else {
            CHOICE_W
        },
    }
}

pub fn sheet_metrics(class: ViewportClass, screen: egui::Vec2) -> PanelMetrics {
    let rect = hud::sheet_rect(
        class,
        egui::Rect::from_min_size(egui::Pos2::ZERO, screen),
        Side::Right,
    );
    panel_metrics(
        class,
        egui::vec2(
            (rect.width() - SHEET_PAD).max(MIN_INNER),
            (rect.height() - SHEET_PAD).max(MIN_INNER),
        ),
    )
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Play,
    Look,
    You,
    Advanced,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Play, Tab::Look, Tab::You, Tab::Advanced];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Play => "play",
            Tab::Look => "look",
            Tab::You => "you",
            Tab::Advanced => "advanced",
        }
    }
}

#[derive(Resource, Default)]
pub struct Settings {
    pub open: bool,
    pub tab: Tab,
    pub developer: bool,
    pub elo: crate::elo::Panel,
}

pub fn developer_sections_shown(developer: bool) -> bool {
    developer
}

#[derive(SystemParam)]
pub struct TableParams<'w> {
    pub tuning: ResMut<'w, Tuning>,
    pub redeal: MessageWriter<'w, Redeal>,
    pub tools: Res<'w, crate::table::plugin_ui::Tools>,
    pub panel: Res<'w, crate::table::plugin_ui::PluginPanel>,
    pub seat_colors: ResMut<'w, colors::SeatColors>,
    pub picked: MessageWriter<'w, colors::ColorPicked>,
    pub playmats: ResMut<'w, crate::table::playmat::PlaymatLibrary>,
    pub thumbs: Res<'w, crate::table::playmat::PlaymatThumbs>,
}

#[derive(SystemParam)]
pub struct NetParams<'w> {
    pub info: ResMut<'w, SessionInfo>,
    pub choice: ResMut<'w, net::TableChoice>,
    pub host: ResMut<'w, net::HostState>,
    pub client: ResMut<'w, net::ClientState>,
    pub peers: ResMut<'w, peers::PeerPanel>,
    pub identity: ResMut<'w, identity::IdentityPanel>,
    pub opponent: ResMut<'w, crate::menu::Opponent>,
    pub name: ResMut<'w, crate::os::profile::PlayerName>,
    pub ai: ResMut<'w, crate::ai::seat::AiLobby>,
}

#[derive(SystemParam)]
pub struct DeckParams<'w> {
    pub import: ResMut<'w, import::ImportPanel>,
    pub seated: ResMut<'w, import::SeatedDeck>,
    pub art: ResMut<'w, ArtCache>,
    pub mirror: Res<'w, Mirror>,
    pub game_table: Res<'w, crate::table::GameTable>,
    pub generation: Res<'w, crate::table::DealGeneration>,
    pub deal: MessageWriter<'w, import::DealDeckRequested>,
    pub prompt: ResMut<'w, crate::deck::battlefield::BattlefieldPrompt>,
    pub history: Res<'w, crate::deck::history::DeckHistory>,
    pub seat_saved: MessageWriter<'w, crate::deck::history::SeatSavedDeck>,
    pub forget_saved: MessageWriter<'w, crate::deck::history::ForgetSavedDeck>,
    pub pinned: ResMut<'w, crate::deck::pinned::PinnedDeck>,
    pub sideboard: ResMut<'w, crate::deck::sideboard::SideboardPanel>,
    pub reloads: MessageWriter<'w, crate::deck::sideboard::ReloadDeckRequested>,
    pub library: ResMut<'w, crate::menu::decks::LibraryState>,
    pub images: ResMut<'w, Assets<Image>>,
    pub registry: ResMut<'w, crate::render::egui_art::EguiArt>,
    pub editor: ResMut<'w, crate::deck::editor::DeckEditor>,
    pub catalog: ResMut<'w, crate::deck::catalog::Catalog>,
    pub browser: ResMut<'w, BrowserThumbs>,
    pub sheet: ResMut<'w, crate::menu::editor::EditorSheet>,
}

#[derive(Resource, Default)]
pub struct BrowserThumbs(pub crate::deck::thumbs::Thumbs);

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>();
        app.world_mut().resource_mut::<Settings>().elo = crate::elo::Panel::load();
        app.add_systems(
            EguiPrimaryContextPass,
            (crate::table::playmat::stage_thumbs, settings_ui).chain(),
        );
    }
}

pub fn box_size(content: egui::Vec2, margin: f32) -> egui::Vec2 {
    (content - egui::vec2(margin * 2.0, margin * 2.0)).max(egui::vec2(MIN_INNER, MIN_INNER))
}

pub fn inner_height(bottom: f32, cursor_top: f32, footer: f32) -> f32 {
    (bottom - cursor_top - footer).max(MIN_INNER)
}

#[cfg(test)]
pub fn boxed(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    salt: &str,
    body: impl FnOnce(&mut egui::Ui, egui::Rect),
) {
    let clip = egui::Rect::from_min_size(ui.cursor().min, size);
    ui.set_clip_rect(clip.intersect(ui.clip_rect()));
    egui::ScrollArea::both()
        .id_salt(salt)
        .max_width(size.x)
        .max_height(size.y)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let inner_w = ui.available_width();
            ui.set_max_width(inner_w);
            ui.set_min_width(inner_w);
            ui.set_min_height(ui.available_height());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            let rect = ui.max_rect();
            body(ui, rect);
        });
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn build_line() -> String {
    format!("v{VERSION} · wire {}", agni_net::session::WIRE_VERSION)
}

pub fn tabs(ui: &mut egui::Ui, settings: &mut Settings) {
    let mut tab = settings.tab;
    let options: Vec<(Tab, &str)> = Tab::ALL.iter().map(|tab| (*tab, tab.label())).collect();
    crate::menu::segmented(ui, &mut tab, &options);
    if tab != settings.tab {
        settings.tab = tab;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn body(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metrics: &PanelMetrics,
    my_seat: &MySeat,
    menu: &mut crate::menu::Menu,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    modules_panel: &modules::ModulesPanel,
    telemetry_panel: &mut telemetry::TelemetryPanel,
) {
    match settings.tab {
        Tab::Play => play::play_tab(ui, table),
        Tab::Look => look::look_tab(ui, metrics, my_seat, table, net, decks),
        Tab::You => {
            settings.elo.show(ui);
            ui.separator();
            you::you_tab(ui, net, my_seat);
        }
        Tab::Advanced => advanced::advanced_tab(
            ui,
            settings,
            menu,
            table,
            net,
            modules_panel,
            telemetry_panel,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn settings_ui(
    mut contexts: EguiContexts,
    mut settings: ResMut<Settings>,
    my_seat: Res<MySeat>,
    mut menu: ResMut<crate::menu::Menu>,
    mut table: TableParams,
    mut net: NetParams,
    mut decks: DeckParams,
    modules_panel: Res<modules::ModulesPanel>,
    mut telemetry_panel: ResMut<telemetry::TelemetryPanel>,
    viewport: Res<Viewport>,
) -> Result {
    net.peers.open = settings.open;
    if !settings.open {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let metrics = sheet_metrics(viewport.class, context.content_rect().size());
    let mut open = true;
    hud::sheet(
        &context,
        "settings",
        viewport.class,
        Side::Right,
        "settings",
        &mut open,
        |ui| {
            tabs(ui, &mut settings);
            ui.add_space(8.0);
            body(
                ui,
                &mut settings,
                &metrics,
                &my_seat,
                &mut menu,
                &mut table,
                &mut net,
                &mut decks,
                &modules_panel,
                &mut telemetry_panel,
            );
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new("settings save as you go")
                    .color(theme::tokens(ui.ctx()).ink_weak)
                    .small(),
            );
        },
    );
    if !open {
        settings.open = false;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_is_the_window_less_its_margins_and_never_collapses() {
        assert_eq!(
            box_size(egui::vec2(1280.0, 720.0), MARGIN),
            egui::vec2(1232.0, 672.0)
        );
        assert_eq!(
            box_size(egui::vec2(30.0, 10.0), MARGIN),
            egui::vec2(MIN_INNER, MIN_INNER)
        );
    }

    #[test]
    fn the_panel_metrics_follow_the_four_reference_sizes() {
        let at = |w: f32, h: f32| {
            let size = box_size(egui::vec2(w, h), MARGIN);
            panel_metrics(crate::viewport::viewport_class(Vec2::new(w, h)), size)
        };
        let desktop = at(1280.0, 800.0);
        assert_eq!(desktop.inner_w, 1232.0);
        assert_eq!(desktop.columns, 2);
        assert_eq!(desktop.field_w, FIELD_W);
        assert_eq!(desktop.name_w, NAME_W);
        assert_eq!(desktop.panel_w, PANEL_W);
        assert_eq!(desktop.list_h, LIST_H);
        assert_eq!(desktop.choice_w, CHOICE_W);
        let tablet = at(1024.0, 768.0);
        assert_eq!(tablet.columns, 2);
        assert_eq!(tablet.choice_w, CHOICE_W);
        assert_eq!(tablet.list_h, LIST_H);
        let landscape = at(800.0, 360.0);
        assert_eq!(landscape.inner_w, 752.0);
        assert_eq!(landscape.columns, 1);
        assert_eq!(landscape.field_w, FIELD_W);
        assert_eq!(landscape.list_h, LIST_H_MIN);
        assert_eq!(landscape.choice_w, CHOICE_W);
        let portrait = at(360.0, 800.0);
        assert_eq!(portrait.inner_w, 312.0);
        assert_eq!(portrait.columns, 1);
        assert_eq!(portrait.field_w, 312.0);
        assert_eq!(portrait.name_w, NAME_W);
        assert_eq!(portrait.panel_w, 312.0);
        assert_eq!(portrait.list_h, LIST_H);
        assert_eq!(portrait.choice_w, 152.0);
        let tiny = panel_metrics(
            ViewportClass::PhonePortrait,
            egui::vec2(MIN_INNER, MIN_INNER),
        );
        assert_eq!(tiny.choice_w, MIN_INNER);
        assert_eq!(tiny.field_w, MIN_INNER);
    }

    #[test]
    fn the_settings_sheet_is_380_wide_on_desktop_and_the_screen_on_a_phone() {
        let desktop = sheet_metrics(ViewportClass::Desktop, egui::vec2(1280.0, 800.0));
        assert_eq!(desktop.inner_w, hud::SHEET_W - SHEET_PAD);
        assert_eq!(desktop.field_w, FIELD_W.min(hud::SHEET_W - SHEET_PAD));
        let phone = sheet_metrics(ViewportClass::PhonePortrait, egui::vec2(360.0, 800.0));
        assert_eq!(phone.inner_w, 328.0);
        assert_eq!(phone.columns, 1);
    }

    #[test]
    fn the_inner_scroll_height_comes_from_the_box_bottom_and_the_cursor_not_the_ui() {
        assert_eq!(inner_height(672.0, 100.0, 36.0), 536.0);
        assert_eq!(inner_height(672.0, 100.0, 8.0), 564.0);
        assert_eq!(inner_height(200.0, 190.0, 36.0), MIN_INNER);
        assert_eq!(inner_height(f32::INFINITY, 100.0, 8.0), f32::INFINITY);
    }

    #[test]
    fn the_default_tab_is_play_and_developer_settings_start_hidden() {
        let settings = Settings::default();
        assert_eq!(settings.tab, Tab::Play);
        assert!(!settings.open);
        assert!(!settings.developer);
        assert!(!developer_sections_shown(settings.developer));
        assert!(developer_sections_shown(true));
        assert_eq!(Tab::ALL[0], Tab::Play);
        assert_eq!(Tab::ALL.len(), 4);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod walk {
    use super::*;
    use crate::deck::import::{ImportedDeck, SeatedDeck, SeatedDeckRecord};
    use crate::deck::thumbs::Thumbs;
    use crate::menu::{self, DeckSeat, Menu};
    use crate::net::TableGame;
    use agni_core::PlayerId;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    const WALK_W: f32 = 312.0;
    const WALK_H: f32 = 1800.0;
    const TELEMETRY_FILTER_ROW_OVERFLOW: f32 = 24.0;

    fn walk_world(seated: bool) -> World {
        let mut world = World::new();
        world.init_resource::<Tuning>();
        world.init_resource::<Messages<Redeal>>();
        world.init_resource::<colors::SeatColors>();
        world.init_resource::<Messages<colors::ColorPicked>>();
        world.init_resource::<crate::table::playmat::PlaymatLibrary>();
        world.init_resource::<crate::table::playmat::PlaymatThumbs>();
        world.init_resource::<SessionInfo>();
        world.init_resource::<net::TableChoice>();
        world.init_resource::<net::HostState>();
        world.init_resource::<net::ClientState>();
        world.init_resource::<peers::PeerPanel>();
        world.init_resource::<identity::IdentityPanel>();
        world.init_resource::<crate::ai::seat::AiLobby>();
        world.init_resource::<crate::menu::Opponent>();
        world.init_resource::<crate::os::profile::PlayerName>();
        world.init_resource::<import::ImportPanel>();
        world.init_resource::<ArtCache>();
        world.init_resource::<Mirror>();
        world.init_resource::<crate::table::GameTable>();
        world.init_resource::<crate::table::DealGeneration>();
        world.init_resource::<Messages<import::DealDeckRequested>>();
        world.init_resource::<crate::deck::battlefield::BattlefieldPrompt>();
        world.init_resource::<crate::deck::history::DeckHistory>();
        world.init_resource::<Messages<crate::deck::history::SeatSavedDeck>>();
        world.init_resource::<Messages<crate::deck::history::ForgetSavedDeck>>();
        world.init_resource::<crate::deck::pinned::PinnedDeck>();
        world.init_resource::<crate::deck::sideboard::SideboardPanel>();
        world.init_resource::<Messages<crate::deck::sideboard::ReloadDeckRequested>>();
        world.init_resource::<crate::menu::decks::LibraryState>();
        world.init_resource::<crate::deck::editor::DeckEditor>();
        world.init_resource::<crate::deck::catalog::Catalog>();
        world.init_resource::<BrowserThumbs>();
        world.init_resource::<crate::menu::editor::EditorSheet>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<crate::render::egui_art::EguiArt>();
        world.init_resource::<MySeat>();
        world.init_resource::<modules::ModulesPanel>();
        world.init_resource::<telemetry::TelemetryPanel>();
        world.init_resource::<Menu>();
        world.init_resource::<Settings>();
        world.init_resource::<crate::table::plugin_ui::Tools>();
        world.init_resource::<crate::table::plugin_ui::PluginPanel>();
        let record = seated.then(|| {
            let deck = crate::deck::pool::deck("lillia-house").expect("the pool deck resolves");
            SeatedDeckRecord {
                seat: PlayerId(0),
                deck: ImportedDeck::Riftbound(deck),
                faces: Default::default(),
                battlefield: None,
                battlefield_played: false,
            }
        });
        world.insert_resource(SeatedDeck(record));
        world
    }

    fn widest(body: impl FnOnce(&mut egui::Ui, egui::Rect)) -> f32 {
        let context = egui::Context::default();
        let mut widest = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WALK_W, WALK_H),
            )),
            ..Default::default()
        };
        let mut body = Some(body);
        let output = context.run_ui(input, |ui| {
            let Some(body) = body.take() else {
                return;
            };
            boxed(ui, egui::vec2(WALK_W, WALK_H), "walk", |ui, rect| {
                body(ui, rect);
                widest = ui.min_rect().width();
            });
        });
        output.drop_without_applying_deltas();
        widest
    }

    fn fits(name: &str, width: f32) {
        fits_within(name, width, 0.0);
    }

    fn fits_within(name: &str, width: f32, allowance: f32) {
        assert!(
            width <= WALK_W + allowance + 0.5,
            "{name} is {width} wide at a {WALK_W} dp box"
        );
    }

    #[test]
    fn every_settings_tab_fits_312_dp_with_and_without_developer_settings() {
        let metrics = sheet_metrics(ViewportClass::PhonePortrait, egui::vec2(360.0, 800.0));
        for seated in [false, true] {
            for developer in [false, true] {
                let mut world = walk_world(seated);
                for tab in Tab::ALL {
                    world
                        .run_system_once(
                            move |my_seat: Res<MySeat>,
                                  mut menu: ResMut<Menu>,
                                  mut table: TableParams,
                                  mut net: NetParams,
                                  mut decks: DeckParams,
                                  modules_panel: Res<modules::ModulesPanel>,
                                  mut telemetry_panel: ResMut<telemetry::TelemetryPanel>| {
                                let mut settings = Settings {
                                    open: true,
                                    tab,
                                    developer,
                                    ..Default::default()
                                };
                                let width = widest(|ui, _| {
                                    tabs(ui, &mut settings);
                                    body(
                                        ui,
                                        &mut settings,
                                        &metrics,
                                        &my_seat,
                                        &mut menu,
                                        &mut table,
                                        &mut net,
                                        &mut decks,
                                        &modules_panel,
                                        &mut telemetry_panel,
                                    );
                                });
                                let allowance = if tab == Tab::Advanced && developer {
                                    TELEMETRY_FILTER_ROW_OVERFLOW
                                } else {
                                    0.0
                                };
                                fits_within(
                                    &format!(
                                        "settings › {} (seated {seated}, developer {developer})",
                                        tab.label()
                                    ),
                                    width,
                                    allowance,
                                );
                            },
                        )
                        .expect("the walk system runs");
                }
            }
        }
    }

    #[test]
    fn every_lobby_section_and_deck_box_step_fits_312_dp() {
        let class = ViewportClass::PhonePortrait;
        for seated in [false, true] {
            for game in TableGame::ALL {
                for enforced in [false, true] {
                    for segment in [
                        menu::Segment::Ai,
                        menu::Segment::Friends,
                        menu::Segment::Join,
                    ] {
                        let mut world = walk_world(seated);
                        world
                            .run_system_once(
                                move |mut menu: ResMut<Menu>,
                                      mut settings: ResMut<Settings>,
                                      my_seat: Res<MySeat>,
                                      mut table: TableParams,
                                      mut net: NetParams,
                                      mut decks: DeckParams,
                                      thumbs: Local<Thumbs>| {
                                    net.choice.game = game;
                                    net.choice.enforced = enforced;
                                    net.opponent.segment = Some(segment);
                                    menu.open_lobby(game);
                                    let name = format!(
                                        "{} lobby (enforced {enforced}, seated {seated}, {segment:?})",
                                        game.label()
                                    );
                                    let width = widest(|ui, _| {
                                        menu::lobby::lobby_screen(
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
                                        );
                                    });
                                    fits(&name, width);
                                    for seat in [DeckSeat::Mine, DeckSeat::Ai] {
                                        let width = widest(|ui, _| {
                                            menu::deckbox::deckbox_body(
                                                ui,
                                                seat,
                                                game,
                                                &my_seat,
                                                &mut table,
                                                &mut net,
                                                &mut decks,
                                                &thumbs,
                                            );
                                        });
                                        fits(&format!("{name} deck box {seat:?}"), width);
                                    }
                                },
                            )
                            .expect("the walk system runs");
                    }
                }
            }
        }
        let mut world = walk_world(false);
        world
            .run_system_once(
                move |mut menu: ResMut<Menu>,
                      mut settings: ResMut<Settings>,
                      my_seat: Res<MySeat>,
                      table: TableParams,
                      net: NetParams,
                      mut decks: DeckParams| {
                    let width = widest(|ui, _| {
                        menu::home::home_screen(
                            ui,
                            class,
                            &mut menu,
                            &mut settings,
                            &my_seat,
                            &net,
                            &table,
                            &decks,
                        );
                    });
                    fits("home", width);
                    menu.open_decks();
                    let width = widest(|ui, _| {
                        menu::decks::library_screen(
                            ui,
                            class,
                            &mut menu,
                            &mut settings,
                            &mut decks,
                        );
                    });
                    fits("deck editor library", width);
                },
            )
            .expect("the walk system runs");
    }
}
