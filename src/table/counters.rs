use super::plugin_ui::Tools;
use super::{GameTable, Mirror, Selected, SessionInfo, SessionRole};
use agni_core::Zone;
use agni_sim::wire::{CounterDecl, CounterScope, CounterTarget, ZoneKind};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::BTreeMap;

#[derive(Message, Debug, Clone, Copy)]
pub struct CounterNudged {
    pub target: CounterTarget,
    pub counter: u16,
    pub delta: i32,
}

pub fn chip(decl: &CounterDecl) -> egui::Color32 {
    let [r, g, b] = decl.color;
    egui::Color32::from_rgb(r, g, b)
}

pub fn ink(decl: &CounterDecl) -> egui::Color32 {
    let [r, g, b] = decl.ink();
    egui::Color32::from_rgb(r, g, b)
}

pub const STATUS_BADGES: [(&str, &str, [u8; 3]); 4] = [
    ("stunned", "Stunned", [126, 108, 178]),
    ("attacker", "Attacker", [198, 104, 66]),
    ("defender", "Defender", [66, 128, 190]),
    ("attached", "Equipped", [120, 140, 90]),
];

pub fn readable_ink(color: [u8; 3]) -> egui::Color32 {
    let [r, g, b] = color;
    let luma = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
    if luma >= 140 {
        egui::Color32::from_rgb(18, 18, 22)
    } else {
        egui::Color32::from_rgb(242, 242, 246)
    }
}

pub fn status_chips(mirror: &Mirror, card: u32) -> Vec<(&'static str, [u8; 3])> {
    let Some(entry) = mirror.view.card(card) else {
        return Vec::new();
    };
    STATUS_BADGES
        .iter()
        .filter(|(key, _, _)| entry.badge(key).is_some())
        .map(|(_, label, color)| (*label, *color))
        .collect()
}

fn swatch(ui: &mut egui::Ui, decl: &CounterDecl, value: i32) {
    let text = format!("{} {value}", decl.label);
    let galley = ui
        .painter()
        .layout_no_wrap(text, egui::FontId::proportional(12.0), ink(decl));
    let padding = egui::vec2(6.0, 2.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + padding * 2.0, egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, chip(decl));
    ui.painter().galley(rect.min + padding, galley, ink(decl));
}

pub fn nudges_allowed(tools: &Tools, role: SessionRole) -> bool {
    tools.free && role != SessionRole::Ended
}

pub fn is_toggle(decl: &CounterDecl) -> bool {
    decl.min == Some(0) && decl.max == Some(1)
}

pub fn nudge(
    ui: &mut egui::Ui,
    decl: &CounterDecl,
    target: CounterTarget,
    value: i32,
    editable: bool,
    nudges: &mut MessageWriter<CounterNudged>,
) {
    if is_toggle(decl) {
        let mut on = value > 0;
        let response = ui.add_enabled(
            editable,
            egui::Checkbox::new(&mut on, egui::RichText::new(&decl.label).color(chip(decl))),
        );
        if response.changed() {
            nudges.write(CounterNudged {
                target,
                counter: decl.id,
                delta: if on { decl.step } else { -decl.step },
            });
        }
        return;
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(editable, egui::Button::new("−").small())
            .clicked()
        {
            nudges.write(CounterNudged {
                target,
                counter: decl.id,
                delta: -decl.step,
            });
        }
        swatch(ui, decl, value);
        if ui
            .add_enabled(editable, egui::Button::new("+").small())
            .clicked()
        {
            nudges.write(CounterNudged {
                target,
                counter: decl.id,
                delta: decl.step,
            });
        }
    });
}

#[derive(Resource, Default, Debug)]
pub struct CardKinds {
    pub known: BTreeMap<String, String>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    asked: std::collections::BTreeSet<String>,
}

impl CardKinds {
    pub fn merged(&self, seated: &crate::deck::import::SeatedDeck) -> BTreeMap<String, String> {
        let mut kinds = self.known.clone();
        kinds.extend(kinds_of(seated));
        kinds
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn learn_card_kinds(
    table: Res<GameTable>,
    mut kinds: ResMut<CardKinds>,
    mut catalog: Local<Option<Option<agni_importers::riftbound::catalog::StaticCatalog>>>,
) {
    use agni_importers::riftbound::catalog::CardLookup;
    if !table.is_changed() {
        return;
    }
    let wanted: Vec<String> = table
        .0
        .cards()
        .iter()
        .map(|card| card.face.name.clone())
        .filter(|name| !name.is_empty() && !kinds.asked.contains(name))
        .collect();
    if wanted.is_empty() {
        return;
    }
    let catalog = catalog.get_or_insert_with(|| {
        crate::os::paths::store_dir().and_then(|dir| {
            agni_importers::riftbound::ingest::load_catalog(&dir)
                .ok()
                .flatten()
        })
    });
    let Some(catalog) = catalog.as_mut() else {
        return;
    };
    for name in wanted {
        if let Ok(Some(card)) = catalog.by_name(&name) {
            kinds
                .known
                .insert(name.clone(), card.kind.as_str().to_string());
        }
        kinds.asked.insert(name);
    }
}

pub fn kinds_of(seated: &crate::deck::import::SeatedDeck) -> BTreeMap<String, String> {
    let mut kinds = BTreeMap::new();
    if let Some(crate::deck::import::ImportedDeck::Riftbound(deck)) =
        seated.0.as_ref().map(|record| &record.deck)
    {
        let entries = deck
            .main_deck
            .iter()
            .chain(deck.runes.iter())
            .chain(deck.battlefields.iter())
            .chain(deck.sideboard.iter())
            .map(|entry| &entry.card)
            .chain(deck.legend.iter())
            .chain(deck.chosen_champion.iter());
        for card in entries {
            if let Some(kind) = &card.kind {
                kinds.insert(card.name.clone(), kind.clone());
            }
        }
    }
    kinds
}

pub fn counters_allowed(kinds: &BTreeMap<String, String>, name: &str, terrain: bool) -> bool {
    match kinds.get(name) {
        Some(kind) => kind == agni_riftbound::KIND_UNIT,
        None => !terrain,
    }
}

pub fn counters_editable_on(zones: &[agni_sim::wire::ZoneDecl], zone: Zone) -> bool {
    let Zone::Plugin(id) = zone else {
        return false;
    };
    zones
        .iter()
        .find(|decl| decl.id == id)
        .is_some_and(|decl| decl.kind == ZoneKind::Battlefield)
}

const POPUP_OFFSET: egui::Vec2 = egui::vec2(46.0, -24.0);
const CARD_REACH: egui::Vec2 = egui::vec2(110.0, 150.0);
const POPUP_SLACK: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinnedPopup {
    pub card: u32,
    pub anchor: egui::Pos2,
    pub rect: egui::Rect,
}

pub fn keeps_pin(pin: &PinnedPopup, pointer: Option<egui::Pos2>) -> bool {
    let Some(pointer) = pointer else {
        return false;
    };
    let reach = egui::Rect::from_center_size(pin.anchor, CARD_REACH);
    pin.rect.expand(POPUP_SLACK).union(reach).contains(pointer)
}

pub fn hovered_card_counters_ui(
    mut contexts: EguiContexts,
    mirror: Res<Mirror>,
    table: Res<GameTable>,
    info: Res<SessionInfo>,
    tools: Res<Tools>,
    selected: Res<Selected>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    hovered: Query<
        (&super::CardView, &GlobalTransform, Has<super::Landscape>),
        With<super::Hovered>,
    >,
    cards: Query<(&super::CardView, &GlobalTransform, Has<super::Landscape>)>,
    seated: Res<crate::deck::import::SeatedDeck>,
    card_kinds: Res<CardKinds>,
    mut nudges: MessageWriter<CounterNudged>,
    mut pinned: Local<Option<PinnedPopup>>,
) -> Result {
    let card_decls: Vec<CounterDecl> = mirror
        .view
        .counter_table
        .iter()
        .filter(|decl| decl.scope == CounterScope::Card)
        .cloned()
        .collect();
    if card_decls.is_empty() || !nudges_allowed(&tools, info.role) {
        *pinned = None;
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let kinds = card_kinds.merged(&seated);
    let editable = |id: u32, terrain: bool| {
        table.0.get(agni_core::CardId(id)).is_some_and(|card| {
            counters_editable_on(&mirror.view.zones, card.zone)
                && counters_allowed(&kinds, &card.face.name, terrain)
        })
    };
    let fresh = hovered
        .iter()
        .next()
        .or_else(|| selected.0.and_then(|entity| cards.get(entity).ok()))
        .and_then(|(view, transform, terrain)| {
            if !editable(view.0 .0, terrain) {
                return None;
            }
            camera
                .world_to_viewport(camera_transform, transform.translation())
                .ok()
                .map(|screen| (view.0 .0, egui::pos2(screen.x, screen.y)))
        });
    let pin = match (fresh, *pinned) {
        (Some((card, anchor)), Some(pin)) if pin.card == card => PinnedPopup { anchor, ..pin },
        (Some((card, anchor)), _) => PinnedPopup {
            card,
            anchor,
            rect: egui::Rect::from_min_size(anchor + POPUP_OFFSET, egui::Vec2::ZERO),
        },
        (None, Some(pin))
            if editable(pin.card, false)
                && keeps_pin(&pin, context.input(|input| input.pointer.latest_pos())) =>
        {
            pin
        }
        (None, _) => {
            *pinned = None;
            return Ok(());
        }
    };
    let target = CounterTarget::Card(pin.card);
    let response = egui::Area::new(egui::Id::new("card-counters"))
        .fixed_pos(pin.anchor + POPUP_OFFSET)
        .order(egui::Order::Foreground)
        .show(&context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                for decl in &card_decls {
                    let value = mirror.view.counter(target, decl.id).unwrap_or(decl.start);
                    nudge(ui, decl, target, value, true, &mut nudges);
                }
            });
        })
        .response;
    *pinned = Some(PinnedPopup {
        rect: response.rect,
        ..pin
    });
    Ok(())
}

pub const BADGE_H: f32 = 20.0;
pub const BADGE_GAP: f32 = 2.0;
pub const BADGE_PAD: f32 = 6.0;
pub const BADGE_INSET: f32 = 2.0;
pub const BADGE_FONT: f32 = 12.0;
pub const MAX_BADGES: usize = 3;
pub const FRAME_OUTSET: f32 = 0.04;
pub const FRAME_WIDTH: f32 = 2.5;
pub const FROST: [u8; 3] = [150, 210, 255];
pub const GOLD: [u8; 3] = [240, 190, 60];
pub const DAMAGE: [u8; 3] = [200, 70, 62];
pub const ABOVE: [u8; 3] = [56, 160, 92];
pub const BELOW: [u8; 3] = [255, 107, 107];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeGlyph {
    Sword,
    Shield,
    Gear,
    Hourglass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    Frost,
    Gold,
}

impl Frame {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Frame::Frost => FROST,
            Frame::Gold => GOLD,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub fill: [u8; 3],
    pub text: Option<String>,
    pub glyph: Option<BadgeGlyph>,
}

impl Badge {
    pub fn text(fill: [u8; 3], text: impl Into<String>) -> Self {
        Self {
            fill,
            text: Some(text.into()),
            glyph: None,
        }
    }

    pub fn glyph(fill: [u8; 3], glyph: BadgeGlyph) -> Self {
        Self {
            fill,
            text: None,
            glyph: Some(glyph),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dress {
    pub badges: Vec<Badge>,
    pub frames: Vec<Frame>,
}

pub fn dress(mirror: &Mirror, card: u32, printed_might: Option<u8>, seat_rgb: [u8; 3]) -> Dress {
    let mut out = Dress::default();
    let held = mirror.view.counters_for(CounterTarget::Card(card));
    let card_decl = |name: &str| {
        mirror
            .view
            .counter_table
            .iter()
            .find(|decl| decl.name == name && decl.scope == CounterScope::Card)
    };
    let value_of = |decl: &CounterDecl| {
        held.iter()
            .find(|value| value.counter == decl.id)
            .map(|value| value.value)
            .unwrap_or(decl.start)
    };
    let might_decl = card_decl("might");
    let damage_decl = card_decl("damage");
    if let (Some(decl), Some(printed)) = (might_decl, printed_might) {
        let current = i32::from(printed) + value_of(decl);
        let (fill, text) = match current.cmp(&i32::from(printed)) {
            std::cmp::Ordering::Greater => (ABOVE, format!("{printed}›{current}")),
            std::cmp::Ordering::Less => (BELOW, format!("{printed}›{current}")),
            std::cmp::Ordering::Equal => (decl.color, format!("{printed}")),
        };
        out.badges.push(Badge::text(fill, text));
    }
    if let Some(decl) = damage_decl {
        let damage = value_of(decl);
        if damage > 0 {
            out.badges.push(Badge::text(DAMAGE, format!("−{damage}")));
        }
    }
    for value in held.iter() {
        let Some(decl) = mirror.view.counter_decl(value.counter) else {
            continue;
        };
        if might_decl.is_some_and(|might| might.id == decl.id)
            || damage_decl.is_some_and(|damage| damage.id == decl.id)
        {
            continue;
        }
        if decl.name == "empowered" {
            if value.value > 0 {
                out.frames.push(Frame::Gold);
            }
            continue;
        }
        let toggle = is_toggle(decl);
        if toggle && value.value <= 0 {
            continue;
        }
        match (toggle, decl.name.as_str()) {
            (true, "temporary") => out
                .badges
                .push(Badge::glyph(decl.color, BadgeGlyph::Hourglass)),
            (true, _) => out.badges.push(Badge::text(decl.color, decl.label.clone())),
            (false, _) => out.badges.push(Badge::text(
                decl.color,
                format!("{} {}", decl.label, value.value),
            )),
        }
    }
    if let Some(entry) = mirror.view.card(card) {
        if entry.badge("stunned").is_some() {
            out.frames.push(Frame::Frost);
        }
        if entry.badge("attacker").is_some() {
            out.badges.push(Badge::glyph(seat_rgb, BadgeGlyph::Sword));
        }
        if entry.badge("defender").is_some() {
            out.badges
                .push(Badge::glyph(STATUS_BADGES[2].2, BadgeGlyph::Shield));
        }
        if entry.badge("attached").is_some() {
            out.badges
                .push(Badge::glyph(STATUS_BADGES[3].2, BadgeGlyph::Gear));
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub badge: Option<usize>,
    pub x: f32,
    pub w: f32,
}

pub fn more_label(hidden: usize) -> String {
    format!("+{hidden}")
}

pub fn stack(widths: &[f32], more_w: f32, card_w: f32) -> Vec<Placed> {
    let n = widths.len();
    let mut shown = n.min(MAX_BADGES);
    loop {
        let hidden = n - shown;
        let mut total: f32 = widths[..shown].iter().sum();
        if shown > 1 {
            total += BADGE_GAP * (shown - 1) as f32;
        }
        if hidden > 0 {
            total += more_w + if shown > 0 { BADGE_GAP } else { 0.0 };
        }
        if total <= card_w || (shown == 0 && hidden == 0) {
            let mut out = Vec::with_capacity(shown + 1);
            let mut x = 0.0;
            for (index, width) in widths[..shown].iter().enumerate() {
                out.push(Placed {
                    badge: Some(index),
                    x,
                    w: *width,
                });
                x += width + BADGE_GAP;
            }
            if hidden > 0 && total <= card_w {
                out.push(Placed {
                    badge: None,
                    x,
                    w: more_w,
                });
            }
            return out;
        }
        if shown == 0 {
            return Vec::new();
        }
        shown -= 1;
    }
}

pub fn paint_badge_glyph(
    painter: &egui::Painter,
    rect: egui::Rect,
    glyph: BadgeGlyph,
    ink: egui::Color32,
) {
    let c = rect.center();
    let r = rect.height() * 0.32;
    let stroke = egui::Stroke::new(1.6, ink);
    match glyph {
        BadgeGlyph::Sword => {
            painter.line_segment(
                [egui::pos2(c.x - r, c.y + r), egui::pos2(c.x + r, c.y - r)],
                egui::Stroke::new(2.2, ink),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.9, c.y + r * 0.2),
                    egui::pos2(c.x - r * 0.2, c.y + r * 0.9),
                ],
                stroke,
            );
        }
        BadgeGlyph::Shield => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - r, c.y - r),
                    egui::pos2(c.x + r, c.y - r),
                    egui::pos2(c.x + r, c.y + r * 0.2),
                    egui::pos2(c.x, c.y + r),
                    egui::pos2(c.x - r, c.y + r * 0.2),
                ],
                ink,
                egui::Stroke::NONE,
            ));
        }
        BadgeGlyph::Gear => {
            painter.circle_stroke(c, r * 0.7, egui::Stroke::new(2.0, ink));
            for step in 0..6 {
                let angle = step as f32 * std::f32::consts::PI / 3.0;
                let out = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment([c + out * r * 0.7, c + out * r * 1.15], stroke);
            }
        }
        BadgeGlyph::Hourglass => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - r, c.y - r),
                    egui::pos2(c.x + r, c.y - r),
                    egui::pos2(c.x, c.y),
                ],
                ink,
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x, c.y),
                    egui::pos2(c.x + r, c.y + r),
                    egui::pos2(c.x - r, c.y + r),
                ],
                ink,
                egui::Stroke::NONE,
            ));
        }
    }
}

pub fn counter_badges_ui(
    mut contexts: EguiContexts,
    seats: super::hud::Seats,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(
        &super::CardView,
        &GlobalTransform,
        &ViewVisibility,
        Has<super::Landscape>,
    )>,
    seated: Res<crate::deck::import::SeatedDeck>,
    card_kinds: Res<CardKinds>,
) -> Result {
    let mirror = &*seats.mirror;
    let table = &seats.table;
    if mirror.view.counter_table.is_empty() && mirror.view.cards.is_empty() {
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let painter = contexts
        .ctx_mut()?
        .layer_painter(egui::LayerId::background());
    let kinds = card_kinds.merged(&seated);
    let font = egui::FontId::proportional(BADGE_FONT);
    for (view, transform, visibility, terrain) in &cards {
        if !visibility.get() {
            continue;
        }
        let Some(card) = table.0.get(view.0) else {
            continue;
        };
        if !counters_allowed(&kinds, &card.face.name, terrain) {
            continue;
        }
        let controller = mirror
            .view
            .card(view.0 .0)
            .map(|entry| entry.seat)
            .unwrap_or(card.owner.0);
        let seat_rgb = seats.label(agni_core::PlayerId(controller)).1;
        let dressed = dress(mirror, view.0 .0, card.face.might, seat_rgb);
        if dressed.badges.is_empty() && dressed.frames.is_empty() {
            continue;
        }
        let (width, height) = if terrain {
            (super::dim::CARD_H, super::dim::CARD_W)
        } else {
            (super::dim::CARD_W, super::dim::CARD_H)
        };
        for frame in &dressed.frames {
            let Some(points) = super::highlight::projected(
                camera,
                camera_transform,
                transform,
                super::highlight::rim_ring(width, height, FRAME_OUTSET),
            ) else {
                continue;
            };
            let [r, g, b] = frame.rgb();
            painter.add(egui::Shape::closed_line(
                points,
                egui::Stroke::new(FRAME_WIDTH, egui::Color32::from_rgb(r, g, b)),
            ));
        }
        if dressed.badges.is_empty() {
            continue;
        }
        let Some(points) = super::highlight::projected(
            camera,
            camera_transform,
            transform,
            super::highlight::rim_ring(width, height, 0.0),
        ) else {
            continue;
        };
        let bounds = egui::Rect::from_points(&points);
        let galleys: Vec<Option<std::sync::Arc<egui::Galley>>> = dressed
            .badges
            .iter()
            .map(|badge| {
                badge.text.as_ref().map(|text| {
                    painter.layout_no_wrap(text.clone(), font.clone(), readable_ink(badge.fill))
                })
            })
            .collect();
        let widths: Vec<f32> = galleys
            .iter()
            .map(|galley| match galley {
                Some(galley) => galley.size().x + BADGE_PAD * 2.0,
                None => BADGE_H,
            })
            .collect();
        let card_w = (bounds.width() - BADGE_INSET * 2.0).max(0.0);
        let hidden_after = |shown: usize| dressed.badges.len() - shown;
        let more_galley = painter.layout_no_wrap(
            more_label(dressed.badges.len()),
            font.clone(),
            egui::Color32::from_rgb(242, 242, 246),
        );
        let more_w = more_galley.size().x + BADGE_PAD * 2.0;
        let placed = stack(&widths, more_w, card_w);
        let shown = placed.iter().filter(|slot| slot.badge.is_some()).count();
        let origin = egui::pos2(
            bounds.min.x + BADGE_INSET,
            bounds.max.y - BADGE_INSET - BADGE_H,
        );
        for slot in placed {
            let rect = egui::Rect::from_min_size(
                egui::pos2(origin.x + slot.x, origin.y),
                egui::vec2(slot.w, BADGE_H),
            );
            match slot.badge {
                Some(index) => {
                    let badge = &dressed.badges[index];
                    let [r, g, b] = badge.fill;
                    let fill = egui::Color32::from_rgb(r, g, b);
                    let ink = readable_ink(badge.fill);
                    painter.rect_filled(rect, BADGE_H / 2.0, fill);
                    painter.rect_stroke(
                        rect,
                        BADGE_H / 2.0,
                        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(120)),
                        egui::StrokeKind::Outside,
                    );
                    match (&galleys[index], badge.glyph) {
                        (Some(galley), _) => {
                            painter.galley(
                                rect.center() - galley.size() / 2.0,
                                galley.clone(),
                                ink,
                            );
                        }
                        (None, Some(glyph)) => paint_badge_glyph(&painter, rect, glyph, ink),
                        (None, None) => {}
                    }
                }
                None => {
                    let galley = painter.layout_no_wrap(
                        more_label(hidden_after(shown)),
                        font.clone(),
                        egui::Color32::from_rgb(242, 242, 246),
                    );
                    painter.rect_filled(rect, BADGE_H / 2.0, egui::Color32::from_rgb(42, 42, 51));
                    painter.galley(
                        rect.center() - galley.size() / 2.0,
                        galley,
                        egui::Color32::WHITE,
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::view::TableView;
    use agni_sim::wire::{CounterPlace, CounterValue};

    fn decl(id: u16, scope: CounterScope, place: CounterPlace) -> CounterDecl {
        CounterDecl {
            id,
            name: format!("counter-{id}"),
            label: format!("Counter {id}"),
            scope,
            start: 20,
            min: None,
            max: None,
            step: 1,
            place,
            color: [200, 70, 62],
        }
    }

    fn mirror_with(table: Vec<CounterDecl>, counters: Vec<CounterValue>) -> Mirror {
        Mirror {
            view: TableView {
                counter_table: table,
                counters,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn riftbound_mirror(card: u32, counters: Vec<(&str, i32)>, badges: &[&str]) -> Mirror {
        let table = agni_riftbound::counter_table();
        let counters = counters
            .into_iter()
            .map(|(name, value)| CounterValue {
                target: CounterTarget::Card(card),
                counter: table.iter().find(|decl| decl.name == name).unwrap().id,
                value,
            })
            .collect();
        let mut mirror = mirror_with(table, counters);
        mirror.view.cards.push(agni_sim::view::ViewCard {
            id: card,
            zone: Zone::Plugin(9),
            seat: 1,
            owner: 1,
            face_visible: true,
            badges: badges
                .iter()
                .map(|key| agni_sim::view::Badge {
                    key: (*key).to_string(),
                    value: serde_bytes::ByteBuf::from(vec![1]),
                })
                .collect(),
        });
        mirror
    }

    #[test]
    fn a_unit_is_dressed_in_might_damage_glyph_chips_and_frames() {
        let seat = [216, 74, 68];
        let plain = dress(&riftbound_mirror(7, vec![], &[]), 7, Some(3), seat);
        assert_eq!(plain.badges, vec![Badge::text([96, 176, 108], "3")]);
        assert!(plain.frames.is_empty());
        let boosted = dress(
            &riftbound_mirror(7, vec![("might", 2)], &[]),
            7,
            Some(3),
            seat,
        );
        assert_eq!(boosted.badges[0], Badge::text(ABOVE, "3›5"));
        let wounded = dress(
            &riftbound_mirror(7, vec![("might", -1), ("damage", 2)], &[]),
            7,
            Some(3),
            seat,
        );
        assert_eq!(
            wounded.badges,
            vec![Badge::text(BELOW, "3›2"), Badge::text(DAMAGE, "−2")]
        );
        let busy = dress(
            &riftbound_mirror(
                7,
                vec![("temporary", 1), ("empowered", 1), ("buffed", 1)],
                &["stunned", "attacker", "defender", "attached", "exhausted"],
            ),
            7,
            Some(3),
            seat,
        );
        assert_eq!(busy.frames, vec![Frame::Gold, Frame::Frost]);
        let glyphs: Vec<Option<BadgeGlyph>> = busy.badges.iter().map(|badge| badge.glyph).collect();
        assert_eq!(
            glyphs,
            vec![
                None,
                Some(BadgeGlyph::Hourglass),
                None,
                Some(BadgeGlyph::Sword),
                Some(BadgeGlyph::Shield),
                Some(BadgeGlyph::Gear),
            ]
        );
        assert_eq!(busy.badges[2].text.as_deref(), Some("Buffed 1"));
        assert_eq!(
            busy.badges[3].fill, seat,
            "the sword wears the attacker's seat colour"
        );
        let stacked = dress(
            &riftbound_mirror(7, vec![("empowered", 2), ("buffed", 2)], &[]),
            7,
            Some(3),
            seat,
        );
        assert_eq!(stacked.frames, vec![Frame::Gold]);
        assert!(stacked
            .badges
            .iter()
            .any(|badge| badge.text.as_deref() == Some("Buffed 2")));
        let gear = dress(&riftbound_mirror(8, vec![], &[]), 8, None, seat);
        assert!(
            gear.badges.is_empty(),
            "a card without might has no might chip"
        );
        assert_eq!(Frame::Frost.rgb(), FROST);
        assert_eq!(Frame::Gold.rgb(), GOLD);
    }

    #[test]
    fn badge_stacking_never_exceeds_the_card_width() {
        let widths = [30.0, 28.0, 20.0, 20.0, 34.0];
        let more_w = 24.0;
        for card_w in [16.0, 30.0, 44.0, 60.0, 80.0, 100.0, 140.0, 400.0] {
            let placed = stack(&widths, more_w, card_w);
            for slot in &placed {
                assert!(
                    slot.x + slot.w <= card_w + 1e-4,
                    "a chip runs past a {card_w} wide card: {placed:?}"
                );
            }
            let shown = placed.iter().filter(|slot| slot.badge.is_some()).count();
            assert!(shown <= MAX_BADGES);
            if shown < widths.len() {
                assert!(
                    placed.last().is_some_and(|slot| slot.badge.is_none()) || placed.is_empty(),
                    "hidden chips are announced by a +N chip at {card_w}: {placed:?}"
                );
            }
        }
        let roomy = stack(&widths, more_w, 400.0);
        assert_eq!(roomy.len(), 4, "three chips and the +2");
        assert_eq!(roomy[3].badge, None);
        assert_eq!(roomy[1].x, 30.0 + BADGE_GAP);
        assert_eq!(more_label(2), "+2");
        let tiny = stack(&widths, more_w, 16.0);
        assert!(tiny.is_empty(), "nothing fits, nothing is drawn");
        let exact = stack(&[20.0, 20.0], more_w, 42.0);
        assert_eq!(exact.len(), 2);
        assert!(stack(&[], more_w, 100.0).is_empty());
    }

    #[test]
    fn a_counter_with_no_stored_value_reads_as_its_start() {
        let mirror = mirror_with(
            vec![decl(0, CounterScope::Seat, CounterPlace::SeatPlate)],
            Vec::new(),
        );
        assert_eq!(mirror.view.counter(CounterTarget::Seat(0), 0), None);
        let shown = mirror
            .view
            .counter(CounterTarget::Seat(0), 0)
            .unwrap_or(mirror.view.counter_table[0].start);
        assert_eq!(shown, 20);
    }

    #[test]
    fn a_card_with_no_counters_paints_nothing() {
        let mirror = mirror_with(
            vec![decl(2, CounterScope::Card, CounterPlace::CardBadge)],
            vec![CounterValue {
                target: CounterTarget::Card(7),
                counter: 2,
                value: 3,
            }],
        );
        assert_eq!(mirror.view.counters_for(CounterTarget::Card(7)).len(), 1);
        assert!(mirror.view.counters_for(CounterTarget::Card(8)).is_empty());
    }

    fn zone(id: u16, kind: ZoneKind) -> agni_sim::wire::ZoneDecl {
        agni_sim::wire::ZoneDecl {
            id,
            name: format!("zone-{id}"),
            kind,
            owner: agni_sim::wire::ZoneOwner::PerSeat,
            visibility: agni_sim::wire::ZoneVisibility::All,
            layout: agni_sim::wire::ZoneLayout::Row,
            place: agni_sim::wire::ZonePlace::Inner,
            span: 1,
            label: format!("Zone {id}"),
        }
    }

    #[test]
    fn card_counters_are_editable_on_a_battlefield_and_nowhere_else() {
        let zones = vec![
            zone(0, ZoneKind::Hand),
            zone(1, ZoneKind::Battlefield),
            zone(2, ZoneKind::Deck),
            zone(3, ZoneKind::Discard),
        ];
        assert!(counters_editable_on(&zones, Zone::Plugin(1)));
        assert!(!counters_editable_on(&zones, Zone::Plugin(0)));
        assert!(!counters_editable_on(&zones, Zone::Plugin(2)));
        assert!(!counters_editable_on(&zones, Zone::Plugin(3)));
        assert!(!counters_editable_on(&zones, Zone::Plugin(9)));
        assert!(!counters_editable_on(&zones, Zone::Hand));
    }

    #[test]
    fn the_riftbound_battlefields_take_card_counters_and_its_hand_does_not() {
        let zones = agni_riftbound::zone_table();
        let battlefield = zones
            .iter()
            .find(|decl| decl.kind == ZoneKind::Battlefield)
            .expect("riftbound declares a battlefield");
        let hand = zones
            .iter()
            .find(|decl| decl.kind == ZoneKind::Hand)
            .expect("riftbound declares a hand");
        assert!(counters_editable_on(&zones, Zone::Plugin(battlefield.id)));
        assert!(!counters_editable_on(&zones, Zone::Plugin(hand.id)));
    }

    #[test]
    fn riftbound_puts_might_and_damage_on_the_card_and_points_on_the_seat() {
        let table = agni_riftbound::counter_table();
        let by_name = |name: &str| {
            table
                .iter()
                .find(|decl| decl.name == name)
                .unwrap_or_else(|| panic!("riftbound declares {name}"))
                .clone()
        };
        assert_eq!(by_name("might").scope, CounterScope::Card);
        assert_eq!(by_name("damage").scope, CounterScope::Card);
        assert_eq!(by_name("damage").min, Some(0));
        assert_eq!(by_name("points").scope, CounterScope::Seat);
        assert_eq!(by_name("xp").scope, CounterScope::Seat);
    }

    #[test]
    fn a_counter_chip_takes_its_declared_color_and_a_readable_ink() {
        let mut dark = decl(0, CounterScope::Card, CounterPlace::CardBadge);
        dark.color = [26, 26, 30];
        assert_eq!(chip(&dark), egui::Color32::from_rgb(26, 26, 30));
        assert_eq!(ink(&dark), egui::Color32::from_rgb(242, 242, 246));

        let mut light = decl(1, CounterScope::Card, CounterPlace::CardBadge);
        light.color = [232, 230, 224];
        assert_eq!(chip(&light), egui::Color32::from_rgb(232, 230, 224));
        assert_eq!(ink(&light), egui::Color32::from_rgb(18, 18, 22));
    }

    #[test]
    fn the_games_give_every_counter_its_own_color() {
        for table in [agni_riftbound::counter_table(), agni_mtg::counter_table()] {
            for decl in &table {
                assert_ne!(
                    decl.color,
                    agni_sim::wire::DEFAULT_COUNTER_COLOR,
                    "{} should declare a colour",
                    decl.name
                );
            }
        }
        let riftbound = agni_riftbound::counter_table();
        let by_name = |name: &str| {
            riftbound
                .iter()
                .find(|decl| decl.name == name)
                .expect("declared")
                .color
        };
        assert_eq!(by_name("damage"), [200, 70, 62]);
        assert_eq!(by_name("might"), [96, 176, 108]);
    }

    #[test]
    fn the_combat_statuses_the_plugin_annotates_are_drawn_as_chips() {
        use agni_sim::view::{Badge, ViewCard};
        let card = |id: u32, keys: &[&str]| ViewCard {
            id,
            zone: Zone::Plugin(1),
            seat: 0,
            owner: 0,
            face_visible: true,
            badges: keys
                .iter()
                .map(|key| Badge {
                    key: (*key).to_string(),
                    value: serde_bytes::ByteBuf::from(vec![1]),
                })
                .collect(),
        };
        let mirror = Mirror {
            view: TableView {
                cards: vec![
                    card(7, &["stunned", "defender", "exhausted"]),
                    card(8, &["attacker", "attached"]),
                    card(9, &[]),
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            status_chips(&mirror, 7)
                .into_iter()
                .map(|(label, _)| label)
                .collect::<Vec<_>>(),
            ["Stunned", "Defender"],
            "exhausted is drawn by rotating the card, not by a chip"
        );
        assert_eq!(
            status_chips(&mirror, 8)
                .into_iter()
                .map(|(label, _)| label)
                .collect::<Vec<_>>(),
            ["Attacker", "Equipped"],
            "an attached gear shows the link chip"
        );
        assert!(status_chips(&mirror, 9).is_empty());
        assert!(status_chips(&mirror, 99).is_empty());
        for (_, _, color) in STATUS_BADGES {
            assert_ne!(readable_ink(color), egui::Color32::TRANSPARENT);
        }
        assert_eq!(
            readable_ink([232, 230, 224]),
            egui::Color32::from_rgb(18, 18, 22)
        );
        assert_eq!(
            readable_ink([26, 26, 30]),
            egui::Color32::from_rgb(242, 242, 246)
        );
    }
}

#[cfg(test)]
mod pin_tests {
    use super::*;

    #[test]
    fn a_zero_or_one_counter_is_a_status_toggle() {
        let toggles: Vec<bool> = agni_riftbound::counter_table()
            .iter()
            .map(is_toggle)
            .collect();
        assert_eq!(toggles, [false, false, false, false, true, false, false]);
    }

    #[test]
    fn counters_belong_to_units_when_the_kind_is_known_and_to_non_terrain_otherwise() {
        let mut kinds = BTreeMap::new();
        kinds.insert("Zhonya's Hourglass".to_string(), "Gear".to_string());
        kinds.insert("Emberwing Scout".to_string(), "Unit".to_string());
        kinds.insert("Ember Rune".to_string(), "Rune".to_string());
        assert!(!counters_allowed(&kinds, "Zhonya's Hourglass", false));
        assert!(counters_allowed(&kinds, "Emberwing Scout", false));
        assert!(!counters_allowed(&kinds, "Ember Rune", false));
        assert!(counters_allowed(&kinds, "Someone Else's Card", false));
        assert!(!counters_allowed(
            &kinds,
            "Someone Else's Battlefield",
            true
        ));
    }

    fn pin() -> PinnedPopup {
        PinnedPopup {
            card: 1,
            anchor: egui::pos2(300.0, 300.0),
            rect: egui::Rect::from_min_size(egui::pos2(346.0, 276.0), egui::vec2(120.0, 60.0)),
        }
    }

    #[test]
    fn the_popup_survives_the_pointer_crossing_from_the_card_to_it() {
        let pin = pin();
        assert!(keeps_pin(&pin, Some(egui::pos2(330.0, 300.0))));
        assert!(keeps_pin(&pin, Some(egui::pos2(400.0, 300.0))));
        assert!(keeps_pin(&pin, Some(egui::pos2(470.0, 340.0))));
        assert!(!keeps_pin(&pin, Some(egui::pos2(600.0, 600.0))));
        assert!(!keeps_pin(&pin, Some(egui::pos2(300.0, 100.0))));
        assert!(!keeps_pin(&pin, None));
    }
}
