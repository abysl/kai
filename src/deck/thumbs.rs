use crate::render::art::ArtCache;
use crate::render::egui_art::EguiArt;
use agni_core::CardFace;
use agni_importers::riftbound::catalog::CatalogCard;
use agni_riftbound::DeckEntry;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::HashMap;

#[derive(Default)]
pub struct Thumbs {
    ids: HashMap<String, egui::TextureId>,
}

impl Thumbs {
    pub fn id(&self, riftbound_id: &str) -> Option<egui::TextureId> {
        self.ids.get(riftbound_id).copied()
    }

    pub fn stage<'a>(
        &mut self,
        contexts: &mut EguiContexts,
        registry: &mut EguiArt,
        images: &mut Assets<Image>,
        entries: impl IntoIterator<Item = &'a DeckEntry>,
        faces: &HashMap<String, CardFace>,
        art: &mut ArtCache,
    ) {
        for entry in entries {
            let key = &entry.card.riftbound_id;
            if self.ids.contains_key(key) {
                continue;
            }
            let Some(face) = faces.get(key) else {
                continue;
            };
            let Some(handle) = art.image(&face.name, images) else {
                continue;
            };
            let texture = registry.texture(contexts, &handle);
            self.ids.insert(key.clone(), texture);
        }
    }

    pub fn stage_cards<'a>(
        &mut self,
        contexts: &mut EguiContexts,
        registry: &mut EguiArt,
        images: &mut Assets<Image>,
        cards: impl IntoIterator<Item = &'a CatalogCard>,
        art: &mut ArtCache,
    ) {
        for card in cards {
            let key = &card.riftbound_id;
            if self.ids.contains_key(key) {
                continue;
            }
            let Some(handle) = art
                .image(&card.name, images)
                .or_else(|| art.image(&card.riftbound_id, images))
            else {
                continue;
            };
            let texture = registry.texture(contexts, &handle);
            self.ids.insert(key.clone(), texture);
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn drop_all(&mut self) {
        self.ids.clear();
    }
}
