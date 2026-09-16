use super::{
    content_width, gear_button, margin, tagline, Menu, Screen, HOME_MAX_W, ICON, PRIMARY_H,
};
use crate::net::TableGame;
use crate::settings::{DeckParams, NetParams, Settings, TableParams};

use crate::table::MySeat;
use crate::table::{colors, plate};
use crate::theme;
use crate::viewport::ViewportClass;
use bevy_egui::egui;

pub const HOME_ORDER: [TableGame; 3] = [TableGame::Riftbound, TableGame::Mtg, TableGame::FreeForm];
pub const DECKS_TITLE: &str = "deck editor";
pub const DECKS_LINE: &str = "build, import, rename and share your Riftbound decks";

const TILE: egui::Vec2 = egui::vec2(220.0, 140.0);
const TILE_MIN_W: f32 = 120.0;

pub fn tile_size(available_width: f32, tiles: usize, gap: f32) -> egui::Vec2 {
    let gaps = gap * tiles.saturating_sub(1) as f32;
    let per_tile = (available_width - gaps) / tiles.max(1) as f32;
    let width = per_tile.clamp(TILE_MIN_W, TILE.x);
    egui::vec2(width, TILE.y * width / TILE.x)
}

pub fn tile_row_width(tile: egui::Vec2, tiles: usize, gap: f32) -> f32 {
    tile.x * tiles as f32 + gap * tiles.saturating_sub(1) as f32
}

pub fn tile_columns(available_width: f32, tiles: usize, gap: f32) -> usize {
    let fit = ((available_width + gap) / (TILE.x + gap)).floor() as usize;
    fit.clamp(1, tiles.max(1))
}

pub fn tile_for(available_width: f32) -> egui::Vec2 {
    if available_width >= TILE.x {
        TILE
    } else {
        tile_size(available_width, 1, 0.0)
    }
}

pub fn rows_not_tiles(class: ViewportClass) -> bool {
    class == ViewportClass::PhonePortrait
}

pub struct Resume {
    pub headline: String,
    pub detail: String,
    pub color: [u8; 3],
}

pub fn resume_card(
    info: &crate::table::SessionInfo,
    view: &agni_sim::wire::PluginView,
    zones: &[agni_sim::wire::ZoneDecl],
    colors: &colors::SeatColors,
    me: agni_core::PlayerId,
    last_game: Option<TableGame>,
) -> Option<Resume> {
    if !info.active() {
        return None;
    }
    let game = last_game
        .map(|game| game.label().to_string())
        .unwrap_or_else(|| crate::net::game_of_zones(zones).label().to_string());
    let seat_name =
        |seat: u8| colors::seat_label(&info.roster, colors, me, agni_core::PlayerId(seat)).0;
    let zone_name = |zone: u16| {
        zones
            .iter()
            .find(|decl| decl.id == zone)
            .map(|decl| decl.label.clone())
            .unwrap_or_else(|| format!("zone {zone}"))
    };
    let (_, color, _) = colors::seat_label(&info.roster, colors, me, me);
    let plate = plate::turn_plate(view, me.0, &seat_name, &zone_name);
    let mut detail = format!("return to your table · {game}");
    if let Some(plate) = &plate {
        if !plate.detail.is_empty() {
            detail.push_str(" · ");
            detail.push_str(&plate.detail);
        }
    }
    Some(Resume {
        headline: plate
            .map(|plate| plate.headline)
            .unwrap_or_else(|| info.label(me)),
        detail,
        color,
    })
}

pub fn home_screen(
    ui: &mut egui::Ui,
    class: ViewportClass,
    menu: &mut Menu,
    settings: &mut Settings,
    my_seat: &MySeat,
    net: &NetParams,
    table: &TableParams,
    decks: &DeckParams,
) {
    let screen_w = ui.max_rect().width();
    let width = content_width(class, screen_w, HOME_MAX_W);
    let left = ui.max_rect().min.x + ((screen_w - width) / 2.0).max(margin(class));
    let rect = egui::Rect::from_min_size(
        egui::pos2(left, ui.max_rect().min.y + margin(class)),
        egui::vec2(width, ui.max_rect().height() - 2.0 * margin(class)),
    );
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    column.set_max_width(width);
    column.set_min_width(width);
    let ui = &mut column;
    ui.label(
        egui::RichText::new("kai")
            .size(32.0)
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
    ui.add_space(16.0);
    let resume = resume_card(
        &net.info,
        &table.panel.view,
        &decks.mirror.view.zones,
        &table.seat_colors,
        my_seat.0,
        menu.last_game,
    );
    if let Some(resume) = resume {
        let [r, g, b] = resume.color;
        let button = egui::Button::new(
            egui::RichText::new(format!("{}\n{}", resume.headline, resume.detail)).size(16.0),
        )
        .wrap()
        .min_size(egui::vec2(width, PRIMARY_H + 8.0));
        let response = ui.add(button);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                response.rect.left_top(),
                egui::vec2(4.0, response.rect.height()),
            ),
            2.0,
            egui::Color32::from_rgb(r, g, b),
        );
        if response.clicked() {
            menu.screen = Screen::Table;
        }
        ui.add_space(24.0);
    }
    ui.label(egui::RichText::new("pick a game").color(theme::tokens(ui.ctx()).ink_weak));
    ui.add_space(8.0);
    let gap = ui.spacing().item_spacing.x;
    if rows_not_tiles(class) {
        for game in HOME_ORDER {
            let button = egui::Button::new(
                egui::RichText::new(format!("{}\n{}", game.label(), tagline(game))).size(15.0),
            )
            .wrap()
            .min_size(egui::vec2(width, 64.0));
            if ui.add(button).clicked() {
                menu.open_lobby(game);
            }
        }
    } else {
        let tile = tile_for(width);
        let columns = tile_columns(width, HOME_ORDER.len(), gap);
        for row in HOME_ORDER.chunks(columns) {
            ui.horizontal(|ui| {
                let row_w = tile_row_width(tile, row.len(), gap);
                ui.add_space((width - row_w).max(0.0) / 2.0);
                for &game in row {
                    let response = ui.add_sized(
                        tile,
                        egui::Button::new(
                            egui::RichText::new(format!("{}\n\n{}", game.label(), tagline(game)))
                                .size(15.0),
                        )
                        .wrap(),
                    );
                    if response.clicked() {
                        menu.open_lobby(game);
                    }
                }
            });
        }
    }
    ui.add_space(16.0);
    let decks =
        egui::Button::new(egui::RichText::new(format!("{DECKS_TITLE}\n{DECKS_LINE}")).size(15.0))
            .wrap()
            .min_size(egui::vec2(width, 64.0));
    if ui.add(decks).clicked() {
        menu.open_decks();
    }
    let remaining = rect.bottom() - ui.cursor().top() - ICON - 8.0;
    if remaining > 0.0 {
        ui.add_space(remaining);
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if gear_button(ui) {
            settings.open = true;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_tiles_shrink_to_fit_the_width_and_keep_their_shape() {
        assert_eq!(tile_size(1000.0, 3, 8.0), TILE);
        assert_eq!(tile_row_width(TILE, 3, 8.0), 676.0);
        let narrow = tile_size(400.0, 3, 8.0);
        assert!(narrow.x < TILE.x);
        assert!((tile_row_width(narrow, 3, 8.0) - 400.0).abs() < 0.01);
        assert!((narrow.y / narrow.x - TILE.y / TILE.x).abs() < 0.001);
        assert_eq!(tile_size(0.0, 3, 8.0).x, TILE_MIN_W);
        assert_eq!(tile_size(500.0, 0, 8.0), TILE);
        assert_eq!(tile_row_width(TILE, 0, 8.0), 0.0);
    }

    #[test]
    fn the_tile_row_fits_three_at_1024_and_becomes_rows_on_a_portrait_phone() {
        let gap = 8.0;
        let width = |w: f32, class: ViewportClass| content_width(class, w, HOME_MAX_W);
        assert_eq!(
            tile_columns(width(1280.0, ViewportClass::Desktop), 3, gap),
            3
        );
        assert_eq!(
            tile_columns(width(1024.0, ViewportClass::Tablet), 3, gap),
            3
        );
        assert_eq!(
            tile_columns(width(800.0, ViewportClass::PhoneLandscape), 3, gap),
            3
        );
        assert_eq!(tile_columns(width(600.0, ViewportClass::Tablet), 3, gap), 2);
        assert_eq!(tile_columns(0.0, 3, gap), 1);
        assert!(rows_not_tiles(ViewportClass::PhonePortrait));
        assert!(!rows_not_tiles(ViewportClass::PhoneLandscape));
        assert!(!rows_not_tiles(ViewportClass::Desktop));
        assert_eq!(tile_for(width(1024.0, ViewportClass::Tablet)), TILE);
        assert!(tile_for(200.0).x < TILE.x);
    }

    #[test]
    fn the_enforced_engine_comes_first_on_home() {
        assert_eq!(HOME_ORDER[0], TableGame::Riftbound);
        assert_eq!(HOME_ORDER.len(), TableGame::ALL.len());
        for game in TableGame::ALL {
            assert!(HOME_ORDER.contains(&game));
        }
    }

    #[test]
    fn the_resume_card_exists_only_for_a_live_session() {
        let info = crate::table::SessionInfo::default();
        let view = agni_sim::wire::PluginView::default();
        let colors = colors::SeatColors::default();
        assert!(resume_card(&info, &view, &[], &colors, agni_core::PlayerId(0), None).is_none());
        let live = crate::table::SessionInfo {
            role: crate::table::SessionRole::Host,
            ..Default::default()
        };
        let card = resume_card(
            &live,
            &view,
            &[],
            &colors,
            agni_core::PlayerId(0),
            Some(TableGame::Riftbound),
        )
        .expect("a live session has a card");
        assert_eq!(card.detail, "return to your table · Riftbound");
        assert_eq!(card.headline, "host (player 1)");
    }
}
