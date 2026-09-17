use agni_core::Table;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_ATTEMPTS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtGame {
    Riftbound,
    Mtg,
}

impl ArtGame {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Riftbound => "riftbound",
            Self::Mtg => "mtg",
        }
    }

    pub fn back_name(self) -> &'static str {
        match self {
            Self::Riftbound => "riftbound card back",
            Self::Mtg => "mtg card back",
        }
    }

    pub fn back_url(self) -> &'static str {
        match self {
            Self::Riftbound => agni_riftbound::CARD_BACK_URL,
            Self::Mtg => agni_mtg::CARD_BACK_URL,
        }
    }

    pub fn back_hash(self) -> &'static str {
        match self {
            Self::Riftbound => "9cea33d1205dccedbfd4eccc31aeb77c0fe65f8a0a9e5d5fd51b9bd06605e5de",
            Self::Mtg => "f089735677a17dc62cbaecaf46719ac2f2f3a88832d01308d8721b4c1be37595",
        }
    }
}

pub const BACK_ID: &str = "card-back";
pub const PLAYMAT_ID: &str = "playmat=";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtRequest {
    pub game: ArtGame,
    pub id: Option<String>,
    pub name: String,
}

impl ArtRequest {
    pub fn by_id(game: ArtGame, id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            game,
            id: Some(id.into()),
            name: name.into(),
        }
    }

    pub fn by_name(game: ArtGame, name: impl Into<String>) -> Self {
        Self {
            game,
            id: None,
            name: name.into(),
        }
    }

    pub fn back(game: ArtGame) -> Self {
        Self::by_id(game, BACK_ID, game.back_name())
    }

    pub fn is_back(&self) -> bool {
        self.id.as_deref() == Some(BACK_ID)
    }

    pub fn playmat(name: impl Into<String>, url: &str) -> Self {
        Self::by_id(ArtGame::Riftbound, format!("{PLAYMAT_ID}{url}"), name)
    }

    pub fn playmat_url(&self) -> Option<&str> {
        self.id.as_deref()?.strip_prefix(PLAYMAT_ID)
    }

    pub fn key(&self) -> String {
        let identity = self.id.as_deref().unwrap_or(&self.name);
        format!("{}/{}", self.game.tag(), identity.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrival {
    pub id: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

pub type Fetched = Result<Option<Vec<u8>>, String>;

pub trait ArtFetcher {
    fn fetch(&mut self, request: &ArtRequest) -> Fetched;
}

pub struct ArtQueue {
    pending: VecDeque<ArtRequest>,
    open: BTreeSet<String>,
    settled: BTreeSet<String>,
    attempts: BTreeMap<String, u32>,
}

impl Default for ArtQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtQueue {
    pub const fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            open: BTreeSet::new(),
            settled: BTreeSet::new(),
            attempts: BTreeMap::new(),
        }
    }

    pub fn enqueue(&mut self, request: ArtRequest) -> bool {
        if request.name.is_empty() && request.id.is_none() {
            return false;
        }
        let key = request.key();
        if self.settled.contains(&key) || !self.open.insert(key) {
            return false;
        }
        self.pending.push_back(request);
        true
    }

    pub fn request_ids<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) -> usize {
        ids.into_iter()
            .filter(|id| self.enqueue(ArtRequest::by_id(ArtGame::Riftbound, *id, *id)))
            .count()
    }

    pub fn take(&mut self) -> Option<ArtRequest> {
        self.pending.pop_front()
    }

    pub fn finish(&mut self, request: &ArtRequest, outcome: Fetched) -> Option<Arrival> {
        let key = request.key();
        match outcome {
            Ok(bytes) => {
                self.open.remove(&key);
                self.attempts.remove(&key);
                self.settled.insert(key);
                bytes.map(|bytes| Arrival {
                    id: request.id.clone().unwrap_or_else(|| request.name.clone()),
                    name: request.name.clone(),
                    bytes,
                })
            }
            Err(_) => {
                let attempts = self.attempts.entry(key.clone()).or_insert(0);
                *attempts += 1;
                if *attempts >= MAX_ATTEMPTS {
                    self.open.remove(&key);
                    self.settled.insert(key);
                } else {
                    self.pending.push_back(request.clone());
                }
                None
            }
        }
    }

    pub fn outstanding(&self) -> usize {
        self.open.len()
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    pub fn forget(&mut self, request: &ArtRequest) {
        let key = request.key();
        self.open.remove(&key);
        self.settled.remove(&key);
        self.attempts.remove(&key);
    }
}

pub fn pump(queue: &mut ArtQueue, fetcher: &mut dyn ArtFetcher, budget: usize) -> Vec<Arrival> {
    let mut arrivals = Vec::new();
    for _ in 0..budget {
        let Some(request) = queue.take() else {
            break;
        };
        let outcome = fetcher.fetch(&request);
        if let Some(arrival) = queue.finish(&request, outcome) {
            arrivals.push(arrival);
        }
    }
    arrivals
}

pub fn art_key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

#[derive(Resource, Default)]
pub struct ArtCache {
    bytes: BTreeMap<String, Vec<u8>>,
    images: BTreeMap<String, Handle<Image>>,
}

impl ArtCache {
    pub fn remove(&mut self, name: &str) {
        self.bytes.remove(&art_key(name));
        self.images.remove(&art_key(name));
    }

    pub fn has(&self, name: &str) -> bool {
        self.bytes.contains_key(&art_key(name))
    }

    pub fn bytes(&self, name: &str) -> Option<&[u8]> {
        self.bytes.get(&art_key(name)).map(Vec::as_slice)
    }

    pub fn insert(&mut self, name: &str, bytes: Vec<u8>) -> bool {
        if name.trim().is_empty() || bytes.is_empty() {
            return false;
        }
        let key = art_key(name);
        if self.bytes.contains_key(&key) {
            return false;
        }
        self.bytes.insert(key, bytes);
        true
    }

    pub fn extend(&mut self, arrivals: impl IntoIterator<Item = (String, Vec<u8>)>) -> bool {
        let mut fresh = false;
        for (name, bytes) in arrivals {
            fresh |= self.insert(&name, bytes);
        }
        fresh
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn image(&mut self, name: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let key = art_key(name);
        if let Some(handle) = self.images.get(&key) {
            return Some(handle.clone());
        }
        let bytes = self.bytes.get(&key)?;
        let image = match decode_image(bytes) {
            Ok(image) => image,
            Err(error) => {
                warn!("card art decode failed for {name}: {error}");
                return None;
            }
        };
        let handle = images.add(image);
        self.images.insert(key, handle.clone());
        Some(handle)
    }
}

pub fn image_extension(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [b'G', b'I', b'F', ..] => "gif",
        [b'B', b'M', ..] => "bmp",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        _ => "jpg",
    }
}

pub fn decode_image(bytes: &[u8]) -> Result<Image, String> {
    Image::from_buffer(
        bytes,
        ImageType::Extension(image_extension(bytes)),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|error| error.to_string())
}

pub fn validate_download(bytes: &[u8], expected: Option<&str>) -> Result<(), String> {
    if expected.is_some_and(|hash| spirit_core::BlobHash::of(bytes).to_string() != hash) {
        return Err("artwork does not match its content address".into());
    }
    decode_image(bytes).map(|_| ())
}

pub fn missing_from_table(
    table: &Table,
    game: ArtGame,
    known: &ArtCache,
    ids: &BTreeMap<String, String>,
) -> Vec<ArtRequest> {
    let mut seen = BTreeSet::new();
    let mut wanted = Vec::new();
    for card in table.cards() {
        if card.face.is_hidden() || known.has(&card.face.name) {
            continue;
        }
        if seen.insert(art_key(&card.face.name)) {
            wanted.push(match ids.get(&art_key(&card.face.name)) {
                Some(id) => ArtRequest::by_id(game, id.clone(), card.face.name.clone()),
                None => ArtRequest::by_name(game, card.face.name.clone()),
            });
        }
    }
    wanted
}

pub fn token_art_ids(tokens: &[agni_sim::wire::TokenDecl]) -> BTreeMap<String, String> {
    tokens
        .iter()
        .filter_map(|token| Some((art_key(&token.name), token.art.clone()?)))
        .collect()
}

pub fn token_art_url(request: &ArtRequest) -> Option<&'static str> {
    if request.game != ArtGame::Riftbound {
        return None;
    }
    let token = agni_riftbound::token_table()
        .into_iter()
        .find(|token| art_key(&token.name) == art_key(&request.name));
    let id = request
        .id
        .as_deref()
        .or_else(|| token.as_ref().and_then(|token| token.art.as_deref()))?;
    match id {
        "ogn-274-298" => Some("https://cdn.piltoverarchive.com/cards/OGN-274.webp"),
        "ogn-272-298" => Some("https://cdn.piltoverarchive.com/cards/OGN-272.webp"),
        "unl-t02" => Some("https://cdn.piltoverarchive.com/cards/UNL-T02.webp"),
        "sfd-t02" => Some("https://cdn.piltoverarchive.com/cards/SFD-T02.webp"),
        "sfd-t01" => Some("https://cdn.piltoverarchive.com/cards/SFD-T01.webp"),
        "ven-t05" => Some("https://cdn.piltoverarchive.com/cards/VEN-T05.webp"),
        "ven-t06" => Some("https://cdn.piltoverarchive.com/cards/VEN-T06.webp"),
        "sfd-t03" => Some("https://cdn.piltoverarchive.com/cards/SFD-T03.webp"),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod worker {
    use super::{Arrival, ArtFetcher, ArtGame, ArtQueue, ArtRequest, Fetched};
    use agni_importers::art::WantedArt;
    use parking_lot::Mutex;
    use std::path::PathBuf;

    use agni_importers::mtg::ingest::DECK_THROTTLE as MTG_THROTTLE;
    use agni_importers::riftbound::ingest::DECK_THROTTLE as RIFTBOUND_THROTTLE;

    struct Shared {
        queue: ArtQueue,
        arrivals: Vec<Arrival>,
        running: bool,
        failures: usize,
    }

    static SHARED: Mutex<Shared> = Mutex::new(Shared {
        queue: ArtQueue::new(),
        arrivals: Vec::new(),
        running: false,
        failures: 0,
    });

    pub struct SourceFetcher {
        dir: PathBuf,
        riftcodex: agni_importers::riftbound::riftcodex::Riftcodex,
        scryfall: agni_importers::mtg::scryfall_named::Scryfall,
        riftbound_catalog: Option<agni_importers::riftbound::catalog::StaticCatalog>,
        mtg_catalog: Option<agni_importers::mtg::catalog::StaticCatalog>,
    }

    impl SourceFetcher {
        pub fn new(dir: PathBuf) -> Self {
            let riftbound_catalog = agni_importers::riftbound::ingest::load_catalog(&dir)
                .ok()
                .flatten();
            let mtg_catalog = agni_importers::mtg::ingest::load_catalog(&dir)
                .ok()
                .flatten();
            Self {
                dir,
                riftcodex: agni_importers::riftbound::riftcodex::Riftcodex::new(),
                scryfall: agni_importers::mtg::scryfall_named::Scryfall::new(),
                riftbound_catalog,
                mtg_catalog,
            }
        }

        fn riftbound_want(
            &mut self,
            request: &ArtRequest,
        ) -> Result<Option<agni_importers::riftbound::catalog::CatalogCard>, String> {
            use agni_importers::riftbound::catalog::CardLookup;
            let mut hit = None;
            if let Some(catalog) = self.riftbound_catalog.as_mut() {
                hit = match &request.id {
                    Some(id) => catalog.by_id(id).map_err(|error| error.to_string())?,
                    None => catalog
                        .by_name(&request.name)
                        .map_err(|error| error.to_string())?,
                };
            }
            if hit
                .as_ref()
                .is_none_or(|card| card.image_url.is_none() || card.is_padded())
            {
                hit = match &request.id {
                    Some(id) => self
                        .riftcodex
                        .by_id(id)
                        .map_err(|error| error.to_string())?
                        .or(hit),
                    None => self
                        .riftcodex
                        .by_name(&request.name)
                        .map_err(|error| error.to_string())?
                        .or(hit),
                };
            }
            Ok(hit.map(|mut card| {
                if card.name.is_empty() {
                    card.name = request.name.clone();
                }
                card
            }))
        }

        fn mtg_want(&mut self, request: &ArtRequest) -> Result<Option<WantedArt>, String> {
            use agni_importers::mtg::catalog::CardLookup;
            let mut hit = self
                .mtg_catalog
                .as_mut()
                .and_then(|catalog| catalog.by_name(&request.name).ok().flatten());
            if hit
                .as_ref()
                .and_then(|card| card.image_url.clone())
                .is_none()
            {
                hit = self
                    .scryfall
                    .by_name(&request.name)
                    .map_err(|error| error.to_string())?
                    .or(hit);
            }
            Ok(hit.map(|card| WantedArt {
                key: card.name.clone(),
                name: card.name,
                image_url: card.image_url,
            }))
        }
    }

    impl ArtFetcher for SourceFetcher {
        fn fetch(&mut self, request: &ArtRequest) -> Fetched {
            if let Some(url) = super::token_art_url(request) {
                let store =
                    spirit_core::BlobStore::open(&self.dir).map_err(|error| error.to_string())?;
                let agent = agni_importers::art::art_agent();
                return agni_importers::art::fetch_one(
                    &store,
                    "riftbound-images",
                    &agent,
                    &request.key(),
                    url,
                )
                .map(|(_, bytes)| Some(bytes))
                .map_err(|error| error.to_string());
            }
            if let Some(url) = request.playmat_url() {
                use std::io::Read;
                let expected = crate::table::playmat::catalog_hash(url)
                    .ok_or("Personal playmats use the direct table connection")?;
                let hash =
                    spirit_core::BlobHash::parse(expected).ok_or("Invalid curated fingerprint")?;
                let store =
                    spirit_core::BlobStore::open(&self.dir).map_err(|error| error.to_string())?;
                let bytes = if let Ok(bytes) = store.get(hash) {
                    bytes
                } else {
                    let mut bytes = Vec::new();
                    agni_importers::art::art_agent()
                        .get(url)
                        .timeout(std::time::Duration::from_secs(20))
                        .call()
                        .map_err(|error| error.to_string())?
                        .into_reader()
                        .take((16 << 20) + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|error| error.to_string())?;
                    if bytes.len() > 16 << 20 {
                        return Err("Curated image exceeds the download limit".into());
                    }
                    bytes
                };
                super::validate_download(&bytes, Some(expected))?;
                agni_importers::art::Journal::open(&store, crate::table::playmat::JOURNAL)
                    .put(&store, &request.name, &bytes)
                    .map_err(|error| error.to_string())?;
                super::assets::republish(&self.dir);
                return Ok(Some(bytes));
            }
            if request.is_back() {
                let store =
                    spirit_core::BlobStore::open(&self.dir).map_err(|error| error.to_string())?;
                let journal = super::assets::BACKS_JOURNAL;
                let from_mesh = super::assets::ensure_from_mesh(&store, journal, &request.key());
                let agent = agni_importers::art::art_agent();
                let (_, bytes) = agni_importers::art::fetch_one(
                    &store,
                    journal,
                    &agent,
                    &request.key(),
                    request.game.back_url(),
                )
                .map_err(|error| error.to_string())?;
                if !from_mesh {
                    super::assets::republish(&self.dir);
                }
                return Ok(Some(bytes));
            }
            let landed = match request.game {
                ArtGame::Riftbound => {
                    let Some(want) = self.riftbound_want(request)? else {
                        return Ok(None);
                    };
                    let from_mesh = spirit_core::BlobStore::open(&self.dir)
                        .map(|store| {
                            super::assets::ensure_from_mesh(
                                &store,
                                agni_importers::riftbound::ingest::JOURNAL_FILE,
                                &want.riftbound_id,
                            )
                        })
                        .unwrap_or(false);
                    let landed = agni_importers::riftbound::ingest::ingest_deck_art(
                        &self.dir,
                        std::slice::from_ref(&want),
                        RIFTBOUND_THROTTLE,
                        |_| {},
                    )
                    .map_err(|error| error.to_string())?;
                    if !from_mesh && !landed.is_empty() {
                        super::assets::republish(&self.dir);
                    }
                    landed
                }
                ArtGame::Mtg => {
                    let Some(want) = self.mtg_want(request)? else {
                        return Ok(None);
                    };
                    let from_mesh = spirit_core::BlobStore::open(&self.dir)
                        .map(|store| {
                            super::assets::ensure_from_mesh(
                                &store,
                                agni_importers::mtg::ingest::JOURNAL_FILE,
                                &want.key,
                            )
                        })
                        .unwrap_or(false);
                    let landed = agni_importers::mtg::ingest::ingest_deck_art(
                        &self.dir,
                        std::slice::from_ref(&want),
                        MTG_THROTTLE,
                        |_| {},
                    )
                    .map_err(|error| error.to_string())?;
                    if !from_mesh && !landed.is_empty() {
                        super::assets::republish(&self.dir);
                    }
                    landed
                }
            };
            Ok(landed.into_iter().next().map(|(_, bytes)| bytes))
        }
    }

    pub fn enqueue(requests: impl IntoIterator<Item = ArtRequest>) {
        let Some(dir) = crate::os::paths::store_dir() else {
            return;
        };
        let mut shared = SHARED.lock();
        let mut fresh = false;
        for request in requests {
            fresh |= shared.queue.enqueue(request);
        }
        if !fresh || shared.running || shared.queue.is_empty() {
            return;
        }
        shared.running = true;
        drop(shared);
        std::thread::spawn(move || run(SourceFetcher::new(dir)));
    }

    fn run(mut fetcher: SourceFetcher) {
        loop {
            let next = {
                let mut shared = SHARED.lock();
                match shared.queue.take() {
                    Some(request) => request,
                    None => {
                        shared.running = false;
                        return;
                    }
                }
            };
            let outcome = fetcher.fetch(&next);
            let failed = outcome.is_err();
            let mut shared = SHARED.lock();
            match shared.queue.finish(&next, outcome) {
                Some(arrival) => shared.arrivals.push(arrival),
                None if failed => shared.failures += 1,
                None => {}
            }
        }
    }

    pub fn take_arrivals() -> Vec<Arrival> {
        std::mem::take(&mut SHARED.lock().arrivals)
    }

    pub fn outstanding() -> usize {
        SHARED.lock().queue.outstanding()
    }

    pub fn status_line() -> Option<String> {
        let outstanding = outstanding();
        (outstanding > 0).then(|| format!("fetching art… {outstanding} left"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use worker::{enqueue, outstanding, status_line, take_arrivals};

#[cfg(not(target_arch = "wasm32"))]
pub fn request_cards<'a>(
    cards: impl IntoIterator<Item = &'a agni_importers::riftbound::catalog::CatalogCard>,
    _cache: &mut ArtCache,
) {
    enqueue(cards.into_iter().map(|card| {
        ArtRequest::by_id(
            ArtGame::Riftbound,
            card.riftbound_id.clone(),
            card.name.clone(),
        )
    }));
}

#[cfg(target_arch = "wasm32")]
pub fn request_cards<'a>(
    cards: impl IntoIterator<Item = &'a agni_importers::riftbound::catalog::CatalogCard>,
    cache: &mut ArtCache,
) {
    for card in cards {
        if let Some(bytes) = crate::net::gateway::riftbound_art(&card.riftbound_id) {
            cache.insert(&card.name, bytes);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn collect_arrivals(mut cache: ResMut<ArtCache>) {
    let arrivals = take_arrivals();
    if arrivals.is_empty() {
        return;
    }
    cache.extend(
        arrivals
            .into_iter()
            .map(|arrival| (arrival.name, arrival.bytes)),
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub fn queue_visible_art(
    table: Res<crate::table::GameTable>,
    mirror: Res<crate::table::Mirror>,
    cache: Res<ArtCache>,
    tokens: Res<crate::table::tokens::PluginTokens>,
) {
    if !table.is_changed() && !cache.is_changed() && !tokens.is_changed() {
        return;
    }
    let Some(game) = crate::net::game_of_zones(&mirror.view.zones).art_game() else {
        return;
    };
    let mut wanted = Vec::new();
    if !cache.has(game.back_name()) {
        wanted.push(ArtRequest::back(game));
    }
    wanted.extend(missing_from_table(
        &table.0,
        game,
        &cache,
        &token_art_ids(&tokens.0),
    ));
    if wanted.is_empty() {
        return;
    }
    enqueue(wanted);
}

#[cfg(target_arch = "wasm32")]
pub fn queue_visible_art(
    table: Res<crate::table::GameTable>,
    mirror: Res<crate::table::Mirror>,
    mut cache: ResMut<ArtCache>,
    tokens: Res<crate::table::tokens::PluginTokens>,
    time: Res<Time>,
    mut next_poll: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if !table.is_changed() && !cache.is_changed() && !tokens.is_changed() && now < *next_poll {
        return;
    }
    *next_poll = now + 0.5;
    let Some(game) = crate::net::game_of_zones(&mirror.view.zones).art_game() else {
        return;
    };
    if !cache.has(game.back_name()) {
        crate::net::gateway::request_back(game, now);
    }
    if game != ArtGame::Riftbound {
        return;
    }
    let ids = token_art_ids(&tokens.0);
    let landed: Vec<(String, Vec<u8>)> = missing_from_table(&table.0, game, &cache, &ids)
        .into_iter()
        .filter_map(|request| {
            let bytes = match &request.id {
                Some(id) => crate::net::gateway::riftbound_art(id),
                None => crate::net::gateway::riftbound_art_named(&request.name),
            };
            if bytes.is_none() {
                if let Some(url) = token_art_url(&request) {
                    crate::net::gateway::request_token_art(&request.name, url, now);
                }
            }
            let bytes = bytes?;
            Some((request.name, bytes))
        })
        .collect();
    if !landed.is_empty() {
        cache.extend(landed);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod runtime_art_tests {
    use super::*;

    #[test]
    fn both_card_backs_have_content_addresses_without_bundled_art() {
        for game in [ArtGame::Riftbound, ArtGame::Mtg] {
            assert!(spirit_core::BlobHash::parse(game.back_hash()).is_some());
            assert!(game.back_url().starts_with("https://"));
        }
    }

    #[test]
    fn invalid_responses_and_wrong_content_never_become_art() {
        assert!(validate_download(b"<html>not found</html>", None).is_err());
        let image = image::DynamicImage::new_rgb8(2, 3);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let bytes = bytes.into_inner();
        let hash = spirit_core::BlobHash::of(&bytes).to_string();
        assert!(validate_download(&bytes, Some(&hash)).is_ok());
        assert!(validate_download(&bytes, Some(&"0".repeat(64))).is_err());
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod assets {
    use agni_importers::art::{asset_entries, asset_key, Journal};
    use spirit_core::{BlobHash, BlobStore};
    use spirit_node::assets::Found;
    use std::path::Path;
    use std::time::Duration;

    pub const BACKS_JOURNAL: &str = "card-backs";
    pub const INDEX_WAIT: Duration = Duration::from_secs(4);
    pub const BLOB_WAIT: Duration = Duration::from_secs(12);

    async fn with_deadline<F: std::future::Future>(
        duration: Duration,
        future: F,
    ) -> Result<F::Output, tokio::time::error::Elapsed> {
        tokio::time::timeout(duration, future).await
    }

    pub fn index_entries(
        dir: &Path,
    ) -> Result<std::collections::BTreeMap<String, BlobHash>, String> {
        let store = BlobStore::open(dir).map_err(|error| error.to_string())?;
        let mut entries = asset_entries(&store);
        if let Ok(Some(manifest)) = agni_importers::riftbound::ingest::load_manifest(dir) {
            for card in &manifest.cards {
                if let Some(hash) = BlobHash::parse(&card.image).filter(|hash| store.has(*hash)) {
                    entries.insert(
                        asset_key(
                            agni_importers::riftbound::ingest::JOURNAL_FILE,
                            &card.riftbound_id,
                        ),
                        hash,
                    );
                }
            }
        }
        Ok(entries)
    }

    pub fn publish_index(dir: &Path) -> Result<usize, String> {
        let store = BlobStore::open(dir).map_err(|error| error.to_string())?;
        let entries = index_entries(dir)?;
        spirit_node::assets::publish(&store, &entries)?;
        Ok(entries.len())
    }

    pub fn republish(dir: &Path) {
        if let Err(error) = publish_index(dir) {
            bevy::log::warn!(target: "kai::art", "assets index not republished: {error}");
        }
    }

    pub fn ensure_from_mesh(store: &BlobStore, journal: &str, name: &str) -> bool {
        let mut local = Journal::open(store, journal);
        if local.get(store, name).is_some() {
            return true;
        }
        let Some(node) = crate::net::node::get() else {
            return false;
        };
        for (peer, manifest) in node.mesh.missing_asset_indexes(store) {
            let pulled = node.block_on(with_deadline(
                INDEX_WAIT,
                spirit_node::mesh::fetch_blob(
                    &node.mesh,
                    &node.endpoint,
                    &node.blobs,
                    manifest,
                    Some(&peer),
                ),
            ));
            if let Ok(Err(error)) = pulled {
                bevy::log::debug!(target: "kai::art", "asset index of {peer}: {error}");
            }
        }
        let key = asset_key(journal, name);
        let hash = match node.mesh.find_asset(store, &key) {
            Found::Held(hash) => hash,
            Found::Provider(hash, peer) => {
                let pulled = node.block_on(with_deadline(
                    BLOB_WAIT,
                    spirit_node::mesh::fetch_blob(
                        &node.mesh,
                        &node.endpoint,
                        &node.blobs,
                        hash,
                        Some(&peer),
                    ),
                ));
                match pulled {
                    Ok(Ok(_)) => hash,
                    Ok(Err(error)) => {
                        bevy::log::info!(target: "kai::art", "{key} from {peer} failed: {error}");
                        return false;
                    }
                    Err(_) => {
                        bevy::log::info!(target: "kai::art", "{key} from {peer} timed out");
                        return false;
                    }
                }
            }
            Found::Unknown => return false,
        };
        if !store.has(hash) {
            return false;
        }
        if let Err(error) = local.link(name, hash) {
            bevy::log::warn!(target: "kai::art", "{key}: journal write failed: {error}");
            return false;
        }
        bevy::log::info!(target: "kai::art", "{key} came from the mesh");
        republish(store.root());
        true
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn worker_deadlines_are_created_only_after_entering_the_runtime() {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_time()
                .build()
                .unwrap();
            let handle = runtime.handle().clone();
            std::thread::spawn(move || {
                let ready = with_deadline(Duration::from_secs(1), std::future::ready(7));
                assert_eq!(handle.block_on(ready).unwrap(), 7);
                let blocked = with_deadline(Duration::from_millis(1), std::future::pending::<()>());
                assert!(handle.block_on(blocked).is_err());
            })
            .join()
            .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_core::{CardFace, PlayerId, Zone};

    #[test]
    fn every_declared_riftbound_token_has_a_runtime_art_source() {
        let tokens = agni_riftbound::token_table();
        let ids = token_art_ids(&tokens);
        for token in tokens {
            let request =
                ArtRequest::by_id(ArtGame::Riftbound, &ids[&art_key(&token.name)], &token.name);
            assert!(token_art_url(&request).is_some(), "{}", token.name);
            assert_eq!(
                token_art_url(&request),
                token_art_url(&ArtRequest::by_name(ArtGame::Riftbound, &token.name)),
                "older module manifests can still find token art by name",
            );
        }
        assert!(token_art_url(&ArtRequest::by_name(ArtGame::Mtg, "Sand Soldier")).is_none());
        assert!(
            token_art_url(&ArtRequest::by_name(ArtGame::Riftbound, "Unlisted token")).is_none()
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    #[ignore = "requires the public token image CDN"]
    fn live_token_art_sources_decode_without_bundled_images() {
        let agent = agni_importers::art::art_agent();
        for token in agni_riftbound::token_table() {
            let request = ArtRequest::by_name(ArtGame::Riftbound, &token.name);
            let url = token_art_url(&request).unwrap();
            let bytes = agni_importers::art::fetch_image(&agent, url).unwrap();
            assert!(decode_image(&bytes).is_ok(), "{}", token.name);
        }
    }

    struct Fake {
        art: BTreeMap<String, Vec<u8>>,
        calls: Vec<String>,
        fail: bool,
    }

    impl Fake {
        fn new(fail: bool) -> Self {
            Self {
                art: BTreeMap::new(),
                calls: Vec::new(),
                fail,
            }
        }

        fn with(mut self, name: &str, bytes: &[u8]) -> Self {
            self.art.insert(name.to_string(), bytes.to_vec());
            self
        }
    }

    impl ArtFetcher for Fake {
        fn fetch(&mut self, request: &ArtRequest) -> Fetched {
            self.calls.push(request.key());
            if self.fail {
                return Err("the source is unreachable".into());
            }
            Ok(self.art.get(&request.name).cloned())
        }
    }

    fn landed(arrivals: Vec<Arrival>) -> ArtCache {
        let mut cache = ArtCache::default();
        cache.extend(
            arrivals
                .into_iter()
                .map(|arrival| (arrival.name, arrival.bytes)),
        );
        cache
    }

    #[test]
    fn the_queue_asks_once_per_card_however_often_it_is_seen() {
        let mut queue = ArtQueue::new();
        assert!(queue.enqueue(ArtRequest::by_name(ArtGame::Mtg, "Thornspire Adept")));
        assert!(!queue.enqueue(ArtRequest::by_name(ArtGame::Mtg, "Thornspire Adept")));
        assert!(!queue.enqueue(ArtRequest::by_name(ArtGame::Mtg, "thornspire adept")));
        assert!(queue.enqueue(ArtRequest::by_name(ArtGame::Riftbound, "Thornspire Adept")));
        assert_eq!(queue.outstanding(), 2);
        let taken = queue.take().unwrap();
        assert!(!queue.enqueue(taken.clone()));
        let mut fake = Fake::new(false).with("Thornspire Adept", b"art");
        let arrival = queue.finish(&taken, fake.fetch(&taken)).unwrap();
        assert_eq!(arrival.name, "Thornspire Adept");
        assert!(!queue.enqueue(taken));
        assert_eq!(fake.calls.len(), 1);
    }

    #[test]
    fn a_missing_face_is_fetched_and_lands_in_the_cache() {
        let mut table = Table::new();
        table.add_face(PlayerId(0), Zone::Hand, CardFace::named("Thornspire Adept"));
        table.add_face(PlayerId(0), Zone::Hand, CardFace::named("Mistfen Causeway"));
        let mut cache = ArtCache::default();
        assert!(cache.insert("Mistfen Causeway", b"already here".to_vec()));
        let wanted = missing_from_table(&table, ArtGame::Mtg, &cache, &BTreeMap::new());
        assert_eq!(wanted.len(), 1);
        assert_eq!(wanted[0].name, "Thornspire Adept");

        let mut queue = ArtQueue::new();
        for request in wanted {
            queue.enqueue(request);
        }
        let mut fake = Fake::new(false).with("Thornspire Adept", b"adept art");
        let arrivals = pump(&mut queue, &mut fake, 8);
        assert_eq!(arrivals.len(), 1);
        assert!(queue.is_empty());
        assert!(cache.extend(
            arrivals
                .into_iter()
                .map(|arrival| (arrival.name, arrival.bytes))
        ));
        assert_eq!(cache.bytes("thornspire adept"), Some(&b"adept art"[..]));
        assert!(missing_from_table(&table, ArtGame::Mtg, &cache, &BTreeMap::new()).is_empty());
        assert!(!cache.insert("Thornspire Adept", b"twice".to_vec()));
    }

    #[test]
    fn an_opponents_reveal_queues_its_art_the_moment_it_lands() {
        let mut table = Table::new();
        table.add_face(PlayerId(0), Zone::Hand, CardFace::named("My Own Card"));
        let empty = ArtCache::default();
        let before = missing_from_table(&table, ArtGame::Riftbound, &empty, &BTreeMap::new());
        assert_eq!(before.len(), 1);
        let mut queue = ArtQueue::new();
        for request in before {
            queue.enqueue(request);
        }
        let mut fake = Fake::new(false).with("My Own Card", b"mine");
        let cache = landed(pump(&mut queue, &mut fake, 8));
        assert!(
            missing_from_table(&table, ArtGame::Riftbound, &cache, &BTreeMap::new()).is_empty()
        );

        table.add_face(PlayerId(1), Zone::Board, CardFace::named("Emberwing Scout"));
        table.add_face(
            PlayerId(1),
            Zone::Board,
            CardFace::named("Gloomvale Trickster"),
        );
        table.add_face(PlayerId(1), Zone::Board, CardFace::hidden());
        let after = missing_from_table(&table, ArtGame::Riftbound, &cache, &BTreeMap::new());
        assert_eq!(after.len(), 2);
        assert!(queue.enqueue(after[0].clone()));
        assert_eq!(queue.outstanding(), 1);
    }

    #[test]
    fn a_failing_source_gives_up_quietly_and_leaves_the_named_placeholder() {
        let mut table = Table::new();
        table.add_face(PlayerId(0), Zone::Hand, CardFace::named("Cinderveil Ward"));
        let mut queue = ArtQueue::new();
        for request in
            missing_from_table(&table, ArtGame::Mtg, &ArtCache::default(), &BTreeMap::new())
        {
            queue.enqueue(request);
        }
        let mut fake = Fake::new(true);
        let arrivals = pump(&mut queue, &mut fake, 8);
        assert!(arrivals.is_empty());
        assert_eq!(fake.calls.len(), MAX_ATTEMPTS as usize);
        assert!(queue.is_empty());
        assert!(!queue.enqueue(ArtRequest::by_name(ArtGame::Mtg, "Cinderveil Ward")));
        let card = &table.cards()[0];
        assert_eq!(card.face.name, "Cinderveil Ward");
        assert!(!ArtCache::default().has(&card.face.name));
    }

    #[test]
    fn a_source_with_no_art_for_a_card_is_settled_without_a_retry() {
        let mut queue = ArtQueue::new();
        queue.enqueue(ArtRequest::by_id(
            ArtGame::Riftbound,
            "ogn-007-298",
            "Emberwing Scout",
        ));
        let mut fake = Fake::new(false);
        let arrivals = pump(&mut queue, &mut fake, 8);
        assert!(arrivals.is_empty());
        assert_eq!(fake.calls, vec!["riftbound/ogn-007-298".to_string()]);
        assert!(queue.is_empty());
    }

    #[test]
    fn ids_requested_bare_land_under_their_own_id_and_never_twice() {
        let mut queue = ArtQueue::new();
        assert_eq!(
            queue.request_ids(["ogn-007-298", "OGN-007-298", "unl-189-219"]),
            2
        );
        assert_eq!(queue.outstanding(), 2);
        assert_eq!(queue.request_ids(["ogn-007-298"]), 0);
        let taken = queue.take().unwrap();
        assert_eq!(taken.key(), "riftbound/ogn-007-298");
        let arrival = queue
            .finish(&taken, Ok(Some(b"art".to_vec())))
            .expect("bytes land");
        assert_eq!(arrival.name, "ogn-007-298");
        let mut cache = ArtCache::default();
        assert!(cache.insert(&arrival.name, arrival.bytes));
        assert!(cache.has("OGN-007-298"));
    }

    #[test]
    fn image_bytes_are_sniffed_by_magic_number() {
        assert_eq!(image_extension(&[0x89, b'P', b'N', b'G', 0]), "png");
        assert_eq!(image_extension(b"GIF89a"), "gif");
        assert_eq!(image_extension(b"RIFF....WEBPVP8 "), "webp");
        assert_eq!(image_extension(&[0xff, 0xd8, 0xff]), "jpg");
    }
}
