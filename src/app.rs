use crate::render::art::ArtCache;
use crate::table::{DealGeneration, GameTable, Mirror, Redeal, SessionInfo};
use crate::{CardTablePlugin, Tuning};
#[cfg(not(target_arch = "wasm32"))]
use agni_core::Rng;
use agni_core::{CardFace, PlayerId, Table, Zone};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{PrimaryWindow, WindowResolution};
#[cfg(not(target_arch = "wasm32"))]
use serde::Deserialize;
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use web_time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
use spirit_core::{BlobHash, BlobStore};

#[cfg(not(target_arch = "wasm32"))]
const HAND_SIZE: usize = 7;

pub const ART_TINT: [u8; 3] = [255; 3];

#[cfg(not(target_arch = "wasm32"))]
fn hand_size(total: usize) -> usize {
    match std::env::var("AGNI_HAND").as_deref() {
        Ok("all") => total,
        Ok(n) => n.parse().unwrap_or(HAND_SIZE).min(total),
        Err(_) => HAND_SIZE.min(total),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize)]
struct Manifest {
    set: String,
    cards: Vec<ManifestCard>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize)]
struct ManifestCard {
    name: String,
    image: String,
}

pub const WINDOW_VAR: &str = "KAI_WINDOW";
pub const SHOT_VAR: &str = "KAI_SHOT";
pub const OPEN_VAR: &str = "KAI_OPEN";
pub const SHOT_FRAME_VAR: &str = "KAI_SHOT_FRAME";
pub const SHOT_KEY: KeyCode = KeyCode::F12;
pub const STARTUP_SHOT_FRAME: u32 = 6;
const OPEN_FRAME: u32 = 2;

pub fn parse_shot_frame(spec: Option<&str>) -> u32 {
    spec.and_then(|spec| spec.trim().parse().ok())
        .filter(|frame| *frame > 0)
        .unwrap_or(STARTUP_SHOT_FRAME)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Open {
    AiSettings,
    DeckBox,
    Decks,
    DeckEditor(EditorSeed),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSeed {
    New,
    Draft,
    Pool(String),
}

pub fn parse_open(spec: &str) -> Option<Open> {
    let mut parts = spec.trim().splitn(3, ':');
    let what = parts.next()?;
    if what == "ai-settings" {
        return parts.next().is_none().then_some(Open::AiSettings);
    }
    if what == "deck-box" {
        return parts.next().is_none().then_some(Open::DeckBox);
    }
    if what == "decks" {
        return parts.next().is_none().then_some(Open::Decks);
    }
    if what != "deck-editor" {
        return None;
    }
    let seed = match (parts.next(), parts.next()) {
        (None, _) | (Some("new"), None) => EditorSeed::New,
        (Some("draft"), None) => EditorSeed::Draft,
        (Some("pool"), Some(slug)) if !slug.is_empty() => EditorSeed::Pool(slug.to_string()),
        _ => return None,
    };
    Some(Open::DeckEditor(seed))
}

pub fn parse_window(spec: &str) -> Option<(u32, u32)> {
    let (w, h) = spec.trim().split_once(['x', 'X', '×'])?;
    let w: u32 = w.trim().parse().ok()?;
    let h: u32 = h.trim().parse().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

pub fn shot_path(shot: &Path, logical: Vec2) -> PathBuf {
    let stem = shot
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("shot");
    let ext = shot
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("png");
    let name = format!(
        "{}x{}-{stem}.{ext}",
        logical.x.round() as u32,
        logical.y.round() as u32
    );
    shot.with_file_name(name)
}

#[derive(Resource, Debug, Default)]
pub struct Harness {
    pub shot: Option<PathBuf>,
    pub startup_pending: bool,
    pub taken: u32,
    pub open: Option<Open>,
    pub shot_frame: u32,
}

impl Harness {
    pub fn from_env() -> Self {
        let shot = std::env::var(SHOT_VAR).ok().map(PathBuf::from);
        let open = std::env::var(OPEN_VAR).ok().and_then(|spec| {
            let parsed = parse_open(&spec);
            if parsed.is_none() {
                warn!("{OPEN_VAR}={spec:?} names nothing kai can open; it stays closed");
            }
            parsed
        });
        Self {
            startup_pending: shot.is_some(),
            shot,
            taken: 0,
            open,
            shot_frame: parse_shot_frame(std::env::var(SHOT_FRAME_VAR).ok().as_deref()),
        }
    }

    pub fn destination(&self, logical: Vec2) -> PathBuf {
        match &self.shot {
            Some(shot) => shot_path(shot, logical),
            None => shot_path(Path::new("kai.png"), logical),
        }
    }
}

fn requested_resolution() -> Option<WindowResolution> {
    let spec = std::env::var(WINDOW_VAR).ok()?;
    match parse_window(&spec) {
        Some((w, h)) => Some(WindowResolution::new(w, h)),
        None => {
            warn!("{WINDOW_VAR}={spec:?} is not WxH; the default window size stays");
            None
        }
    }
}

fn primary_window() -> Window {
    let resolution = requested_resolution().unwrap_or_default();
    Window {
        title: "kai".into(),
        name: Some("kai".into()),
        resolution,
        #[cfg(target_os = "android")]
        mode: bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
        #[cfg(target_os = "android")]
        resizable: false,
        #[cfg(target_arch = "wasm32")]
        fit_canvas_to_parent: true,
        ..default()
    }
}

#[bevy_main]
pub fn main() {
    if let Some(plan) = crate::autoplay::from_env_and_args() {
        if let Err(error) = crate::autoplay::install(&plan, true) {
            eprintln!("autoplay plan refused: {error}");
            return;
        }
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(primary_window()),
                ..default()
            })
            .set(bevy::log::LogPlugin {
                custom_layer: crate::telemetry::layer,
                ..default()
            }),
    )
    .add_plugins(CardTablePlugin)
    .add_plugins(crate::table::hud::ResponsivePlugin)
    .add_plugins(crate::settings::SettingsPlugin)
    .add_plugins(crate::menu::MenuPlugin)
    .add_plugins(crate::theme::ThemePlugin)
    .add_plugins(crate::telemetry::TelemetryPlugin)
    .add_plugins(crate::autoplay::AutoplayPlugin);
    crate::deck::catalog::register(&mut app);
    crate::deck::editor::register(&mut app);
    crate::deck::exchange::register(&mut app);
    app.add_systems(Update, (redeal_on_request, crate::net::route_drops));
    app.insert_resource(Harness::from_env())
        .add_systems(Update, (open_on_request, take_shots).chain());
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    app.add_systems(Update, crate::os::icon::apply);
    #[cfg(target_arch = "wasm32")]
    crate::net::page::show_panics();
    app.run();
}

fn open_on_request(
    mut harness: ResMut<Harness>,
    frames: Res<bevy::diagnostic::FrameCount>,
    mut menu: ResMut<crate::menu::Menu>,
    mut choice: ResMut<crate::net::TableChoice>,
    mut editor: ResMut<crate::deck::editor::DeckEditor>,
    mut ai: ResMut<crate::ai::seat::AiLobby>,
) {
    if frames.0 < OPEN_FRAME {
        return;
    }
    let seed = match harness.open.take() {
        Some(Open::AiSettings) => {
            menu.open_lobby(crate::net::TableGame::FreeForm);
            crate::menu::ai_setup::open(&mut menu, &mut ai, true);
            return;
        }
        Some(Open::DeckEditor(seed)) => seed,
        Some(Open::DeckBox) => {
            choice.game = crate::net::TableGame::Riftbound;
            menu.open_lobby(crate::net::TableGame::Riftbound);
            menu.open_sheet(crate::menu::Sheet::DeckBox(crate::menu::DeckSeat::Mine));
            return;
        }
        Some(Open::Decks) => {
            menu.open_decks();
            return;
        }
        None => return,
    };
    use crate::deck::editor::{Draft, Origin, NEW_LABEL};
    let draft = match seed {
        EditorSeed::New => Some(Draft::new(NEW_LABEL, Origin::New)),
        EditorSeed::Draft => editor.draft.take(),
        EditorSeed::Pool(slug) => match crate::deck::pool::deck(&slug) {
            Ok(deck) => {
                let label = crate::deck::pool::of_slug(&slug)
                    .map(|found| crate::menu::decks::copy_label(&found.label))
                    .unwrap_or(slug.clone());
                Some(Draft::from_deck(deck, &label, Origin::Pool(slug)))
            }
            Err(error) => {
                warn!("{OPEN_VAR}: {error}");
                None
            }
        },
    };
    let Some(draft) = draft else {
        warn!("{OPEN_VAR}: nothing to open");
        return;
    };
    choice.game = crate::net::TableGame::Riftbound;
    menu.open_lobby(crate::net::TableGame::Riftbound);
    crate::deck::editor::open(&mut editor, &mut menu, draft);
}

fn take_shots(
    mut commands: Commands,
    mut harness: ResMut<Harness>,
    keys: Res<ButtonInput<KeyCode>>,
    frames: Res<bevy::diagnostic::FrameCount>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let startup = harness.startup_pending && frames.0 >= harness.shot_frame.max(STARTUP_SHOT_FRAME);
    if !startup && !keys.just_pressed(SHOT_KEY) {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let logical = window.resolution.size();
    let path = if startup {
        harness.startup_pending = false;
        harness
            .shot
            .clone()
            .unwrap_or_else(|| harness.destination(logical))
    } else {
        harness.destination(logical)
    };
    harness.taken += 1;
    info!(
        "shot {} at {}x{} → {}",
        harness.taken,
        logical.x.round() as u32,
        logical.y.round() as u32,
        path.display()
    );
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

fn redeal_on_request(
    mut requests: MessageReader<Redeal>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut generation: ResMut<DealGeneration>,
    mut art: ResMut<ArtCache>,
    tuning: Res<Tuning>,
    info: Res<SessionInfo>,
) {
    if requests.read().next().is_none() {
        return;
    }
    if info.active() {
        return;
    }
    let mut fresh = Table::new();
    for face in sample_faces(tuning.foil_chance, &mut art) {
        fresh.add_face(PlayerId(0), Zone::Hand, face);
    }
    table.0 = fresh;
    mirror.solo_exhausted.clear();
    generation.0 += 1;
}

#[cfg(target_arch = "wasm32")]
pub fn sample_faces(_foil_chance: f32, _art: &mut ArtCache) -> Vec<CardFace> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn sample_faces(foil_chance: f32, art: &mut ArtCache) -> Vec<CardFace> {
    let Some(dir) = crate::os::paths::store_dir() else {
        return Vec::new();
    };
    match store_faces(&dir, foil_chance, art) {
        Ok(faces) => faces,
        Err(error) => {
            warn!("no spirit store hand ({error}); the table stays empty");
            Vec::new()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn store_faces(
    store_dir: &std::path::Path,
    foil_chance: f32,
    art: &mut ArtCache,
) -> Result<Vec<CardFace>, Box<dyn std::error::Error>> {
    let store = BlobStore::open(store_dir)?;
    let ref_path = store.root().join("refs").join("hob");
    if !ref_path.is_file() {
        return Err("spirit store holds no hob set".into());
    }
    let ref_text = std::fs::read_to_string(ref_path)?;
    let manifest_hash = BlobHash::parse(&ref_text).ok_or("ref is not a blob hash")?;
    let manifest: Manifest =
        agni_sim::abi::decode(&store.get(manifest_hash)?).ok_or("manifest does not decode")?;
    if manifest.cards.is_empty() {
        return Err(format!("manifest for {} lists no cards", manifest.set).into());
    }

    let seed = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64;
    let mut rng = Rng::from_seed(seed);
    let deal = hand_size(manifest.cards.len());
    let mut picks: Vec<usize> = (0..manifest.cards.len()).collect();
    for i in (1..picks.len()).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        picks.swap(i, j);
    }
    picks.truncate(deal);

    let foil_threshold = (foil_chance.clamp(0.0, 1.0) * 10_000.0) as u32;
    let mut faces = Vec::with_capacity(deal);
    for index in picks {
        let card = &manifest.cards[index];
        let image_hash =
            BlobHash::parse(&card.image).ok_or_else(|| format!("bad hash for {}", card.name))?;
        art.insert(&card.name, store.get(image_hash)?);
        let foil = rng.below(10_000) < foil_threshold;
        faces.push(CardFace {
            name: card.name.clone(),
            tint: ART_TINT,
            foil,
            kind: None,
            energy: None,
            power: None,
            might: None,
            domain: Vec::new(),
        });
    }
    Ok(faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_spec_is_width_by_height() {
        assert_eq!(parse_window("1280x800"), Some((1280, 800)));
        assert_eq!(parse_window(" 360X800 "), Some((360, 800)));
        assert_eq!(parse_window("800×360"), Some((800, 360)));
        assert_eq!(parse_window("1280"), None);
        assert_eq!(parse_window("0x800"), None);
        assert_eq!(parse_window("wide x tall"), None);
    }

    #[test]
    fn the_open_hook_names_the_deck_editor_and_its_seed() {
        assert_eq!(
            parse_open("deck-editor"),
            Some(Open::DeckEditor(EditorSeed::New))
        );
        assert_eq!(
            parse_open("deck-editor:new"),
            Some(Open::DeckEditor(EditorSeed::New))
        );
        assert_eq!(
            parse_open(" deck-editor:draft "),
            Some(Open::DeckEditor(EditorSeed::Draft))
        );
        assert_eq!(
            parse_open("deck-editor:pool:lillia-house"),
            Some(Open::DeckEditor(EditorSeed::Pool("lillia-house".into())))
        );
        assert_eq!(parse_open("deck-editor:pool:"), None);
        assert_eq!(parse_open("deck-box"), Some(Open::DeckBox));
        assert_eq!(parse_open("deck-box:ai"), None);
        assert_eq!(parse_open("decks"), Some(Open::Decks));
        assert_eq!(parse_open("decks:x"), None);
        assert_eq!(parse_open("ai-settings"), Some(Open::AiSettings));
        assert_eq!(parse_open("ai-settings:key"), None);
        assert_eq!(parse_shot_frame(None), STARTUP_SHOT_FRAME);
        assert_eq!(parse_shot_frame(Some(" 90 ")), 90);
        assert_eq!(parse_shot_frame(Some("0")), STARTUP_SHOT_FRAME);
        assert_eq!(parse_shot_frame(Some("later")), STARTUP_SHOT_FRAME);
        assert_eq!(parse_open("deck-editor:saved:abc"), None);
        assert_eq!(parse_open("settings"), None);
    }

    #[test]
    fn a_shot_is_named_by_its_size_beside_the_requested_path() {
        let shot = Path::new("/shots/table-default.png");
        assert_eq!(
            shot_path(shot, Vec2::new(1280.0, 800.0)),
            PathBuf::from("/shots/1280x800-table-default.png")
        );
        assert_eq!(
            shot_path(shot, Vec2::new(360.0, 800.0)),
            PathBuf::from("/shots/360x800-table-default.png")
        );
        assert_eq!(
            shot_path(Path::new("lobby"), Vec2::new(1024.0, 768.0)),
            PathBuf::from("1024x768-lobby.png")
        );
    }

    #[test]
    fn without_a_requested_path_the_shot_lands_in_the_working_directory() {
        let harness = Harness::default();
        assert_eq!(
            harness.destination(Vec2::new(800.0, 360.0)),
            PathBuf::from("800x360-kai.png")
        );
        assert!(!harness.startup_pending);
    }
}
