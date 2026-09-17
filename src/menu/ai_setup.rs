use super::{Menu, Sheet, TOUCH};
use crate::ai::{provider::Provider, seat::AiLobby, setup::Setup};
use crate::settings::{DeckParams, NetParams};
use crate::table::{hud, SessionRole};
use crate::viewport::ViewportClass;
use bevy_egui::egui;

pub fn open(menu: &mut Menu, lobby: &mut AiLobby, start: bool) {
    lobby.setup = Setup::open(lobby);
    menu.open_sheet(Sheet::AiSetup { start });
}

pub fn show(
    context: &egui::Context,
    class: ViewportClass,
    menu: &mut Menu,
    net: &mut NetParams,
    decks: &mut DeckParams,
) {
    let Some(Sheet::AiSetup { start }) = menu.sheet else {
        net.ai.setup = Setup::default();
        return;
    };
    let mut open = true;
    let mut confirmed = false;
    hud::sheet(
        context,
        "AI player settings",
        class,
        hud::Side::Right,
        "AI player settings",
        &mut open,
        |ui| {
            confirmed = body(ui, &mut net.ai.setup, start);
        },
    );
    if confirmed {
        let setup = net.ai.setup.clone();
        if let Err(error) = setup.apply(&mut net.ai) {
            net.ai.setup.error = error;
            return;
        }
        if start {
            match net.info.role {
                SessionRole::Host => {
                    let lobby = net.ai.clone();
                    net.ai.status =
                        super::opponent::start_ai(&mut net.opponent, &decks.seated, &lobby);
                }
                SessionRole::Solo => {
                    if let Some(reason) = crate::net::host_block(&net.choice) {
                        net.ai.setup.error = reason;
                        return;
                    }
                    crate::net::host_table(&mut net.info);
                    net.opponent.pending_ai = true;
                }
                _ => {
                    net.ai.setup.error = "Only the table host can add an AI player".into();
                    return;
                }
            }
        } else {
            net.ai.status = "AI settings saved for the next AI player".into();
        }
        open = false;
    }
    if !open {
        menu.sheet = None;
        net.ai.setup = Setup::default();
    }
}

fn body(ui: &mut egui::Ui, setup: &mut Setup, start: bool) -> bool {
    setup.poll();
    ui.spacing_mut().interact_size.y = TOUCH;
    ui.label("Choose how the AI player makes its moves.");
    ui.checkbox(&mut setup.random, "Random player (no API key or charges)");
    if !setup.random {
        ui.add_space(8.0);
        let mut provider = setup.credentials.provider;
        egui::ComboBox::from_id_salt("AI provider")
            .selected_text(provider.label())
            .show_ui(ui, |ui| {
                for option in Provider::ALL {
                    ui.selectable_value(&mut provider, option, option.label());
                }
            });
        setup.choose_provider(provider);
        ui.label("API key");
        ui.add_sized(
            egui::vec2(ui.available_width(), TOUCH),
            egui::TextEdit::singleline(&mut setup.credentials.key.0)
                .password(true)
                .hint_text(format!("{} API key", provider.label()))
                .desired_width(f32::INFINITY),
        );
        ui.small("Kept in memory until you close Kai or this browser tab. Never saved to disk or sent to Kai's server.");
        ui.add_space(8.0);
        ui.label("Model");
        ui.add_sized(
            egui::vec2(ui.available_width(), TOUCH),
            egui::TextEdit::singleline(&mut setup.search)
                .hint_text("Search model names or IDs")
                .desired_width(f32::INFINITY),
        );
        let search = setup.search.to_lowercase();
        let width = ui.available_width().max(100.0);
        egui::ScrollArea::vertical()
            .id_salt("AI model list")
            .max_height(160.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(width);
                let mut count = 0;
                for model in &setup.models {
                    if !model.id.to_lowercase().contains(&search)
                        && !model.name.to_lowercase().contains(&search)
                    {
                        continue;
                    }
                    count += 1;
                    if ui
                        .add_sized(
                            [width, TOUCH],
                            egui::Button::selectable(
                                setup.model == model.id,
                                format!("{}\n{}", model.name, model.id),
                            )
                            .wrap(),
                        )
                        .clicked()
                    {
                        setup.model = model.id.clone();
                    }
                }
                if count == 0 {
                    ui.label("No matching models");
                }
            });
        if !setup.model.is_empty() {
            ui.small(format!("Selected: {}", setup.model));
        }
        if setup.loading() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading tool-capable models…");
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        } else if ui.button("Refresh model list").clicked() {
            setup.refresh();
        }
        ui.small("Only models that support game-action tools are listed. Loading this list does not verify your key or credit.");
        ui.add_space(8.0);
        ui.label("The selected provider receives the AI's visible game state, card text and your AI chat messages. Playing can incur API charges on your account.");
    }
    if !setup.error.is_empty() {
        ui.label(&setup.error);
    }
    ui.add_space(12.0);
    let validation = setup.validate();
    if let Err(reason) = &validation {
        ui.label(reason);
    }
    ui.add_enabled(
        validation.is_ok(),
        egui::Button::new(if start {
            "Add AI player"
        } else {
            "Save AI settings"
        })
        .min_size(egui::vec2(ui.available_width(), TOUCH)),
    )
    .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_ai_opens_a_sheet_without_confirming_or_starting() {
        let mut menu = Menu::default();
        let mut lobby = AiLobby::default();
        lobby.kind = crate::ai::driver::MindKind::Random;
        open(&mut menu, &mut lobby, true);
        assert_eq!(menu.sheet, Some(Sheet::AiSetup { start: true }));
        assert!(!lobby.configured);
        assert_eq!(menu.ladder(false), super::super::Rung::CloseSheet);
        assert!(menu.sheet.is_none());
    }

    #[test]
    fn settings_render_at_phone_and_desktop_widths_without_credentials() {
        for width in [328.0, 480.0, 960.0] {
            let context = egui::Context::default();
            let mut setup = Setup::default();
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    assert!(!body(ui, &mut setup, true));
                },
            );
            output.drop_without_applying_deltas();
        }
    }
}
