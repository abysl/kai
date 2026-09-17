use super::*;
use crate::render::art::{ArtCache, ArtRequest};
use bevy::math::Affine2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FELT: &str = "";
pub const ART_PREFIX: &str = "playmat:";
pub const CARD_PREFIX: &str = "card:";
pub const LIBRARY_FILE: &str = "playmats.json";
pub const WEB_PLAYMAT_NOTE: &str =
    "curated art is loaded from the content service; custom links use the gateway's supported sources";
pub const JOURNAL: &str = "playmats";
const THUMB: egui::Vec2 = egui::vec2(120.0, 62.0);
const CAPTION_H: f32 = 22.0;
const TILE_PAD: f32 = 4.0;
pub const TILE: egui::Vec2 = egui::vec2(THUMB.x, THUMB.y + CAPTION_H);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaymatEntry {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub official: bool,
}

pub const CATALOG: [(&str, &str); 4] = [
    (
        "Violet portrait",
        "https://kai.rae.blue/gateway/blob/8094d4c6f18176c4a02ca12f121cff81f9c7348aa05e4bed0c1480df4514b4f2",
    ),
    (
        "Pride painter",
        "https://kai.rae.blue/gateway/blob/2e032aeb2e51c2d188d62f3c23ac5d3363374905c3b18ceb141b7f8c1c35bc72",
    ),
    (
        "Moonlit duet",
        "https://kai.rae.blue/gateway/blob/57399207905ee6819eb8ce447eed8bb05c2740f234775b35c031ea3d04540518",
    ),
    (
        "Snow Moon Ahri",
        "https://kai.rae.blue/gateway/blob/8b3e0e6aeb059a3731a3b58a1fdac0dfeaf328cf2f97840d63d6bc6e937a16ec",
    ),
];

pub fn catalog_hash(url: &str) -> Option<&str> {
    CATALOG
        .iter()
        .any(|(_, source)| *source == url)
        .then(|| url.rsplit('/').next())
        .flatten()
}

pub fn retired_choice(name: &str) -> bool {
    matches!(
        name,
        "Shurima sands" | "Shadow spire" | "Akali" | "Vi and the Rift"
    )
}

pub fn artist_credit(url: &str) -> Option<(&'static str, &'static str)> {
    match CATALOG.iter().position(|(_, source)| *source == url)? {
        3 => Some(("Clya Lyren", "https://clyalyren.com/")),
        _ => Some(("bbi", "https://x.com/totatso")),
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct PlaymatLibrary {
    pub entries: Vec<PlaymatEntry>,
    pub link: String,
    pub link_name: String,
    pub note: Option<String>,
}

impl Default for PlaymatLibrary {
    fn default() -> Self {
        Self {
            entries: CATALOG
                .iter()
                .map(|(name, url)| PlaymatEntry {
                    name: (*name).into(),
                    url: (*url).into(),
                    official: true,
                })
                .collect(),
            link: String::new(),
            link_name: String::new(),
            note: None,
        }
    }
}

pub fn art_name(entry_name: &str) -> String {
    format!("{ART_PREFIX}{entry_name}")
}

pub fn cache_name(choice: &str) -> Option<String> {
    if choice.is_empty() {
        None
    } else if let Some(card) = choice.strip_prefix(CARD_PREFIX) {
        Some(card.to_string())
    } else {
        Some(art_name(choice))
    }
}

pub fn shareable(choice: &str, library: &PlaymatLibrary) -> Option<String> {
    if choice.is_empty() {
        None
    } else if choice.starts_with(CARD_PREFIX) {
        Some(choice.to_string())
    } else {
        library.entry(choice).map(|entry| entry.url.clone())
    }
}

pub fn cache_name_of_shared(shared: &str, library: &PlaymatLibrary) -> Option<String> {
    if let Some(card) = shared.strip_prefix(CARD_PREFIX) {
        return Some(card.to_string());
    }
    if !(shared.starts_with("http://") || shared.starts_with("https://")) {
        return None;
    }
    Some(
        match library.entries.iter().find(|entry| entry.url == shared) {
            Some(entry) => art_name(&entry.name),
            None => format!("{ART_PREFIX}{shared}"),
        },
    )
}

pub fn request_for_shared(shared: &str, library: &PlaymatLibrary) -> Option<ArtRequest> {
    let name = cache_name_of_shared(shared, library)?;
    Some(if let Some(card) = shared.strip_prefix(CARD_PREFIX) {
        ArtRequest::by_name(crate::render::art::ArtGame::Riftbound, card)
    } else {
        ArtRequest::playmat(&name, shared)
    })
}

pub fn label_of(choice: &str) -> String {
    if choice.is_empty() {
        "felt".into()
    } else if let Some(card) = choice.strip_prefix(CARD_PREFIX) {
        format!("{card} (battlefield)")
    } else {
        choice.to_string()
    }
}

pub fn name_from_link(link: &str) -> String {
    let trimmed = link.split(['?', '#']).next().unwrap_or(link);
    let file = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let stem = file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file);
    let cleaned: String = stem
        .chars()
        .map(|c| if c == '-' || c == '_' { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "custom playmat".into()
    } else {
        cleaned.chars().take(40).collect()
    }
}

pub fn cover_uv(image: (f32, f32), mat: (f32, f32), flipped: bool) -> Affine2 {
    let image_aspect = image.0 / image.1.max(1e-3);
    let mat_aspect = mat.0 / mat.1.max(1e-3);
    let (scale_x, scale_y) = if image_aspect > mat_aspect {
        (mat_aspect / image_aspect, 1.0)
    } else {
        (1.0, image_aspect / mat_aspect)
    };
    let offset = Vec2::new((1.0 - scale_x) / 2.0, (1.0 - scale_y) / 2.0);
    let fit = Affine2::from_scale_angle_translation(Vec2::new(scale_x, scale_y), 0.0, offset);
    if flipped {
        Affine2::from_scale_angle_translation(Vec2::new(-1.0, -1.0), 0.0, Vec2::ONE) * fit
    } else {
        fit
    }
}

impl PlaymatLibrary {
    pub fn entry(&self, name: &str) -> Option<&PlaymatEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn add(&mut self, name: &str, url: &str) -> Result<String, String> {
        let url = url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("a playmat link starts with http:// or https://".into());
        }
        let mut name = name.trim().to_string();
        if name.is_empty() {
            name = name_from_link(url);
        }
        if self.entries.iter().any(|entry| entry.name == name) {
            return Err(format!("a playmat named {name} already exists"));
        }
        self.entries.push(PlaymatEntry {
            name: name.clone(),
            url: url.to_string(),
            official: false,
        });
        Ok(name)
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.name != name || entry.official);
        before != self.entries.len()
    }

    pub fn custom(&self) -> impl Iterator<Item = &PlaymatEntry> {
        self.entries.iter().filter(|entry| !entry.official)
    }

    pub fn merge_saved(&mut self, saved: Vec<PlaymatEntry>) {
        for entry in saved {
            if self.entries.iter().all(|held| held.name != entry.name) {
                self.entries.push(PlaymatEntry {
                    official: false,
                    ..entry
                });
            }
        }
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    pub fn load() -> Self {
        let mut library = Self::default();
        let path = crate::os::paths::config_dir().join(LIBRARY_FILE);
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(saved) = serde_json::from_str::<Vec<PlaymatEntry>>(&text) {
                library.merge_saved(saved);
            }
        }
        library
    }

    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    pub fn load() -> Self {
        Self::default()
    }

    pub fn save(&self) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let custom: Vec<&PlaymatEntry> = self.custom().collect();
            let path = crate::os::paths::config_dir().join(LIBRARY_FILE);
            match serde_json::to_string_pretty(&custom) {
                Ok(json) => {
                    if let Err(error) = std::fs::write(&path, json) {
                        bevy::log::warn!(
                            "playmat library save failed ({}): {error}",
                            path.display()
                        );
                    }
                }
                Err(error) => bevy::log::warn!("playmat library serialize failed: {error}"),
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct PlaymatThumbs {
    ids: BTreeMap<String, egui::TextureId>,
}

pub fn stage_thumbs(
    mut contexts: EguiContexts,
    settings: Res<crate::settings::Settings>,
    library: Res<PlaymatLibrary>,
    seated: Res<crate::deck::import::SeatedDeck>,
    mut images: ResMut<Assets<Image>>,
    mut art: ResMut<ArtCache>,
    mut thumbs: ResMut<PlaymatThumbs>,
    mut registry: ResMut<crate::render::egui_art::EguiArt>,
) {
    if !settings.open {
        thumbs.ids.clear();
        return;
    }
    let mut wanted: Vec<String> = library
        .entries
        .iter()
        .map(|entry| art_name(&entry.name))
        .collect();
    wanted.extend(battlefields_of(&seated));
    for name in wanted {
        if thumbs.ids.contains_key(&name) {
            continue;
        }
        let Some(handle) = art.image(&name, &mut images) else {
            continue;
        };
        let texture = registry.texture(&mut contexts, &handle);
        thumbs.ids.insert(name, texture);
    }
}

pub fn battlefields_of(seated: &crate::deck::import::SeatedDeck) -> Vec<String> {
    match seated.0.as_ref().map(|record| &record.deck) {
        Some(crate::deck::import::ImportedDeck::Riftbound(deck)) => deck
            .battlefields
            .iter()
            .map(|entry| entry.card.name.clone())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn restore_saved(library: Res<PlaymatLibrary>, mut art: ResMut<ArtCache>) {
    let Some(dir) = crate::os::paths::store_dir() else {
        return;
    };
    let Ok(store) = spirit_core::BlobStore::open(&dir) else {
        return;
    };
    let journal = agni_importers::art::load_journal(&store, JOURNAL);
    for entry in &library.entries {
        let key = art_name(&entry.name);
        if art.has(&key) {
            continue;
        }
        let Some(hash) = journal.get(&key.to_ascii_lowercase()) else {
            continue;
        };
        if let Ok(bytes) = store.get(*hash) {
            art.insert(&key, bytes);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn ensure_fetched(
    tuning: Res<Tuning>,
    library: Res<PlaymatLibrary>,
    art: Res<ArtCache>,
    settings: Res<crate::settings::Settings>,
    time: Res<bevy::prelude::Time>,
) {
    for entry in library
        .entries
        .iter()
        .filter(|entry| settings.open || entry.name == tuning.playmat)
    {
        let name = art_name(&entry.name);
        if !art.has(&name) {
            crate::net::gateway::request_asset(JOURNAL, &name, &entry.url, time.elapsed_secs_f64());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn fetch_roster_mats(
    info: Res<SessionInfo>,
    library: Res<PlaymatLibrary>,
    my_seat: Res<MySeat>,
    art: Res<ArtCache>,
    time: Res<bevy::prelude::Time>,
) {
    for seat in info.roster.iter().filter(|seat| seat.seat != my_seat.0 .0) {
        let Some(shared) = seat.playmat.as_deref() else {
            continue;
        };
        if shared.starts_with(CARD_PREFIX) {
            continue;
        }
        let Some(name) = cache_name_of_shared(shared, &library) else {
            continue;
        };
        if art.has(&name) {
            continue;
        }
        crate::net::gateway::request_asset(JOURNAL, &name, shared, time.elapsed_secs_f64());
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_fetched(
    tuning: Res<Tuning>,
    library: Res<PlaymatLibrary>,
    art: Res<ArtCache>,
    settings: Res<crate::settings::Settings>,
) {
    if !tuning.is_changed() && !library.is_changed() && !settings.is_changed() {
        return;
    }
    crate::render::art::enqueue(
        library
            .entries
            .iter()
            .filter(|entry| settings.open || entry.name == tuning.playmat)
            .filter(|entry| !art.has(&art_name(&entry.name)))
            .map(|entry| ArtRequest::playmat(art_name(&entry.name), &entry.url)),
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_roster_mats(
    info: Res<SessionInfo>,
    library: Res<PlaymatLibrary>,
    my_seat: Res<MySeat>,
    art: Res<ArtCache>,
) {
    if !info.is_changed() {
        return;
    }
    let wanted: Vec<ArtRequest> = info
        .roster
        .iter()
        .filter(|seat| seat.seat != my_seat.0 .0)
        .filter_map(|seat| seat.playmat.as_deref())
        .filter(|shared| cache_name_of_shared(shared, &library).is_some_and(|name| !art.has(&name)))
        .filter_map(|shared| request_for_shared(shared, &library))
        .collect();
    if !wanted.is_empty() {
        crate::render::art::enqueue(wanted);
    }
}

pub fn mat_of_seat(
    seat: PlayerId,
    my_seat: PlayerId,
    tuning: &Tuning,
    roster: &[agni_net::session::SeatInfo],
    library: &PlaymatLibrary,
) -> Option<String> {
    if seat == my_seat || roster.is_empty() {
        return cache_name(&tuning.playmat);
    }
    agni_net::session::roster_playmat(roster, seat.0)
        .and_then(|shared| cache_name_of_shared(shared, library))
}

fn swatch(
    ui: &mut egui::Ui,
    texture: Option<egui::TextureId>,
    selected: bool,
    caption: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(TILE, egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.style().interact_selectable(&response, selected);
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        4.0,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let art = egui::Rect::from_min_size(rect.min, THUMB);
    match texture {
        Some(texture) => painter.image(
            texture,
            art.shrink(1.0),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        ),
        None => painter.rect_filled(
            art.shrink(1.0),
            4.0,
            if selected {
                egui::Color32::from_gray(90)
            } else {
                egui::Color32::from_gray(58)
            },
        ),
    };
    let galley = painter.layout(
        caption.to_string(),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.text_color(),
        TILE.x - 2.0 * TILE_PAD,
    );
    painter.galley(
        egui::pos2(rect.min.x + TILE_PAD, art.max.y + TILE_PAD),
        galley,
        visuals.text_color(),
    );
    response
}

pub fn playmat_section(
    ui: &mut egui::Ui,
    metrics: &crate::settings::PanelMetrics,
    tuning: &mut ResMut<Tuning>,
    library: &mut ResMut<PlaymatLibrary>,
    thumbs: &PlaymatThumbs,
    art: &ArtCache,
    seated: &crate::deck::import::SeatedDeck,
) {
    let current = tuning.playmat.clone();
    ui.label(egui::RichText::new(format!("playmat: {}", label_of(&current))).weak());
    ui.horizontal_wrapped(|ui| {
        if swatch(ui, None, current.is_empty(), "felt").clicked() {
            tuning.playmat = FELT.into();
        }
        let entries: Vec<PlaymatEntry> = library.entries.clone();
        for entry in entries {
            let name = art_name(&entry.name);
            let selected = current == entry.name;
            let status = if art.has(&name) {
                ""
            } else if selected {
                " · fetching…"
            } else {
                " · not fetched"
            };
            let caption = format!("{}{status}", entry.name);
            let hover = match artist_credit(&entry.url) {
                Some((artist, _)) => format!("{caption}\nArt by {artist}"),
                None => caption.clone(),
            };
            if swatch(ui, thumbs.ids.get(&name).copied(), selected, &caption)
                .on_hover_text(&hover)
                .clicked()
            {
                tuning.playmat = entry.name.clone();
            }
            if !entry.official && ui.small_button("remove").clicked() {
                library.remove(&entry.name);
                library.save();
                if current == entry.name {
                    tuning.playmat = FELT.into();
                }
            }
        }
        for card in battlefields_of(seated) {
            let choice = format!("{CARD_PREFIX}{card}");
            let selected = current == choice;
            let caption = format!("{card} (battlefield)");
            if swatch(ui, thumbs.ids.get(&card).copied(), selected, &caption)
                .on_hover_text(&caption)
                .clicked()
            {
                tuning.playmat = choice.clone();
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("playmat artists:");
        ui.hyperlink_to("Clya Lyren · Ahri", "https://clyalyren.com/");
        ui.hyperlink_to("bbi · portrait, pride and duet", "https://x.com/totatso");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("add from link");
        ui.add(
            egui::TextEdit::singleline(&mut library.link)
                .hint_text("https://…/image.jpg")
                .desired_width(metrics.field_w.min(260.0)),
        );
        ui.add(
            egui::TextEdit::singleline(&mut library.link_name)
                .hint_text("name (optional)")
                .desired_width(metrics.name_w),
        );
        if ui.button("add").clicked() {
            let (link, link_name) = (library.link.clone(), library.link_name.clone());
            match library.add(&link_name, &link) {
                Ok(name) => {
                    library.save();
                    library.link.clear();
                    library.link_name.clear();
                    library.note = Some(format!("added {name} — fetching it now"));
                    tuning.playmat = name;
                }
                Err(error) => library.note = Some(error),
            }
        }
    });
    #[cfg(target_arch = "wasm32")]
    ui.label(egui::RichText::new(WEB_PLAYMAT_NOTE).weak());
    if let Some(note) = &library.note {
        ui.label(egui::RichText::new(note).weak());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_fit_scales_the_longer_axis_and_centres_the_crop() {
        let wide = cover_uv((3000.0, 1000.0), (13.0, 6.0), false);
        let uv = wide.transform_point2(Vec2::new(0.5, 0.5));
        assert!((uv - Vec2::new(0.5, 0.5)).length() < 1e-5);
        let corner = wide.transform_point2(Vec2::ZERO);
        assert!(corner.x > 0.0 && (corner.y).abs() < 1e-5, "{corner:?}");
        let tall = cover_uv((1000.0, 2000.0), (13.0, 6.0), false);
        let corner = tall.transform_point2(Vec2::ZERO);
        assert!(corner.x.abs() < 1e-5 && corner.y > 0.0, "{corner:?}");
        let flipped = cover_uv((3000.0, 1000.0), (13.0, 6.0), true);
        let far = flipped.transform_point2(Vec2::ZERO);
        let near = wide.transform_point2(Vec2::ONE);
        assert!((far - near).length() < 1e-5, "{far:?} vs {near:?}");
    }

    #[test]
    fn a_link_becomes_a_library_entry_with_a_derived_name_and_only_custom_ones_leave() {
        let mut library = PlaymatLibrary::default();
        assert_eq!(library.entries.len(), CATALOG.len());
        assert!(library.add("", "ftp://nope").is_err());
        let name = library
            .add("", "https://example.org/art/Ionia_Grove-4k.jpg?w=1")
            .unwrap();
        assert_eq!(name, "Ionia Grove 4k");
        assert!(library
            .add("Ionia Grove 4k", "https://example.org/x.png")
            .is_err());
        assert!(!library.remove(CATALOG[0].0));
        assert!(library.remove("Ionia Grove 4k"));
        assert_eq!(library.custom().count(), 0);
        library.merge_saved(vec![PlaymatEntry {
            name: "mine".into(),
            url: "https://example.org/mine.png".into(),
            official: true,
        }]);
        assert!(!library.entry("mine").unwrap().official);
    }

    #[test]
    fn choices_map_to_cache_names_and_labels() {
        assert_eq!(cache_name(""), None);
        assert_eq!(cache_name("Akali").as_deref(), Some("playmat:Akali"));
        assert_eq!(
            cache_name("card:Bandle Tree").as_deref(),
            Some("Bandle Tree")
        );
        assert_eq!(label_of(""), "felt");
        assert_eq!(label_of("card:Bandle Tree"), "Bandle Tree (battlefield)");
        assert_eq!(name_from_link("https://x/y/"), "custom playmat");
    }

    #[test]
    fn curated_mats_are_content_addressed_and_replace_the_retired_catalog() {
        let library = PlaymatLibrary::default();
        assert_eq!(library.entries.len(), 4);
        for entry in &library.entries {
            assert!(catalog_hash(&entry.url).is_some());
            assert!(artist_credit(&entry.url).is_some());
            assert!(!retired_choice(&entry.name));
            assert!(shareable(&entry.name, &library).is_some());
        }
        for name in ["Shurima sands", "Shadow spire", "Akali", "Vi and the Rift"] {
            assert!(library.entry(name).is_none());
            assert_eq!(
                Tuning {
                    playmat: name.into(),
                    ..default()
                }
                .normalized()
                .playmat,
                FELT
            );
        }
        assert_eq!(
            Tuning {
                playmat: "my custom mat".into(),
                ..default()
            }
            .normalized()
            .playmat,
            "my custom mat"
        );
        assert!(catalog_hash("https://example.org/art.png").is_none());
        assert_eq!(
            artist_credit(CATALOG[3].1),
            Some(("Clya Lyren", "https://clyalyren.com/"))
        );
        assert_eq!(
            artist_credit(CATALOG[0].1),
            Some(("bbi", "https://x.com/totatso"))
        );
        assert_eq!(artist_credit("https://example.org/art.png"), None);
    }
}
