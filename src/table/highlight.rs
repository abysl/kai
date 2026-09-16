use super::*;
use agni_sim::wire::{Affordance, LegalKind, Origin, PluginView, TargetRef};
use std::collections::BTreeMap;

pub const LEGAL_OUTSET: f32 = 0.12;
pub const ENEMY_OUTSET: f32 = 0.19;
pub const RIM_WIDTH: f32 = 2.5;
pub const PLAY_LIFT: f32 = 0.07;
pub const PULSE_SECS: f32 = 1.1;
pub const PULSE_FLOOR: u8 = 90;
pub const TINT_ALPHA: u8 = 52;
pub const TINT_EDGE: u8 = 216;
pub const TINT_WIDTH: f32 = 3.0;
pub const GREY_LEVEL: f32 = 0.45;
pub const EXHAUST_LEVEL: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RimKind {
    Answer,
    React,
    Play,
    Hide,
    Activate,
    March,
    Enemy,
}

pub fn rim_of(kind: LegalKind) -> RimKind {
    match kind {
        LegalKind::Play { .. } => RimKind::Play,
        LegalKind::March => RimKind::March,
        LegalKind::Activate { .. } => RimKind::Activate,
        LegalKind::React => RimKind::React,
        LegalKind::Answer => RimKind::Answer,
        LegalKind::Hide => RimKind::Hide,
    }
}

pub fn strongest(kinds: &[LegalKind]) -> Option<RimKind> {
    kinds.iter().copied().map(rim_of).min()
}

impl RimKind {
    pub const LEGALITY: [RimKind; 5] = [
        RimKind::Play,
        RimKind::March,
        RimKind::Activate,
        RimKind::React,
        RimKind::Answer,
    ];

    pub const ALL: [RimKind; 7] = [
        RimKind::Answer,
        RimKind::React,
        RimKind::Play,
        RimKind::Hide,
        RimKind::Activate,
        RimKind::March,
        RimKind::Enemy,
    ];

    pub fn legend(self) -> &'static str {
        match self {
            RimKind::Play => "playable",
            RimKind::March => "attack",
            RimKind::Activate => "ability",
            RimKind::React => "respond",
            RimKind::Answer => "choose",
            RimKind::Hide => "hide",
            RimKind::Enemy => "targeted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Palette {
    #[default]
    Standard,
    ColourBlind,
}

impl Palette {
    pub const BOTH: [Palette; 2] = [Palette::Standard, Palette::ColourBlind];

    pub fn of(tuning: &Tuning) -> Self {
        if tuning.colour_blind {
            Palette::ColourBlind
        } else {
            Palette::Standard
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RimStyle {
    pub width: f32,
    pub stroke: Stroke,
    pub chevron: bool,
    pub tag: bool,
    pub pulses: bool,
    pub ring: bool,
}

pub const DASH: f32 = 6.0;
pub const DASH_GAP: f32 = 4.0;
pub const DOT: f32 = 2.0;
pub const DOT_GAP: f32 = 4.0;
pub const CHEVRON: f32 = 7.0;
pub const TAG: f32 = 9.0;

pub fn rim_style(kind: RimKind) -> RimStyle {
    let plain = RimStyle {
        width: 2.0,
        stroke: Stroke::Solid,
        chevron: false,
        tag: false,
        pulses: false,
        ring: false,
    };
    match kind {
        RimKind::Play => plain,
        RimKind::March => RimStyle {
            chevron: true,
            ..plain
        },
        RimKind::Activate => RimStyle { tag: true, ..plain },
        RimKind::React => RimStyle {
            stroke: Stroke::Dashed,
            ..plain
        },
        RimKind::Answer => RimStyle {
            width: 3.0,
            pulses: true,
            ..plain
        },
        RimKind::Hide => RimStyle {
            stroke: Stroke::Dotted,
            ..plain
        },
        RimKind::Enemy => RimStyle {
            ring: true,
            ..plain
        },
    }
}

pub fn rim_color(kind: RimKind) -> egui::Color32 {
    rim_color_in(kind, Palette::Standard)
}

pub fn rim_color_in(kind: RimKind, palette: Palette) -> egui::Color32 {
    let [r, g, b] = rim_rgb_in(kind, palette);
    egui::Color32::from_rgb(r, g, b)
}

pub fn rim_rgb(kind: RimKind) -> [u8; 3] {
    rim_rgb_in(kind, Palette::Standard)
}

pub fn rim_rgb_in(kind: RimKind, palette: Palette) -> [u8; 3] {
    match (palette, kind) {
        (Palette::Standard, RimKind::Play | RimKind::Hide) => [140, 240, 96],
        (Palette::ColourBlind, RimKind::Play | RimKind::Hide) => [30, 80, 255],
        (_, RimKind::March) => [40, 200, 255],
        (Palette::Standard, RimKind::Activate) => [255, 150, 0],
        (Palette::ColourBlind, RimKind::Activate) => [255, 224, 0],
        (_, RimKind::React) => [214, 80, 255],
        (_, RimKind::Answer) => [190, 198, 214],
        (Palette::Standard, RimKind::Enemy) => [255, 40, 130],
        (Palette::ColourBlind, RimKind::Enemy) => [255, 128, 0],
    }
}

pub fn same_family(left: RimKind, right: RimKind) -> bool {
    matches!(
        (left, right),
        (RimKind::Play, RimKind::Hide) | (RimKind::Hide, RimKind::Play)
    )
}

pub fn apart(left: [u8; 3], right: [u8; 3]) -> f32 {
    let square = |a: u8, b: u8| {
        let delta = a as f32 - b as f32;
        delta * delta
    };
    (square(left[0], right[0]) + square(left[1], right[1]) + square(left[2], right[2])).sqrt()
}

pub fn faded(color: egui::Color32, alpha: u8) -> egui::Color32 {
    let [r, g, b, _] = color.to_array();
    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

pub fn pulse_alpha(seconds: f32) -> u8 {
    let phase = seconds / PULSE_SECS * std::f32::consts::TAU;
    let wave = 0.5 + 0.5 * phase.sin();
    let floor = PULSE_FLOOR as f32;
    (floor + wave * (255.0 - floor)).round().clamp(0.0, 255.0) as u8
}

pub fn rim_ring(width: f32, height: f32, outset: f32) -> [Vec3; 4] {
    let x = width / 2.0 + outset;
    let z = height / 2.0 + outset;
    let y = dim::CARD_THICK / 2.0;
    [
        Vec3::new(-x, y, -z),
        Vec3::new(x, y, -z),
        Vec3::new(x, y, z),
        Vec3::new(-x, y, z),
    ]
}

pub fn zone_quad(position: Vec3, yaw: f32, size: Vec2) -> [Vec3; 4] {
    let spin = Quat::from_rotation_y(yaw);
    let x = size.x / 2.0;
    let z = size.y / 2.0;
    [(-x, -z), (x, -z), (x, z), (-x, z)]
        .map(|(dx, dz)| position + spin * Vec3::new(dx, dim::BOARD_Y, dz))
}

pub fn dimmed(color: Color) -> Color {
    dimmed_to(color, GREY_LEVEL)
}

pub fn dimmed_to(color: Color, level: f32) -> Color {
    let mut shown = color.to_srgba();
    shown.red *= level;
    shown.green *= level;
    shown.blue *= level;
    Color::Srgba(shown)
}

pub fn exhausted_cards(mirror: &Mirror) -> BTreeSet<u32> {
    mirror
        .view
        .cards
        .iter()
        .filter(|entry| entry.badge(EXHAUSTED).is_some())
        .map(|entry| entry.id)
        .chain(
            mirror
                .solo_exhausted
                .iter()
                .copied()
                .filter(|id| mirror.view.card(*id).is_none()),
        )
        .collect()
}

pub fn restored(kept: Color, current: Color) -> Color {
    let mut out = kept;
    out.set_alpha(current.alpha());
    out
}

pub fn acting(view: &PluginView) -> bool {
    plugin_ui::enforced(view)
        && view.affordances.iter().any(|affordance| {
            affordance.enabled
                && matches!(
                    affordance.hotkey.as_deref(),
                    Some(plugin_ui::PASS_KEY | plugin_ui::ADVANCE_KEY)
                )
        })
}

pub fn actionable(kinds: &[LegalKind]) -> bool {
    kinds.iter().any(|kind| {
        matches!(
            kind,
            LegalKind::Play { .. } | LegalKind::React | LegalKind::Hide | LegalKind::Answer
        )
    })
}

pub fn greyed(view: &PluginView, hand: &BTreeSet<u32>) -> BTreeSet<u32> {
    if !acting(view) {
        return BTreeSet::new();
    }
    hand.iter()
        .copied()
        .filter(|card| {
            !view
                .legal
                .iter()
                .any(|row| row.card == *card && actionable(&row.kinds))
        })
        .collect()
}

pub fn lift_step(position: Vec3, applied: Option<Vec3>, want: bool) -> Option<Vec3> {
    let raised = applied == Some(position);
    match (want, raised) {
        (true, false) => Some(position + Vec3::Y * PLAY_LIFT),
        (false, true) => Some(position - Vec3::Y * PLAY_LIFT),
        _ => None,
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Rims {
    pub legal: BTreeMap<u32, RimKind>,
    pub pulse: BTreeSet<u32>,
    pub enemy: BTreeSet<u32>,
    pub destinations: BTreeMap<u32, Vec<u16>>,
    pub hides: BTreeMap<u32, Vec<u16>>,
    pub lift: BTreeSet<u32>,
    pub grey: BTreeSet<u32>,
    pub dim: BTreeSet<u32>,
}

impl Rims {
    pub fn kind(&self, card: u32) -> Option<RimKind> {
        self.legal.get(&card).copied()
    }

    pub fn pulses(&self, card: u32) -> bool {
        self.pulse.contains(&card)
    }

    pub fn hostile(&self, card: u32) -> bool {
        self.enemy.contains(&card)
    }

    pub fn lifts(&self, card: u32) -> bool {
        self.lift.contains(&card)
    }

    pub fn greys(&self, card: u32) -> bool {
        self.grey.contains(&card)
    }

    pub fn dims(&self, card: u32) -> bool {
        self.dim.contains(&card)
    }

    pub fn dim_level(&self, card: u32) -> Option<f32> {
        if self.greys(card) {
            Some(GREY_LEVEL)
        } else if self.dims(card) {
            Some(EXHAUST_LEVEL)
        } else {
            None
        }
    }

    pub fn destinations(&self, card: u32) -> &[u16] {
        self.destinations
            .get(&card)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn hides(&self, card: u32) -> &[u16] {
        self.hides.get(&card).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn hides_at(&self, card: u32) -> Option<u16> {
        self.hides(card).first().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.legal.is_empty()
            && self.pulse.is_empty()
            && self.enemy.is_empty()
            && self.grey.is_empty()
            && self.dim.is_empty()
    }
}

fn prompt_open(view: &PluginView, viewer: u8) -> bool {
    view.prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == viewer)
}

fn named_by(affordances: &[Affordance]) -> impl Iterator<Item = u32> + '_ {
    affordances
        .iter()
        .filter(|affordance| affordance.enabled)
        .filter_map(|affordance| affordance.card)
}

pub fn rims(
    view: &PluginView,
    viewer: u8,
    hand: &BTreeSet<u32>,
    owner: &BTreeMap<u32, u8>,
) -> Rims {
    let mut out = Rims::default();
    for row in &view.legal {
        if let Some(kind) = strongest(&row.kinds) {
            out.legal.insert(row.card, kind);
        }
        if !row.zones.is_empty() {
            out.destinations.insert(row.card, row.zones.clone());
        }
        if row.kinds.contains(&LegalKind::Hide) && !row.hidden.is_empty() {
            out.hides.insert(row.card, row.hidden.clone());
        }
        let playable = row
            .kinds
            .iter()
            .any(|kind| matches!(kind, LegalKind::Play { .. } | LegalKind::Hide));
        if playable && hand.contains(&row.card) {
            out.lift.insert(row.card);
        }
        if row.kinds.contains(&LegalKind::Answer) {
            out.pulse.insert(row.card);
        }
    }
    if prompt_open(view, viewer) {
        for card in named_by(&view.affordances) {
            out.pulse.insert(card);
            out.legal.entry(card).or_insert(RimKind::Answer);
        }
    }
    for arrow in &view.arrows {
        let TargetRef::Card(target) = arrow.to else {
            continue;
        };
        let source = match arrow.from {
            Origin::Card(card) => owner.get(&card).copied(),
            Origin::Item(item) => view
                .chain
                .iter()
                .find(|row| row.item == item)
                .map(|row| row.seat),
        };
        if source.is_some_and(|seat| seat != viewer) {
            out.enemy.insert(target);
        }
    }
    out.grey = greyed(view, hand);
    out
}

pub fn owners(table: &GameTable) -> BTreeMap<u32, u8> {
    table
        .cards()
        .iter()
        .map(|card| (card.id.0, card.owner.0))
        .collect()
}

pub fn refresh_rims(
    panel: Res<plugin_ui::PluginPanel>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    mut current: ResMut<Rims>,
) {
    if !panel.is_changed() && !table.is_changed() && !mirror.is_changed() && !my_seat.is_changed() {
        return;
    }
    let dim = exhausted_cards(&mirror);
    if panel.view.legal.is_empty()
        && panel.view.arrows.is_empty()
        && panel.view.affordances.is_empty()
        && dim.is_empty()
        && current.is_empty()
    {
        return;
    }
    let hand: BTreeSet<u32> = my_hand_ids(&table, &mirror, my_seat.0)
        .into_iter()
        .map(|id| id.0)
        .collect();
    let owner = owners(&table);
    let mut next = rims(&panel.view, my_seat.0 .0, &hand, &owner);
    next.dim = dim;
    if *current != next {
        *current = next;
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Lifted(pub Vec3);

pub(super) fn lift_playable(
    mut commands: Commands,
    rims: Res<Rims>,
    mut cards: Query<(Entity, &CardView, &mut Slot, Option<&Lifted>)>,
) {
    for (entity, card, mut slot, lifted) in &mut cards {
        let want = rims.lifts(card.0 .0);
        let applied = lifted.map(|state| state.0);
        if let Some(position) = lift_step(slot.position, applied, want) {
            slot.position = position;
            if want {
                commands.entity(entity).insert(Lifted(position));
                continue;
            }
        }
        if !want && applied.is_some() {
            commands.entity(entity).remove::<Lifted>();
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Greyed {
    pub kept: Color,
    pub level: f32,
}

pub(super) fn grey_unaffordable(
    mut commands: Commands,
    rims: Res<Rims>,
    cards: Query<(
        Entity,
        &CardView,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&Children>,
        Option<&Greyed>,
    )>,
    art_planes: Query<&MeshMaterial3d<StandardMaterial>, With<FoilArt>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, card, body, children, greyed) in &cards {
        let want = rims.dim_level(card.0 .0);
        if want == greyed.map(|state| state.level) {
            continue;
        }
        let handle = body.map(|material| material.0.clone()).or_else(|| {
            children
                .into_iter()
                .flatten()
                .find_map(|child| art_planes.get(*child).ok().map(|plane| plane.0.clone()))
        });
        let Some(handle) = handle else {
            continue;
        };
        let Some(mut material) = materials.get_mut(&handle) else {
            continue;
        };
        let kept = match greyed {
            None => material.base_color,
            Some(state) => restored(state.kept, material.base_color),
        };
        match want {
            Some(level) => {
                material.base_color = dimmed_to(kept, level);
                commands.entity(entity).insert(Greyed { kept, level });
            }
            None => {
                material.base_color = kept;
                commands.entity(entity).remove::<Greyed>();
            }
        }
    }
}

pub fn projected(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &GlobalTransform,
    corners: [Vec3; 4],
) -> Option<Vec<egui::Pos2>> {
    let points: Vec<egui::Pos2> = corners
        .iter()
        .filter_map(|corner| {
            camera
                .world_to_viewport(camera_transform, transform.transform_point(*corner))
                .ok()
                .map(|screen| egui::pos2(screen.x, screen.y))
        })
        .collect();
    (points.len() == 4).then_some(points)
}

pub fn top_edge(points: &[egui::Pos2]) -> Option<(egui::Pos2, egui::Pos2)> {
    let n = points.len();
    if n < 2 {
        return None;
    }
    (0..n)
        .map(|index| (points[index], points[(index + 1) % n]))
        .min_by(|left, right| {
            let ly = (left.0.y + left.1.y) / 2.0;
            let ry = (right.0.y + right.1.y) / 2.0;
            ly.partial_cmp(&ry).unwrap_or(std::cmp::Ordering::Equal)
        })
}

pub fn chevron_shape(edge: (egui::Pos2, egui::Pos2), stroke: egui::Stroke) -> egui::Shape {
    let mid = egui::pos2((edge.0.x + edge.1.x) / 2.0, (edge.0.y + edge.1.y) / 2.0);
    let along = (edge.1 - edge.0).normalized();
    let up = egui::vec2(along.y, -along.x);
    let up = if up.y > 0.0 { -up } else { up };
    let tip = mid + up * CHEVRON;
    egui::Shape::line(
        vec![mid - along * CHEVRON, tip, mid + along * CHEVRON],
        stroke,
    )
}

pub fn tag_shape(edge: (egui::Pos2, egui::Pos2), color: egui::Color32) -> egui::Shape {
    let corner = if edge.0.x > edge.1.x { edge.0 } else { edge.1 };
    let other = if edge.0.x > edge.1.x { edge.1 } else { edge.0 };
    let along = (other - corner).normalized();
    let down = egui::vec2(-along.y, along.x);
    let down = if down.y < 0.0 { -down } else { down };
    egui::Shape::convex_polygon(
        vec![corner, corner + along * TAG, corner + down * TAG],
        color,
        egui::Stroke::NONE,
    )
}

pub fn rim_shapes(
    points: Vec<egui::Pos2>,
    style: RimStyle,
    color: egui::Color32,
) -> Vec<egui::Shape> {
    let stroke = egui::Stroke::new(style.width, color);
    let mut closed = points.clone();
    if let Some(first) = points.first() {
        closed.push(*first);
    }
    let mut out = vec![match style.stroke {
        Stroke::Solid => egui::Shape::closed_line(points.clone(), stroke),
        Stroke::Dashed => {
            egui::Shape::Vec(egui::Shape::dashed_line(&closed, stroke, DASH, DASH_GAP))
        }
        Stroke::Dotted => egui::Shape::Vec(egui::Shape::dashed_line(&closed, stroke, DOT, DOT_GAP)),
    }];
    if let Some(edge) = top_edge(&points) {
        if style.chevron {
            out.push(chevron_shape(edge, stroke));
        }
        if style.tag {
            out.push(tag_shape(edge, color));
        }
    }
    out
}

pub const DRAWER_RIM_INSET: f32 = 2.0;

pub fn drawer_rim_rects(rects: &[(u32, egui::Rect)]) -> Vec<(u32, egui::Rect)> {
    let mut ordered: Vec<(u32, egui::Rect)> = rects.to_vec();
    ordered.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
    (0..ordered.len())
        .map(|index| {
            let (id, rect) = ordered[index];
            let right = ordered
                .get(index + 1)
                .map_or(rect.max.x, |(_, next)| next.min.x.min(rect.max.x));
            let clipped = egui::Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y))
                .shrink(DRAWER_RIM_INSET);
            (id, clipped)
        })
        .collect()
}

fn rect_points(rect: egui::Rect) -> Vec<egui::Pos2> {
    vec![
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn legal_rims_ui(
    mut contexts: EguiContexts,
    rims: Res<Rims>,
    time: Res<Time>,
    tuning: Res<Tuning>,
    viewport: Res<crate::viewport::Viewport>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(
        &CardView,
        &GlobalTransform,
        &ViewVisibility,
        Has<Landscape>,
        &Slot,
    )>,
) -> Result {
    if rims.legal.is_empty() && rims.enemy.is_empty() {
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let palette = Palette::of(&tuning);
    let painter = contexts
        .ctx_mut()?
        .layer_painter(egui::LayerId::background());
    let alpha = pulse_alpha(time.elapsed_secs());
    let phone = viewport.class.is_phone();
    let mut drawer: Vec<(u32, egui::Rect)> = Vec::new();
    let paint = |painter: &egui::Painter, id: u32, kind: RimKind, points: Vec<egui::Pos2>| {
        let style = rim_style(kind);
        let color = if rims.pulses(id) || style.pulses {
            faded(rim_color_in(kind, palette), alpha)
        } else {
            rim_color_in(kind, palette)
        };
        for shape in rim_shapes(points, style, color) {
            painter.add(shape);
        }
    };
    for (card, transform, visibility, landscape, slot) in &cards {
        if !visibility.get() {
            continue;
        }
        let id = card.0 .0;
        let (width, height) = if landscape {
            (dim::CARD_H, dim::CARD_W)
        } else {
            (dim::CARD_W, dim::CARD_H)
        };
        if phone && slot.facing == Facing::Camera {
            if let Some(points) = projected(
                camera,
                camera_transform,
                transform,
                rim_ring(width, height, 0.0),
            ) {
                drawer.push((id, egui::Rect::from_points(&points)));
            }
            continue;
        }
        if let Some(kind) = rims.kind(id) {
            if let Some(points) = projected(
                camera,
                camera_transform,
                transform,
                rim_ring(width, height, LEGAL_OUTSET),
            ) {
                paint(&painter, id, kind, points);
            }
        }
        if rims.hostile(id) {
            if let Some(points) = projected(
                camera,
                camera_transform,
                transform,
                rim_ring(width, height, ENEMY_OUTSET),
            ) {
                let style = rim_style(RimKind::Enemy);
                for shape in rim_shapes(points, style, rim_color_in(RimKind::Enemy, palette)) {
                    painter.add(shape);
                }
            }
        }
    }
    for (id, rect) in drawer_rim_rects(&drawer) {
        if let Some(kind) = rims.kind(id) {
            paint(&painter, id, kind, rect_points(rect));
        }
    }
    Ok(())
}

pub fn tinted(anchor_seat: Option<u8>, viewer: u8, zone: u16, wanted: &[u16]) -> bool {
    wanted.contains(&zone) && anchor_seat.is_none_or(|seat| seat == viewer)
}

pub const DIM_ALPHA: u8 = 77;

pub fn tint_layers(rims: &Rims, cards: &[u32]) -> Vec<(RimKind, Vec<u16>)> {
    let mut layers: BTreeMap<RimKind, Vec<u16>> = BTreeMap::new();
    for card in cards {
        let kind = rims.kind(*card).unwrap_or(RimKind::March);
        let kind = if kind == RimKind::Hide {
            RimKind::Play
        } else {
            kind
        };
        layers
            .entry(kind)
            .or_default()
            .extend(rims.destinations(*card).iter().copied());
        layers
            .entry(RimKind::Hide)
            .or_default()
            .extend(rims.hides(*card).iter().copied());
    }
    layers
        .into_iter()
        .map(|(kind, mut zones)| {
            zones.sort_unstable();
            zones.dedup();
            (kind, zones)
        })
        .filter(|(_, zones)| !zones.is_empty())
        .collect()
}

pub fn dimmed_zones(anchors: &[zones::ZoneAnchor], lit: &[u16], viewer: u8) -> Vec<usize> {
    anchors
        .iter()
        .enumerate()
        .filter(|(_, anchor)| !tinted(anchor.seat.map(|seat| seat.0), viewer, anchor.zone, lit))
        .map(|(index, _)| index)
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn march_tint_ui(
    mut contexts: EguiContexts,
    rims: Res<Rims>,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    held: Res<Held>,
    selected: Res<Selected>,
    tuning: Res<Tuning>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    hovered: Query<Entity, (With<Hovered>, With<CardView>)>,
    views: Query<&CardView>,
) -> Result {
    let palette = Palette::of(&tuning);
    let focus = held
        .card
        .or_else(|| hovered.iter().next())
        .or(selected.0)
        .and_then(|entity| views.get(entity).ok());
    let cards: Vec<u32> = focus.map(|card| card.0 .0).into_iter().collect();
    let layers = tint_layers(&rims, &cards);
    if layers.is_empty() {
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let painter = contexts
        .ctx_mut()?
        .layer_painter(egui::LayerId::background());
    let anchors = zones::anchors(
        &mirror.view.zones,
        players.0,
        info.battlefields_in_play(players.0),
    );
    if held.card.is_some() {
        let lit: Vec<u16> = layers
            .iter()
            .flat_map(|(_, zones)| zones.iter().copied())
            .collect();
        for index in dimmed_zones(&anchors, &lit, my_seat.0 .0) {
            let anchor = &anchors[index];
            let Some(points) = projected(
                camera,
                camera_transform,
                &GlobalTransform::IDENTITY,
                zone_quad(anchor.position, anchor.yaw, anchor.size),
            ) else {
                continue;
            };
            painter.add(egui::Shape::convex_polygon(
                points,
                egui::Color32::from_black_alpha(DIM_ALPHA),
                egui::Stroke::NONE,
            ));
        }
    }
    for (kind, wanted) in layers {
        let ink = rim_color_in(kind, palette);
        for anchor in &anchors {
            if !tinted(
                anchor.seat.map(|seat| seat.0),
                my_seat.0 .0,
                anchor.zone,
                &wanted,
            ) {
                continue;
            }
            let Some(points) = projected(
                camera,
                camera_transform,
                &GlobalTransform::IDENTITY,
                zone_quad(anchor.position, anchor.yaw, anchor.size),
            ) else {
                continue;
            };
            painter.add(egui::Shape::convex_polygon(
                points,
                faded(ink, TINT_ALPHA),
                egui::Stroke::new(TINT_WIDTH, faded(ink, TINT_EDGE)),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Arrow, ArrowKind, ChainRow, Legal, PromptSummary};

    #[test]
    fn drawer_rims_stay_inside_the_visible_slice_of_each_fanned_card() {
        let card =
            |x: f32| egui::Rect::from_min_size(egui::pos2(x, 620.0), egui::vec2(96.0, 134.0));
        let rects = vec![(7, card(92.0)), (5, card(8.0)), (9, card(176.0))];
        let rims = drawer_rim_rects(&rects);
        assert_eq!(
            rims.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [5, 7, 9]
        );
        let first = rims[0].1;
        assert_eq!(first.min.x, 8.0 + DRAWER_RIM_INSET);
        assert_eq!(
            first.max.x,
            92.0 - DRAWER_RIM_INSET,
            "the rim ends where the next card covers this one"
        );
        assert_eq!(first.min.y, 620.0 + DRAWER_RIM_INSET);
        assert_eq!(first.max.y, 754.0 - DRAWER_RIM_INSET);
        let last = rims[2].1;
        assert_eq!(
            last.max.x,
            272.0 - DRAWER_RIM_INSET,
            "the top card keeps its full width"
        );
        for (_, rim) in &rims {
            for (_, other) in &rims {
                if rim != other {
                    assert!(
                        rim.max.x <= other.min.x || other.max.x <= rim.min.x,
                        "no two drawer rims cross: {rim:?} {other:?}"
                    );
                }
            }
        }
    }

    fn legal_row(card: u32, kinds: Vec<LegalKind>, zones: Vec<u16>) -> Legal {
        Legal {
            card,
            kinds,
            zones,
            hidden: Vec::new(),
        }
    }

    fn view_with(legal: Vec<Legal>) -> PluginView {
        PluginView {
            legal,
            ..Default::default()
        }
    }

    fn button(card: u32) -> Affordance {
        Affordance {
            label: "pick".into(),
            enabled: true,
            card: Some(card),
            ..Default::default()
        }
    }

    fn seats(pairs: &[(u32, u8)]) -> BTreeMap<u32, u8> {
        pairs.iter().copied().collect()
    }

    fn hand(ids: &[u32]) -> BTreeSet<u32> {
        ids.iter().copied().collect()
    }

    const EVERY_KIND: [RimKind; 7] = [
        RimKind::Play,
        RimKind::March,
        RimKind::Activate,
        RimKind::React,
        RimKind::Hide,
        RimKind::Answer,
        RimKind::Enemy,
    ];

    fn hide_row(card: u32, hidden: Vec<u16>) -> Legal {
        Legal {
            card,
            kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
            zones: vec![agni_riftbound::ZONE_BASE],
            hidden,
        }
    }

    #[test]
    fn every_kind_paints_its_own_colour_and_hide_wears_plays() {
        for palette in Palette::BOTH {
            let colors: BTreeSet<[u8; 4]> = EVERY_KIND
                .iter()
                .map(|kind| rim_color_in(*kind, palette).to_array())
                .collect();
            assert_eq!(colors.len(), EVERY_KIND.len() - 1, "{palette:?}");
            assert_eq!(
                rim_rgb_in(RimKind::Hide, palette),
                rim_rgb_in(RimKind::Play, palette),
                "hide is a way of playing, so it is the play colour dotted"
            );
            for kind in EVERY_KIND {
                let [r, g, b] = rim_rgb_in(kind, palette);
                assert_eq!(rim_color_in(kind, palette).to_array(), [r, g, b, 255]);
            }
        }
        assert_eq!(
            rim_rgb(RimKind::Play),
            rim_rgb_in(RimKind::Play, Palette::Standard)
        );
        assert_eq!(
            rim_color(RimKind::React),
            rim_color_in(RimKind::React, Palette::Standard)
        );
        assert_eq!(apart([0, 0, 0], [0, 0, 0]), 0.0);
        assert!((apart([0, 0, 0], [3, 4, 0]) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn the_five_legality_hues_are_pairwise_apart_in_both_palettes() {
        for palette in Palette::BOTH {
            for (index, left) in RimKind::LEGALITY.iter().enumerate() {
                for right in &RimKind::LEGALITY[index + 1..] {
                    let distance = apart(rim_rgb_in(*left, palette), rim_rgb_in(*right, palette));
                    assert!(
                        distance >= 60.0,
                        "{left:?} and {right:?} sit {distance} apart in {palette:?}"
                    );
                }
                let enemy = apart(
                    rim_rgb_in(*left, palette),
                    rim_rgb_in(RimKind::Enemy, palette),
                );
                assert!(
                    enemy >= 60.0,
                    "{left:?} sits {enemy} from the enemy ring in {palette:?}"
                );
            }
        }
        assert_eq!(
            rim_rgb_in(RimKind::Play, Palette::ColourBlind)[2],
            255,
            "play turns blue"
        );
        assert_eq!(
            rim_rgb_in(RimKind::Enemy, Palette::ColourBlind),
            [255, 128, 0],
            "enemy turns orange"
        );
        assert_eq!(Palette::of(&Tuning::default()), Palette::Standard);
        let blind = Tuning {
            colour_blind: true,
            ..Tuning::default()
        };
        assert_eq!(Palette::of(&blind), Palette::ColourBlind);
    }

    #[test]
    fn every_rim_pairs_its_hue_with_a_stroke_so_colour_is_never_the_only_signal() {
        for (index, left) in RimKind::ALL.iter().enumerate() {
            for right in &RimKind::ALL[index + 1..] {
                let styles_differ = rim_style(*left) != rim_style(*right);
                assert!(
                    styles_differ,
                    "{left:?} and {right:?} are drawn with the same stroke pattern"
                );
                if same_family(*left, *right) {
                    assert_eq!(rim_rgb(*left), rim_rgb(*right));
                    assert_ne!(rim_style(*left).stroke, rim_style(*right).stroke);
                }
            }
        }
        assert_eq!(rim_style(RimKind::Play).stroke, Stroke::Solid);
        assert!(rim_style(RimKind::March).chevron);
        assert!(rim_style(RimKind::Activate).tag);
        assert_eq!(rim_style(RimKind::React).stroke, Stroke::Dashed);
        assert!(rim_style(RimKind::Answer).pulses);
        assert_eq!(rim_style(RimKind::Answer).width, 3.0);
        assert_eq!(rim_style(RimKind::Hide).stroke, Stroke::Dotted);
        assert!(rim_style(RimKind::Enemy).ring);
        assert!(same_family(RimKind::Hide, RimKind::Play));
        assert!(!same_family(RimKind::Play, RimKind::March));
        let legends: BTreeSet<&str> = RimKind::ALL.iter().map(|kind| kind.legend()).collect();
        assert_eq!(legends.len(), RimKind::ALL.len());
    }

    #[test]
    fn the_answer_rim_outranks_react_which_outranks_play_ability_and_march() {
        assert_eq!(
            strongest(&[LegalKind::March, LegalKind::Play { accelerate: false }]),
            Some(RimKind::Play)
        );
        assert_eq!(
            strongest(&[LegalKind::March, LegalKind::Activate { ability: 0 }]),
            Some(RimKind::Activate)
        );
        assert_eq!(
            strongest(&[
                LegalKind::Activate { ability: 0 },
                LegalKind::Play { accelerate: false }
            ]),
            Some(RimKind::Play)
        );
        assert_eq!(
            strongest(&[LegalKind::Play { accelerate: false }, LegalKind::React]),
            Some(RimKind::React)
        );
        assert_eq!(
            strongest(&[LegalKind::React, LegalKind::Answer, LegalKind::March]),
            Some(RimKind::Answer)
        );
        assert_eq!(
            strongest(&[LegalKind::Hide, LegalKind::Play { accelerate: false }]),
            Some(RimKind::Play)
        );
        assert_eq!(strongest(&[LegalKind::Hide]), Some(RimKind::Hide));
        let order: Vec<RimKind> = RimKind::ALL.to_vec();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, order, "ALL lists the kinds in precedence order");
    }

    #[test]
    fn a_rim_is_drawn_from_its_pattern_with_the_chevron_and_tag_on_the_top_edge() {
        let points = vec![
            egui::pos2(10.0, 10.0),
            egui::pos2(50.0, 10.0),
            egui::pos2(50.0, 70.0),
            egui::pos2(10.0, 70.0),
        ];
        let edge = top_edge(&points).unwrap();
        assert_eq!(edge, (egui::pos2(10.0, 10.0), egui::pos2(50.0, 10.0)));
        let ink = rim_color(RimKind::March);
        assert_eq!(
            rim_shapes(points.clone(), rim_style(RimKind::Play), ink).len(),
            1
        );
        assert_eq!(
            rim_shapes(points.clone(), rim_style(RimKind::March), ink).len(),
            2
        );
        assert_eq!(
            rim_shapes(points.clone(), rim_style(RimKind::Activate), ink).len(),
            2
        );
        let dashed = rim_shapes(points.clone(), rim_style(RimKind::React), ink);
        assert_eq!(dashed.len(), 1);
        assert!(
            matches!(dashed[0], egui::Shape::Vec(_)),
            "a dashed ring is many segments"
        );
        let solid = rim_shapes(points.clone(), rim_style(RimKind::Play), ink);
        assert!(matches!(solid[0], egui::Shape::Path(_)));
        if let egui::Shape::LineSegment { .. } | egui::Shape::Path(_) =
            &rim_shapes(points.clone(), rim_style(RimKind::March), ink)[1]
        {
        } else {
            panic!("the chevron is a polyline");
        }
        let tag = &rim_shapes(points, rim_style(RimKind::Activate), ink)[1];
        let egui::Shape::Path(path) = tag else {
            panic!("the tag is a filled triangle");
        };
        assert_eq!(path.points.len(), 3);
        assert!(
            path.points.contains(&egui::pos2(50.0, 10.0)),
            "at the top-right corner"
        );
        assert!(top_edge(&[]).is_none());
    }

    #[test]
    fn an_exhausted_card_dims_to_sixty_percent_and_an_unaffordable_one_further() {
        let mut rims = Rims::default();
        rims.dim.insert(4);
        rims.grey.insert(5);
        rims.grey.insert(6);
        rims.dim.insert(6);
        assert_eq!(rims.dim_level(4), Some(EXHAUST_LEVEL));
        assert_eq!(rims.dim_level(5), Some(GREY_LEVEL));
        assert_eq!(rims.dim_level(6), Some(GREY_LEVEL), "the stronger dim wins");
        assert_eq!(rims.dim_level(7), None);
        assert!(!rims.is_empty());
        let mut mirror = Mirror::default();
        mirror.solo_exhausted.insert(9);
        mirror.view.cards.push(agni_sim::view::ViewCard {
            id: 3,
            zone: Zone::Plugin(1),
            seat: 0,
            owner: 0,
            face_visible: true,
            badges: vec![agni_sim::view::Badge {
                key: EXHAUSTED.to_string(),
                value: serde_bytes::ByteBuf::from(vec![1]),
            }],
        });
        mirror.view.cards.push(agni_sim::view::ViewCard {
            id: 4,
            zone: Zone::Plugin(1),
            seat: 0,
            owner: 0,
            face_visible: true,
            badges: Vec::new(),
        });
        assert_eq!(exhausted_cards(&mirror), [3, 9].into_iter().collect());
        let bright = Color::srgb(1.0, 0.5, 0.0);
        let dim = dimmed_to(bright, EXHAUST_LEVEL).to_srgba();
        assert!((dim.red - 0.6).abs() < 1e-5 && (dim.green - 0.3).abs() < 1e-5);
    }

    #[test]
    fn no_rim_colour_can_be_mistaken_for_a_seat_colour_or_an_arrow() {
        for (kind, palette) in EVERY_KIND
            .iter()
            .flat_map(|kind| Palette::BOTH.map(|palette| (*kind, palette)))
        {
            let ink = rim_rgb_in(kind, palette);
            for (name, seat) in colors::SEAT_COLORS {
                assert!(
                    apart(ink, seat) >= 55.0,
                    "{kind:?} sits {} from the {name} seat rim drawn just inside it",
                    apart(ink, seat)
                );
            }
            for arrow in [
                agni_sim::wire::ArrowKind::Spell,
                agni_sim::wire::ArrowKind::Ability,
                agni_sim::wire::ArrowKind::Attack,
                agni_sim::wire::ArrowKind::Counter,
                agni_sim::wire::ArrowKind::Combat,
            ] {
                assert!(
                    apart(ink, arrows::tint(arrow)) >= 50.0,
                    "{kind:?} sits {} from the {arrow:?} arrow",
                    apart(ink, arrows::tint(arrow))
                );
            }
        }
    }

    #[test]
    fn a_legal_kind_maps_to_its_rim_and_the_strongest_wins() {
        assert_eq!(rim_of(LegalKind::Play { accelerate: true }), RimKind::Play);
        assert_eq!(
            rim_of(LegalKind::Activate { ability: 2 }),
            RimKind::Activate
        );
        assert_eq!(
            strongest(&[LegalKind::March, LegalKind::Activate { ability: 0 }]),
            Some(RimKind::Activate)
        );
        assert_eq!(
            strongest(&[
                LegalKind::March,
                LegalKind::Answer,
                LegalKind::Play { accelerate: false }
            ]),
            Some(RimKind::Answer)
        );
        assert_eq!(strongest(&[]), None);
    }

    #[test]
    fn a_view_without_legal_rows_or_arrows_makes_no_rims() {
        let empty = rims(&PluginView::default(), 0, &hand(&[]), &seats(&[]));
        assert!(empty.is_empty());
        assert_eq!(empty, Rims::default());
        assert!(empty.destinations(7).is_empty());
    }

    #[test]
    fn each_row_becomes_a_rim_and_a_march_row_carries_its_destinations() {
        let view = view_with(vec![
            legal_row(1, vec![LegalKind::Play { accelerate: true }], vec![9]),
            legal_row(2, vec![LegalKind::March], vec![10, 11]),
            legal_row(3, vec![LegalKind::Activate { ability: 0 }], vec![]),
            legal_row(4, vec![LegalKind::React], vec![]),
            legal_row(5, vec![LegalKind::Answer], vec![]),
        ]);
        let out = rims(&view, 0, &hand(&[1]), &seats(&[]));
        assert_eq!(out.kind(1), Some(RimKind::Play));
        assert_eq!(out.kind(2), Some(RimKind::March));
        assert_eq!(out.kind(3), Some(RimKind::Activate));
        assert_eq!(out.kind(4), Some(RimKind::React));
        assert_eq!(out.kind(5), Some(RimKind::Answer));
        assert_eq!(out.destinations(2), &[10, 11]);
        assert_eq!(
            out.destinations(1),
            &[9],
            "a play row's zones light while the card is held, like a march's"
        );
        assert!(out.destinations(3).is_empty());
    }

    #[test]
    fn only_a_playable_card_that_is_in_my_hand_lifts() {
        let view = view_with(vec![
            legal_row(1, vec![LegalKind::Play { accelerate: false }], vec![]),
            legal_row(2, vec![LegalKind::Play { accelerate: false }], vec![]),
            legal_row(3, vec![LegalKind::March], vec![9]),
        ]);
        let out = rims(&view, 0, &hand(&[1, 3]), &seats(&[]));
        assert!(out.lifts(1));
        assert!(!out.lifts(2));
        assert!(!out.lifts(3));
    }

    #[test]
    fn an_affordance_only_candidate_gets_a_ring_for_its_pulse_to_modulate() {
        let mut view = view_with(vec![]);
        view.affordances = vec![button(2)];
        view.prompt = Some(PromptSummary {
            seat: 0,
            why: "choose".into(),
            min: 1,
            max: 1,
            picked: 0,
            optional: false,
        });
        let out = rims(&view, 0, &hand(&[]), &seats(&[]));
        assert!(out.pulses(2));
        assert_eq!(
            out.kind(2),
            Some(RimKind::Answer),
            "a pulse with no ring paints nothing"
        );
    }

    #[test]
    fn a_legal_row_keeps_its_own_kind_against_an_affordance_of_the_same_card() {
        let mut view = view_with(vec![legal_row(
            2,
            vec![LegalKind::Play { accelerate: false }],
            vec![],
        )]);
        view.affordances = vec![button(2)];
        view.prompt = Some(PromptSummary {
            seat: 0,
            why: "choose".into(),
            min: 1,
            max: 1,
            picked: 0,
            optional: false,
        });
        let out = rims(&view, 0, &hand(&[]), &seats(&[]));
        assert_eq!(out.kind(2), Some(RimKind::Play));
    }

    #[test]
    fn prompt_candidates_pulse_and_a_prompt_for_the_other_seat_does_not() {
        let mut view = view_with(vec![legal_row(1, vec![LegalKind::Answer], vec![])]);
        view.affordances = vec![button(2), button(3)];
        view.prompt = Some(PromptSummary {
            seat: 0,
            why: "choose".into(),
            min: 1,
            max: 1,
            picked: 0,
            optional: false,
        });
        let mine = rims(&view, 0, &hand(&[]), &seats(&[]));
        assert!(mine.pulses(1) && mine.pulses(2) && mine.pulses(3));
        let theirs = rims(&view, 1, &hand(&[]), &seats(&[]));
        assert!(theirs.pulses(1));
        assert!(!theirs.pulses(2) && !theirs.pulses(3));
    }

    #[test]
    fn an_enemy_arrow_reddens_its_target_and_my_own_does_not() {
        let mut view = view_with(vec![]);
        view.arrows = vec![
            Arrow {
                from: Origin::Card(10),
                to: TargetRef::Card(20),
                kind: ArrowKind::Spell,
            },
            Arrow {
                from: Origin::Card(11),
                to: TargetRef::Card(21),
                kind: ArrowKind::Spell,
            },
            Arrow {
                from: Origin::Card(10),
                to: TargetRef::Zone(4),
                kind: ArrowKind::Attack,
            },
        ];
        let owner = seats(&[(10, 1), (11, 0), (20, 0), (21, 1)]);
        let out = rims(&view, 0, &hand(&[]), &owner);
        assert!(out.hostile(20));
        assert!(!out.hostile(21));
        assert!(!out.is_empty());
    }

    #[test]
    fn an_item_arrow_reddens_through_the_chain_rows_controller() {
        let mut view = view_with(vec![]);
        view.arrows = vec![
            Arrow {
                from: Origin::Item(3),
                to: TargetRef::Card(20),
                kind: ArrowKind::Ability,
            },
            Arrow {
                from: Origin::Item(4),
                to: TargetRef::Card(21),
                kind: ArrowKind::Ability,
            },
        ];
        view.chain = vec![
            ChainRow {
                item: 3,
                card: None,
                seat: 1,
            },
            ChainRow {
                item: 4,
                card: None,
                seat: 0,
            },
        ];
        let out = rims(&view, 0, &hand(&[]), &seats(&[(20, 0), (21, 0)]));
        assert!(
            out.hostile(20),
            "an item the other seat controls is hostile"
        );
        assert!(!out.hostile(21), "my own trigger is not");
        let orphan = rims(&view_with(vec![]), 0, &hand(&[]), &seats(&[]));
        assert!(orphan.is_empty());
    }

    #[test]
    fn a_card_at_a_shared_battlefield_is_owned_by_who_played_it_not_by_the_zones_seat() {
        let mut table = agni_core::Table::new();
        let mine = table.add(PlayerId(0), Zone::Plugin(9), "Vi", [1, 1, 1]);
        let theirs = table.add(PlayerId(1), Zone::Plugin(3), "Jinx", [2, 2, 2]);
        assert!(table.apply(agni_core::Intent::MoveCard {
            card: theirs,
            to: Zone::Plugin(9),
            seat: PlayerId(0),
            index: 0,
        }));
        let held = table
            .cards()
            .iter()
            .find(|card| card.id == theirs)
            .expect("the enemy unit sits at the shared battlefield");
        assert_eq!(
            (held.owner, held.seat),
            (PlayerId(1), PlayerId(0)),
            "a shared zone files the card under seat 0 whoever owns it"
        );
        let map = owners(&GameTable(table));
        assert_eq!(map.get(&theirs.0), Some(&1), "the enemy unit stays theirs");
        assert_eq!(map.get(&mine.0), Some(&0));
    }

    #[test]
    fn only_the_viewers_own_copy_of_a_per_seat_destination_is_tinted() {
        let wanted = [8u16, 9];
        assert!(tinted(Some(0), 0, 8, &wanted), "my own base tints");
        assert!(!tinted(Some(1), 0, 8, &wanted), "the enemy base does not");
        assert!(tinted(None, 0, 9, &wanted), "a shared battlefield tints");
        assert!(!tinted(None, 0, 7, &wanted), "an unlisted zone does not");
    }

    #[test]
    fn folding_the_same_view_twice_yields_an_equal_set() {
        let mut view = view_with(vec![
            legal_row(1, vec![LegalKind::Play { accelerate: false }], vec![]),
            legal_row(2, vec![LegalKind::March], vec![7, 8]),
        ]);
        view.arrows = vec![Arrow {
            from: Origin::Card(9),
            to: TargetRef::Card(1),
            kind: ArrowKind::Counter,
        }];
        let owner = seats(&[(9, 1), (1, 0)]);
        let first = rims(&view, 0, &hand(&[1]), &owner);
        let second = rims(&view, 0, &hand(&[1]), &owner);
        assert_eq!(first, second);
    }

    #[test]
    fn a_lift_is_applied_once_and_dropped_once() {
        let base = Vec3::new(1.0, 0.5, 2.0);
        let up = lift_step(base, None, true).expect("a playable card rises");
        assert_eq!(up, base + Vec3::Y * PLAY_LIFT);
        assert_eq!(lift_step(up, Some(up), true), None);
        let down = lift_step(up, Some(up), false).expect("it settles back");
        assert_eq!(down, base);
        assert_eq!(lift_step(base, None, false), None);
    }

    #[test]
    fn a_relaid_slot_is_lifted_again_from_its_new_base() {
        let old = Vec3::new(0.0, 0.5, 0.0);
        let lifted = lift_step(old, None, true).expect("rises");
        let fresh = Vec3::new(0.4, 0.5, 0.0);
        assert_eq!(
            lift_step(fresh, Some(lifted), true),
            Some(fresh + Vec3::Y * PLAY_LIFT)
        );
        assert_eq!(lift_step(fresh, Some(lifted), false), None);
    }

    #[test]
    fn the_pulse_stays_in_band_and_repeats() {
        assert_eq!(pulse_alpha(0.0), pulse_alpha(PULSE_SECS));
        assert_eq!(pulse_alpha(PULSE_SECS / 4.0), 255);
        assert_eq!(pulse_alpha(PULSE_SECS * 0.75), PULSE_FLOOR);
        for step in 0..64 {
            let alpha = pulse_alpha(step as f32 * 0.05);
            assert!(alpha >= PULSE_FLOOR);
        }
    }

    fn hue_of(color: egui::Color32) -> [u8; 3] {
        let [r, g, b, _] = color.to_srgba_unmultiplied();
        [r, g, b]
    }

    #[test]
    fn a_faded_rim_stays_nearer_its_own_hue_than_any_other_kinds() {
        for kind in EVERY_KIND {
            let mine = rim_rgb(kind);
            for alpha in [90u8, 160, 255] {
                let dim = faded(rim_color(kind), alpha);
                assert_eq!(dim.a(), alpha, "{kind:?} keeps the alpha it was given");
                let shown = hue_of(dim);
                assert!(
                    apart(shown, mine) <= 2.0,
                    "{kind:?} drifted to {shown:?} at alpha {alpha}"
                );
                for other in EVERY_KIND
                    .iter()
                    .filter(|held| **held != kind && !same_family(**held, kind))
                {
                    assert!(
                        apart(shown, mine) < apart(shown, rim_rgb(*other)),
                        "{kind:?} at alpha {alpha} reads as {other:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_legal_ring_sits_outside_the_m0_affordance_rim() {
        let mine = rim_ring(dim::CARD_W, dim::CARD_H, LEGAL_OUTSET);
        let m0 = plugin_ui::rim_corners(dim::CARD_W, dim::CARD_H);
        let enemy = rim_ring(dim::CARD_W, dim::CARD_H, ENEMY_OUTSET);
        assert!(mine[2].x > m0[2].x && mine[2].z > m0[2].z);
        assert!(enemy[2].x > mine[2].x && enemy[2].z > mine[2].z);
        assert_eq!(mine[0], Vec3::new(-mine[2].x, mine[2].y, -mine[2].z));
        let wide = rim_ring(dim::CARD_H, dim::CARD_W, LEGAL_OUTSET);
        assert!(wide[2].x > wide[2].z);
    }

    #[test]
    fn a_zone_quad_covers_the_anchor_rect_and_turns_with_the_seat() {
        let size = Vec2::new(2.0, 1.0);
        let flat = zone_quad(Vec3::ZERO, 0.0, size);
        assert!((flat[2].x - 1.0).abs() < 1e-5);
        assert!((flat[2].z - 0.5).abs() < 1e-5);
        assert!(flat.iter().all(|c| (c.y - dim::BOARD_Y).abs() < 1e-5));
        let turned = zone_quad(Vec3::ZERO, std::f32::consts::PI, size);
        assert!((turned[2].x + 1.0).abs() < 1e-4);
        assert!((turned[2].z + 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_hide_row_lights_its_own_rim_and_names_where_the_card_may_go() {
        let bf1 = 9u16;
        let view = view_with(vec![
            hide_row(7, vec![bf1]),
            legal_row(8, vec![LegalKind::React], vec![12]),
        ]);
        let out = rims(&view, 0, &hand(&[7]), &seats(&[(7, 0), (8, 0)]));
        assert_eq!(
            out.kind(7),
            Some(RimKind::Play),
            "a card that can be played normally keeps the play rim"
        );
        assert_eq!(
            out.hides(7),
            [bf1],
            "the hide destinations are read from the row's own list, never from the play zones"
        );
        assert_eq!(out.hides_at(7), Some(bf1));
        assert!(
            out.lifts(7),
            "and it still lifts out of the hand as an actionable card"
        );
        let only_hide = view_with(vec![Legal {
            card: 7,
            kinds: vec![LegalKind::Hide],
            zones: Vec::new(),
            hidden: vec![bf1],
        }]);
        let out = rims(&only_hide, 0, &hand(&[7]), &seats(&[(7, 0)]));
        assert_eq!(
            out.kind(7),
            Some(RimKind::Hide),
            "a card only hideable wears the hide rim"
        );
        assert!(out.lifts(7));
        assert_eq!(out.hides(8), &[] as &[u16]);
        assert_eq!(out.hides_at(8), None);
    }

    fn enforced_view(legal: Vec<Legal>, hotkey: Option<&str>) -> PluginView {
        let mut view = view_with(legal);
        view.status = vec!["turn 2 · {seat 0} · action phase · rules enforced".into()];
        if let Some(key) = hotkey {
            view.affordances.push(Affordance {
                label: "end turn".into(),
                enabled: true,
                hotkey: Some(key.into()),
                ..Default::default()
            });
        }
        view
    }

    #[test]
    fn a_hand_card_with_no_play_row_greys_while_the_seat_is_acting() {
        let view = enforced_view(
            vec![
                legal_row(1, vec![LegalKind::Play { accelerate: false }], vec![9]),
                legal_row(2, vec![LegalKind::React], vec![]),
                legal_row(3, vec![LegalKind::Hide], vec![9]),
                legal_row(4, vec![LegalKind::Answer], vec![]),
                legal_row(50, vec![LegalKind::March], vec![9]),
            ],
            Some(plugin_ui::ADVANCE_KEY),
        );
        let held = hand(&[1, 2, 3, 4, 5, 6]);
        assert!(acting(&view));
        assert_eq!(greyed(&view, &held), hand(&[5, 6]));
        let out = rims(&view, 0, &held, &seats(&[]));
        assert!(out.greys(5) && out.greys(6));
        assert!(!out.greys(1) && !out.greys(2) && !out.greys(3) && !out.greys(4));
        assert!(!out.greys(50), "a unit on the board is never greyed");
        assert_eq!(out.kind(5), None, "a greyed card wears no rim");
        assert!(!out.lifts(5), "and does not lift");
        assert!(!out.is_empty());
    }

    #[test]
    fn a_priority_holder_greys_its_non_reactions() {
        let view = enforced_view(
            vec![legal_row(2, vec![LegalKind::React], vec![])],
            Some(plugin_ui::PASS_KEY),
        );
        assert_eq!(greyed(&view, &hand(&[1, 2])), hand(&[1]));
    }

    #[test]
    fn nothing_greys_while_the_seat_is_only_waiting_or_answering_a_prompt() {
        let waiting = enforced_view(vec![], None);
        assert!(!acting(&waiting));
        assert!(greyed(&waiting, &hand(&[1, 2])).is_empty());
        let mut prompted = enforced_view(vec![legal_row(1, vec![LegalKind::Answer], vec![])], None);
        prompted.affordances = vec![button(1)];
        prompted.prompt = Some(PromptSummary {
            seat: 0,
            why: "set aside up to 2 cards to redraw".into(),
            min: 0,
            max: 2,
            picked: 0,
            optional: false,
        });
        assert!(greyed(&prompted, &hand(&[1, 2])).is_empty());
        let mut disabled = enforced_view(vec![], Some(plugin_ui::ADVANCE_KEY));
        disabled.affordances[0].enabled = false;
        assert!(greyed(&disabled, &hand(&[1])).is_empty());
    }

    #[test]
    fn a_free_table_and_a_table_without_a_plugin_grey_nothing() {
        let mut free = enforced_view(vec![], Some(plugin_ui::ADVANCE_KEY));
        free.status = vec!["turn 2 · {seat 0} · action phase · free table".into()];
        assert!(!acting(&free));
        assert!(greyed(&free, &hand(&[1, 2, 3])).is_empty());
        let out = rims(&free, 0, &hand(&[1, 2, 3]), &seats(&[]));
        assert!(out.grey.is_empty());
        assert!(out.is_empty());
        let none = PluginView::default();
        assert!(greyed(&none, &hand(&[1, 2, 3])).is_empty());
        assert!(rims(&none, 0, &hand(&[1]), &seats(&[])).is_empty());
    }

    #[test]
    fn the_actionable_kinds_are_the_ones_a_hand_card_can_be_dragged_for() {
        assert!(actionable(&[LegalKind::Play { accelerate: true }]));
        assert!(actionable(&[LegalKind::React]));
        assert!(actionable(&[LegalKind::Hide]));
        assert!(actionable(&[LegalKind::Answer]));
        assert!(actionable(&[
            LegalKind::March,
            LegalKind::Play { accelerate: false }
        ]));
        assert!(!actionable(&[LegalKind::March]));
        assert!(!actionable(&[LegalKind::Activate { ability: 0 }]));
        assert!(!actionable(&[]));
    }

    #[test]
    fn a_dimmed_face_keeps_its_alpha_and_comes_back_exactly() {
        let art = Color::srgba(1.0, 1.0, 1.0, 0.7);
        let dim = dimmed(art).to_srgba();
        assert!((dim.red - GREY_LEVEL).abs() < 1e-6);
        assert!((dim.green - GREY_LEVEL).abs() < 1e-6);
        assert!((dim.blue - GREY_LEVEL).abs() < 1e-6);
        assert!(
            (dim.alpha - 0.7).abs() < 1e-6,
            "the foil art keeps its blend"
        );
        let level = dimmed(Color::WHITE).to_srgba().red;
        assert!(level > 0.2 && level < 0.7, "visibly dimmed, still readable");
        let tinted = Color::srgb_u8(200, 40, 90);
        let back = restored(tinted, dimmed(tinted));
        assert_eq!(back, tinted);
        let refaded = restored(art, Color::srgba(0.1, 0.1, 0.1, 0.4)).to_srgba();
        assert!((refaded.red - 1.0).abs() < 1e-6);
        assert!(
            (refaded.alpha - 0.4).abs() < 1e-6,
            "a tuning change to the alpha survives"
        );
    }

    #[test]
    fn a_hideable_card_tints_the_battlefields_it_may_be_hidden_at() {
        let bf1 = 9u16;
        let bf2 = 10u16;
        let view = view_with(vec![
            hide_row(7, vec![bf1, bf2]),
            legal_row(50, vec![LegalKind::March], vec![bf1]),
        ]);
        let out = rims(&view, 0, &hand(&[7]), &seats(&[(7, 0), (50, 0)]));
        assert_eq!(
            tint_layers(&out, &[7]),
            vec![
                (RimKind::Play, vec![agni_riftbound::ZONE_BASE]),
                (RimKind::Hide, vec![bf1, bf2])
            ],
            "picking up a playable-and-hideable card lights the base in green and the battlefields in the hide colour"
        );
        let anchors = vec![
            zones::ZoneAnchor {
                zone: bf1,
                seat: None,
                position: Vec3::ZERO,
                yaw: 0.0,
                size: Vec2::ONE,
            },
            zones::ZoneAnchor {
                zone: 30,
                seat: Some(PlayerId(0)),
                position: Vec3::ZERO,
                yaw: 0.0,
                size: Vec2::ONE,
            },
            zones::ZoneAnchor {
                zone: 30,
                seat: Some(PlayerId(1)),
                position: Vec3::ZERO,
                yaw: 0.0,
                size: Vec2::ONE,
            },
        ];
        assert_eq!(
            dimmed_zones(&anchors, &[bf1], 0),
            [1, 2],
            "everything that is not lit dims while the card is held"
        );
        assert_eq!(
            tint_layers(&out, &[50]),
            vec![(RimKind::March, vec![bf1])],
            "a marching unit lights only its march destinations"
        );
        assert!(
            tint_layers(&out, &[8]).is_empty(),
            "a card with neither offer tints nothing"
        );
        let only_hide = view_with(vec![Legal {
            card: 7,
            kinds: vec![LegalKind::Hide],
            zones: Vec::new(),
            hidden: vec![bf2],
        }]);
        let out = rims(&only_hide, 0, &hand(&[7]), &seats(&[(7, 0)]));
        assert_eq!(
            tint_layers(&out, &[7]),
            vec![(RimKind::Hide, vec![bf2])],
            "the hide rim's own violet is what tints the battlefield"
        );
    }
}
