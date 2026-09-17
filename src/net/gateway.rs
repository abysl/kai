use crate::render::art::ArtCache;

use agni_importers::riftbound::catalog::{
    CardKind, CatalogCard, SUPERTYPE_CHAMPION, SUPERTYPE_SIGNATURE,
};
use agni_importers::riftbound::resolve::canonical_name;
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;

use crate::net::defaults::DefaultPeer;

const PROBE_TIMEOUT_MS: i32 = 4000;

#[derive(Deserialize)]
struct GatewayStatus {
    node_id: String,
}

#[derive(Deserialize)]
struct RefEntry {
    name: String,
    complete: bool,
}

#[derive(Deserialize, Clone, Default)]
struct BridgeCard {
    name: String,
    image: String,
    #[serde(default)]
    riftbound_id: Option<String>,
    #[serde(default)]
    card_type: String,
    #[serde(default)]
    supertype: String,
    #[serde(default)]
    domain: Vec<String>,
    #[serde(default)]
    energy: Option<i64>,
    #[serde(default)]
    might: Option<i64>,
    #[serde(default)]
    power: Option<i64>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    set_id: String,
    #[serde(default)]
    image_url: String,
    #[serde(default)]
    tags: Vec<String>,
}

fn small(value: Option<i64>) -> Option<u8> {
    value.and_then(|value| u8::try_from(value).ok())
}

fn present(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

fn catalog_card(card: &BridgeCard) -> Option<CatalogCard> {
    let riftbound_id = card.riftbound_id.clone()?;
    Some(CatalogCard {
        name: canonical_name(&riftbound_id, &card.name),
        riftbound_id,
        kind: CardKind::parse(&card.card_type),
        champion: card.supertype == SUPERTYPE_CHAMPION,
        image_url: present(&card.image_url),
        energy: small(card.energy),
        power: small(card.power),
        might: small(card.might),
        domain: card.domain.clone(),
        tags: card.tags.clone(),
        signature: card.supertype == SUPERTYPE_SIGNATURE,
        set_id: present(&card.set_id),
        text: present(&card.text),
    })
}

#[derive(Deserialize)]
struct BridgeManifest {
    set: String,
    cards: Vec<BridgeCard>,
}

struct Bridge {
    base: &'static str,
    cards: Vec<BridgeCard>,
}

static BRIDGE: Mutex<Option<Bridge>> = Mutex::new(None);
static RIFTBOUND: Mutex<Vec<BridgeCard>> = Mutex::new(Vec::new());
static RIFTBOUND_GENERATION: AtomicU32 = AtomicU32::new(0);
static STATUS_LINE: Mutex<String> = Mutex::new(String::new());
static ART: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());
static FETCHING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
static ARRIVALS: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());

pub fn request_back(game: crate::render::art::ArtGame, now: f64) {
    let url = format!("https://kai.rae.blue/gateway/blob/{}", game.back_hash());
    request_remote_art(game.back_name(), &url, Some(game.back_hash()), now);
}

fn set_status(line: String) {
    *STATUS_LINE.lock() = line;
}

pub fn status() -> String {
    STATUS_LINE.lock().clone()
}

pub fn boot() {
    set_status("bridging…".into());
    wasm_bindgen_futures::spawn_local(async {
        if let Some(origin) = page_origin() {
            let same_origin = DefaultPeer {
                label: "this site",
                node_id: "",
            };
            if try_gateway(&same_origin, Box::leak(origin.into_boxed_str())).await {
                return;
            }
        }
        set_status(NO_BRIDGE.into());
    });
}

fn page_origin() -> Option<String> {
    let origin = web_sys::window()?.location().origin().ok()?;
    crate::net::defaults::proxied_origin(&origin)
}

pub const NO_BRIDGE: &str = "bundled set — this site proxies no gateway";

async fn try_gateway(gateway: &DefaultPeer, base: &'static str) -> bool {
    let Ok(text) = fetch_text(&format!("{base}/gateway/status"), true).await else {
        return false;
    };
    let Ok(status) = serde_json::from_str::<GatewayStatus>(&text) else {
        return false;
    };
    crate::net::node::seed_when_ready(status.node_id.clone());
    if !gateway.node_id.is_empty() && status.node_id != gateway.node_id {
        warn!(
            "gateway {} answers as node {} but the build expects {}",
            gateway.label, status.node_id, gateway.node_id
        );
    }
    let Ok(refs_text) = fetch_text(&format!("{base}/gateway/refs"), false).await else {
        return false;
    };
    let Ok(refs) = serde_json::from_str::<Vec<RefEntry>>(&refs_text) else {
        return false;
    };
    if refs
        .iter()
        .any(|entry| entry.name == "riftbound" && entry.complete)
    {
        let url = format!("{base}/gateway/ref/riftbound/manifest");
        if let Ok(text) = fetch_text(&url, false).await {
            if let Ok(manifest) = serde_json::from_str::<BridgeManifest>(&text) {
                *RIFTBOUND.lock() = manifest.cards;
                RIFTBOUND_GENERATION.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    let mut cards = Vec::new();
    if let Some(chosen) = refs
        .iter()
        .find(|entry| entry.name == "hob" && entry.complete)
        .or_else(|| refs.iter().find(|entry| entry.complete))
    {
        let url = format!("{base}/gateway/ref/{}/manifest", chosen.name);
        if let Ok(manifest_text) = fetch_text(&url, false).await {
            if let Ok(manifest) = serde_json::from_str::<BridgeManifest>(&manifest_text) {
                info!(
                    "bridged via {} node {} set {}",
                    gateway.label, status.node_id, manifest.set
                );
                cards = manifest.cards;
            }
        }
    }
    set_status(format!(
        "bridged via {} — {} card faces reachable",
        gateway.label,
        cards.len() + RIFTBOUND.lock().len()
    ));
    *BRIDGE.lock() = Some(Bridge { base, cards });
    true
}

pub fn riftbound_generation() -> u32 {
    RIFTBOUND_GENERATION.load(Ordering::SeqCst)
}

pub fn riftbound_catalog() -> Option<Vec<CatalogCard>> {
    let cards: Vec<CatalogCard> = RIFTBOUND.lock().iter().filter_map(catalog_card).collect();
    (!cards.is_empty()).then_some(cards)
}

pub fn gateway_base() -> Option<&'static str> {
    BRIDGE.lock().as_ref().map(|bridge| bridge.base)
}

pub fn riftbound_art(riftbound_id: &str) -> Option<Vec<u8>> {
    let hash = RIFTBOUND
        .lock()
        .iter()
        .find(|card| {
            card.riftbound_id
                .as_deref()
                .is_some_and(|id| id.eq_ignore_ascii_case(riftbound_id))
        })
        .map(|card| card.image.clone())?;
    art_by_hash(hash)
}

pub fn riftbound_art_named(name: &str) -> Option<Vec<u8>> {
    let wanted = crate::render::art::art_key(name);
    let hash = RIFTBOUND
        .lock()
        .iter()
        .find(|card| crate::render::art::art_key(&card.name) == wanted)
        .map(|card| card.image.clone())?;
    art_by_hash(hash)
}

fn art_by_hash(hash: String) -> Option<Vec<u8>> {
    let cached = ART.lock().get(&hash).cloned();
    if cached.is_none() {
        if let Some(base) = gateway_base() {
            request_art(base, hash);
        }
    }
    cached
}

fn request_art(base: &'static str, hash: String) {
    if !FETCHING.lock().insert(hash.clone()) {
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        match fetch_bytes(&format!("{base}/gateway/blob/{hash}")).await {
            Ok(bytes) => ARRIVALS.lock().push((hash, bytes)),
            Err(_) => {
                FETCHING.lock().remove(&hash);
            }
        }
    });
}

static ASSET_NAMES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static ASSET_RETRY: Mutex<BTreeMap<String, f64>> = Mutex::new(BTreeMap::new());
static ASSET_BUSY: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
pub const ASSET_RETRY_SECS: f64 = 3.0;
pub const ASSET_GIVE_UP_SECS: f64 = 90.0;

pub fn request_token_art(name: &str, url: &'static str, now: f64) {
    request_remote_art(name, url, None, now);
}

pub fn request_remote_art(name: &str, url: &str, expected: Option<&str>, now: f64) {
    let key = format!("remote/{name}");
    if ASSET_RETRY.lock().get(&key).is_some_and(|due| now < *due)
        || !ASSET_BUSY.lock().insert(key.clone())
    {
        return;
    }
    let name = name.to_string();
    let url = url.to_string();
    let expected = expected.map(str::to_string);
    wasm_bindgen_futures::spawn_local(async move {
        let outcome = n0_future::time::timeout(std::time::Duration::from_secs(20), async {
            let bytes = fetch_bytes(&url).await.map_err(|error| error.to_string())?;
            crate::render::art::validate_download(&bytes, expected.as_deref())?;
            Ok::<_, String>(bytes)
        })
        .await
        .map_err(|_| "artwork download timed out".to_string())
        .and_then(|result| result);
        match outcome {
            Ok(bytes) => {
                ASSET_NAMES.lock().insert(key.clone(), name);
                ARRIVALS.lock().push((key.clone(), bytes));
                ASSET_RETRY
                    .lock()
                    .insert(key.clone(), now + ASSET_GIVE_UP_SECS);
            }
            Err(error) => {
                warn!("art {name}: {error}");
                ASSET_RETRY
                    .lock()
                    .insert(key.clone(), now + ASSET_RETRY_SECS);
            }
        }
        ASSET_BUSY.lock().remove(&key);
    });
}

pub fn asset_key(journal: &str, name: &str) -> String {
    format!("{journal}/{}", name.trim().to_ascii_lowercase())
}

pub fn request_asset(journal: &str, name: &str, url: &str, now: f64) {
    if let Some(hash) = crate::table::playmat::catalog_hash(url) {
        request_remote_art(name, url, Some(hash), now);
        return;
    }
    let Some(base) = gateway_base() else {
        return;
    };
    let key = asset_key(journal, name);
    {
        let retry = ASSET_RETRY.lock();
        if retry.get(&key).is_some_and(|due| now < *due) {
            return;
        }
    }
    if !ASSET_BUSY.lock().insert(key.clone()) {
        return;
    }
    let cache_name = name.to_string();
    let query = format!(
        "{base}/gateway/resolve/asset?key={}&url={}",
        agni_importers::naming::encode_component(&key),
        agni_importers::naming::encode_component(url)
    );
    wasm_bindgen_futures::spawn_local(async move {
        let outcome = fetch_status_text(&query).await;
        ASSET_BUSY.lock().remove(&key);
        match outcome {
            Ok((200, body)) => {
                let hash = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|value| value["hash"].as_str().map(str::to_string));
                match hash {
                    Some(hash) => {
                        ASSET_NAMES.lock().insert(hash.clone(), cache_name);
                        ASSET_RETRY.lock().remove(&key);
                        request_art(base, hash);
                    }
                    None => {
                        ASSET_RETRY.lock().insert(key, now + ASSET_GIVE_UP_SECS);
                    }
                }
            }
            Ok((202, _)) => {
                ASSET_RETRY.lock().insert(key, now + ASSET_RETRY_SECS);
            }
            Ok((status, body)) => {
                warn!("asset {key}: gateway said {status}: {body}");
                ASSET_RETRY.lock().insert(key, now + ASSET_GIVE_UP_SECS);
            }
            Err(error) => {
                warn!("asset {key}: {error}");
                ASSET_RETRY.lock().insert(key, now + ASSET_RETRY_SECS);
            }
        }
    });
}

pub fn apply_art(mut cache: ResMut<ArtCache>) {
    let arrivals: Vec<(String, Vec<u8>)> = std::mem::take(&mut *ARRIVALS.lock());
    if arrivals.is_empty() {
        return;
    }
    let mut art = ART.lock();
    for (hash, bytes) in &arrivals {
        art.insert(hash.clone(), bytes.clone());
    }
    drop(art);
    {
        let mut names = ASSET_NAMES.lock();
        let named: Vec<(String, Vec<u8>)> = arrivals
            .iter()
            .filter_map(|(hash, bytes)| names.remove(hash).map(|name| (name, bytes.clone())))
            .collect();
        if !named.is_empty() {
            cache.extend(named);
        }
    }
    let guard = BRIDGE.lock();
    let Some(bridge) = guard.as_ref() else {
        return;
    };
    let riftbound = RIFTBOUND.lock();
    let landed: Vec<(String, Vec<u8>)> = bridge
        .cards
        .iter()
        .chain(riftbound.iter())
        .filter_map(|card| {
            arrivals
                .iter()
                .find(|(hash, _)| *hash == card.image)
                .map(|(_, bytes)| (card.name.clone(), bytes.clone()))
        })
        .collect();
    cache.extend(landed);
}

async fn fetch_response(url: &str, probe: bool) -> Result<web_sys::Response, ()> {
    let window = web_sys::window().ok_or(())?;
    let fetched = window.fetch_with_str(url);
    let raced = if probe {
        let timeout = js_sys::Promise::new(&mut |_resolve, reject| {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&reject, PROBE_TIMEOUT_MS);
        });
        js_sys::Promise::race(&js_sys::Array::of2(&fetched, &timeout))
    } else {
        fetched
    };
    let value = wasm_bindgen_futures::JsFuture::from(raced)
        .await
        .map_err(|_| ())?;
    let response: web_sys::Response = wasm_bindgen::JsCast::dyn_into(value).map_err(|_| ())?;
    if response.ok() {
        Ok(response)
    } else {
        Err(())
    }
}

pub(crate) async fn fetch_text(url: &str, probe: bool) -> Result<String, ()> {
    let response = fetch_response(url, probe).await?;
    let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|_| ())?)
        .await
        .map_err(|_| ())?;
    text.as_string().ok_or(())
}

pub(crate) async fn fetch_status_text(url: &str) -> Result<(u16, String), String> {
    let window = web_sys::window().ok_or("no window")?;
    let value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| format!("the gateway fetch failed: {error:?}"))?;
    let response: web_sys::Response = wasm_bindgen::JsCast::dyn_into(value)
        .map_err(|_| "fetch returned no response".to_string())?;
    let status = response.status();
    let text = wasm_bindgen_futures::JsFuture::from(
        response
            .text()
            .map_err(|error| format!("the reply carried no text: {error:?}"))?,
    )
    .await
    .map_err(|error| format!("the reply carried no text: {error:?}"))?;
    Ok((status, text.as_string().unwrap_or_default()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    NotFound,
    Other(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("the gateway has no such blob"),
            Self::Other(error) => f.write_str(error),
        }
    }
}

fn js_error(value: wasm_bindgen::JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

pub(crate) async fn fetch_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    let window = web_sys::window().ok_or_else(|| FetchError::Other("no window".into()))?;
    let value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| FetchError::Other(format!("the fetch failed: {}", js_error(error))))?;
    let response: web_sys::Response = wasm_bindgen::JsCast::dyn_into(value)
        .map_err(|_| FetchError::Other("fetch returned no response".into()))?;
    match response.status() {
        200..=299 => {}
        404 => return Err(FetchError::NotFound),
        status => return Err(FetchError::Other(format!("the gateway answered {status}"))),
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(
        response
            .array_buffer()
            .map_err(|error| FetchError::Other(js_error(error)))?,
    )
    .await
    .map_err(|error| FetchError::Other(js_error(error)))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}
