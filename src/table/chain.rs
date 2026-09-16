use super::hud::{self, Hud, CHAIN_ROW_H};
use super::*;

pub const CHAIN_ID: &str = "chain";
pub const THUMB_W: f32 = 52.0;
pub const THUMB_H: f32 = 72.0;
pub const RAIL_THUMB_W: f32 = 28.0;
pub const RAIL_THUMB_H: f32 = 40.0;
pub const RAIL_SHOWN: usize = 4;

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainSheet {
    pub open: bool,
}

pub fn rail_rows(rows: &[Row], rail_w: f32) -> (&[Row], usize) {
    let fits = ((rail_w - 80.0) / (RAIL_THUMB_W + 4.0)).floor().max(1.0) as usize;
    split(rows, RAIL_SHOWN.min(fits))
}

#[derive(Resource, Default, Debug)]
pub struct ChainHover(pub Option<CardId>);

#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ChainRects {
    pub panel: Option<egui::Rect>,
    pub rows: Vec<(u16, Vec2)>,
}

impl ChainRects {
    pub fn contains(&self, at: Vec2) -> bool {
        self.panel
            .is_some_and(|panel| panel.contains(egui::pos2(at.x, at.y)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub item: u16,
    pub card: Option<u32>,
    pub seat: u8,
    pub name: String,
}

pub fn rows(
    view: &agni_sim::wire::PluginView,
    table: &Table,
    mirror: &Mirror,
    me: PlayerId,
) -> Vec<Row> {
    if !view.chain.is_empty() {
        return view
            .chain
            .iter()
            .rev()
            .map(|row| Row {
                item: row.item,
                card: row.card,
                seat: row.seat,
                name: row
                    .card
                    .map(|card| plugin_ui::card_label(table, &mirror.view, me, card))
                    .unwrap_or_else(|| "ability".to_string()),
            })
            .collect();
    }
    let Some(chain) = zones::stack_zone(&mirror.view.zones) else {
        return Vec::new();
    };
    let zone = Zone::Plugin(chain);
    let mut stacked = Vec::new();
    for seat in 0..(mirror.view.seats.len().max(1) as u8) {
        for card in table.in_area(PlayerId(seat), zone) {
            stacked.push(Row {
                item: u16::try_from(stacked.len()).unwrap_or(u16::MAX),
                card: Some(card.id.0),
                seat: card.seat.0,
                name: card.face.name.clone(),
            });
        }
    }
    stacked.reverse();
    stacked
}

pub fn rail_hidden(phone: bool, rows: usize, dragging: bool) -> bool {
    phone && rows == 0 && !dragging
}

pub fn rail_rect(rail: egui::Rect, stage: egui::Rect, floating: bool) -> egui::Rect {
    if floating && rail.height() <= 0.0 {
        egui::Rect::from_min_size(
            egui::pos2(rail.min.x, stage.min.y + hud::GAP),
            egui::vec2(rail.width(), hud::CHAIN_RAIL_H),
        )
    } else {
        rail
    }
}

pub fn answer_for(view: &agni_sim::wire::PluginView, me: u8, card: Option<u32>) -> Option<usize> {
    let card = card?;
    if !view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == me)
    {
        return None;
    }
    view.affordances
        .iter()
        .position(|affordance| affordance.enabled && affordance.card == Some(card))
}

pub fn len(view: &agni_sim::wire::PluginView, table: &Table, mirror: &Mirror) -> usize {
    if !view.chain.is_empty() {
        return view.chain.len();
    }
    let Some(chain) = zones::stack_zone(&mirror.view.zones) else {
        return 0;
    };
    let zone = Zone::Plugin(chain);
    (0..(mirror.view.seats.len().max(1) as u8))
        .map(|seat| table.in_area(PlayerId(seat), zone).count())
        .sum()
}

pub fn split(rows: &[Row], shown: usize) -> (&[Row], usize) {
    let shown = shown.min(rows.len());
    (&rows[..shown], rows.len() - shown)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn chain_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    panel: Res<plugin_ui::PluginPanel>,
    seats: hud::Seats,
    tools: Res<plugin_ui::Tools>,
    menu: Res<crate::menu::Menu>,
    tuning: Res<Tuning>,
    held: Res<Held>,
    cards: Query<(&CardView, &CardArt)>,
    mut art: hud::Art,
    mut chain_hover: ResMut<ChainHover>,
    mut chain_rects: ResMut<ChainRects>,
    mut chain_sheet: ResMut<ChainSheet>,
    mut dropped: MessageWriter<CardDropped>,
    mut sender: hud::Sender,
) -> Result {
    let table = &seats.table.0;
    let mirror = &*seats.mirror;
    let my_seat = seats.me();
    if !menu.at_table() || mirror.view.zones.is_empty() {
        if chain_rects.panel.is_some() {
            *chain_rects = ChainRects::default();
        }
        return Ok(());
    }
    let rows = rows(&panel.view, table, mirror, my_seat);
    let rail = hud.0.chain_rail.is_some();
    let dragging = held.card.is_some();
    let rect = rail_rect(
        hud.0.chain,
        hud.0.stage,
        rail && dragging && rows.is_empty(),
    );
    let (full, more) = if rail {
        rail_rows(&rows, rect.width())
    } else {
        split(&rows, hud.0.chain_rows)
    };
    let context = contexts.ctx_mut()?.clone();
    let editable = seats.info.role != SessionRole::Ended && tools.free;
    let base = zones::base_zone(&mirror.view.zones).map(Zone::Plugin);
    let trash = mirror
        .view
        .zones
        .iter()
        .find(|decl| decl.kind == agni_sim::wire::ZoneKind::Discard)
        .map(|decl| Zone::Plugin(decl.id));
    if rail_hidden(rail, rows.len(), dragging) {
        if chain_rects.panel.is_some() {
            *chain_rects = ChainRects::default();
        }
        if chain_sheet.open {
            chain_sheet.open = false;
        }
        return Ok(());
    }
    let mut hover = None;
    let mut answer = None;
    let mut anchors = Vec::new();
    let mut textures: Vec<(u32, egui::TextureId)> = Vec::new();
    for row in if rail && chain_sheet.open {
        &rows[..]
    } else {
        full
    } {
        let Some(card) = row.card else {
            continue;
        };
        let texture = match cards.iter().find(|(view, _)| view.0 .0 == card) {
            Some((_, on_table)) => Some(art.registry.texture(&mut contexts, &on_table.0)),
            None => table
                .get(CardId(card))
                .and_then(|held| art.texture(&mut contexts, &held.face.name)),
        };
        if let Some(texture) = texture {
            textures.push((card, texture));
        }
    }
    if rail {
        let response = hud::slot(&context, CHAIN_ID, rect, |ui| {
            let frame = hud::panel_frame(ui.style());
            let frame = if dragging {
                frame.stroke(egui::Stroke::new(
                    2.0,
                    highlight::rim_color(highlight::RimKind::Play),
                ))
            } else {
                frame
            };
            frame
                .show(ui, |ui| {
                    ui.set_min_width(rect.width() - 16.0);
                    ui.set_max_width(rect.width() - 16.0);
                    ui.horizontal(|ui| {
                        ui.set_min_height(RAIL_THUMB_H);
                        ui.label(
                            egui::RichText::new(format!("chain · {}", rows.len()))
                                .strong()
                                .color(hud::INK),
                        );
                        if rows.is_empty() {
                            ui.label(
                                egui::RichText::new("drop here to play")
                                    .size(12.0)
                                    .color(hud::INK_WEAK),
                            );
                        }
                        let size = egui::vec2(RAIL_THUMB_W, RAIL_THUMB_H);
                        for row in full {
                            let thumb = match row
                                .card
                                .and_then(|card| textures.iter().find(|(id, _)| *id == card))
                            {
                                Some((_, texture)) => {
                                    ui.image(egui::load::SizedTexture::new(*texture, size))
                                }
                                None => {
                                    let (thumb, response) =
                                        ui.allocate_exact_size(size, egui::Sense::hover());
                                    ui.painter().rect_filled(
                                        thumb,
                                        3.0,
                                        egui::Color32::from_gray(58),
                                    );
                                    response
                                }
                            };
                            let color = seats.label(PlayerId(row.seat)).1;
                            ui.painter().rect_stroke(
                                thumb.rect,
                                3.0,
                                egui::Stroke::new(
                                    1.5,
                                    egui::Color32::from_rgb(color[0], color[1], color[2]),
                                ),
                                egui::StrokeKind::Outside,
                            );
                            let center = thumb.rect.center();
                            anchors.push((row.item, Vec2::new(center.x, center.y)));
                        }
                        if more > 0 {
                            ui.label(
                                egui::RichText::new(format!("+{more}"))
                                    .strong()
                                    .color(hud::INK_WEAK),
                            );
                        }
                    });
                })
                .response
        })
        .inner;
        if response.interact(egui::Sense::click()).clicked() && !rows.is_empty() {
            chain_sheet.open = !chain_sheet.open;
        }
        let mut open = chain_sheet.open;
        hud::sheet(
            &context,
            "chain sheet",
            hud.0.class,
            hud::Side::Right,
            &format!("chain · {}", rows.len()),
            &mut open,
            |ui| {
                for (index, row) in rows.iter().enumerate() {
                    let color = seats.label(PlayerId(row.seat)).1;
                    ui.horizontal(|ui| {
                        ui.set_min_height(CHAIN_ROW_H - 8.0);
                        let size = egui::vec2(THUMB_W, THUMB_H);
                        match row
                            .card
                            .and_then(|card| textures.iter().find(|(id, _)| *id == card))
                        {
                            Some((_, texture)) => {
                                ui.image(egui::load::SizedTexture::new(*texture, size));
                            }
                            None => {
                                let (thumb, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                                ui.painter()
                                    .rect_filled(thumb, 3.0, egui::Color32::from_gray(58));
                            }
                        }
                        colors::swatch(ui, color, 10.0, tuning.colour_blind);
                        let text = if index == 0 {
                            format!("{} · top", row.name)
                        } else {
                            row.name.clone()
                        };
                        match answer_for(&panel.view, my_seat.0, row.card) {
                            Some(option) => {
                                if ui.button(text).clicked() {
                                    answer = Some(option);
                                }
                            }
                            None => {
                                ui.label(text);
                            }
                        }
                    });
                }
            },
        );
        if let Some(index) = answer {
            sender.fire(&panel.view.affordances[index]);
        }
        if chain_sheet.open != open {
            chain_sheet.open = open;
        }
        let next = ChainRects {
            panel: Some(response.rect),
            rows: anchors,
        };
        if *chain_rects != next {
            *chain_rects = next;
        }
        if chain_hover.0.is_some() {
            chain_hover.0 = None;
        }
        return Ok(());
    }
    let response = hud::slot(&context, CHAIN_ID, rect, |ui| {
        let frame = hud::panel_frame(ui.style());
        let frame = if dragging {
            frame.stroke(egui::Stroke::new(
                2.0,
                highlight::rim_color(highlight::RimKind::Play),
            ))
        } else {
            frame
        };
        frame.show(ui, |ui| {
            ui.set_min_width(rect.width() - 16.0);
            ui.set_max_width(rect.width() - 16.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            ui.label(
                egui::RichText::new(format!("chain · {}", rows.len()))
                    .strong()
                    .color(hud::INK),
            );
            if rows.is_empty() {
                ui.label(
                    egui::RichText::new(if dragging {
                        "drop here to play"
                    } else {
                        "nothing on the chain"
                    })
                    .size(12.0)
                    .color(hud::INK_WEAK),
                );
            }
            for (index, row) in full.iter().enumerate() {
                let color = seats.label(PlayerId(row.seat)).1;
                let line = ui
                    .horizontal(|ui| {
                        ui.set_min_height(CHAIN_ROW_H - 8.0);
                        let size = egui::vec2(THUMB_W, THUMB_H);
                        match row
                            .card
                            .and_then(|card| textures.iter().find(|(id, _)| *id == card))
                        {
                            Some((_, texture)) => {
                                ui.image(egui::load::SizedTexture::new(*texture, size));
                            }
                            None => {
                                let (thumb, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                                ui.painter()
                                    .rect_filled(thumb, 3.0, egui::Color32::from_gray(58));
                            }
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                colors::swatch(ui, color, 10.0, tuning.colour_blind);
                                let text = if index == 0 {
                                    format!("{} · top", row.name)
                                } else {
                                    row.name.clone()
                                };
                                ui.label(egui::RichText::new(text).color(hud::INK));
                            });
                            if let Some(card) =
                                row.card.filter(|_| editable && row.seat == my_seat.0)
                            {
                                let mut resolve = |to: Option<Zone>, label: &str| {
                                    let Some(to) = to else {
                                        return;
                                    };
                                    if ui.small_button(label).clicked() {
                                        let seat = PlayerId(row.seat);
                                        dropped.write(CardDropped {
                                            card: CardId(card),
                                            to,
                                            seat,
                                            index: table.in_area(seat, to).count(),
                                            hidden: false,
                                        });
                                    }
                                };
                                resolve(base, "resolve to base");
                                resolve(trash, "resolve to trash");
                            }
                        });
                    })
                    .response;
                let center = line.rect.center();
                anchors.push((row.item, Vec2::new(center.x, center.y)));
                if line.hovered() {
                    hover = row.card.map(CardId);
                }
                if let Some(index) = answer_for(&panel.view, my_seat.0, row.card) {
                    let hit = ui.interact(
                        line.rect,
                        egui::Id::new(("chain answer", row.item)),
                        egui::Sense::click(),
                    );
                    ui.painter().rect_stroke(
                        line.rect.expand(2.0),
                        4.0,
                        egui::Stroke::new(2.0, highlight::rim_color(highlight::RimKind::Answer)),
                        egui::StrokeKind::Outside,
                    );
                    if hit.clicked() {
                        answer = Some(index);
                    }
                }
            }
            if more > 0 {
                ui.label(
                    egui::RichText::new(format!("and {more} more"))
                        .size(12.0)
                        .color(hud::INK_WEAK),
                );
            }
        });
    })
    .response;
    let next = ChainRects {
        panel: Some(response.rect),
        rows: anchors,
    };
    if *chain_rects != next {
        *chain_rects = next;
    }
    if chain_hover.0 != hover {
        chain_hover.0 = hover;
    }
    if let Some(index) = answer {
        sender.fire(&panel.view.affordances[index]);
    }
    Ok(())
}

pub fn on_drop_on_chain(
    event: On<Pointer<DragEnd>>,
    rects: Res<ChainRects>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    cards: Query<&CardView>,
    mut dropped: MessageWriter<CardDropped>,
) {
    let Ok(view) = cards.get(event.event_target()) else {
        return;
    };
    let at = event.pointer_location.position;
    if !rects.contains(at) {
        return;
    }
    if table.get(view.0).is_none_or(|card| card.owner != my_seat.0) {
        return;
    }
    if let Some(drop) = chain_drop(&table, &mirror, my_seat.0, view.0) {
        dropped.write(drop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{ChainRow, PluginView};

    fn row(item: u16, card: Option<u32>, seat: u8) -> ChainRow {
        ChainRow { item, card, seat }
    }

    #[test]
    fn the_phone_rail_shows_four_thumbnails_at_most_and_counts_the_rest() {
        let rows: Vec<Row> = (0..6)
            .map(|index| Row {
                item: index,
                card: Some(u32::from(index)),
                seat: 0,
                name: format!("card {index}"),
            })
            .collect();
        let (shown, more) = rail_rows(&rows, 360.0);
        assert_eq!(shown.len(), RAIL_SHOWN);
        assert_eq!(more, 2);
        let (narrow, more) = rail_rows(&rows, 120.0);
        assert_eq!(narrow.len(), 1);
        assert_eq!(more, 5);
        let (few, none) = rail_rows(&rows[..2], 360.0);
        assert_eq!(few.len(), 2);
        assert_eq!(none, 0);
    }

    #[test]
    fn a_chain_row_wearing_an_answer_candidate_is_the_button_for_that_answer() {
        use agni_sim::wire::{Affordance, AffordanceKind, PromptSummary};
        use serde_bytes::ByteBuf;
        let pick = |card: u32| Affordance {
            label: format!("{{card {card}}}"),
            hotkey: None,
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![card as u8]),
            card: Some(card),
        };
        let view = PluginView {
            prompt: Some(PromptSummary {
                seat: 0,
                why: "Defy: choose a spell to counter".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            affordances: vec![pick(41), pick(52)],
            chain: vec![row(3, Some(41), 1), row(4, Some(60), 0)],
            ..Default::default()
        };
        assert_eq!(answer_for(&view, 0, Some(41)), Some(0));
        assert_eq!(answer_for(&view, 0, Some(60)), None);
        assert_eq!(answer_for(&view, 0, None), None);
        assert_eq!(
            answer_for(&view, 1, Some(41)),
            None,
            "the other seat's row is not their question"
        );
        let rail = egui::Rect::from_min_size(egui::pos2(8.0, 208.0), egui::vec2(344.0, 0.0));
        let stage = egui::Rect::from_min_size(egui::pos2(0.0, 216.0), egui::vec2(360.0, 400.0));
        let floating = rail_rect(rail, stage, true);
        assert_eq!(floating.height(), hud::CHAIN_RAIL_H);
        assert_eq!(floating.min.y, stage.min.y + hud::GAP);
        assert_eq!(rail_rect(rail, stage, false), rail);
    }

    #[test]
    fn the_phone_rail_is_absent_while_the_chain_is_empty_and_nothing_is_held() {
        assert!(rail_hidden(true, 0, false));
        assert!(!rail_hidden(true, 1, false));
        assert!(
            !rail_hidden(true, 0, true),
            "a held card needs its drop target"
        );
        assert!(
            !rail_hidden(false, 0, false),
            "the desktop panel always shows"
        );
    }

    #[test]
    fn the_panel_lists_the_chain_newest_first_and_folds_the_rest() {
        let mut table = Table::new();
        let zones = agni_riftbound::zone_table();
        let base = Zone::Plugin(agni_riftbound::ZONE_BASE);
        let poro = table.add(PlayerId(0), base, "Punching Poro", [0; 3]);
        let mirror = Mirror {
            view: agni_sim::view::TableView {
                zones,
                ..Default::default()
            },
            ..Default::default()
        };
        let view = PluginView {
            chain: vec![
                row(1, Some(poro.0), 0),
                row(2, None, 1),
                row(3, Some(999), 1),
                row(4, Some(poro.0), 0),
            ],
            ..Default::default()
        };
        let rows = rows(&view, &table, &mirror, PlayerId(0));
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].item, 4, "the top of the chain leads");
        assert_eq!(rows[0].name, "Punching Poro");
        assert_eq!(rows[1].name, "card 999");
        assert_eq!(rows[2].name, "ability");
        assert_eq!(rows[2].seat, 1);
        let (full, more) = split(&rows, 3);
        assert_eq!(full.len(), 3);
        assert_eq!(more, 1);
        let (full, more) = split(&rows, 0);
        assert!(full.is_empty());
        assert_eq!(more, 4);
    }

    #[test]
    fn a_free_table_reads_the_stack_zone_when_the_view_carries_no_chain() {
        let mut table = Table::new();
        let zones = agni_riftbound::zone_table();
        let chain = Zone::Plugin(zones::stack_zone(&zones).unwrap());
        let first = table.add(PlayerId(0), chain, "Cleave", [0; 3]);
        let second = table.add(PlayerId(1), chain, "Back Off", [0; 3]);
        let mirror = Mirror {
            view: agni_sim::view::TableView {
                zones,
                seats: vec![
                    agni_sim::log::Seat {
                        seat: 0,
                        name: "rae".into(),
                    },
                    agni_sim::log::Seat {
                        seat: 1,
                        name: "claude".into(),
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        let rows = rows(&PluginView::default(), &table, &mirror, PlayerId(0));
        assert_eq!(rows.len(), 2);
        assert_eq!(len(&PluginView::default(), &table, &mirror), 2);
        assert_eq!(
            len(&PluginView::default(), &Table::new(), &Mirror::default()),
            0
        );
        assert_eq!(rows[0].card, Some(second.0));
        assert_eq!(rows[1].card, Some(first.0));
        let empty = ChainRects::default();
        assert!(!empty.contains(Vec2::new(5.0, 5.0)));
        let panel = ChainRects {
            panel: Some(egui::Rect::from_min_max(
                egui::pos2(1000.0, 10.0),
                egui::pos2(1200.0, 300.0),
            )),
            rows: Vec::new(),
        };
        assert!(panel.contains(Vec2::new(1100.0, 100.0)));
        assert!(!panel.contains(Vec2::new(900.0, 100.0)));
    }
}
