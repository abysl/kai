use super::{NetParams, Settings, TableParams};
use crate::deck::import;
use crate::engine::modules;
use crate::net::{node, TableGame};

use crate::telemetry;
use crate::theme;
use bevy_egui::egui;

pub fn version_key(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

pub fn newest_plugin<'a>(
    rows: &'a [modules::ModuleRow],
    name: &str,
) -> Option<&'a modules::ModuleRow> {
    rows.iter()
        .filter(|row| row.name == name && row.role == spirit_sdk::modules::Role::Plugin)
        .max_by(|a, b| {
            a.held
                .cmp(&b.held)
                .then_with(|| version_key(&a.version).cmp(&version_key(&b.version)))
        })
}

pub fn plugin_status(rows: &[modules::ModuleRow], game: TableGame) -> String {
    let Some(plugin_ref) = game.plugin_ref() else {
        return "no plugin — a free-form table with no zones".into();
    };
    match newest_plugin(rows, plugin_ref) {
        Some(row) => format!(
            "modules/{} v{} · {} · {}",
            row.name,
            row.version,
            row.hash_hex.chars().take(8).collect::<String>(),
            row.source
        ),
        None => {
            format!("modules/{plugin_ref} — not in the store yet (bundled copy seeds on launch)")
        }
    }
}

pub fn footer_line(rows: &[modules::ModuleRow]) -> String {
    let riftbound = newest_plugin(rows, "riftbound")
        .map(|row| format!(" · modules/riftbound v{}", row.version))
        .unwrap_or_default();
    format!("{}{riftbound}", super::build_line())
}

fn title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(text)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
}

pub fn advanced_tab(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    menu: &mut crate::menu::Menu,
    table: &mut TableParams,
    net: &mut NetParams,
    modules_panel: &modules::ModulesPanel,
    telemetry_panel: &mut telemetry::TelemetryPanel,
) {
    ui.checkbox(&mut settings.developer, "show developer settings");
    if !super::developer_sections_shown(settings.developer) {
        ui.label(
            egui::RichText::new("tuning sliders, modules, telemetry, the AI model and the sample hand live behind that toggle")
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(footer_line(modules_panel.rows()))
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
        return;
    }
    title(ui, "table tuning");
    let redeal_allowed = crate::table::redeal_allowed(
        table.tools.free,
        crate::table::hud::between_games(&net.info, &table.panel.view),
    );
    egui::ScrollArea::vertical()
        .id_salt("tuning sliders")
        .scroll_source(
            egui::scroll_area::ScrollSource::SCROLL_BAR
                | egui::scroll_area::ScrollSource::MOUSE_WHEEL,
        )
        .max_height(f32::INFINITY)
        .show(ui, |ui| {
            crate::table::tuning_section(
                ui,
                &mut table.tuning,
                redeal_allowed,
                #[cfg(not(target_arch = "wasm32"))]
                &mut table.redeal,
            );
        });
    if !net.info.active()
        && ui
            .button("open a tuning table")
            .on_hover_text("the empty felt without a session, for the sliders")
            .clicked()
    {
        menu.screen = crate::menu::Screen::Table;
        settings.open = false;
    }
    title(ui, "card art");
    import::full_set_controls(ui);
    if let Some(note) = crate::deck::catalog::tags_note() {
        ui.label(
            egui::RichText::new(note)
                .color(theme::tokens(ui.ctx()).amber)
                .small(),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        title(ui, "AI model");
        crate::table::drawer::model_controls(ui, &mut net.ai);
    }
    title(ui, "modules");
    for game in TableGame::ALL {
        ui.label(
            egui::RichText::new(format!(
                "{}: {}",
                game.label(),
                plugin_status(modules_panel.rows(), game)
            ))
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
        );
    }
    modules::modules_section(ui, modules_panel);
    title(ui, "node");
    ui.label(
        egui::RichText::new(format!("{} · {}", node::status(), net.peers.summary()))
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
    title(ui, "telemetry");
    telemetry::telemetry_section(ui, telemetry_panel);
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(footer_line(modules_panel.rows()))
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<modules::ModuleRow> {
        vec![modules::ModuleRow {
            name: "riftbound".into(),
            role: spirit_sdk::modules::Role::Plugin,
            version: "0.2.0".into(),
            hash_hex: "deadbeefcafe".into(),
            signer: None,
            source: "store",
            held: true,
        }]
    }

    #[test]
    fn the_plugin_status_names_the_store_row_or_says_it_is_missing() {
        let rows = rows();
        assert_eq!(
            plugin_status(&rows, TableGame::Riftbound),
            "modules/riftbound v0.2.0 · deadbeef · store"
        );
        assert!(plugin_status(&rows, TableGame::Mtg).contains("not in the store yet"));
        assert!(plugin_status(&rows, TableGame::FreeForm).starts_with("no plugin"));
    }

    #[test]
    fn the_footer_carries_the_version_the_wire_and_the_plugin() {
        let line = footer_line(&rows());
        assert!(line.starts_with(&format!("v{} · wire ", super::super::VERSION)));
        assert!(line.ends_with(" · modules/riftbound v0.2.0"));
        assert_eq!(footer_line(&[]), super::super::build_line());
    }

    #[test]
    fn the_newest_held_version_is_the_one_reported_not_the_first_row() {
        let mut many = rows();
        for (version, held) in [("0.11.0", true), ("0.9.1", true), ("0.12.0", false)] {
            many.push(modules::ModuleRow {
                version: version.into(),
                hash_hex: format!("{version}00000000"),
                held,
                ..rows().remove(0)
            });
        }
        assert_eq!(newest_plugin(&many, "riftbound").unwrap().version, "0.11.0");
        assert!(footer_line(&many).ends_with(" · modules/riftbound v0.11.0"));
        assert!(version_key("0.11.0") > version_key("0.9.1"));
    }
}
