use crate::deck::import::{
    DealDeckRequested, ImportedDeck, PlaceBattlefieldRequested, SeatedDeck, SeatedDeckRecord,
};
use crate::deck::thumbs::Thumbs;
use crate::render::art::ArtCache;
use crate::table::plugin_ui::{TrayItem, TrayItems};
use crate::table::SessionInfo;
use agni_riftbound::DeckEntry;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub const TILE_W: f32 = 160.0;
pub const TILE_H: f32 = 112.0;
pub const TILE_MIN_W: f32 = 120.0;

pub fn tile_fit(available_width: f32, gap: f32) -> (usize, egui::Vec2) {
    let columns = (((available_width + gap) / (TILE_MIN_W + gap)).floor() as usize).max(1);
    let width =
        ((available_width - gap * (columns - 1) as f32) / columns as f32).clamp(1.0, TILE_W);
    (columns, egui::vec2(width, width * TILE_H / TILE_W))
}
const CHOOSER_TITLE: &str = "choose your battlefield";

#[derive(Resource, Default, Debug)]
pub struct BattlefieldPrompt {
    pub open: bool,
    pub placed: Option<(u32, usize)>,
}

pub const ROLL_GATE: &str = "choose your battlefield first";

pub fn options(record: &SeatedDeckRecord) -> &[DeckEntry] {
    match &record.deck {
        ImportedDeck::Riftbound(deck) => &deck.battlefields,
        ImportedDeck::Mtg(_) => &[],
    }
}

pub fn held(entries: &[DeckEntry]) -> usize {
    entries.iter().map(|entry| entry.count as usize).sum()
}

pub fn expanded_index(entries: &[DeckEntry], entry: usize) -> usize {
    entries
        .iter()
        .take(entry)
        .map(|entry| entry.count as usize)
        .sum()
}

pub fn asks(record: &SeatedDeckRecord) -> bool {
    held(options(record)) > 1
}

pub fn needs_choice(record: &SeatedDeckRecord) -> bool {
    asks(record) && record.battlefield.is_none()
}

pub fn battlefield_zones(zones: &[agni_sim::wire::ZoneDecl]) -> Vec<agni_core::Zone> {
    zones
        .iter()
        .filter(|decl| decl.name.starts_with(agni_riftbound::BATTLEFIELD_PREFIX))
        .map(|decl| agni_core::Zone::Plugin(decl.id))
        .collect()
}

pub fn placed_on_table(
    table: &agni_core::Table,
    zones: &[agni_sim::wire::ZoneDecl],
    me: agni_core::PlayerId,
) -> bool {
    let bands = battlefield_zones(zones);
    table
        .cards()
        .iter()
        .any(|card| card.owner == me && bands.contains(&card.zone))
}

pub fn legend_on_table(table: &agni_core::Table, me: agni_core::PlayerId) -> bool {
    let zone = agni_core::Zone::Plugin(agni_riftbound::ZONE_LEGEND);
    table
        .cards()
        .iter()
        .any(|card| card.owner == me && card.zone == zone)
}

pub fn placement_pending(
    record: &SeatedDeckRecord,
    table: &agni_core::Table,
    zones: &[agni_sim::wire::ZoneDecl],
    me: agni_core::PlayerId,
) -> bool {
    asks(record) && legend_on_table(table, me) && !placed_on_table(table, zones, me)
}

pub fn chosen_name(record: &SeatedDeckRecord) -> Option<&str> {
    record
        .battlefield
        .and_then(|entry| options(record).get(entry))
        .map(|entry| entry.card.name.as_str())
}

pub fn watch_seating(seated: Res<SeatedDeck>, mut prompt: ResMut<BattlefieldPrompt>) {
    if !seated.is_changed() {
        return;
    }
    if seated.0.as_ref().is_some_and(needs_choice) {
        prompt.open = true;
    }
}

pub fn watch_new_game(
    info: Res<SessionInfo>,
    mut seated: ResMut<SeatedDeck>,
    mut prompt: ResMut<BattlefieldPrompt>,
    mut deals: MessageReader<DealDeckRequested>,
    mut was_active: Local<bool>,
) {
    let dealt = deals.read().next().is_some();
    if dealt {
        if let Some(record) = seated.bypass_change_detection().0.as_mut() {
            record.battlefield_played = true;
        }
    }
    let active = info.active();
    if active && !*was_active {
        if let Some(record) = seated.0.as_mut() {
            if record.battlefield_played && asks(record) {
                record.battlefield = None;
                record.battlefield_played = false;
                prompt.open = true;
            }
        }
    }
    *was_active = active;
}

pub fn place_chosen(
    info: Res<SessionInfo>,
    table: Res<crate::table::GameTable>,
    mirror: Res<crate::table::Mirror>,
    my_seat: Res<crate::table::MySeat>,
    generation: Res<crate::table::DealGeneration>,
    seated: Res<SeatedDeck>,
    mut prompt: ResMut<BattlefieldPrompt>,
    mut placements: MessageWriter<PlaceBattlefieldRequested>,
) {
    if !info.active() {
        return;
    }
    let Some(record) = seated.0.as_ref() else {
        return;
    };
    let Some(pick) = record.battlefield else {
        return;
    };
    if !placement_pending(record, &table.0, &mirror.view.zones, my_seat.0) {
        return;
    }
    let key = (generation.0, pick);
    if prompt.placed == Some(key) {
        return;
    }
    prompt.placed = Some(key);
    placements.write(PlaceBattlefieldRequested);
}

pub fn tray_items(
    entries: &[DeckEntry],
    texture: &dyn Fn(&str) -> Option<egui::TextureId>,
) -> Vec<TrayItem> {
    entries
        .iter()
        .map(|entry| TrayItem {
            label: entry.card.name.clone(),
            texture: texture(&entry.card.riftbound_id),
            frame: None,
            wide: true,
        })
        .collect()
}

pub fn placement_note(placed: Option<usize>) -> String {
    match placed {
        Some(0) => "the first player places no battlefield at this table; the rest stay in your deck box".into(),
        Some(count) if count > 1 => format!(
            "you place {count} battlefields at this table: the one you choose leads and the next in deck order follow"
        ),
        _ => "one battlefield of yours goes onto the table; the rest stay in your deck box".into(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prompt_ui(
    mut contexts: EguiContexts,
    mut prompt: ResMut<BattlefieldPrompt>,
    mut seated: ResMut<SeatedDeck>,
    mut images: ResMut<Assets<Image>>,
    mut art: ResMut<ArtCache>,
    mut registry: ResMut<crate::render::egui_art::EguiArt>,
    menu: Res<crate::menu::Menu>,
    table: Res<crate::table::GameTable>,
    mirror: Res<crate::table::Mirror>,
    my_seat: Res<crate::table::MySeat>,
    mut tray: ResMut<TrayItems>,
    mut thumbs: Local<Thumbs>,
) -> Result {
    let waiting = seated
        .bypass_change_detection()
        .0
        .as_ref()
        .is_some_and(|record| {
            !legend_on_table(&table.0, my_seat.0)
                || placed_on_table(&table.0, &mirror.view.zones, my_seat.0) && !asks(record)
        });
    if !prompt.open || !menu.at_table() || waiting {
        thumbs.drop_all();
        if !tray.is_empty() && tray.title == CHOOSER_TITLE {
            tray.clear();
        }
        return Ok(());
    }
    let Some(record) = seated.bypass_change_detection().0.as_mut() else {
        prompt.open = false;
        return Ok(());
    };
    let entries: Vec<DeckEntry> = options(record).to_vec();
    if held(&entries) <= 1 {
        prompt.open = false;
        return Ok(());
    }
    thumbs.stage(
        &mut contexts,
        &mut registry,
        &mut images,
        entries.iter(),
        &record.faces,
        &mut art,
    );
    let items = tray_items(&entries, &|id| thumbs.id(id));
    let picked = tray.picked.take();
    if tray.title != CHOOSER_TITLE || tray.items != items {
        tray.title = CHOOSER_TITLE.to_string();
        tray.items = items;
    }
    if let Some(index) = picked {
        let record = seated.0.as_mut().expect("the record was read above");
        record.battlefield = Some(index);
        record.battlefield_played = false;
        prompt.open = false;
        tray.clear();
    }
    Ok(())
}

pub const SELECTED_STROKE: egui::Color32 = egui::Color32::from_rgb(140, 240, 96);

pub struct WideTile<'a> {
    pub size: egui::Vec2,
    pub texture: Option<egui::TextureId>,
    pub name: &'a str,
    pub domain: &'a [String],
    pub selected: bool,
    pub dashed: bool,
    pub portrait: bool,
}

pub const PORTRAIT_ART_TOP: f32 = 0.08;

pub fn art_uv(size: egui::Vec2, portrait: bool) -> egui::Rect {
    if !portrait {
        return egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    }
    let band = (size.y / size.x) * (crate::table::dim::CARD_W / crate::table::dim::CARD_H);
    egui::Rect::from_min_max(
        egui::pos2(0.0, PORTRAIT_ART_TOP),
        egui::pos2(1.0, (PORTRAIT_ART_TOP + band).min(1.0)),
    )
}

pub fn wide_tile(ui: &mut egui::Ui, tile: WideTile<'_>) -> egui::Response {
    let tokens = crate::theme::tokens(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(tile.size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    match tile.texture {
        Some(texture) => {
            painter.image(
                texture,
                rect,
                art_uv(tile.size, tile.portrait),
                egui::Color32::WHITE,
            );
        }
        None => {
            painter.rect_filled(rect, 4.0, tokens.surface_2);
            if tile.dashed {
                let dash = egui::Stroke::new(1.0, tokens.ink_weak);
                let inset = rect.shrink(3.0);
                let corners = [
                    inset.left_top(),
                    inset.right_top(),
                    inset.right_bottom(),
                    inset.left_bottom(),
                ];
                for i in 0..4 {
                    painter.add(egui::Shape::dashed_line(
                        &[corners[i], corners[(i + 1) % 4]],
                        dash,
                        6.0,
                        4.0,
                    ));
                }
            }
        }
    }
    if !tile.name.is_empty() {
        let band = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 22.0), rect.max);
        if tile.texture.is_some() {
            painter.rect_filled(band, 0.0, egui::Color32::from_black_alpha(150));
        }
        let ink = if tile.texture.is_some() {
            egui::Color32::WHITE
        } else {
            tokens.ink
        };
        painter.text(
            band.left_center() + egui::vec2(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            tile.name,
            egui::FontId::proportional(12.0),
            ink,
        );
    }
    for (i, domain) in tile.domain.iter().enumerate() {
        let center = rect.right_top() + egui::vec2(-8.0 - 12.0 * i as f32, 8.0);
        painter.circle_filled(center, 4.0, crate::theme::domain_color(&tokens, domain));
    }
    if tile.selected {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, SELECTED_STROKE),
            egui::StrokeKind::Inside,
        );
    }
    if response.hovered() {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, tokens.ink_weak),
            egui::StrokeKind::Inside,
        );
    }
    response
}

pub fn battlefield_step(
    ui: &mut egui::Ui,
    entries: &[DeckEntry],
    chosen: Option<usize>,
    placed: Option<usize>,
    texture: &dyn Fn(&str) -> Option<egui::TextureId>,
) -> Option<usize> {
    let mut picked = None;
    ui.label(egui::RichText::new(placement_note(placed)).weak());
    ui.add_space(4.0);
    let gap = ui.spacing().item_spacing.x;
    let (columns, size) = tile_fit(ui.available_width(), gap);
    for row in entries
        .iter()
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(columns)
    {
        ui.horizontal_top(|ui| {
            for (index, entry) in row {
                ui.vertical(|ui| {
                    ui.set_width(size.x);
                    let selected = chosen == Some(*index);
                    let hit = wide_tile(
                        ui,
                        WideTile {
                            size,
                            texture: texture(&entry.card.riftbound_id),
                            name: "",
                            domain: &[],
                            selected,
                            dashed: false,
                            portrait: false,
                        },
                    );
                    let label = egui::Button::new(entry.card.name.as_str())
                        .selected(selected)
                        .min_size(egui::vec2(size.x, 0.0));
                    if hit.clicked() || ui.add(label).clicked() {
                        picked = Some(*index);
                    }
                });
            }
        });
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_riftbound::ResolvedCard;

    #[test]
    fn the_battlefield_tiles_fit_two_across_a_phone_sheet_and_never_fewer_than_one() {
        let (columns, size) = tile_fit(312.0, 8.0);
        assert_eq!(columns, 2);
        assert_eq!(size.x, 152.0, "two tiles and the gap fill 312");
        assert!((size.y - 152.0 * TILE_H / TILE_W).abs() < 1e-3);
        let (columns, size) = tile_fit(100.0, 8.0);
        assert_eq!((columns, size.x), (1, 100.0));
        let (columns, size) = tile_fit(1000.0, 8.0);
        assert_eq!(columns, 7);
        assert!(
            size.x <= TILE_W && size.x >= TILE_MIN_W,
            "wide sheets never grow a tile past its design size: {}",
            size.x
        );
        assert!(tile_fit(2000.0, 8.0).1.x <= TILE_W);
    }

    #[test]
    fn a_portrait_card_is_cropped_to_its_art_band_and_a_landscape_one_fills_the_tile() {
        let size = egui::vec2(TILE_W, TILE_H);
        let full = art_uv(size, false);
        assert_eq!(full.min, egui::pos2(0.0, 0.0));
        assert_eq!(full.max, egui::pos2(1.0, 1.0));
        let band = art_uv(size, true);
        assert_eq!(band.min.x, 0.0);
        assert_eq!(band.max.x, 1.0);
        assert!((band.min.y - PORTRAIT_ART_TOP).abs() < 1e-6);
        assert!(
            band.max.y > 0.5 && band.max.y < 0.65,
            "the band shows the art, not the text box: {band:?}"
        );
        assert!(art_uv(egui::vec2(10.0, 400.0), true).max.y <= 1.0);
    }

    #[test]
    fn the_placement_note_counts_what_this_seat_places() {
        assert!(placement_note(None).starts_with("one battlefield of yours"));
        assert!(placement_note(Some(1)).starts_with("one battlefield of yours"));
        assert!(placement_note(Some(0)).starts_with("the first player places no battlefield"));
        assert!(placement_note(Some(2)).starts_with("you place 2 battlefields"));
    }

    fn entry(name: &str, count: u32) -> DeckEntry {
        DeckEntry {
            card: ResolvedCard {
                name: name.into(),
                riftbound_id: name.to_lowercase().replace(' ', "-"),
                ..Default::default()
            },
            count,
        }
    }

    #[test]
    fn at_the_table_the_choice_is_a_tray_of_wide_faces() {
        let entries = [entry("Rockfall Path", 1), entry("Sunken Temple", 1)];
        let items = tray_items(&entries, &|id| {
            (id == "rockfall-path").then_some(egui::TextureId::Managed(7))
        });
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "Rockfall Path");
        assert_eq!(items[0].texture, Some(egui::TextureId::Managed(7)));
        assert_eq!(items[1].texture, None);
        assert!(items.iter().all(|item| item.wide && item.frame.is_none()));
    }

    #[test]
    fn placement_is_pending_once_my_legend_is_out_and_until_my_battlefield_lands() {
        use agni_core::{PlayerId, Table, Zone};
        let zones = agni_riftbound::zone_table();
        let bands = battlefield_zones(&zones);
        assert_eq!(bands.len(), agni_riftbound::BATTLEFIELD_COUNT);
        let record = SeatedDeckRecord {
            seat: PlayerId(1),
            deck: ImportedDeck::Riftbound(agni_riftbound::ResolvedDeck {
                battlefields: vec![entry("Rockfall Path", 1), entry("Sunken Temple", 1)],
                ..Default::default()
            }),
            faces: Default::default(),
            battlefield: None,
            battlefield_played: false,
        };
        let me = PlayerId(1);
        let mut table = Table::default();
        assert!(asks(&record));
        assert!(
            !placement_pending(&record, &table, &zones, me),
            "nothing dealt yet, nothing to place"
        );
        table.add(
            PlayerId(0),
            Zone::Plugin(agni_riftbound::ZONE_LEGEND),
            "Irelia - Blade Dancer",
            [0; 3],
        );
        assert!(!placement_pending(&record, &table, &zones, me));
        table.add(
            me,
            Zone::Plugin(agni_riftbound::ZONE_LEGEND),
            "Lillia - Bashful Bloom",
            [0; 3],
        );
        assert!(legend_on_table(&table, me));
        assert!(placement_pending(&record, &table, &zones, me));
        table.add(PlayerId(0), bands[0], "Seat of Power", [0; 3]);
        assert!(
            placement_pending(&record, &table, &zones, me),
            "the opponent's battlefield is not mine"
        );
        table.add(me, bands[1], "Rockfall Path", [0; 3]);
        assert!(placed_on_table(&table, &zones, me));
        assert!(!placement_pending(&record, &table, &zones, me));
        let single = SeatedDeckRecord {
            deck: ImportedDeck::Riftbound(agni_riftbound::ResolvedDeck {
                battlefields: vec![entry("Only", 1)],
                ..Default::default()
            }),
            ..record
        };
        assert!(!asks(&single));
        let bare = Table::default();
        assert!(!placement_pending(&single, &bare, &zones, me));
    }

    #[test]
    fn the_expanded_index_skips_earlier_copies() {
        let entries = [entry("Alpha", 1), entry("Beta", 2), entry("Gamma", 1)];
        assert_eq!(expanded_index(&entries, 0), 0);
        assert_eq!(expanded_index(&entries, 1), 1);
        assert_eq!(expanded_index(&entries, 2), 3);
        assert_eq!(held(&entries), 4);
    }
}
