use super::hud::{self, Hud};
use super::*;
use agni_sim::wire::{CounterScope, CounterTarget};
use bevy::ecs::system::SystemParam;

pub const CAPTION_SIZE: f32 = 12.0;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Caption {
    pub lines: Vec<(String, Tint)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tint {
    #[default]
    Plain,
    Above,
    Below,
    Weak,
}

pub fn caption(
    card: &agni_core::Card,
    mirror: &Mirror,
    me: PlayerId,
    statuses: &[(&str, [u8; 3])],
) -> Caption {
    let face = &card.face;
    let mut lines = Vec::new();
    let mut head: Vec<String> = Vec::new();
    if let Some(kind) = &face.kind {
        head.push(kind.clone());
    }
    if let Some(energy) = face.energy {
        head.push(format!("{energy} energy"));
    }
    if let Some(power) = face.power {
        head.push(format!("{power} power"));
    }
    let facedown_mine = super::sync::face_down_mine(card, mirror, me);
    if facedown_mine {
        let mut line = format!("face down · {}", face.name);
        if !head.is_empty() {
            line.push_str(" · ");
            line.push_str(&head.join(" · "));
        }
        lines.push((line, Tint::Weak));
    } else if !head.is_empty() {
        lines.push((head.join(" · "), Tint::Plain));
    }
    if let Some(printed) = face.might {
        let bonus = mirror
            .view
            .counter_table
            .iter()
            .find(|decl| decl.name == "might" && decl.scope == CounterScope::Card)
            .and_then(|decl| mirror.view.counter(CounterTarget::Card(card.id.0), decl.id))
            .unwrap_or(0);
        let current = i32::from(printed) + bonus;
        let (text, tint) = match current.cmp(&i32::from(printed)) {
            std::cmp::Ordering::Greater => (format!("might {printed} › {current}"), Tint::Above),
            std::cmp::Ordering::Less => (format!("might {printed} › {current}"), Tint::Below),
            std::cmp::Ordering::Equal => (format!("might {printed}"), Tint::Plain),
        };
        lines.push((text, tint));
    }
    for (label, _) in statuses {
        lines.push((label.to_lowercase(), Tint::Weak));
    }
    Caption { lines }
}

pub fn tint_color(tint: Tint) -> egui::Color32 {
    match tint {
        Tint::Plain => hud::INK,
        Tint::Above => hud::GREEN,
        Tint::Below => hud::DANGER,
        Tint::Weak => hud::INK_WEAK,
    }
}

pub fn face_size(slot: egui::Rect, wide: bool, scale: f32) -> egui::Vec2 {
    let width = slot.width();
    let room = (slot.height() - hud::INSPECTOR_CAPTION_H).max(0.0);
    let fitted = if wide {
        egui::vec2(width, width * dim::CARD_W / dim::CARD_H)
    } else {
        let height = (width * dim::CARD_H / dim::CARD_W).min(room);
        egui::vec2(height * dim::CARD_W / dim::CARD_H, height)
    };
    fitted * scale.max(0.0)
}

#[derive(SystemParam)]
struct InspectorInputs<'w, 's> {
    hud: Res<'w, Hud>,
    held: Res<'w, Held>,
    table: Res<'w, GameTable>,
    mirror: Res<'w, Mirror>,
    my_seat: Res<'w, MySeat>,
    tuning: Res<'w, Tuning>,
    chain_hover: Res<'w, chain::ChainHover>,
    pile_hover: Res<'w, ui::PileHover>,
    selected: Res<'w, Selected>,
    pinned: Res<'w, interaction::Pinned>,
    menu: Res<'w, crate::menu::Menu>,
    hovered: Query<
        'w,
        's,
        (
            &'static CardArt,
            Option<&'static Landscape>,
            &'static CardView,
        ),
        (With<Hovered>, With<CardView>),
    >,
    cards: Query<
        'w,
        's,
        (
            &'static CardArt,
            Option<&'static Landscape>,
            &'static CardView,
        ),
        With<CardView>,
    >,
}

pub(super) fn inspector_ui(
    mut contexts: EguiContexts,
    input: InspectorInputs,
    mut art_cache: ResMut<art::ArtCache>,
    mut images: ResMut<Assets<Image>>,
    mut registry: ResMut<crate::render::egui_art::EguiArt>,
    hovered: Query<(&CardArt, Option<&Landscape>, &CardView), (With<Hovered>, With<CardView>)>,
    cards: Query<(&CardArt, Option<&Landscape>, &CardView), With<CardView>>,
) -> Result {
    let Some(slot) = input.hud.0.inspector else {
        return Ok(());
    };
    if !input.menu.at_table() {
        return Ok(());
    }
    let focus = input
        .pinned
        .0
        .and_then(|entity| input.cards.get(entity).ok())
        .or_else(|| input.hovered.iter().next())
        .or_else(|| {
            input
                .selected
                .0
                .and_then(|entity| input.cards.get(entity).ok())
        });
    let from_table = focus.map(|(art, landscape, view)| {
        let peeked = input
            .table
            .get(view.0)
            .and_then(|card| super::sync::peeked_face(card, &input.mirror, input.my_seat.0))
            .and_then(|name| art_cache.image(name, &mut images));
        (
            view.0,
            peeked.unwrap_or_else(|| art.0.clone()),
            landscape.is_some(),
        )
    });
    let from_pile = input.pile_hover.0.and_then(|id| {
        let card = input.table.get(id)?;
        let face = sync::drawn_face_in(card, &input.mirror.view);
        (!face.face.is_hidden())
            .then(|| art_cache.image(&face.face.name, &mut images))
            .flatten()
            .map(|handle| (id, handle, false))
    });
    let from_chain = input.chain_hover.0.and_then(|id| {
        let card = input.table.get(id)?;
        let face = sync::drawn_face_in(card, &input.mirror.view);
        let handle = (!face.face.is_hidden())
            .then(|| art_cache.image(&face.face.name, &mut images))
            .flatten()?;
        Some((id, handle, false))
    });
    let target = if input.held.card.is_none() {
        if input.pinned.0.is_some() {
            from_table
        } else {
            from_chain.or(from_pile).or(from_table)
        }
    } else {
        None
    };
    let Some((id, handle, wide)) = target else {
        return Ok(());
    };
    let caption = input
        .table
        .get(id)
        .map(|card| {
            let statuses = counters::status_chips(&input.mirror, id.0);
            caption(card, &input.mirror, input.my_seat.0, &statuses)
        })
        .unwrap_or_default();
    let texture = registry.texture(&mut contexts, &handle);
    let context = contexts.ctx_mut()?.clone();
    let size = face_size(slot, wide, input.tuning.preview_scale);
    egui::Area::new(egui::Id::new("inspector"))
        .fixed_pos(egui::pos2(
            slot.min.x,
            slot.max.y - size.y - hud::INSPECTOR_CAPTION_H,
        ))
        .order(egui::Order::Middle)
        .interactable(false)
        .show(&context, |ui| {
            ui.set_max_width(slot.width());
            ui.image(egui::load::SizedTexture::new(texture, size));
            if !caption.lines.is_empty() {
                egui::Frame::new()
                    .fill(hud::SURFACE)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.set_max_width(slot.width() - 12.0);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for (line, tint) in &caption.lines {
                            ui.label(
                                egui::RichText::new(line)
                                    .size(CAPTION_SIZE)
                                    .color(tint_color(*tint)),
                            );
                        }
                    });
            }
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_core::CardFace;

    fn mirror_with_might(card: u32, bonus: i32) -> Mirror {
        Mirror {
            view: agni_sim::view::TableView {
                zones: agni_riftbound::zone_table(),
                revealed: vec![card],
                counter_table: agni_riftbound::counter_table(),
                counters: vec![agni_sim::wire::CounterValue {
                    target: CounterTarget::Card(card),
                    counter: agni_riftbound::counter_table()
                        .iter()
                        .find(|decl| decl.name == "might")
                        .map(|decl| decl.id)
                        .unwrap(),
                    value: bonus,
                }],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn the_caption_reads_kind_cost_and_printed_to_current_might() {
        let mut table = Table::new();
        let base = Zone::Plugin(agni_riftbound::ZONE_BASE);
        let id = table.add_face(
            PlayerId(0),
            base,
            CardFace {
                name: "Punching Poro".into(),
                kind: Some("Unit".into()),
                energy: Some(2),
                might: Some(3),
                ..CardFace::default()
            },
        );
        let card = table.get(id).unwrap();
        let boosted = caption(
            card,
            &mirror_with_might(id.0, 2),
            PlayerId(0),
            &[("Stunned", [0; 3])],
        );
        assert_eq!(
            boosted.lines,
            vec![
                ("Unit · 2 energy".to_string(), Tint::Plain),
                ("might 3 › 5".to_string(), Tint::Above),
                ("stunned".to_string(), Tint::Weak),
            ]
        );
        let wounded = caption(card, &mirror_with_might(id.0, -1), PlayerId(0), &[]);
        assert_eq!(wounded.lines[1], ("might 3 › 2".to_string(), Tint::Below));
        let plain = caption(card, &Mirror::default(), PlayerId(0), &[]);
        assert_eq!(plain.lines[1], ("might 3".to_string(), Tint::Plain));
        let mut unrevealed = mirror_with_might(id.0, 0);
        unrevealed.view.revealed.clear();
        let facedown = caption(card, &unrevealed, PlayerId(0), &[]);
        assert_eq!(
            facedown.lines[0],
            (
                "face down · Punching Poro · Unit · 2 energy".to_string(),
                Tint::Weak
            )
        );
        assert_eq!(tint_color(Tint::Above), hud::GREEN);
    }

    #[test]
    fn the_face_fills_the_slot_width_and_a_battlefield_lies_wide() {
        let slot = egui::Rect::from_min_size(egui::pos2(12.0, 500.0), egui::vec2(168.0, 291.2));
        let tall = face_size(slot, false, 1.0);
        assert!((tall.x - 168.0).abs() < 1e-3, "{tall:?}");
        assert!((tall.y - 235.2).abs() < 1e-3, "{tall:?}");
        let wide = face_size(slot, true, 1.0);
        assert_eq!(wide.x, 168.0);
        assert!((wide.y - 120.0).abs() < 1e-3, "{wide:?}");
        let short = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(168.0, 156.0));
        let clipped = face_size(short, false, 1.0);
        assert!(clipped.y <= 100.0, "{clipped:?}");
    }

    #[test]
    fn the_preview_scale_grows_the_face_from_its_slot_fit_and_never_shrinks_past_zero() {
        let slot = egui::Rect::from_min_size(egui::pos2(12.0, 500.0), egui::vec2(168.0, 291.2));
        let unscaled = face_size(slot, false, 1.0);
        let doubled = face_size(slot, false, 2.0);
        assert!((doubled.x - unscaled.x * 2.0).abs() < 1e-3, "{doubled:?}");
        assert!((doubled.y - unscaled.y * 2.0).abs() < 1e-3, "{doubled:?}");
        let default_scale = face_size(slot, false, Tuning::default().preview_scale);
        assert!(
            (default_scale.x - unscaled.x * 2.2).abs() < 1e-3,
            "{default_scale:?}"
        );
        let negative = face_size(slot, false, -3.0);
        assert_eq!(negative, egui::Vec2::ZERO);
    }
}
