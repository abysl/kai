use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{Window, WindowPlugin, WindowResolution};
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use kai::table::chips::{self, Chip, ChipAction};

#[derive(Resource, Default)]
struct Captured(bool);

fn chip(label: &str, action: ChipAction, group: u8) -> Chip {
    Chip {
        label: label.into(),
        reason: None,
        action,
        group,
    }
}

fn destination(chip: &Chip) -> Option<(u16, bool)> {
    match chip.action {
        ChipAction::Play { zone } => Some((zone, false)),
        ChipAction::Hide { zone } => Some((zone, true)),
        _ => None,
    }
}

fn draw(mut contexts: EguiContexts) -> Result {
    let context = contexts.ctx_mut()?;
    egui::Area::new(egui::Id::new("location fixture"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(context, |ui| {
            ui.set_min_size(egui::vec2(800.0, 360.0));
            ui.set_max_size(egui::vec2(800.0, 360.0));
            ui.heading("location picker fixture · 800 × 360");
            ui.label("Hidden-capable hand unit · legal destinations");
            ui.horizontal_wrapped(|ui| {
                let offers = vec![
                    chip("play › Base", ChipAction::Play { zone: 1 }, 1),
                    chip("play › Battlefield 1", ChipAction::Play { zone: 2 }, 1),
                    chip("play › Battlefield 2", ChipAction::Play { zone: 3 }, 1),
                    chip("hide at Battlefield 1 · 1 rune", ChipAction::Hide { zone: 2 }, 2),
                    chip("hide at Battlefield 2 · 1 rune", ChipAction::Hide { zone: 3 }, 2),
                ];
                assert_eq!(
                    offers.iter().filter_map(destination).collect::<Vec<_>>(),
                    [(1, false), (2, false), (3, false), (2, true), (3, true)]
                );
                let all = chips::chips(offers, false, false, false);
                let (shown, more) = chips::shown(&all, false);
                assert_eq!(more, 0);
                for (index, chip) in shown.iter().enumerate() {
                    chips::chip_widget(ui, chips::card_chip_text(Some(index + 1), chip), true);
                }
            });
            ui.separator();
            ui.label("Already face-down card · legal actions");
            ui.horizontal_wrapped(|ui| {
                let offers = vec![
                    chip("play from hidden", ChipAction::PlayFromFacedown, 2),
                    chip("reveal", ChipAction::Reveal, 2),
                ];
                assert!(offers.iter().all(|chip| destination(chip).is_none()));
                let all = chips::chips(offers, false, false, false);
                let (shown, more) = chips::shown(&all, false);
                assert_eq!(more, 0);
                for (index, chip) in shown.iter().enumerate() {
                    chips::chip_widget(ui, chips::card_chip_text(Some(index + 1), chip), true);
                }
            });
            ui.separator();
            ui.label("Battlefield 3 is intentionally absent: it is not legal in this fixture.");
        });
    Ok(())
}

fn capture(
    mut commands: Commands,
    mut captured: ResMut<Captured>,
    frames: Res<bevy::diagnostic::FrameCount>,
) {
    if captured.0 || frames.0 < 30 {
        return;
    }
    captured.0 = true;
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk("/build/agni-takeover/captures/kai-location-fixture.png"));
}

fn camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "kai location fixture".into(),
                    name: Some("kai".into()),
                    resolution: WindowResolution::new(800, 360),
                    ..default()
                }),
                ..default()
            }),
        )
        .add_plugins(EguiPlugin::default())
        .insert_resource(Captured::default())
        .add_systems(Startup, camera)
        .add_systems(EguiPrimaryContextPass, draw)
        .add_systems(Update, capture)
        .run();
}
