use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::WindowCreated;
use bevy::winit::WINIT_WINDOWS;
use winit::window::Icon;

const PNG: &[u8] = include_bytes!("../../assets/icon/kai-256.png");

pub fn apply(mut created: MessageReader<WindowCreated>, _main_thread: NonSendMarker) {
    let windows: Vec<Entity> = created.read().map(|event| event.window).collect();
    if windows.is_empty() {
        return;
    }
    let Some(icon) = decode() else {
        warn!("the embedded window icon failed to decode");
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        for entity in windows {
            if let Some(window) = winit_windows.get_window(entity) {
                window.set_window_icon(Some(icon.clone()));
            }
        }
    });
}

fn decode() -> Option<Icon> {
    let image = image::load_from_memory(PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}
