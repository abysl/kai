use agni_core::{CardFace, PlayerId};
use agni_deck::{expand, total};
use agni_sim::wire::DealGroup;
use bevy::prelude::*;
use bevy_egui::egui;
use parking_lot::Mutex;
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};

use crate::net::TableGame;
use crate::render::art::ArtCache;
use crate::table::MySeat;

pub const PLACEHOLDER_TINT: [u8; 3] = [107, 112, 128];

static RESULTS: Mutex<Vec<Result<ResolvedImport, String>>> = Mutex::new(Vec::new());
#[cfg(not(target_arch = "wasm32"))]
static ART_STATUS: Mutex<Option<String>> = Mutex::new(None);
#[cfg(not(target_arch = "wasm32"))]
static ART_BUSY: AtomicBool = AtomicBool::new(false);
#[cfg(not(target_arch = "wasm32"))]
static FULL_SET_LANDED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ImportedDeck {
    Riftbound(agni_riftbound::ResolvedDeck),
    Mtg(agni_mtg::ResolvedDeck),
}

pub struct DeckCard {
    pub key: String,
    pub name: String,
    pub image_url: Option<String>,
    pub kind: Option<String>,
    pub energy: Option<u8>,
    pub power: Option<u8>,
    pub might: Option<u8>,
    pub domain: Vec<String>,
}

impl DeckCard {
    pub fn face(&self) -> CardFace {
        let mut face = placeholder_face(&self.name).with_cost(self.energy, self.power);
        face.kind = self.kind.clone();
        face.might = self.might;
        face.domain = self.domain.clone();
        face
    }
}

impl ImportedDeck {
    pub fn game(&self) -> TableGame {
        match self {
            Self::Riftbound(_) => TableGame::Riftbound,
            Self::Mtg(_) => TableGame::Mtg,
        }
    }

    pub fn cards(&self) -> Vec<DeckCard> {
        let mut cards = Vec::new();
        match self {
            Self::Riftbound(deck) => {
                let mut push = |card: &agni_riftbound::ResolvedCard| {
                    cards.push(DeckCard {
                        key: card.riftbound_id.clone(),
                        name: card.name.clone(),
                        image_url: card.image_url.clone(),
                        kind: card.kind.clone(),
                        energy: card.energy,
                        power: card.power,
                        might: card.might,
                        domain: card.domain.clone(),
                    })
                };
                deck.legend.iter().for_each(&mut push);
                deck.chosen_champion.iter().for_each(&mut push);
                for zone in [
                    &deck.main_deck,
                    &deck.runes,
                    &deck.battlefields,
                    &deck.sideboard,
                ] {
                    zone.iter().for_each(|entry| push(&entry.card));
                }
            }
            Self::Mtg(deck) => {
                let mut push = |card: &agni_mtg::ResolvedCard| {
                    cards.push(DeckCard {
                        key: card.name.clone(),
                        name: card.name.clone(),
                        image_url: card.image_url.clone(),
                        kind: None,
                        energy: None,
                        power: None,
                        might: None,
                        domain: Vec::new(),
                    })
                };
                deck.commander.iter().for_each(&mut push);
                for zone in [&deck.main_deck, &deck.sideboard] {
                    zone.iter().for_each(|entry| push(&entry.card));
                }
            }
        }
        cards
    }

    pub fn summary(&self) -> Vec<String> {
        match self {
            Self::Riftbound(deck) => {
                let mut lines = Vec::new();
                if let Some(legend) = &deck.legend {
                    lines.push(format!("legend: {}", legend.name));
                }
                if let Some(champion) = &deck.chosen_champion {
                    lines.push(format!("champion: {}", champion.name));
                }
                lines.push(format!(
                    "main {} · runes {} · battlefields {} · sideboard {}",
                    total(&deck.main_deck),
                    total(&deck.runes),
                    total(&deck.battlefields),
                    total(&deck.sideboard),
                ));
                lines
            }
            Self::Mtg(deck) => {
                let mut lines = Vec::new();
                if let Some(commander) = &deck.commander {
                    lines.push(format!("commander: {}", commander.name));
                }
                lines.push(format!(
                    "library {} · sideboard {}",
                    total(&deck.main_deck),
                    total(&deck.sideboard),
                ));
                lines
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedImport {
    pub deck: ImportedDeck,
    pub unresolved: Vec<(String, String)>,
    pub code: Option<String>,
    pub title: Option<String>,
}

impl ResolvedImport {
    pub fn label(&self) -> String {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| crate::deck::history::label(&self.deck))
    }

    pub fn riftbound(&self) -> Option<&agni_riftbound::ResolvedDeck> {
        match &self.deck {
            ImportedDeck::Riftbound(deck) => Some(deck),
            ImportedDeck::Mtg(_) => None,
        }
    }

    pub fn report(&self) -> Option<agni_riftbound::legality::Report> {
        self.riftbound().map(|deck| {
            agni_riftbound::legality::check(deck, agni_riftbound::legality::Mode::Standard)
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ImportAction {
    Save {
        deck: ImportedDeck,
        label: String,
        source: String,
    },
    Edit {
        deck: agni_riftbound::ResolvedDeck,
        label: String,
        source: String,
        unresolved: Vec<String>,
    },
}

#[derive(Resource, Default)]
pub struct ImportPanel {
    pub paste: String,
    pub busy: bool,
    pub note: Option<String>,
    pub error: Option<String>,
    pub resolved: Option<ResolvedImport>,
    pub report: Option<(agni_riftbound::legality::Report, String)>,
    pub auto_deal: bool,
    pub source: Option<String>,
    pub qr: Option<String>,
    pub flash: Option<(String, f64)>,
}

pub const FLASH_SECS: f64 = 2.0;
pub const PASTE_ROWS: usize = 4;

impl ImportPanel {
    pub fn set_resolved(&mut self, resolved: ResolvedImport) {
        self.report = resolved.report().map(|report| {
            let text = findings_text(&report);
            (report, text)
        });
        self.resolved = Some(resolved);
    }

    pub fn clear_resolved(&mut self) {
        self.resolved = None;
        self.report = None;
    }

    pub fn set_flash(&mut self, context: &egui::Context, text: String) {
        self.flash = Some((text, context.input(|input| input.time)));
    }

    pub fn flash_text(&self, context: &egui::Context) -> Option<String> {
        let (text, at) = self.flash.as_ref()?;
        let now = context.input(|input| input.time);
        if now - at > FLASH_SECS {
            return None;
        }
        context.request_repaint_after(std::time::Duration::from_millis(250));
        Some(text.clone())
    }
}

pub struct SeatedDeckRecord {
    pub seat: PlayerId,
    pub deck: ImportedDeck,
    pub faces: HashMap<String, CardFace>,
    pub battlefield: Option<usize>,
    pub battlefield_played: bool,
}

#[derive(Resource, Default)]
pub struct SeatedDeck(pub Option<SeatedDeckRecord>);

#[derive(Message, Debug, Clone, Copy)]
pub struct DealDeckRequested;

#[derive(Message, Debug, Clone, Copy)]
pub struct PlaceBattlefieldRequested;

fn face_for(record: &SeatedDeckRecord, key: &str, name: &str) -> CardFace {
    record
        .faces
        .get(key)
        .cloned()
        .unwrap_or_else(|| placeholder_face(name))
}

pub fn placeholder_face(name: &str) -> CardFace {
    CardFace {
        name: name.to_string(),
        tint: PLACEHOLDER_TINT,
        foil: false,
        kind: None,
        energy: None,
        power: None,
        might: None,
        domain: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DealSetup {
    pub players: u8,
    pub seat: u8,
    pub first_player: u8,
    pub generation: u32,
    pub battlefields: u8,
}

impl DealSetup {
    pub fn contribution(&self, pick: usize) -> agni_riftbound::Battlefields {
        agni_riftbound::contribution_of(
            self.battlefields,
            self.players,
            self.seat,
            self.first_player,
            pick,
        )
    }

    pub fn placed(&self) -> usize {
        match self.contribution(0) {
            agni_riftbound::Battlefields::None => 0,
            agni_riftbound::Battlefields::One(_) => 1,
            agni_riftbound::Battlefields::Many { count, .. } => count,
            agni_riftbound::Battlefields::All => usize::MAX,
        }
    }
}

pub fn battlefield_choice(
    record: &SeatedDeckRecord,
    held: usize,
    setup: Option<DealSetup>,
) -> agni_riftbound::Battlefields {
    if held == 0 {
        return agni_riftbound::Battlefields::None;
    }
    let entries = crate::deck::battlefield::options(record);
    let pick = record
        .battlefield
        .map(|entry| crate::deck::battlefield::expanded_index(entries, entry))
        .unwrap_or(0)
        .min(held - 1);
    match setup {
        Some(setup) => setup.contribution(pick),
        None => agni_riftbound::Battlefields::One(pick),
    }
}

pub fn deal_plan_for(record: &SeatedDeckRecord) -> Vec<DealGroup> {
    deal_plan_for_setup(record, None)
}

pub fn deal_plan_for_setup(record: &SeatedDeckRecord, setup: Option<DealSetup>) -> Vec<DealGroup> {
    riftbound_plan(record, setup, Part::Whole)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Whole,
    Body,
    Battlefields,
}

pub fn body_plan(record: &SeatedDeckRecord, setup: Option<DealSetup>) -> Vec<DealGroup> {
    let part = if crate::deck::battlefield::asks(record) {
        Part::Body
    } else {
        Part::Whole
    };
    riftbound_plan(record, setup, part)
}

pub fn battlefield_plan(record: &SeatedDeckRecord, setup: Option<DealSetup>) -> Vec<DealGroup> {
    riftbound_plan(record, setup, Part::Battlefields)
}

pub fn battlefield_only(groups: &[DealGroup]) -> bool {
    !groups.is_empty()
        && groups.iter().all(|group| {
            matches!(&group.target, agni_sim::wire::DealTarget::Spread(prefix) if prefix == agni_riftbound::BATTLEFIELD_PREFIX)
        })
}

fn riftbound_plan(
    record: &SeatedDeckRecord,
    setup: Option<DealSetup>,
    part: Part,
) -> Vec<DealGroup> {
    match &record.deck {
        ImportedDeck::Riftbound(deck) => {
            let face = |card: &agni_riftbound::ResolvedCard| {
                face_for(record, &card.riftbound_id, &card.name)
            };
            let battlefields = expand(&deck.battlefields, face);
            let choice = match part {
                Part::Body => agni_riftbound::Battlefields::None,
                Part::Whole | Part::Battlefields => {
                    battlefield_choice(record, battlefields.len(), setup)
                }
            };
            if part == Part::Battlefields {
                return agni_riftbound::deal_plan_with(
                    &agni_riftbound::DeckFaces {
                        legend: None,
                        chosen_champion: None,
                        main_deck: Vec::new(),
                        runes: Vec::new(),
                        battlefields,
                        sideboard: Vec::new(),
                    },
                    choice,
                );
            }
            agni_riftbound::deal_plan_with(
                &agni_riftbound::DeckFaces {
                    legend: deck.legend.as_ref().map(face),
                    chosen_champion: deck.chosen_champion.as_ref().map(face),
                    main_deck: expand(&deck.main_deck, face),
                    runes: expand(&deck.runes, face),
                    battlefields,
                    sideboard: expand(&deck.sideboard, face),
                },
                choice,
            )
        }
        ImportedDeck::Mtg(deck) => {
            let face = |card: &agni_mtg::ResolvedCard| face_for(record, &card.name, &card.name);
            agni_mtg::deal_plan(&agni_mtg::DeckFaces {
                commander: deck.commander.as_ref().map(face),
                library: expand(&deck.main_deck, face),
            })
        }
    }
}

fn small(value: &serde_json::Value) -> Option<u8> {
    value.as_u64().and_then(|value| u8::try_from(value).ok())
}

fn riftbound_card(value: &serde_json::Value) -> Option<agni_riftbound::ResolvedCard> {
    Some(agni_riftbound::ResolvedCard {
        name: value["name"].as_str()?.to_string(),
        riftbound_id: value["riftbound_id"].as_str()?.to_string(),
        image_url: value["image_url"].as_str().map(str::to_string),
        kind: value["kind"].as_str().map(str::to_string),
        energy: small(&value["energy"]),
        power: small(&value["power"]),
        might: small(&value["might"]),
        domain: value["domain"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        tags: value["tags"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        signature: value["signature"].as_bool().unwrap_or(false),
    })
}

fn riftbound_entries(value: &serde_json::Value) -> Vec<agni_riftbound::DeckEntry> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(agni_riftbound::DeckEntry {
                        card: riftbound_card(item)?,
                        count: item["count"].as_u64()? as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_reply(status: u16, body: &str) -> Result<ResolvedImport, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| body.trim().to_string())?;
    if status != 200 {
        return Err(value["error"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| body.trim().to_string()));
    }
    let deck = &value["deck"];
    let deck = agni_riftbound::ResolvedDeck {
        legend: riftbound_card(&deck["legend"]),
        chosen_champion: riftbound_card(&deck["chosen_champion"]),
        main_deck: riftbound_entries(&deck["main_deck"]),
        runes: riftbound_entries(&deck["runes"]),
        battlefields: riftbound_entries(&deck["battlefields"]),
        sideboard: riftbound_entries(&deck["sideboard"]),
    };
    let unresolved = value["unresolved"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some((
                        item["identifier"].as_str()?.to_string(),
                        item["reason"].as_str().unwrap_or("unresolved").to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let code = value["code"].as_str().map(str::to_string);
    let title = value["title"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string);
    Ok(ResolvedImport {
        deck: ImportedDeck::Riftbound(deck),
        unresolved,
        code,
        title,
    })
}

pub fn face_map(deck: &ImportedDeck) -> HashMap<String, CardFace> {
    let mut faces = HashMap::new();
    for card in deck.cards() {
        faces.entry(card.key.clone()).or_insert_with(|| card.face());
    }
    faces
}

pub fn stage_store_art(deck: &ImportedDeck, cache: &mut ArtCache) {
    let game = deck.game();
    for card in deck.cards() {
        if cache.has(&card.name) {
            continue;
        }
        if let Some(bytes) = art_bytes(game, &card.key) {
            cache.insert(&card.name, bytes);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn art_from_store(dir: &std::path::Path, riftbound_id: &str) -> Option<Vec<u8>> {
    use spirit_core::{BlobHash, BlobStore};
    let manifest = agni_importers::riftbound::ingest::load_manifest(dir).ok()??;
    let card = manifest
        .cards
        .iter()
        .find(|card| card.riftbound_id.eq_ignore_ascii_case(riftbound_id))?;
    let hash = BlobHash::parse(&card.image)?;
    BlobStore::open(dir).ok()?.get(hash).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn art_bytes(game: TableGame, key: &str) -> Option<Vec<u8>> {
    let dir = crate::os::paths::store_dir()?;
    match game {
        TableGame::Riftbound => art_from_store(&dir, key),
        TableGame::Mtg => agni_importers::mtg::ingest::art_from_store(&dir, key),
        TableGame::FreeForm => None,
    }
}

#[cfg(target_arch = "wasm32")]
fn art_bytes(game: TableGame, key: &str) -> Option<Vec<u8>> {
    match game {
        TableGame::Riftbound => crate::net::gateway::riftbound_art(key),
        TableGame::Mtg | TableGame::FreeForm => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn art_by_names(dir: &std::path::Path, names: &[String]) -> Vec<(String, Vec<u8>)> {
    use spirit_core::{BlobHash, BlobStore};
    let Ok(Some(manifest)) = agni_importers::riftbound::ingest::load_manifest(dir) else {
        return Vec::new();
    };
    let Ok(store) = BlobStore::open(dir) else {
        return Vec::new();
    };
    names
        .iter()
        .filter_map(|name| {
            let card = manifest.cards.iter().find(|card| &card.name == name)?;
            let hash = BlobHash::parse(&card.image)?;
            Some((name.clone(), store.get(hash).ok()?))
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn set_art_status(line: String) {
    *ART_STATUS.lock() = Some(line);
}

#[cfg(not(target_arch = "wasm32"))]
fn art_status_line() -> Option<String> {
    crate::render::art::status_line().or_else(|| ART_STATUS.lock().clone())
}

#[cfg(not(target_arch = "wasm32"))]
fn art_idle() -> bool {
    !ART_BUSY.load(Ordering::SeqCst)
}

#[cfg(any(test, not(target_arch = "wasm32")))]
pub(crate) fn art_requests(
    record: &SeatedDeckRecord,
    cache: &ArtCache,
) -> Vec<crate::render::art::ArtRequest> {
    let Some(game) = record.deck.game().art_game() else {
        return Vec::new();
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut wanted = Vec::new();
    for card in record.deck.cards() {
        if cache.has(&card.name) || !seen.insert(card.key.to_ascii_lowercase()) {
            continue;
        }
        wanted.push(match record.deck {
            ImportedDeck::Riftbound(_) => {
                crate::render::art::ArtRequest::by_id(game, card.key, card.name)
            }
            ImportedDeck::Mtg(_) => crate::render::art::ArtRequest::by_name(game, card.name),
        });
    }
    wanted
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn prefetch_deck_art(record: &SeatedDeckRecord, cache: &ArtCache) {
    crate::render::art::enqueue(art_requests(record, cache));
}

#[cfg(not(target_arch = "wasm32"))]
fn start_full_ingest() {
    if ART_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(dir) = crate::os::paths::store_dir() else {
        ART_BUSY.store(false, Ordering::SeqCst);
        set_art_status("no spirit store on this device — card art cannot land".into());
        return;
    };
    std::thread::spawn(move || {
        set_art_status("fetching the riftbound card list…".into());
        let outcome = agni_importers::riftbound::ingest::ingest(&dir, |progress| {
            set_art_status(match progress.stage {
                "pages" => format!(
                    "fetching card list page {}/{}…",
                    progress.done, progress.total
                ),
                _ => format!("fetching card images {}/{}…", progress.done, progress.total),
            });
        });
        match outcome {
            Ok(done) => {
                set_art_status(format!(
                    "riftbound set ingested — {} cards, {} images fetched, {} reused",
                    done.cards, done.images_fetched, done.images_reused
                ));
                FULL_SET_LANDED.store(true, Ordering::SeqCst);
            }
            Err(error) => {
                set_art_status(format!(
                    "set ingest failed: {error} — running it again resumes where it stopped"
                ));
            }
        }
        ART_BUSY.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_store_art(
    table: Res<crate::table::GameTable>,
    seated: Res<SeatedDeck>,
    mut cache: ResMut<ArtCache>,
) {
    if !FULL_SET_LANDED.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(record) = seated.0.as_ref() {
        stage_store_art(&record.deck, &mut cache);
    }
    let Some(dir) = crate::os::paths::store_dir() else {
        return;
    };
    let artless: Vec<String> = table
        .0
        .cards()
        .iter()
        .filter(|card| !card.face.is_hidden() && !cache.has(&card.face.name))
        .map(|card| card.face.name.clone())
        .collect();
    let landed = art_by_names(&dir, &artless);
    if !landed.is_empty() {
        cache.extend(landed);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn dispatch_riftbound(query: agni_importers::riftbound::query::DeckQuery) {
    std::thread::spawn(move || {
        let result = run_riftbound_query(&query);
        RESULTS.lock().push(result);
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_riftbound_query(
    query: &agni_importers::riftbound::query::DeckQuery,
) -> Result<ResolvedImport, String> {
    use agni_importers::riftbound::catalog::{Cached, Layered};
    use agni_importers::riftbound::ingest::load_catalog;
    use agni_importers::riftbound::query::UreqFetch;
    use agni_importers::riftbound::riftcodex::Riftcodex;
    let dir = crate::os::paths::store_dir().ok_or("no spirit store path available")?;
    let mut fetch = UreqFetch::new();
    match load_catalog(&dir) {
        Ok(Some(catalog)) => resolve_with(
            query,
            &mut fetch,
            &mut Cached::new(Layered {
                first: catalog,
                second: Riftcodex::new(),
            }),
        ),
        _ => resolve_with(query, &mut fetch, &mut Cached::new(Riftcodex::new())),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_with(
    query: &agni_importers::riftbound::query::DeckQuery,
    fetch: &mut dyn agni_importers::riftbound::query::Fetch,
    cards: &mut dyn agni_importers::deck::CardLookup<agni_importers::riftbound::Riftbound>,
) -> Result<ResolvedImport, String> {
    let reply = agni_importers::riftbound::query::resolve_query(query, fetch, cards);
    parse_reply(reply.status, &reply.body.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn dispatch_mtg(text: String) {
    std::thread::spawn(move || {
        let result = run_mtg_query(&text);
        RESULTS.lock().push(result);
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_mtg_query(text: &str) -> Result<ResolvedImport, String> {
    use agni_importers::mtg::catalog::{Cached, Layered};
    use agni_importers::mtg::ingest::load_catalog;
    use agni_importers::mtg::resolve::{is_empty, resolve};
    use agni_importers::mtg::scryfall_named::Scryfall;
    let parsed = agni_importers::mtg::parse_text(text).map_err(|error| error.to_string())?;
    let dir = crate::os::paths::store_dir().ok_or("no spirit store path available")?;
    let mut remote = Cached::new(Scryfall::new());
    let resolution = match load_catalog(&dir).ok().flatten() {
        Some(local) => {
            let mut layered = Layered {
                first: local,
                second: remote,
            };
            resolve(&parsed, &mut layered)
        }
        None => resolve(&parsed, &mut remote),
    }
    .map_err(|error| error.to_string())?;
    if is_empty(&resolution) {
        return Err("no cards resolved — scryfall named none of them".into());
    }
    Ok(ResolvedImport {
        deck: ImportedDeck::Mtg(resolution.deck),
        unresolved: resolution
            .unresolved
            .into_iter()
            .map(|entry| (entry.identifier, entry.reason))
            .collect(),
        code: None,
        title: None,
    })
}

#[cfg(target_arch = "wasm32")]
fn dispatch_web(query_string: String) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = match crate::net::gateway::gateway_base() {
            None => Err(
                "no gateway reachable — deck resolution in the browser rides the gateway bridge"
                    .to_string(),
            ),
            Some(base) => {
                let url = format!("{base}/gateway/resolve/deck?{query_string}");
                match crate::net::gateway::fetch_status_text(&url).await {
                    Ok((status, body)) => parse_reply(status, &body),
                    Err(error) => Err(error),
                }
            }
        };
        RESULTS.lock().push(result);
    });
}

fn dispatch_paste(game: TableGame, text: String) {
    match game {
        TableGame::Riftbound => {
            #[cfg(not(target_arch = "wasm32"))]
            dispatch_riftbound(agni_importers::riftbound::query::DeckQuery::Text(text));
            #[cfg(target_arch = "wasm32")]
            dispatch_web(format!(
                "text={}",
                agni_importers::naming::encode_component(&text)
            ));
        }
        TableGame::Mtg => {
            #[cfg(not(target_arch = "wasm32"))]
            dispatch_mtg(text);
            #[cfg(target_arch = "wasm32")]
            {
                let _ = text;
                RESULTS.lock().push(Err(
                    "MTG deck resolution has no gateway route yet — import from desktop for now"
                        .into(),
                ));
            }
        }
        TableGame::FreeForm => {}
    }
}

fn dispatch_url(game: TableGame, url: String) {
    if game != TableGame::Riftbound {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    dispatch_riftbound(agni_importers::riftbound::query::DeckQuery::Url(url));
    #[cfg(target_arch = "wasm32")]
    dispatch_web(format!(
        "url={}",
        agni_importers::naming::encode_component(&url)
    ));
}

fn parses_locally(game: TableGame, text: &str) -> Result<usize, String> {
    match game {
        TableGame::Riftbound => agni_importers::riftbound::parse_any(text)
            .map(|parsed| parsed.entries.len())
            .map_err(|error| error.to_string()),
        TableGame::Mtg => agni_importers::mtg::parse_text(text)
            .map(|parsed| parsed.entries.len())
            .map_err(|error| error.to_string()),
        TableGame::FreeForm => Err("pick MTG or Riftbound before importing a deck".into()),
    }
}

pub fn collect_results(mut panel: ResMut<ImportPanel>) {
    let arrivals: Vec<Result<ResolvedImport, String>> = std::mem::take(&mut *RESULTS.lock());
    for arrival in arrivals {
        panel.busy = false;
        panel.note = None;
        match arrival {
            Ok(resolved) => {
                panel.error = None;
                panel.set_resolved(resolved);
            }
            Err(error) => panel.error = Some(error),
        }
    }
}

fn is_link(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

pub(crate) fn detect_game(text: &str) -> Option<TableGame> {
    if is_link(text) || agni_importers::riftbound::parse_any(text).is_ok() {
        return Some(TableGame::Riftbound);
    }
    if agni_importers::mtg::parse_text(text).is_ok() {
        return Some(TableGame::Mtg);
    }
    None
}

fn seat_resolved(
    deck: ImportedDeck,
    seat: PlayerId,
    seated: &mut SeatedDeck,
    cache: &mut ArtCache,
    source: &str,
) -> usize {
    seat_deck(deck, seat, seated, cache, Some(source))
}

pub fn seat_deck(
    deck: ImportedDeck,
    seat: PlayerId,
    seated: &mut SeatedDeck,
    cache: &mut ArtCache,
    remember_as: Option<&str>,
) -> usize {
    if let Some(source) = remember_as {
        crate::deck::history::remember(&deck, source);
    }
    stage_seat(deck, seat, seated, cache)
}

fn stage_seat(
    deck: ImportedDeck,
    seat: PlayerId,
    seated: &mut SeatedDeck,
    cache: &mut ArtCache,
) -> usize {
    stage_store_art(&deck, cache);
    let faces = face_map(&deck);
    let count = faces.len();
    let record = SeatedDeckRecord {
        seat,
        deck,
        faces,
        battlefield: None,
        battlefield_played: false,
    };
    #[cfg(not(target_arch = "wasm32"))]
    prefetch_deck_art(&record, cache);
    seated.0 = Some(record);
    count
}

pub fn apply_history(
    mut seat_requests: MessageReader<crate::deck::history::SeatSavedDeck>,
    mut forget_requests: MessageReader<crate::deck::history::ForgetSavedDeck>,
    my_seat: Res<MySeat>,
    mut seated: ResMut<SeatedDeck>,
    mut cache: ResMut<ArtCache>,
    mut panel: ResMut<ImportPanel>,
) {
    for request in seat_requests.read() {
        match crate::deck::history::store::recall(&request.game, request.ci) {
            Some(deck) => {
                let label = crate::deck::history::label(&deck);
                let faces = seat_resolved(deck, my_seat.0, &mut seated, &mut cache, "history");
                panel.auto_deal = true;
                panel.error = None;
                panel.note = Some(format!(
                    "seated {label} from history — {faces} faces staged"
                ));
            }
            None => {
                panel.error =
                    Some("that deck is in the list but its bytes are not held here yet".into());
            }
        }
    }
    for request in forget_requests.read() {
        if let Err(error) = crate::deck::history::store::forget(&request.game, request.ci) {
            panel.error = Some(format!("could not forget that deck: {error}"));
        }
    }
}

pub fn auto_deal(
    matchmaking: Res<crate::net::matchmaking::Queue>,
    mut panel: ResMut<ImportPanel>,
    seated: Res<SeatedDeck>,
    info: Res<crate::table::SessionInfo>,
    mirror: Res<crate::table::Mirror>,
    mut deal_requests: MessageWriter<DealDeckRequested>,
) {
    if !panel.auto_deal || matchmaking.active() {
        return;
    }
    let Some(record) = &seated.0 else {
        panel.auto_deal = false;
        return;
    };
    let table_game = crate::net::game_of_zones(&mirror.view.zones);
    if info.active() && record.deck.game() == table_game {
        deal_requests.write(DealDeckRequested);
        panel.auto_deal = false;
        panel.note = Some("deck dealt to your battlefield".into());
        return;
    }
    let waiting = format!(
        "deck seated — it deals itself once you join a {} table",
        record.deck.game().label()
    );
    if panel.note.as_deref() != Some(&waiting) {
        panel.note = Some(waiting);
    }
}

pub fn is_link_line(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .is_some_and(is_link)
}

pub const NOT_A_DECK: &str = "the clipboard holds neither a deck list, a deck code nor a deck link";
pub const EMPTY_CLIPBOARD: &str = "the clipboard is empty — copy a deck list, code or link first";

pub fn game_of_paste(text: &str) -> Option<TableGame> {
    let text = text.trim();
    if is_link_line(text) {
        return Some(TableGame::Riftbound);
    }
    detect_game(text)
}

pub fn begin_import_any(panel: &mut ImportPanel) {
    match game_of_paste(&panel.paste) {
        Some(game) => begin_import(panel, game),
        None if panel.paste.trim().is_empty() => {
            panel.error = Some(EMPTY_CLIPBOARD.into());
        }
        None => panel.error = Some(NOT_A_DECK.into()),
    }
}

pub fn begin_import(panel: &mut ImportPanel, game: TableGame) {
    let text = panel.paste.trim().to_string();
    if text.is_empty() {
        panel.error = Some(EMPTY_CLIPBOARD.into());
        return;
    }
    panel.error = None;
    if is_link_line(&text) && agni_importers::riftbound::link::code_in_url(&text).is_none() {
        if game != TableGame::Riftbound {
            panel.error =
                Some("deck links are Riftbound only — paste an MTG decklist instead".into());
            return;
        }
        panel.busy = true;
        panel.source = Some(format!("link:{text}"));
        panel.note = Some("fetching the deck link…".into());
        dispatch_url(game, text);
        return;
    }
    match parses_locally(game, &text) {
        Err(error) => panel.error = Some(error),
        Ok(entries) => {
            panel.busy = true;
            panel.source = Some("paste".into());
            panel.note = Some(format!("resolving {entries} entries…"));
            dispatch_paste(game, text);
        }
    }
}

pub fn import_status(ui: &mut egui::Ui, panel: &ImportPanel) {
    let tokens = crate::theme::tokens(ui.ctx());
    if let Some(error) = &panel.error {
        ui.colored_label(tokens.danger, error);
    } else if let Some(flash) = panel.flash_text(ui.ctx()) {
        ui.label(egui::RichText::new(flash).weak());
    } else if let Some(note) = panel
        .note
        .as_ref()
        .filter(|_| panel.busy || panel.resolved.is_some())
    {
        ui.label(egui::RichText::new(note).weak());
    }
}

pub fn result_panel(
    ui: &mut egui::Ui,
    panel: &mut ImportPanel,
    load_label: &str,
) -> Option<ImportAction> {
    let tokens = crate::theme::tokens(ui.ctx());
    let mut action = None;
    let mut status = None;
    if let Some(resolved) = &panel.resolved {
        ui.add_space(4.0);
        if let Some(title) = &resolved.title {
            ui.label(egui::RichText::new(title).strong());
        }
        let summary = resolved.deck.summary();
        let (first, rest) = summary
            .split_first()
            .map_or((None, &summary[..]), |(f, r)| (Some(f), r));
        ui.horizontal_wrapped(|ui| {
            if let Some(line) = first {
                ui.label(line);
            }
            if let Some((report, findings)) = &panel.report {
                crate::deck::rows::verdict_chip(ui, report.verdict).on_hover_text(findings);
            }
        });
        let source = panel.source.clone().unwrap_or_else(|| "import".into());
        ui.horizontal_wrapped(|ui| {
            if let Some(deck) = resolved.riftbound() {
                let load = ui.add(
                    egui::Button::new(egui::RichText::new(load_label).strong())
                        .fill(tokens.green.gamma_multiply(0.35)),
                );
                if load.clicked() {
                    action = Some(ImportAction::Edit {
                        deck: deck.clone(),
                        label: resolved.label(),
                        source: source.clone(),
                        unresolved: resolved
                            .unresolved
                            .iter()
                            .map(|(identifier, _)| identifier.clone())
                            .collect(),
                    });
                }
            }
            if ui.button(SAVE_LABEL).clicked() {
                action = Some(ImportAction::Save {
                    deck: resolved.deck.clone(),
                    label: resolved.label(),
                    source: source.clone(),
                });
            }
            if let Some(deck) = resolved.riftbound() {
                if let Some(copied) = crate::deck::exchange::share_menu(ui, deck, &mut panel.qr) {
                    status = Some(copied);
                }
            }
        });
        for line in rest {
            ui.label(line);
        }
        if !resolved.unresolved.is_empty() {
            let names: Vec<&str> = resolved
                .unresolved
                .iter()
                .map(|(identifier, _)| identifier.as_str())
                .collect();
            ui.colored_label(
                tokens.amber,
                format!(
                    "{} cards not found: {}",
                    resolved.unresolved.len(),
                    names.join(", ")
                ),
            );
        }
    }
    if let Some(status) = status {
        panel.set_flash(ui.ctx(), status);
    }
    action
}

pub const SAVE_LABEL: &str = "save to your decks";
pub const LOAD_LABEL: &str = "load into the editor";

pub fn findings_text(report: &agni_riftbound::legality::Report) -> String {
    if report.findings.is_empty() {
        return "every construction rule checks out".to_string();
    }
    report
        .findings
        .iter()
        .map(|finding| format!("{} — rule {}", finding.detail, finding.cite))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn art_note(record: &SeatedDeckRecord, cache: &ArtCache) -> Option<String> {
    let missing = record
        .faces
        .values()
        .filter(|face| !cache.has(&face.name))
        .count();
    (missing > 0).then(|| {
        format!("{missing} cards await art — they play as name placeholders until it lands")
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn full_set_controls(ui: &mut egui::Ui) {
    if ui
        .add_enabled(
            art_idle(),
            egui::Button::new("download full riftbound set (~1,500 images)"),
        )
        .clicked()
    {
        start_full_ingest();
    }
    if let Some(status) = art_status_line() {
        ui.label(egui::RichText::new(status).weak());
    }
}

#[cfg(target_arch = "wasm32")]
pub fn full_set_controls(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("art syncs via your gateways as they hold the card set").weak());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply_fixture() -> String {
        serde_json::json!({
            "game": "riftbound",
            "source": { "kind": "text", "value": "…" },
            "code": "CEBAIAAA",
            "title": "Sentinel Rush by Nobody",
            "deck": {
                "legend": { "name": "Vanguard Sentinel", "riftbound_id": "ogn-201-298", "image_url": "https://img.example/ogn-201-298.png", "tags": ["Vanguard"] },
                "chosen_champion": { "name": "Emberwing Scout", "riftbound_id": "ogn-007-298", "image_url": null, "signature": true },
                "main_deck": [
                    { "name": "Emberwing Scout", "riftbound_id": "ogn-007-298", "image_url": null, "count": 3 }
                ],
                "runes": [
                    { "name": "Ember Rune", "riftbound_id": "ogn-042-298", "image_url": null, "count": 12 }
                ],
                "battlefields": [],
                "sideboard": []
            },
            "unresolved": [
                { "identifier": "Completely Unknown", "reason": "no card in the catalog matches" }
            ]
        })
        .to_string()
    }

    fn mtg_deck() -> ImportedDeck {
        let card = |name: &str| agni_mtg::ResolvedCard {
            name: name.into(),
            image_url: Some(format!("https://cards.example/{name}.jpg")),
        };
        ImportedDeck::Mtg(agni_mtg::ResolvedDeck {
            commander: Some(card("Serelith, Tidebound Oracle")),
            main_deck: vec![
                agni_mtg::DeckEntry {
                    card: card("Thornspire Adept"),
                    count: 40,
                },
                agni_mtg::DeckEntry {
                    card: card("Mistfen Causeway"),
                    count: 20,
                },
            ],
            sideboard: vec![agni_mtg::DeckEntry {
                card: card("Cinderveil Ward"),
                count: 3,
            }],
        })
    }

    fn seat(deck: ImportedDeck) -> SeatedDeckRecord {
        let faces = face_map(&deck);
        SeatedDeckRecord {
            seat: PlayerId(0),
            deck,
            faces,
            battlefield: None,
            battlefield_played: false,
        }
    }

    #[test]
    fn a_gateway_reply_becomes_a_seated_riftbound_deck() {
        let resolved = parse_reply(200, &reply_fixture()).unwrap();
        assert_eq!(resolved.deck.game(), TableGame::Riftbound);
        let ImportedDeck::Riftbound(deck) = &resolved.deck else {
            panic!("the gateway route resolves riftbound decks");
        };
        assert_eq!(deck.legend.as_ref().unwrap().riftbound_id, "ogn-201-298");
        assert_eq!(deck.runes[0].count, 12);
        assert_eq!(resolved.code.as_deref(), Some("CEBAIAAA"));
        assert_eq!(resolved.unresolved.len(), 1);
        assert_eq!(resolved.title.as_deref(), Some("Sentinel Rush by Nobody"));
        assert_eq!(resolved.label(), "Sentinel Rush by Nobody");
        assert_eq!(deck.legend.as_ref().unwrap().tags, ["Vanguard"]);
        assert!(!deck.legend.as_ref().unwrap().signature);
        assert!(deck.chosen_champion.as_ref().unwrap().signature);
        assert!(deck.chosen_champion.as_ref().unwrap().tags.is_empty());
        assert!(deck.main_deck[0].card.tags.is_empty());
        assert!(!deck.main_deck[0].card.signature);
    }

    #[test]
    fn a_reply_without_a_title_labels_the_deck_by_its_legend() {
        let body = reply_fixture().replace("\"title\":\"Sentinel Rush by Nobody\",", "");
        let resolved = parse_reply(200, &body).unwrap();
        assert_eq!(resolved.title, None);
        assert_eq!(resolved.label(), "Vanguard Sentinel");
        let blank = reply_fixture().replace("Sentinel Rush by Nobody", "  ");
        assert_eq!(parse_reply(200, &blank).unwrap().title, None);
    }

    #[test]
    fn gateway_errors_surface_verbatim() {
        let refused = parse_reply(
            503,
            "{\"error\":\"this node has no riftbound catalog; run ingest-riftbound against its store\"}",
        )
        .unwrap_err();
        assert!(refused.contains("ingest-riftbound"));
        let raw = parse_reply(200, "not json at all").unwrap_err();
        assert_eq!(raw, "not json at all");
    }

    #[test]
    fn an_mtg_deck_deals_a_commander_and_a_shuffled_library() {
        let record = seat(mtg_deck());
        let plan = deal_plan_for(&record);
        assert_eq!(plan.len(), 2);
        assert_eq!(
            plan[0].target,
            agni_sim::wire::DealTarget::Zone("command".into())
        );
        assert_eq!(plan[0].faces.len(), 1);
        assert_eq!(
            plan[1].target,
            agni_sim::wire::DealTarget::Zone("library".into())
        );
        assert_eq!(plan[1].faces.len(), 60);
        assert!(plan[1].shuffle);
        assert_eq!(plan[1].draw, agni_mtg::OPENING_HAND_SIZE);
        assert!(plan[1]
            .faces
            .iter()
            .all(|face| face.tint == PLACEHOLDER_TINT && !face.is_hidden()));
    }

    #[test]
    fn a_riftbound_deck_still_deals_its_own_anatomy() {
        let resolved = parse_reply(200, &reply_fixture()).unwrap();
        let record = seat(resolved.deck);
        let plan = deal_plan_for(&record);
        let targets: Vec<String> = plan
            .iter()
            .map(|group| match &group.target {
                agni_sim::wire::DealTarget::Zone(name) => name.clone(),
                agni_sim::wire::DealTarget::Spread(prefix) => prefix.clone(),
            })
            .collect();
        assert_eq!(targets, ["legend", "champion", "rune-deck", "main-deck"]);
    }

    fn riftbound_with_battlefields(names: &[&str]) -> ImportedDeck {
        let card = |name: &str| agni_riftbound::ResolvedCard {
            name: name.to_string(),
            riftbound_id: name.to_lowercase().replace(' ', "-"),
            ..Default::default()
        };
        ImportedDeck::Riftbound(agni_riftbound::ResolvedDeck {
            legend: Some(card("Legend")),
            chosen_champion: Some(card("Champion")),
            main_deck: vec![agni_riftbound::DeckEntry {
                card: card("Filler"),
                count: 40,
            }],
            runes: vec![agni_riftbound::DeckEntry {
                card: card("Rune"),
                count: 12,
            }],
            battlefields: names
                .iter()
                .map(|name| agni_riftbound::DeckEntry {
                    card: card(name),
                    count: 1,
                })
                .collect(),
            sideboard: Vec::new(),
        })
    }

    fn spread_faces(plan: &[DealGroup]) -> Vec<String> {
        plan.iter()
            .filter(|group| matches!(group.target, agni_sim::wire::DealTarget::Spread(_)))
            .flat_map(|group| group.faces.iter().map(|face| face.name.clone()))
            .collect()
    }

    #[test]
    fn a_deck_with_several_battlefields_waits_for_a_choice_then_deals_only_that_one() {
        let mut record = seat(riftbound_with_battlefields(&["Alpha", "Beta", "Gamma"]));
        assert!(crate::deck::battlefield::needs_choice(&record));
        assert_eq!(spread_faces(&deal_plan_for(&record)), ["Alpha"]);
        record.battlefield = Some(1);
        assert!(!crate::deck::battlefield::needs_choice(&record));
        assert_eq!(crate::deck::battlefield::chosen_name(&record), Some("Beta"));
        assert_eq!(spread_faces(&deal_plan_for(&record)), ["Beta"]);
        let duel = DealSetup {
            players: 2,
            seat: 0,
            first_player: 0,
            generation: 0,
            battlefields: 2,
        };
        assert_eq!(
            spread_faces(&deal_plan_for_setup(&record, Some(duel))),
            ["Beta"]
        );
        assert_eq!(duel.placed(), 1);
        let three = DealSetup {
            seat: 1,
            battlefields: 3,
            ..duel
        };
        assert_eq!(three.placed(), 2);
        assert_eq!(
            spread_faces(&deal_plan_for_setup(&record, Some(three))),
            ["Beta", "Alpha"],
            "the seat after the first player places two: its choice, then the next in deck order"
        );
        let first = DealSetup { seat: 0, ..three };
        assert_eq!(first.placed(), 1);
        assert_eq!(
            spread_faces(&deal_plan_for_setup(&record, Some(first))),
            ["Beta"]
        );
    }

    #[test]
    fn a_single_battlefield_deck_needs_no_choice_and_the_first_player_sits_out_a_war() {
        let record = seat(riftbound_with_battlefields(&["Only"]));
        assert!(!crate::deck::battlefield::needs_choice(&record));
        assert_eq!(spread_faces(&deal_plan_for(&record)), ["Only"]);
        let war_first = DealSetup {
            players: 4,
            seat: 2,
            first_player: 2,
            generation: 0,
            battlefields: 3,
        };
        assert!(spread_faces(&deal_plan_for_setup(&record, Some(war_first))).is_empty());
        let war_other = DealSetup {
            seat: 3,
            ..war_first
        };
        assert_eq!(
            spread_faces(&deal_plan_for_setup(&record, Some(war_other))),
            ["Only"]
        );
        let none = seat(riftbound_with_battlefields(&[]));
        assert!(!crate::deck::battlefield::needs_choice(&none));
        assert!(spread_faces(&deal_plan_for(&none)).is_empty());
    }

    #[test]
    fn a_human_deals_the_body_first_and_places_the_chosen_battlefield_after() {
        let mut record = seat(riftbound_with_battlefields(&["Alpha", "Beta", "Gamma"]));
        let duel = DealSetup {
            players: 2,
            seat: 1,
            first_player: 0,
            generation: 0,
            battlefields: 2,
        };
        let body = body_plan(&record, Some(duel));
        assert!(
            spread_faces(&body).is_empty(),
            "the body deal carries no battlefield"
        );
        assert!(
            body.iter().any(|group| group.target
                == agni_sim::wire::DealTarget::Zone(agni_riftbound::ZONE_NAME_LEGEND.into())),
            "the legend goes out at once so the opponent sees it"
        );
        assert!(!battlefield_only(&body));
        record.battlefield = Some(1);
        let placement = battlefield_plan(&record, Some(duel));
        assert_eq!(spread_faces(&placement), ["Beta"]);
        assert_eq!(
            placement.len(),
            1,
            "only the battlefield group: {placement:?}"
        );
        assert!(battlefield_only(&placement));
        assert!(!battlefield_only(&[]));
        let single = seat(riftbound_with_battlefields(&["Only"]));
        assert_eq!(
            spread_faces(&body_plan(&single, Some(duel))),
            ["Only"],
            "a deck with nothing to choose deals whole"
        );
    }

    #[test]
    fn art_requests_key_riftbound_by_id_and_mtg_by_name() {
        let cache = ArtCache::default();
        let record = seat(mtg_deck());
        let wanted = art_requests(&record, &cache);
        assert_eq!(wanted.len(), 4);
        assert!(wanted.iter().all(|request| request.id.is_none()));
        assert!(wanted
            .iter()
            .any(|request| request.name == "Serelith, Tidebound Oracle"));

        let resolved = parse_reply(200, &reply_fixture()).unwrap();
        let record = seat(resolved.deck);
        let wanted = art_requests(&record, &cache);
        assert_eq!(wanted.len(), 3);
        assert!(wanted.iter().all(|request| request
            .id
            .as_deref()
            .is_some_and(|id| id.starts_with("ogn-"))));
    }

    fn galleys(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_string(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                )),
                _ => None,
            })
            .collect()
    }

    fn step_frames(
        panel: &mut ImportPanel,
        click: Option<&str>,
    ) -> (Vec<String>, Option<ImportAction>) {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 640.0));
        let mut texts = Vec::new();
        let mut action = None;
        let mut target = None;
        for frame in 0..3 {
            let mut events = Vec::new();
            if frame == 2 {
                if let Some(center) = target {
                    events.push(egui::Event::PointerMoved(center));
                    events.push(egui::Event::PointerButton {
                        pos: center,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    });
                    events.push(egui::Event::PointerButton {
                        pos: center,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let returned = result_panel(ui, panel, LOAD_LABEL);
                if returned.is_some() {
                    action = returned;
                }
            });
            output.textures_delta.clear();
            let found = galleys(&output);
            if let Some(label) = click {
                target = found
                    .iter()
                    .find(|(text, _)| text == label)
                    .map(|(_, rect)| rect.center());
            }
            texts = found.into_iter().map(|(text, _)| text).collect();
        }
        (texts, action)
    }

    fn resolved_panel(deck: ImportedDeck) -> ImportPanel {
        let mut panel = ImportPanel {
            source: Some("link:https://riftdecks.com/deck/1".into()),
            ..Default::default()
        };
        panel.set_resolved(ResolvedImport {
            deck,
            unresolved: vec![("Completely Unknown".into(), "no such card".into())],
            code: None,
            title: Some("Sample by Nobody".into()),
        });
        panel
    }

    #[test]
    fn the_import_result_offers_save_for_both_games_and_load_and_share_for_riftbound() {
        let resolved = parse_reply(200, &reply_fixture()).unwrap();
        let mut panel = resolved_panel(resolved.deck);
        let (texts, action) = step_frames(&mut panel, Some(LOAD_LABEL));
        assert!(texts.iter().any(|text| text == LOAD_LABEL), "{texts:?}");
        assert!(texts.iter().any(|text| text == "share"), "{texts:?}");
        assert!(texts.iter().any(|text| text == SAVE_LABEL), "{texts:?}");
        assert!(
            !texts.iter().any(|text| text == "seat this deck"),
            "seating belongs to the lobby: {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("problem")),
            "the verdict chip judges the pasted list: {texts:?}"
        );
        let Some(ImportAction::Edit {
            deck,
            label,
            source,
            unresolved,
        }) = action
        else {
            panic!("tapping load hands the deck to the editor: {action:?}");
        };
        assert_eq!(deck.legend.as_ref().unwrap().name, "Vanguard Sentinel");
        assert_eq!(label, "Sample by Nobody");
        assert_eq!(source, "link:https://riftdecks.com/deck/1");
        assert_eq!(unresolved, ["Completely Unknown"]);

        let mut panel = resolved_panel(parse_reply(200, &reply_fixture()).unwrap().deck);
        let (_, action) = step_frames(&mut panel, Some(SAVE_LABEL));
        let Some(ImportAction::Save {
            deck,
            label,
            source,
        }) = action
        else {
            panic!("tapping save hands the deck to the library: {action:?}");
        };
        assert_eq!(deck.game(), TableGame::Riftbound);
        assert_eq!(label, "Sample by Nobody");
        assert_eq!(source, "link:https://riftdecks.com/deck/1");

        let mut panel = resolved_panel(mtg_deck());
        let (texts, action) = step_frames(&mut panel, Some(SAVE_LABEL));
        assert!(!texts.iter().any(|text| text == LOAD_LABEL), "{texts:?}");
        assert!(!texts.iter().any(|text| text == "share"), "{texts:?}");
        assert!(
            matches!(action, Some(ImportAction::Save { ref deck, .. }) if deck.game() == TableGame::Mtg),
            "an MTG list saves too: {action:?}"
        );
    }

    #[test]
    fn the_paste_box_detects_the_game_and_refuses_what_is_no_deck() {
        assert_eq!(
            game_of_paste("https://riftdecks.com/riftbound-metagame/deck-x-1"),
            Some(TableGame::Riftbound)
        );
        assert_eq!(
            game_of_paste("Legend\n1 Lillia - Bashful Bloom\n"),
            Some(TableGame::Riftbound)
        );
        let mut panel = ImportPanel::default();
        begin_import_any(&mut panel);
        assert_eq!(panel.error.as_deref(), Some(EMPTY_CLIPBOARD));
        panel.paste = "3\n".into();
        begin_import_any(&mut panel);
        assert_eq!(panel.error.as_deref(), Some(NOT_A_DECK));
        assert!(!panel.busy);
    }

    #[test]
    fn a_flash_outlives_the_battlefield_hint_for_two_seconds_only() {
        let context = egui::Context::default();
        let at = |time: f64| egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        let mut panel = ImportPanel::default();
        context.begin_pass(at(10.0));
        panel.set_flash(&context, "copied to clipboard".into());
        assert_eq!(
            panel.flash_text(&context).as_deref(),
            Some("copied to clipboard")
        );
        context.end_pass().textures_delta.clear();
        context.begin_pass(at(10.0 + FLASH_SECS + 0.5));
        assert_eq!(panel.flash_text(&context), None);
        context.end_pass().textures_delta.clear();
    }

    #[test]
    fn a_deck_with_art_already_cached_asks_for_nothing() {
        let record = seat(mtg_deck());
        let mut cache = ArtCache::default();
        for face in record.faces.values() {
            cache.insert(&face.name, b"already here".to_vec());
        }
        assert!(art_requests(&record, &cache).is_empty());
    }
}
