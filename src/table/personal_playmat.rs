use super::*;
use crate::render::art::ArtCache;
use agni_net::personal_asset::{self, Ticket};
use image::{ImageDecoder, ImageReader};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

pub const CHOICE: &str = "Your picture";
pub const CACHE: &str = "personal-playmat:own";

#[derive(Resource, Default)]
pub struct PersonalPlaymat {
    bytes: Option<Vec<u8>>,
    pub shared: Option<String>,
    loaded: bool,
    requests: BTreeMap<u8, Request>,
    arrivals: Arc<Mutex<Vec<(u8, String, Result<Vec<u8>, String>)>>>,
}

struct Request {
    shared: String,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    retry_at: f64,
}

fn decode(bytes: &[u8], max_side: u32) -> Result<image::DynamicImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err("Choose a PNG, JPEG, or WebP picture".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max_side);
    limits.max_image_height = Some(max_side);
    limits.max_alloc = Some(128 << 20);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > 24_000_000 {
        return Err("Choose a picture with at most 24 million pixels".into());
    }
    image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())
}

pub fn normalize(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.is_empty() || bytes.len() > crate::os::picture::MAX_INPUT {
        return Err("Choose a picture smaller than 16 MiB".into());
    }
    let image = decode(bytes, 8192)?.thumbnail(2048, 2048).to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 85)
        .encode_image(&image)
        .map_err(|e| e.to_string())?;
    if output.len() > personal_asset::MAX_BYTES {
        return Err("That picture is too detailed; choose a smaller image".into());
    }
    Ok(output)
}

fn valid_received(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= personal_asset::MAX_BYTES && decode(bytes, 2048).is_ok()
}

pub fn shared_cache(shared: &str) -> Option<String> {
    let ticket = Ticket::parse(shared)?;
    Some(format!(
        "personal-playmat:{}",
        spirit_core::BlobHash::from_bytes(ticket.hash)
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn load() -> Option<Vec<u8>> {
    use std::io::Read;
    let file =
        std::fs::File::open(crate::os::paths::config_dir().join("personal-playmat.jpg")).ok()?;
    let mut bytes = Vec::new();
    file.take((personal_asset::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    valid_received(&bytes).then_some(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
fn save(bytes: &[u8]) -> Result<(), String> {
    let dir = crate::os::paths::config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("personal-playmat.jpg"), bytes).map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
fn load() -> Option<Vec<u8>> {
    use base64::Engine;
    let text = web_sys::window()?
        .local_storage()
        .ok()??
        .get_item("kai.personal-playmat")
        .ok()??;
    if text.len() > personal_asset::MAX_BYTES * 2 {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .ok()?;
    valid_received(&bytes).then_some(bytes)
}

#[cfg(target_arch = "wasm32")]
fn save(bytes: &[u8]) -> Result<(), String> {
    use base64::Engine;
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("Browser storage unavailable")?;
    storage
        .set_item(
            "kai.personal-playmat",
            &base64::engine::general_purpose::STANDARD.encode(bytes),
        )
        .map_err(|_| "Browser storage is full; picture available until this tab closes".into())
}

pub fn update(
    mut personal: ResMut<PersonalPlaymat>,
    mut tuning: ResMut<Tuning>,
    mut library: ResMut<playmat::PlaymatLibrary>,
    mut art: ResMut<ArtCache>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    time: Res<Time>,
) {
    if !personal.loaded {
        personal.loaded = true;
        personal.bytes = load();
        if let Some(bytes) = &personal.bytes {
            art.insert(CACHE, bytes.clone());
        }
    }
    if let Some(selection) = crate::os::picture::take() {
        match selection.and_then(|bytes| normalize(&bytes)) {
            Ok(bytes) => {
                library.note = Some(match save(&bytes) {
                    Ok(()) => "Picture selected. Shared only with players at your table.".into(),
                    Err(error) => error,
                });
                art.remove(CACHE);
                art.insert(CACHE, bytes.clone());
                personal.bytes = Some(bytes);
                personal.shared = None;
                if let Some(node) = crate::net::node::get() {
                    node.personal_asset.clear();
                }
                tuning.playmat = CHOICE.into();
            }
            Err(error) => library.note = Some(error),
        }
    }
    let Some(node) = crate::net::node::get() else {
        return;
    };
    let live = matches!(info.role, SessionRole::Host | SessionRole::Client);
    if live && tuning.playmat == CHOICE {
        if personal.shared.is_none() {
            personal.shared = personal.bytes.as_ref().and_then(|bytes| {
                node.personal_asset
                    .publish(node.endpoint.id(), bytes.clone())
                    .ok()
                    .map(|ticket| ticket.encode())
            });
        }
    } else {
        node.personal_asset.clear();
        personal.shared = None;
    }
    let wanted: BTreeMap<u8, String> = info
        .roster
        .iter()
        .filter(|seat| {
            live && !tuning.disable_opponent_playmat && seat.connected && seat.seat != my_seat.0 .0
        })
        .filter_map(|seat| Some((seat.seat, seat.playmat.as_ref()?.clone())))
        .filter(|(_, shared)| Ticket::parse(shared).is_some())
        .take(8)
        .collect();
    personal.requests.retain(|seat, request| {
        let keep = wanted.get(seat) == Some(&request.shared);
        if !keep {
            if let Some(cancel) = request.cancel.take() {
                let _ = cancel.send(());
            }
            if let Some(name) = shared_cache(&request.shared) {
                art.remove(&name);
            }
        }
        keep
    });
    let arrivals = std::mem::take(&mut *personal.arrivals.lock());
    for (seat, shared, result) in arrivals {
        if wanted.get(&seat) != Some(&shared) {
            continue;
        }
        if let Some(request) = personal.requests.get_mut(&seat) {
            request.cancel = None;
        }
        if let Ok(bytes) = result {
            if valid_received(&bytes) {
                if let Some(name) = shared_cache(&shared) {
                    art.insert(&name, bytes);
                }
            }
        }
    }
    for (seat, shared) in wanted {
        let name = shared_cache(&shared).unwrap();
        if art.has(&name)
            || personal.requests.get(&seat).is_some_and(|request| {
                request.cancel.is_some() || request.retry_at > time.elapsed_secs_f64()
            })
        {
            continue;
        }
        let ticket = Ticket::parse(&shared).unwrap();
        let endpoint = node.endpoint.clone();
        let arrivals = personal.arrivals.clone();
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        personal.requests.insert(
            seat,
            Request {
                shared: shared.clone(),
                cancel: Some(cancel),
                retry_at: time.elapsed_secs_f64() + 30.0,
            },
        );
        node.spawn(async move {
            tokio::select! {
                _ = cancelled => {}
                result = personal_asset::fetch(&endpoint, &ticket) => {
                    let mut arrivals = arrivals.lock();
                    if arrivals.len() < 8 { arrivals.push((seat, shared, result)); }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_bounded_and_reencoded() {
        assert!(normalize(b"<html>not artwork</html>").is_err());
        assert!(normalize(&vec![0; crate::os::picture::MAX_INPUT + 1]).is_err());
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let encoded = normalize(png.get_ref()).unwrap();
        assert!(encoded.starts_with(&[0xff, 0xd8]));
        assert!(valid_received(&encoded));
        assert!(shared_cache("https://example.org/random").is_none());
    }
}
