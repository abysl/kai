use bevy::asset::AssetId;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct EguiArt {
    ids: HashMap<AssetId<Image>, egui::TextureId>,
    handles: Vec<Handle<Image>>,
}

impl EguiArt {
    pub fn texture(
        &mut self,
        contexts: &mut EguiContexts,
        handle: &Handle<Image>,
    ) -> egui::TextureId {
        if let Some(id) = self.ids.get(&handle.id()) {
            return *id;
        }
        let texture = contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle.clone()));
        self.handles.push(handle.clone());
        self.ids.insert(handle.id(), texture);
        texture
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}
