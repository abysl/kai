use crate::net::node;
use crate::os::clipboard;
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::BTreeSet;

#[derive(Resource, Default)]
pub struct IdentityPanel {
    pub status: String,
    ticket_input: String,
    texture: Option<egui::TextureHandle>,
}

pub fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

pub fn watch_refs(mut panel: ResMut<IdentityPanel>, mut seen: Local<Option<BTreeSet<String>>>) {
    if node::get().is_none() {
        return;
    }
    let statuses = spirit_node::peers::registry().ref_statuses();
    let previous = seen.take().unwrap_or_else(|| {
        statuses
            .iter()
            .filter(|status| status.complete())
            .map(|status| status.name.clone())
            .collect()
    });
    let (complete, fresh) = spirit_node::peers::newly_complete(&previous, &statuses);
    *seen = Some(complete);
    if !fresh.is_empty() {
        panel.status = format!("synced set {}", fresh.join(", "));
    }
}

const QR_SIDE: f32 = 132.0;

pub fn identity_header(ui: &mut egui::Ui, panel: &mut IdentityPanel, connectivity: &[String]) {
    let Some(node) = node::get() else {
        ui.horizontal_top(|ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(QR_SIDE, QR_SIDE), egui::Sense::hover());
            ui.painter().rect_stroke(
                rect,
                4.0,
                ui.visuals().widgets.noninteractive.bg_stroke,
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "no QR yet",
                egui::FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("identity").strong());
                ui.label(node::status());
            });
        });
        return;
    };
    if panel.texture.is_none() {
        match crate::os::qr::image(&node.ticket) {
            Ok(image) => {
                panel.texture = Some(ui.ctx().load_texture(
                    "kai-identity-qr",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
            Err(error) => panel.status = format!("qr render failed: {error}"),
        }
    }
    let mut clip_status = None;
    ui.horizontal_top(|ui| {
        if let Some(texture) = &panel.texture {
            ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(QR_SIDE, QR_SIDE)));
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(format!("you are {}", short_id(&node.node_id))).strong());
            for line in connectivity {
                ui.label(egui::RichText::new(line).weak());
            }
            ui.horizontal(|ui| {
                if let Some(status) = clipboard::copy_button(ui, "copy ticket", &node.ticket) {
                    clip_status = Some(status);
                }
                if let Some(status) = clipboard::copy_button(ui, "copy node id", &node.node_id) {
                    clip_status = Some(status);
                }
            });
        });
    });
    if let Some(status) = clip_status {
        panel.status = status;
    }
}

pub fn identity_section(ui: &mut egui::Ui, panel: &mut IdentityPanel) {
    if node::get().is_none() {
        ui.label(node::status());
        return;
    }
    let mut clip_status = None;
    #[cfg(target_os = "android")]
    if ui.button("scan identity QR").clicked() {
        if let Err(error) = crate::os::android::request_scan() {
            panel.status = format!("scan launch failed: {error}");
        }
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut panel.ticket_input)
                .hint_text("paste identity ticket")
                .desired_width(200.0),
        );
        if let Some(status) =
            clipboard::paste_button(ui, clipboard::IDENTITY_TICKET, &mut panel.ticket_input)
        {
            clip_status = Some(status);
        }
        let ready = !panel.ticket_input.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("add peer"))
            .clicked()
        {
            let input = panel.ticket_input.trim().to_string();
            panel.ticket_input.clear();
            clip_status = Some(match node::add_peer(&input) {
                Ok(id) => format!("added peer {}", short_id(&id)),
                Err(error) => format!("add peer failed: {error}"),
            });
        }
    });
    if let Some(status) = clip_status {
        panel.status = status;
    }
    if !panel.status.is_empty() {
        ui.label(panel.status.clone());
    } else {
        ui.label(node::status());
    }
}
