use crate::deck::{import, sideboard};
use crate::engine::modules;
use crate::net::{self, identity, node, peers};
#[cfg(target_os = "android")]
use crate::os::ime;
use crate::render::art;
use crate::{FoilExtension, FoilMaterial};
use agni_core::{CardId, PlayerId, Table, Zone};
use agni_sim::view::TableView;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use std::collections::BTreeSet;
pub mod dim {
    use std::sync::atomic::{AtomicU32, Ordering};

    pub const CARD_W: f32 = 1.0;
    pub const CARD_H: f32 = 1.4;
    pub const CARD_THICK: f32 = 0.02;

    pub const QUAD_W_DEFAULT: f32 = 12.0;
    pub const QUAD_W_MIN: f32 = 8.0;
    pub const QUAD_D: f32 = 6.0;
    pub const FOV: f32 = std::f32::consts::FRAC_PI_4;

    static QUAD_W: AtomicU32 = AtomicU32::new(QUAD_W_DEFAULT.to_bits());

    pub fn quad_w() -> f32 {
        f32::from_bits(QUAD_W.load(Ordering::Relaxed))
    }

    pub fn set_quad_w(width: f32) {
        QUAD_W.store(width.to_bits(), Ordering::Relaxed);
    }
    pub const HAND_NEAR: f32 = 1.6;
    pub const HAND_STRIP: f32 = 1.5;

    pub const BOARD_Y: f32 = 0.02;
    pub const BOARD_Z: f32 = 0.2;
    pub const BOARD_SPACING: f32 = 1.15;
    pub const STACK_STEP: f32 = 0.004;

    pub const HAND_TILT: f32 = -0.42;
    pub const HAND_VISIBLE: f32 = 7.0;

    pub const ZOOM_MIN: f32 = 0.5;
    pub const ZOOM_MAX: f32 = 2.0;
    pub const ZOOM_LINE_STEP: f32 = 0.05;
    pub const ZOOM_PIXEL_STEP: f32 = 0.001;
    pub const PITCH_MIN: f32 = 45.0;
    pub const PITCH_ARENA: f32 = 62.0;
    pub const PITCH_TOP_DOWN: f32 = 90.0;
    pub const FRAME_NEAR_INSET: f32 = 0.5;
    pub const VIEW_VERSION: u32 = 3;

    pub const HAND_DROP_Z: f32 = 2.0;
    pub const PAN_SPEED: f32 = 0.0006;
    pub const PAN_LIMIT: f32 = 14.0;
    pub const SAVE_DEBOUNCE_SECS: f32 = 0.5;
}

mod anim;
pub mod arrows;
pub mod auto;
pub mod camera;
pub mod chain;
pub mod chips;
pub mod coach;
pub mod colors;
pub mod counters;
pub mod drawer;
pub mod gesture;
pub mod highlight;
pub mod history;
pub mod hud;
pub mod inspector;
mod interaction;
mod layout;
pub mod manual;
pub mod personal_playmat;
pub mod plate;
pub mod playmat;
pub mod plugin_ui;
pub mod primary;
mod scene;
mod sync;
#[cfg(test)]
mod tests;
pub mod toast;
pub mod tokens;
mod tuning;
mod ui;
pub mod undo;
pub mod winner;
pub mod zones;

pub use tuning::{Theme, Tuning};
pub use ui::{redeal_allowed, tuning_section};

pub const EXHAUSTED: &str = "exhausted";
pub const ATTACHED: &str = "attached";
pub const SOLO_SEATS: usize = 2;
pub const CARD_BACK: Color = Color::srgb(0.32, 0.14, 0.10);

use anim::*;
use interaction::*;
use layout::*;
use scene::*;
use sync::*;
use ui::*;

pub struct CardTablePlugin;

impl Plugin for CardTablePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .add_plugins(MaterialPlugin::<FoilMaterial>::default())
            .add_plugins(EguiPlugin::default())
            .add_message::<CardDropped>()
            .add_message::<ExhaustToggled>()
            .add_message::<RevealRequested>()
            .add_message::<TokenSpawn>()
            .add_message::<Redeal>()
            .add_message::<manual::Requested>()
            .init_resource::<manual::Panel>()
            .insert_resource(Tuning::load())
            .init_resource::<GameTable>()
            .init_resource::<Mirror>()
            .init_resource::<DealGeneration>()
            .init_resource::<HandScroll>()
            .init_resource::<ViewSeat>()
            .init_resource::<PlayerCount>()
            .init_resource::<camera::Extent>()
            .init_resource::<MySeat>()
            .init_resource::<SessionInfo>()
            .init_resource::<undo::UndoUi>()
            .add_message::<net::undo::Command>()
            .add_systems(EguiPrimaryContextPass, undo::undo_ui)
            .init_resource::<Held>()
            .init_resource::<Selected>()
            .init_resource::<interaction::Pinned>()
            .init_resource::<interaction::DropChooser>()
            .init_resource::<gesture::Tracker>()
            .init_resource::<plugin_ui::ClaimedKeys>()
            .init_resource::<primary::TurnActivity>()
            .init_resource::<chips::MarchAll>()
            .init_resource::<toast::Refusals>()
            .init_resource::<toast::LastIntent>()
            .init_resource::<plugin_ui::Tools>()
            .init_resource::<auto::Auto>()
            .init_resource::<interaction::DrawerSwipe>()
            .init_resource::<chips::ChipRow>()
            .add_observer(interaction::on_click_felt)
            .add_observer(interaction::on_press_card)
            .add_observer(interaction::on_release)
            .add_observer(interaction::on_cancel)
            .add_observer(interaction::on_drag)
            .add_observer(chain::on_drop_on_chain)
            .add_systems(
                PreUpdate,
                (
                    crate::help::help_keys,
                    drawer::drawer_keys,
                    interaction::escape_ladder,
                )
                    .chain()
                    .after(bevy::input::InputSystems)
                    .after(crate::viewport::read_back_key),
            )
            .init_resource::<crate::help::HelpSheet>()
            .init_resource::<coach::Coach>()
            .init_resource::<anim::Beats>()
            .init_resource::<drawer::DrawerPanel>()
            .init_resource::<history::History>()
            .init_resource::<history::HistoryHover>()
            .init_resource::<hud::Hud>()
            .init_resource::<hud::Insets>()
            .init_resource::<hud::Drawer>()
            .init_resource::<hud::Banner>()
            .init_resource::<hud::TableMenu>()
            .init_resource::<chain::ChainRects>()
            .init_resource::<chain::ChainSheet>()
            .init_resource::<ui::PileSheet>()
            .init_resource::<plugin_ui::TrayItems>()
            .init_resource::<art::ArtCache>()
            .init_resource::<crate::viewport::Viewport>()
            .insert_resource(crate::viewport::InputKind::assumed())
            .add_systems(
                PreUpdate,
                (crate::viewport::classify, crate::viewport::track_input),
            )
            .add_systems(Startup, setup_scene)
            .add_systems(
                Update,
                (
                    sync_player_count,
                    camera::fit_table,
                    sync_seats,
                    sync_zones,
                    sync_hand_backs,
                    sync_cards,
                    orient_cards,
                    (hotkeys, selection_keys, tick_gestures),
                    expire_selection,
                    zoom_camera,
                    pan_camera,
                    camera::edge_pan,
                    camera::toggle_camera_lock,
                    camera::apply_camera,
                    (layout_cards, interaction::sync_hand_pickable).chain(),
                    (settle_hand_hover, mirror_touch_selection).chain(),
                    animate_cards,
                    apply_foil_alpha,
                    hide_viewed_hand,
                    save_tuning,
                )
                    .chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    zone_overlay_ui,
                    pile_sheet_ui,
                    card_label_ui,
                    combat_plate_ui,
                    inspector::inspector_ui,
                    chain::chain_ui,
                    empty_table_ui,
                    hand_count_ui,
                    plate::turn_plate_ui,
                    plate::phase_bar_ui,
                    plate::seat_plates_ui,
                    primary::primary_ui,
                    drawer::drawer_tabs_ui,
                    drawer::drawer_ui,
                    history::history_ui,
                    hud::menu_button_ui,
                )
                    .chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (hud::table_menu_ui, crate::help::help_ui)
                    .chain()
                    .after(hud::menu_button_ui),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (anim::beat_rings_ui, coach::coach_ui)
                    .chain()
                    .after(primary::primary_ui)
                    .after(plugin_ui::plugin_ui),
            )
            .add_systems(
                Update,
                (anim::interrupt_beats, anim::queue_beats)
                    .chain()
                    .after(plugin_ui::refresh_plugin_view)
                    .before(animate_cards),
            )
            .add_systems(
                Update,
                coach::refresh_coach
                    .after(highlight::refresh_rims)
                    .after(toast::collect_refusals),
            )
            .add_systems(
                Update,
                hud::refresh_hud
                    .after(sync_player_count)
                    .after(toast::collect_refusals)
                    .after(plugin_ui::refresh_plugin_view)
                    .before(camera::fit_table),
            )
            .init_resource::<sideboard::SideboardPanel>()
            .add_message::<sideboard::ReloadDeckRequested>()
            .add_systems(EguiPrimaryContextPass, sideboard::sideboard_ui);
        app.init_resource::<colors::SeatColors>()
            .add_message::<colors::ColorPicked>()
            .add_systems(Update, colors::watch_seat_color.before(sync_player_count));
        app.init_resource::<net::HostState>()
            .init_resource::<net::ClientState>()
            .init_resource::<net::TableChoice>()
            .add_message::<import::DealDeckRequested>()
            .add_message::<import::PlaceBattlefieldRequested>()
            .add_systems(
                Update,
                (
                    toast::note_intents,
                    net::drain_net,
                    net::undo::route,
                    net::route_redeal,
                    net::route_annotations,
                    net::route_deck_deals,
                    net::route_battlefield_placements,
                    net::route_deck_reloads,
                    net::route_color_picks,
                    net::route_playmat_picks,
                    net::route_reveals,
                    net::route_spawns,
                    net::route_manual,
                )
                    .chain()
                    .before(sync_player_count),
            );
        app.init_resource::<import::ImportPanel>()
            .init_resource::<import::SeatedDeck>()
            .init_resource::<crate::deck::battlefield::BattlefieldPrompt>()
            .add_systems(
                Update,
                (
                    import::collect_results,
                    crate::deck::battlefield::watch_seating,
                    crate::deck::battlefield::watch_new_game,
                    crate::deck::battlefield::place_chosen,
                    import::auto_deal,
                )
                    .chain()
                    .before(net::route_deck_deals),
            )
            .add_systems(EguiPrimaryContextPass, crate::deck::battlefield::prompt_ui);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Update,
            (
                art::collect_arrivals,
                import::apply_store_art,
                art::queue_visible_art,
            )
                .chain()
                .after(net::route_deck_deals)
                .before(sync_player_count),
        );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Update,
            art::queue_visible_art
                .after(net::route_deck_deals)
                .before(sync_player_count),
        );
        app.init_resource::<peers::PeerPanel>()
            .add_systems(Update, peers::refresh_peers);
        app.init_resource::<identity::IdentityPanel>()
            .add_systems(Startup, node::start)
            .add_systems(Update, identity::watch_refs);
        app.init_resource::<modules::ModulesPanel>()
            .add_systems(Update, modules::refresh_modules);
        app.init_resource::<camera::CameraLock>()
            .init_resource::<camera::EdgeDrift>();
        app.init_resource::<net::NewGameWatch>().add_systems(
            Update,
            net::redeal_after_new_game.before(net::route_deck_deals),
        );
        app.insert_resource(playmat::PlaymatLibrary::load())
            .init_resource::<personal_playmat::PersonalPlaymat>()
            .init_resource::<playmat::PlaymatThumbs>()
            .init_resource::<crate::render::egui_art::EguiArt>()
            .init_resource::<chain::ChainHover>()
            .init_resource::<interaction::RecentDrag>();
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Startup, playmat::restore_saved);
        app.add_systems(
            Update,
            (playmat::ensure_fetched, playmat::fetch_roster_mats).before(sync_seats),
        );
        app.add_systems(
            Update,
            personal_playmat::update
                .before(net::route_playmat_picks)
                .before(sync_seats),
        );
        app.init_resource::<plugin_ui::PluginPanel>()
            .init_resource::<plugin_ui::RollSecrets>()
            .add_message::<plugin_ui::PluginActionRequested>()
            .add_systems(
                Update,
                (
                    plugin_ui::plugin_hotkeys,
                    plugin_ui::auto_reveal,
                    net::route_plugin_actions,
                )
                    .chain()
                    .before(sync_player_count),
            )
            .add_systems(
                Update,
                (
                    plugin_ui::refresh_plugin_view,
                    plugin_ui::refresh_tools,
                    toast::collect_refusals,
                    toast::expire_refusals,
                    auto::auto_pilot,
                    auto::sync_ui_scale,
                )
                    .chain()
                    .after(net::route_plugin_actions)
                    .after(toast::note_intents),
            )
            .add_systems(EguiPrimaryContextPass, plugin_ui::plugin_ui)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    chips::card_chips_ui,
                    chips::strip_digits_ui,
                    chips::drop_chooser_ui,
                    chips::march_all_ui,
                    chips::mulligan_ui,
                )
                    .chain()
                    .after(plugin_ui::plugin_ui),
            )
            .add_systems(
                Update,
                chips::drive_march_all.after(plugin_ui::refresh_plugin_view),
            )
            .add_systems(
                EguiPrimaryContextPass,
                toast::card_toast_ui.after(chips::card_chips_ui),
            );
        app.init_resource::<tokens::TokenPanel>()
            .init_resource::<tokens::PluginTokens>()
            .add_systems(
                Update,
                (
                    tokens::refresh_tokens,
                    tokens::leave_placement,
                    drawer::sync_tokens_tab,
                )
                    .chain()
                    .after(hotkeys)
                    .after(plugin_ui::refresh_tools),
            )
            .add_systems(
                Update,
                history::refresh_history
                    .after(plugin_ui::refresh_plugin_view)
                    .after(toast::collect_refusals),
            )
            .add_systems(EguiPrimaryContextPass, tokens::placement_ui);
        app.init_resource::<winner::WinnerDialog>()
            .add_message::<winner::WinnerChoice>()
            .add_systems(EguiPrimaryContextPass, winner::winner_ui)
            .add_systems(
                Update,
                winner::route_winner_choices.before(net::route_redeal),
            );
        app.init_resource::<counters::CardKinds>()
            .add_message::<counters::CounterNudged>()
            .add_systems(Update, net::route_counters.before(sync_player_count));
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, counters::learn_card_kinds.after(sync_cards));
        app.add_systems(
            EguiPrimaryContextPass,
            (
                counters::hovered_card_counters_ui,
                counters::counter_badges_ui,
            ),
        );
        app.init_resource::<crate::deck::history::DeckHistory>()
            .add_message::<crate::deck::history::SeatSavedDeck>()
            .add_message::<crate::deck::history::ForgetSavedDeck>()
            .add_systems(
                Update,
                (
                    crate::deck::history::refresh_history,
                    crate::deck::import::apply_history,
                ),
            );
        #[cfg(target_os = "android")]
        app.add_systems(
            Update,
            (crate::os::android::drain_tickets, ime::follow_focus),
        )
        .add_systems(
            PreUpdate,
            ime::drain_text.before(bevy_egui::EguiPreUpdateSet::ProcessInput),
        );
        app.init_resource::<arrows::ArrowFades>()
            .add_systems(EguiPrimaryContextPass, arrows::draw_arrows);
        app.init_resource::<highlight::Rims>()
            .add_systems(
                Update,
                highlight::refresh_rims.after(plugin_ui::refresh_plugin_view),
            )
            .add_systems(
                Update,
                highlight::lift_playable
                    .after(highlight::refresh_rims)
                    .after(layout_cards)
                    .before(animate_cards),
            )
            .add_systems(
                Update,
                highlight::grey_unaffordable
                    .after(highlight::refresh_rims)
                    .after(sync_cards),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (highlight::legal_rims_ui, highlight::march_tint_ui),
            );
        #[cfg(target_arch = "wasm32")]
        {
            crate::net::gateway::boot();
            crate::net::page::boot();
            crate::engine::web::boot();
            app.add_systems(Update, crate::net::gateway::apply_art);
        }
    }
}

#[derive(Resource, Debug, Default, Deref, DerefMut)]
pub struct GameTable(pub Table);
#[derive(Resource, Debug, Default)]
pub struct Mirror {
    pub view: TableView,
    pub solo_exhausted: BTreeSet<u32>,
}

impl Mirror {
    pub fn rotated(&self, card: u32) -> bool {
        self.view
            .card(card)
            .map(|entry| entry.badge(EXHAUSTED).is_some())
            .unwrap_or_else(|| self.solo_exhausted.contains(&card))
    }

    pub fn attached_to(&self, card: u32) -> Option<u32> {
        let bytes = self.view.card(card)?.badge(ATTACHED)?;
        let bytes: [u8; 4] = bytes.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }
}
#[derive(Resource, Debug, PartialEq)]
pub struct ViewSeat(pub PlayerId);
#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerCount(pub usize);

impl Default for PlayerCount {
    fn default() -> Self {
        Self(SOLO_SEATS)
    }
}
impl Default for ViewSeat {
    fn default() -> Self {
        Self(PlayerId(0))
    }
}
#[derive(Resource, Debug, PartialEq)]
pub struct MySeat(pub PlayerId);

impl Default for MySeat {
    fn default() -> Self {
        Self(PlayerId(0))
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    #[default]
    Solo,
    Starting,
    Joining,
    Host,
    Client,
    Ended,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Recovery {
    #[default]
    Nothing,
    Rejoin {
        host: String,
    },
    Rehost,
}
#[derive(Resource, Debug, Default)]
pub struct SessionInfo {
    pub undo: agni_net::session::UndoStatus,
    pub undo_generation: u64,
    pub role: SessionRole,
    pub status: String,
    pub roster: Vec<agni_net::session::SeatInfo>,
    pub recovery: Recovery,
    pub options: Option<Vec<u8>>,
    pub notices: Vec<String>,
}

impl SessionInfo {
    pub fn active(&self) -> bool {
        self.role != SessionRole::Solo
    }

    pub fn options_in_play(&self, players: usize) -> agni_riftbound::TableOptions {
        agni_riftbound::TableOptions::in_play(
            self.options.as_deref(),
            u8::try_from(players).unwrap_or(u8::MAX),
        )
    }

    pub fn battlefields_in_play(&self, players: usize) -> usize {
        usize::from(self.options_in_play(players).battlefields)
    }

    pub fn label(&self, my_seat: PlayerId) -> String {
        match self.role {
            SessionRole::Solo => "solo".into(),
            SessionRole::Starting => "starting host…".into(),
            SessionRole::Joining => "joining…".into(),
            SessionRole::Host => "host (player 1)".into(),
            SessionRole::Client => format!("player {}", my_seat.0 + 1),
            SessionRole::Ended => "session ended".into(),
        }
    }
}
#[derive(Resource, Debug, Default)]
pub struct HandScroll(pub f32);
#[derive(Resource, Debug, Default)]
pub struct DealGeneration(pub u32);
#[derive(Message, Debug, Clone, Copy)]
pub struct Redeal;
#[derive(Message, Debug, Clone, Copy)]
pub struct CardDropped {
    pub card: CardId,
    pub to: Zone,
    pub seat: PlayerId,
    pub index: usize,
    pub hidden: bool,
}
#[derive(Message, Debug, Clone, Copy)]
pub struct RevealRequested(pub CardId);
#[derive(Message, Debug, Clone)]
pub struct TokenSpawn {
    pub face: agni_core::CardFace,
    pub to: Zone,
    pub seat: PlayerId,
}
#[derive(Message, Debug, Clone, Copy)]
pub struct ExhaustToggled {
    pub card: CardId,
    pub on: bool,
}
#[derive(Component, Debug, Clone, Copy)]
pub struct CardView(pub CardId);
#[derive(Component, Debug, Clone, PartialEq)]
struct FaceKey {
    name: String,
    foil: bool,
    art: bool,
}

fn face_key(card: &agni_core::Card, art: &art::ArtCache, back: Option<&str>) -> FaceKey {
    FaceKey {
        name: card.face.name.clone(),
        foil: card.face.foil,
        art: if card.face.is_hidden() {
            back.is_some_and(|name| art.has(name))
        } else {
            art.has(&card.face.name)
        },
    }
}
fn wants_label(key: &FaceKey) -> bool {
    !key.art && !key.name.is_empty()
}
pub(crate) fn label_lines(name: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in name.split_whitespace() {
        if line.is_empty() {
            line = word.to_string();
        } else if line.len() + 1 + word.len() <= 12 {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
fn card_shown(card: &agni_core::Card, my_seat: PlayerId) -> bool {
    card.zone != Zone::Hand || card.seat == my_seat
}
fn my_hand_ids(table: &Table, mirror: &Mirror, my_seat: PlayerId) -> Vec<CardId> {
    let mut ids: Vec<CardId> = table.in_area(my_seat, Zone::Hand).map(|c| c.id).collect();
    let fan = zones::hand_zone(&mirror.view.zones);
    if fan != Zone::Hand {
        ids.extend(table.in_area(my_seat, fan).map(|c| c.id));
    }
    ids
}
#[derive(Component, Clone)]
struct CardArt(Handle<Image>);
#[derive(Component)]
pub struct Hovered;
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selected(pub Option<Entity>);
#[derive(bevy::ecs::system::SystemParam)]
pub struct Focus<'w, 's> {
    hovered: Query<'w, 's, &'static CardView, With<Hovered>>,
    views: Query<'w, 's, &'static CardView>,
    selected: Res<'w, Selected>,
}

impl Focus<'_, '_> {
    pub fn card(&self) -> Option<CardView> {
        self.hovered.iter().next().copied().or_else(|| {
            self.selected
                .0
                .and_then(|entity| self.views.get(entity).ok().copied())
        })
    }
}
#[derive(Component)]
struct FoilArt;
#[derive(Component)]
struct FoilBody;
#[derive(Resource)]
struct CardMesh(Handle<Mesh>);
#[derive(Resource)]
struct CardMeshes {
    art: Handle<Mesh>,
    wide_body: Handle<Mesh>,
    wide_art: Handle<Mesh>,
}
#[derive(Component)]
pub struct Landscape;
#[derive(Component, Clone, Copy)]
struct DropSeat(PlayerId);
#[derive(Component, Clone, Copy)]
struct DropZone {
    zone: u16,
    seat: PlayerId,
    yaw: f32,
}
#[derive(Component)]
struct SnapToSlot;
#[derive(Component)]
pub struct SeatDecor;
#[derive(Component)]
struct ZoneDecor;
#[derive(Component, Clone, Copy)]
struct OpponentHand(PlayerId);
#[derive(Resource, Default, Debug)]
pub struct Held {
    pub card: Option<Entity>,
    pub target: Option<Vec3>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Facing {
    Flat,
    Camera,
    Back,
}
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slot {
    pub(crate) position: Vec3,
    pub(crate) facing: Facing,
    pub(crate) yaw: f32,
    pub(crate) rot: f32,
}
pub fn seat_center(seat: PlayerId, count: usize) -> Vec3 {
    seat_center_in(seat, count, dim::quad_w())
}
pub fn seat_center_in(seat: PlayerId, count: usize, quad_w: f32) -> Vec3 {
    let cols = count.div_ceil(2);
    let index = seat.0 as usize;
    let (row, col) = if index < cols {
        (0, index)
    } else {
        (1, index - cols)
    };
    let x = (col as f32 - (cols as f32 - 1.0) / 2.0) * quad_w;
    let z = if row == 0 {
        dim::QUAD_D / 2.0
    } else {
        -dim::QUAD_D / 2.0
    };
    Vec3::new(x, 0.0, z)
}
pub fn seat_yaw(seat: PlayerId, count: usize) -> f32 {
    let cols = count.div_ceil(2);
    if (seat.0 as usize) < cols {
        0.0
    } else {
        std::f32::consts::PI
    }
}
fn deal_origin(owner: PlayerId, players: usize, tuning: &Tuning) -> Vec3 {
    seat_center(owner, players)
        + Quat::from_rotation_y(seat_yaw(owner, players))
            * Vec3::new(0.0, tuning.hand_y, dim::HAND_NEAR + (tuning.hand_z - 3.4))
}
#[cfg(test)]
pub(crate) fn contested_slots(zones: &[agni_sim::wire::ZoneDecl]) -> Vec<u16> {
    zones
        .iter()
        .filter(|decl| {
            decl.owner == agni_sim::wire::ZoneOwner::Shared
                && decl.place == agni_sim::wire::ZonePlace::Center
        })
        .map(|decl| decl.id)
        .collect()
}
