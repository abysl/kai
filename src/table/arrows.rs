use super::*;
use agni_sim::wire::{Arrow, ArrowKind, ChainRow, Origin, TargetRef};

pub const BOW: f32 = 0.22;
pub const CURVE_STEPS: usize = 24;
pub const SHAFT_WIDTH: f32 = 3.0;
pub const HEAD_LEN: f32 = 16.0;
pub const HEAD_HALF_W: f32 = 7.0;
pub const FADE_SECS: f32 = 0.16;
pub const HEAD_SPREAD: f32 = 19.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub from: Origin,
    pub to: TargetRef,
}

pub fn item_card(chain: &[ChainRow], item: u16) -> Option<u32> {
    chain
        .iter()
        .find(|row| row.item == item)
        .and_then(|row| row.card)
}

pub fn item_seat(chain: &[ChainRow], item: u16) -> Option<u8> {
    chain
        .iter()
        .find(|row| row.item == item)
        .map(|row| row.seat)
}

pub fn stable_origin(from: Origin, chain: &[ChainRow]) -> Origin {
    match from {
        Origin::Item(item) => item_card(chain, item).map_or(from, Origin::Card),
        card => card,
    }
}

pub fn stable_target(to: TargetRef, chain: &[ChainRow]) -> TargetRef {
    match to {
        TargetRef::Item(item) => item_card(chain, item).map_or(to, TargetRef::Card),
        other => other,
    }
}

pub fn key(arrow: &Arrow, chain: &[ChainRow]) -> Key {
    Key {
        from: stable_origin(arrow.from, chain),
        to: stable_target(arrow.to, chain),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    pub key: Key,
    pub kind: ArrowKind,
    pub from: Origin,
    pub to: TargetRef,
    pub alpha: f32,
    pub live: bool,
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ArrowFades(pub Vec<Fade>);

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Anchors {
    pub cards: Vec<(u32, Vec2)>,
    pub seats: Vec<(u8, Vec2)>,
    pub zones: Vec<(u16, Option<u8>, Vec2)>,
    pub items: Vec<(u16, Vec2)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub key: Key,
    pub kind: ArrowKind,
    pub alpha: f32,
    pub shaft: Vec<Vec2>,
    pub head: [Vec2; 3],
}

impl Anchors {
    pub fn card(&self, card: u32) -> Option<Vec2> {
        self.cards
            .iter()
            .find(|(id, _)| *id == card)
            .map(|(_, at)| *at)
    }

    pub fn seat(&self, seat: u8) -> Option<Vec2> {
        self.seats
            .iter()
            .find(|(id, _)| *id == seat)
            .map(|(_, at)| *at)
    }

    pub fn item(&self, item: u16) -> Option<Vec2> {
        self.items
            .iter()
            .find(|(id, _)| *id == item)
            .map(|(_, at)| *at)
    }

    pub fn zone(&self, zone: u16, prefer: Option<u8>) -> Option<Vec2> {
        let mine = self
            .zones
            .iter()
            .find(|(id, seat, _)| *id == zone && *seat == prefer && prefer.is_some());
        let shared = self
            .zones
            .iter()
            .find(|(id, seat, _)| *id == zone && seat.is_none());
        let any = self.zones.iter().find(|(id, _, _)| *id == zone);
        mine.or(shared).or(any).map(|(_, _, at)| *at)
    }
}

pub fn owner_of(
    origin: Origin,
    owner: &dyn Fn(u32) -> Option<u8>,
    chain: &[ChainRow],
) -> Option<u8> {
    match origin {
        Origin::Card(card) => owner(card),
        Origin::Item(item) => item_seat(chain, item),
    }
}

pub fn from_anchor(origin: Origin, anchors: &Anchors) -> Option<Vec2> {
    match origin {
        Origin::Card(card) => anchors.card(card),
        Origin::Item(item) => anchors.item(item),
    }
}

pub fn to_anchor(target: TargetRef, prefer: Option<u8>, anchors: &Anchors) -> Option<Vec2> {
    match target {
        TargetRef::Card(card) => anchors.card(card),
        TargetRef::Seat(seat) => anchors.seat(seat),
        TargetRef::Zone(zone) => anchors.zone(zone, prefer),
        TargetRef::Item(item) => anchors.item(item),
    }
}

pub fn control(from: Vec2, to: Vec2, bow: f32) -> Vec2 {
    let mid = (from + to) * 0.5;
    let span = to - from;
    let len = span.length();
    if len <= f32::EPSILON {
        return mid + Vec2::new(0.0, -bow);
    }
    let perp = Vec2::new(span.y, -span.x) / len;
    let perp = if perp.y > 0.0 { -perp } else { perp };
    mid + perp * (bow * len)
}

pub fn point_on(from: Vec2, control: Vec2, to: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    from * (u * u) + control * (2.0 * u * t) + to * (t * t)
}

pub fn tangent(from: Vec2, control: Vec2, to: Vec2, t: f32) -> Vec2 {
    (control - from) * (2.0 * (1.0 - t)) + (to - control) * (2.0 * t)
}

pub fn samples(from: Vec2, control: Vec2, to: Vec2, steps: usize) -> Vec<Vec2> {
    let steps = steps.max(1);
    (0..=steps)
        .map(|step| point_on(from, control, to, step as f32 / steps as f32))
        .collect()
}

pub fn arrowhead(tip: Vec2, aim: Vec2, len: f32, half_width: f32) -> [Vec2; 3] {
    let dir = if aim.length() <= f32::EPSILON {
        Vec2::new(0.0, -1.0)
    } else {
        aim.normalize()
    };
    let side = Vec2::new(-dir.y, dir.x);
    let base = tip - dir * len;
    [tip, base + side * half_width, base - side * half_width]
}

pub fn item_anchors(
    chain: &[ChainRow],
    cards: &[(u32, Vec2)],
    rows: &[(u16, Vec2)],
) -> Vec<(u16, Vec2)> {
    chain
        .iter()
        .filter_map(|row| {
            let on_card = row
                .card
                .and_then(|card| cards.iter().find(|(id, _)| *id == card).map(|(_, at)| *at));
            let on_panel = rows
                .iter()
                .find(|(item, _)| *item == row.item)
                .map(|(_, at)| *at);
            let at = on_panel.or(on_card)?;
            Some((row.item, at))
        })
        .collect()
}

pub fn advance(fades: &mut Vec<Fade>, arrows: &[Arrow], chain: &[ChainRow], dt: f32) {
    for fade in fades.iter_mut() {
        fade.live = false;
    }
    for arrow in arrows {
        let wanted = key(arrow, chain);
        match fades.iter_mut().find(|fade| fade.key == wanted) {
            Some(fade) => {
                fade.kind = arrow.kind;
                fade.from = arrow.from;
                fade.to = arrow.to;
                fade.live = true;
            }
            None => fades.push(Fade {
                key: wanted,
                kind: arrow.kind,
                from: arrow.from,
                to: arrow.to,
                alpha: 0.0,
                live: true,
            }),
        }
    }
    let step = if FADE_SECS <= 0.0 {
        1.0
    } else {
        dt / FADE_SECS
    };
    for fade in fades.iter_mut() {
        fade.alpha = if fade.live {
            (fade.alpha + step).min(1.0)
        } else {
            (fade.alpha - step).max(0.0)
        };
    }
    fades.retain(|fade| fade.live || fade.alpha > 0.0);
}

pub fn spread(index: usize, total: usize, span: Vec2) -> Vec2 {
    if total <= 1 {
        return Vec2::ZERO;
    }
    let len = span.length();
    let perp = if len <= f32::EPSILON {
        Vec2::new(1.0, 0.0)
    } else {
        Vec2::new(-span.y, span.x) / len
    };
    let step = index as f32 - (total as f32 - 1.0) / 2.0;
    perp * (step * HEAD_SPREAD)
}

pub fn plan(
    fades: &[Fade],
    anchors: &Anchors,
    owner: &dyn Fn(u32) -> Option<u8>,
    chain: &[ChainRow],
) -> Vec<Plan> {
    let mut ends: Vec<(&Fade, Vec2, Vec2)> = Vec::new();
    for fade in fades {
        if fade.alpha <= 0.0 {
            continue;
        }
        let Some(start) = from_anchor(fade.from, anchors) else {
            continue;
        };
        let prefer = owner_of(fade.from, owner, chain);
        let Some(end) = to_anchor(fade.to, prefer, anchors) else {
            continue;
        };
        ends.push((fade, start, end));
    }
    let sharing: Vec<usize> = ends
        .iter()
        .map(|(fade, _, _)| {
            ends.iter()
                .filter(|(held, _, _)| held.to == fade.to)
                .count()
        })
        .collect();
    let mut seen: Vec<(TargetRef, usize)> = Vec::new();
    let mut plans = Vec::new();
    for ((fade, start, end), total) in ends.iter().zip(sharing) {
        let slot = match seen.iter_mut().find(|(target, _)| *target == fade.to) {
            Some((_, taken)) => {
                *taken += 1;
                *taken
            }
            None => {
                seen.push((fade.to, 0));
                0
            }
        };
        let end = *end + spread(slot, total, *end - *start);
        let bend = control(*start, end, BOW);
        let shaft = samples(*start, bend, end, CURVE_STEPS);
        let head = arrowhead(end, tangent(*start, bend, end, 1.0), HEAD_LEN, HEAD_HALF_W);
        plans.push(Plan {
            key: fade.key,
            kind: fade.kind,
            alpha: fade.alpha,
            shaft,
            head,
        });
    }
    plans
}

pub fn provisional(card: u32, kind: Option<highlight::RimKind>, zone: u16) -> Fade {
    let kind = match kind {
        Some(highlight::RimKind::March) => ArrowKind::Attack,
        _ => ArrowKind::Spell,
    };
    let from = Origin::Card(card);
    let to = TargetRef::Zone(zone);
    Fade {
        key: Key { from, to },
        kind,
        from,
        to,
        alpha: 1.0,
        live: true,
    }
}

pub fn tint(kind: ArrowKind) -> [u8; 3] {
    match kind {
        ArrowKind::Spell => [0x6C, 0xB6, 0xFF],
        ArrowKind::Ability => [0xFF, 0xC1, 0x4D],
        ArrowKind::Attack => [0xFF, 0x7A, 0x3C],
        ArrowKind::Counter => [0xB1, 0x8C, 0xFF],
        ArrowKind::Combat => [0xFF, 0x4D, 0x5E],
    }
}

pub fn opacity(alpha: f32) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn color(kind: ArrowKind, alpha: f32) -> egui::Color32 {
    let [r, g, b] = tint(kind);
    egui::Color32::from_rgba_unmultiplied(r, g, b, opacity(alpha))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_arrows(
    mut contexts: EguiContexts,
    time: Res<Time>,
    panel: Res<plugin_ui::PluginPanel>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    held: Res<Held>,
    rims: Res<highlight::Rims>,
    mut fades: ResMut<ArrowFades>,
    chain_rects: Res<chain::ChainRects>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(&CardView, &GlobalTransform, &ViewVisibility)>,
    slots: Query<(&CardView, &Slot)>,
) -> Result {
    let arrows = panel.view.arrows.clone();
    let chain = panel.view.chain.clone();
    let dragging = held.card.zip(held.target);
    if arrows.is_empty() && fades.0.is_empty() && dragging.is_none() {
        return Ok(());
    }
    advance(&mut fades.0, &arrows, &chain, time.delta_secs());
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let project = |at: Vec3| {
        camera
            .world_to_viewport(camera_transform, at)
            .ok()
            .map(|screen| Vec2::new(screen.x, screen.y))
    };
    let mut anchors = Anchors::default();
    for (view, transform, visibility) in &cards {
        if !visibility.get() {
            continue;
        }
        if let Some(at) = project(transform.translation()) {
            anchors.cards.push((view.0 .0, at));
        }
    }
    for seat in 0..players.0 as u8 {
        if let Some(at) = project(seat_center(PlayerId(seat), players.0)) {
            anchors.seats.push((seat, at));
        }
    }
    let world_zones = zones::anchors(
        &mirror.view.zones,
        players.0,
        info.battlefields_in_play(players.0),
    );
    for anchor in &world_zones {
        if let Some(at) = project(anchor.position) {
            anchors
                .zones
                .push((anchor.zone, anchor.seat.map(|seat| seat.0), at));
        }
    }
    anchors.items = item_anchors(&chain, &anchors.cards, &chain_rects.rows);
    let mut shown: Vec<Fade> = fades.0.clone();
    if let Some((entity, target)) = dragging {
        if let Ok((view, slot)) = slots.get(entity) {
            let id = view.0 .0;
            let lit = interaction::lit_zones(&rims, id);
            let snapped = interaction::snap_zone(
                target,
                &world_zones,
                &lit,
                my_seat.0,
                interaction::DROP_REACH,
            );
            if let (Some(anchor), Some(origin)) = (snapped, project(slot.position)) {
                anchors.cards.retain(|(card, _)| *card != id);
                anchors.cards.push((id, origin));
                shown.push(provisional(id, rims.kind(id), anchor.zone));
            }
        }
    }
    let owner = |card: u32| table.0.get(CardId(card)).map(|held| held.owner.0);
    let painter = context.layer_painter(egui::LayerId::background());
    for drawn in plan(&shown, &anchors, &owner, &chain) {
        let stroke = egui::Stroke::new(SHAFT_WIDTH, color(drawn.kind, drawn.alpha));
        painter.add(egui::Shape::line(
            drawn
                .shaft
                .iter()
                .map(|at| egui::pos2(at.x, at.y))
                .collect(),
            stroke,
        ));
        painter.add(egui::Shape::convex_polygon(
            drawn.head.iter().map(|at| egui::pos2(at.x, at.y)).collect(),
            color(drawn.kind, drawn.alpha),
            egui::Stroke::NONE,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrow(from: Origin, to: TargetRef, kind: ArrowKind) -> Arrow {
        Arrow { from, to, kind }
    }

    fn row(item: u16, card: Option<u32>, seat: u8) -> ChainRow {
        ChainRow { item, card, seat }
    }

    const NO_CHAIN: &[ChainRow] = &[];

    fn anchors() -> Anchors {
        Anchors {
            cards: vec![
                (71, Vec2::new(100.0, 400.0)),
                (81, Vec2::new(500.0, 400.0)),
                (60, Vec2::new(620.0, 260.0)),
            ],
            seats: vec![(0, Vec2::new(400.0, 700.0)), (1, Vec2::new(400.0, 60.0))],
            zones: vec![
                (9, None, Vec2::new(300.0, 300.0)),
                (12, Some(0), Vec2::new(200.0, 640.0)),
                (12, Some(1), Vec2::new(200.0, 120.0)),
            ],
            items: vec![(4, Vec2::new(900.0, 140.0))],
        }
    }

    #[test]
    fn a_held_card_plans_one_arrow_from_its_origin_slot_to_the_zone_under_the_pointer() {
        let march = provisional(71, Some(highlight::RimKind::March), 9);
        assert_eq!(march.kind, ArrowKind::Attack);
        assert_eq!(march.from, Origin::Card(71));
        assert_eq!(march.to, TargetRef::Zone(9));
        assert_eq!(march.alpha, 1.0);
        let play = provisional(71, Some(highlight::RimKind::Play), 12);
        assert_eq!(play.kind, ArrowKind::Spell);
        assert_eq!(provisional(71, None, 12).kind, ArrowKind::Spell);
        let owner = |card: u32| (card == 71).then_some(0u8);
        let drawn = plan(&[march], &anchors(), &owner, NO_CHAIN);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].shaft[0], Vec2::new(100.0, 400.0));
        let tip = drawn[0].head[0];
        assert!((tip - Vec2::new(300.0, 300.0)).length() < 1.0, "{tip:?}");
    }

    #[test]
    fn the_control_point_sits_off_the_midline_and_bows_upward() {
        let from = Vec2::new(0.0, 100.0);
        let to = Vec2::new(200.0, 100.0);
        let bend = control(from, to, BOW);
        assert_eq!(bend.x, 100.0);
        assert!(bend.y < 100.0, "{bend:?}");
        assert!((bend.y - (100.0 - BOW * 200.0)).abs() < 1e-4);
    }

    #[test]
    fn a_vertical_arrow_still_gets_a_sideways_bow_and_a_zero_length_one_does_not_divide_by_zero() {
        let bend = control(Vec2::new(50.0, 0.0), Vec2::new(50.0, 400.0), BOW);
        assert!((bend.x - (50.0 + BOW * 400.0)).abs() < 1e-4, "{bend:?}");
        assert_eq!(bend.y, 200.0);
        let same = Vec2::new(7.0, 9.0);
        let degenerate = control(same, same, BOW);
        assert!(degenerate.is_finite());
        assert_eq!(degenerate, Vec2::new(7.0, 9.0 - BOW));
    }

    #[test]
    fn the_curve_starts_at_the_source_ends_at_the_target_and_its_tangent_aims_at_the_target() {
        let from = Vec2::new(10.0, 500.0);
        let to = Vec2::new(410.0, 200.0);
        let bend = control(from, to, BOW);
        assert_eq!(point_on(from, bend, to, 0.0), from);
        assert_eq!(point_on(from, bend, to, 1.0), to);
        let mid = point_on(from, bend, to, 0.5);
        assert!((mid - (from + to) * 0.5).length() > 1.0, "{mid:?}");
        assert_eq!(tangent(from, bend, to, 0.0), (bend - from) * 2.0);
        assert_eq!(tangent(from, bend, to, 1.0), (to - bend) * 2.0);
        let line = samples(from, bend, to, CURVE_STEPS);
        assert_eq!(line.len(), CURVE_STEPS + 1);
        assert_eq!(line[0], from);
        assert_eq!(line[CURVE_STEPS], to);
        assert_eq!(samples(from, bend, to, 0).len(), 2);
    }

    #[test]
    fn the_arrowhead_is_a_triangle_tipped_at_the_target_and_squared_to_the_curve() {
        let head = arrowhead(
            Vec2::new(300.0, 100.0),
            Vec2::new(4.0, 0.0),
            HEAD_LEN,
            HEAD_HALF_W,
        );
        assert_eq!(head[0], Vec2::new(300.0, 100.0));
        let base = (head[1] + head[2]) * 0.5;
        assert!((base - Vec2::new(300.0 - HEAD_LEN, 100.0)).length() < 1e-4);
        assert!(((head[1] - head[2]).length() - HEAD_HALF_W * 2.0).abs() < 1e-4);
        let flat = arrowhead(Vec2::ZERO, Vec2::ZERO, HEAD_LEN, HEAD_HALF_W);
        assert!(flat.iter().all(|point| point.is_finite()));
    }

    #[test]
    fn every_origin_and_target_case_resolves_to_a_screen_point_or_to_nothing() {
        let anchors = anchors();
        assert_eq!(
            from_anchor(Origin::Card(71), &anchors),
            Some(Vec2::new(100.0, 400.0))
        );
        assert_eq!(
            from_anchor(Origin::Item(4), &anchors),
            Some(Vec2::new(900.0, 140.0))
        );
        assert_eq!(from_anchor(Origin::Card(999), &anchors), None);
        assert_eq!(
            to_anchor(TargetRef::Card(81), None, &anchors),
            Some(Vec2::new(500.0, 400.0))
        );
        assert_eq!(
            to_anchor(TargetRef::Seat(1), None, &anchors),
            Some(Vec2::new(400.0, 60.0))
        );
        assert_eq!(
            to_anchor(TargetRef::Zone(9), Some(1), &anchors),
            Some(Vec2::new(300.0, 300.0))
        );
        assert_eq!(
            to_anchor(TargetRef::Zone(12), Some(1), &anchors),
            Some(Vec2::new(200.0, 120.0))
        );
        assert_eq!(
            to_anchor(TargetRef::Zone(12), None, &anchors),
            Some(Vec2::new(200.0, 640.0))
        );
        assert_eq!(
            to_anchor(TargetRef::Item(4), None, &anchors),
            Some(Vec2::new(900.0, 140.0))
        );
        assert_eq!(to_anchor(TargetRef::Seat(7), None, &anchors), None);
        assert_eq!(to_anchor(TargetRef::Zone(3), None, &anchors), None);
    }

    #[test]
    fn a_chain_item_anchors_on_its_panel_row_and_falls_back_to_a_card_on_the_felt() {
        let cards = anchors().cards;
        let chain = vec![
            row(4, Some(71), 0),
            row(5, Some(404), 1),
            row(6, Some(81), 0),
        ];
        let rows = vec![(6, Vec2::new(1100.0, 80.0)), (5, Vec2::new(1100.0, 160.0))];
        let items = item_anchors(&chain, &cards, &rows);
        assert_eq!(
            items[0],
            (4, Vec2::new(100.0, 400.0)),
            "an item the panel has no row for anchors on its card"
        );
        assert_eq!(
            items[2],
            (6, Vec2::new(1100.0, 80.0)),
            "the panel row wins over the card so the arrow leaves the chain, not a deal origin"
        );
        assert_eq!(items[1], (5, Vec2::new(1100.0, 160.0)));
        assert_eq!(items.len(), 3);
        let rows = vec![
            (6, Vec2::new(1100.0, 80.0)),
            (5, Vec2::new(1100.0, 160.0)),
            (4, Vec2::new(1100.0, 240.0)),
        ];
        let hidden = vec![row(4, None, 0)];
        assert_eq!(
            item_anchors(&hidden, &cards, &rows),
            [(4, Vec2::new(1100.0, 240.0))]
        );
        assert!(
            item_anchors(&hidden, &cards, &[]).is_empty(),
            "no panel row and no card is no anchor, not a guessed one"
        );
    }

    #[test]
    fn an_arrow_keeps_one_key_when_its_source_leaves_the_table_mid_resolution() {
        let chain = vec![row(4, Some(71), 1)];
        let on_board = arrow(Origin::Card(71), TargetRef::Card(81), ArrowKind::Ability);
        let orphaned = arrow(Origin::Item(4), TargetRef::Card(81), ArrowKind::Ability);
        assert_eq!(key(&on_board, &chain), key(&orphaned, &chain));
        let mut fades: Vec<Fade> = Vec::new();
        advance(&mut fades, &[on_board], &chain, FADE_SECS);
        assert_eq!(fades[0].alpha, 1.0);
        advance(&mut fades, &[orphaned], &chain, 0.0);
        assert_eq!(
            fades.len(),
            1,
            "the arrow does not blink into a second fade"
        );
        assert_eq!(fades[0].alpha, 1.0);
        assert_eq!(fades[0].from, Origin::Item(4));
        let counter = arrow(Origin::Card(52), TargetRef::Item(4), ArrowKind::Counter);
        assert_eq!(key(&counter, &chain).to, TargetRef::Card(71));
        assert_eq!(
            key(&counter, NO_CHAIN).to,
            TargetRef::Item(4),
            "with no chain row the raw item stands as its own identity"
        );
        assert_eq!(owner_of(Origin::Item(4), &|_| None, &chain), Some(1));
        assert_eq!(owner_of(Origin::Item(9), &|_| None, &chain), None);
        assert_eq!(owner_of(Origin::Card(71), &|_| Some(0), &chain), Some(0));
    }

    #[test]
    fn arrowheads_that_share_a_target_fan_out_instead_of_stacking() {
        let anchors = Anchors {
            cards: vec![
                (1, Vec2::new(100.0, 600.0)),
                (2, Vec2::new(200.0, 600.0)),
                (3, Vec2::new(300.0, 600.0)),
                (9, Vec2::new(200.0, 200.0)),
            ],
            ..Default::default()
        };
        let live: Vec<Arrow> = [1u32, 2, 3]
            .into_iter()
            .map(|id| arrow(Origin::Card(id), TargetRef::Card(9), ArrowKind::Combat))
            .collect();
        let mut fades: Vec<Fade> = Vec::new();
        advance(&mut fades, &live, NO_CHAIN, FADE_SECS);
        let plans = plan(&fades, &anchors, &|_| None, NO_CHAIN);
        assert_eq!(plans.len(), 3);
        let tips: Vec<Vec2> = plans.iter().map(|drawn| drawn.head[0]).collect();
        for (index, tip) in tips.iter().enumerate() {
            for other in tips.iter().skip(index + 1) {
                assert!(
                    (*tip - *other).length() > HEAD_SPREAD - 1.0,
                    "{tip:?} and {other:?} land on the same pixel"
                );
            }
        }
        assert_eq!(spread(0, 1, Vec2::new(0.0, 10.0)), Vec2::ZERO);
        assert!(spread(0, 3, Vec2::ZERO).is_finite());
        let lone = plan(
            &[Fade {
                key: Key {
                    from: Origin::Card(1),
                    to: TargetRef::Card(9),
                },
                kind: ArrowKind::Combat,
                from: Origin::Card(1),
                to: TargetRef::Card(9),
                alpha: 1.0,
                live: true,
            }],
            &anchors,
            &|_| None,
            NO_CHAIN,
        );
        assert_eq!(
            lone[0].head[0],
            Vec2::new(200.0, 200.0),
            "a single arrow still lands on the card's centre"
        );
    }

    #[test]
    fn an_arrow_fades_in_while_it_is_offered_and_out_once_it_is_gone() {
        let mut fades: Vec<Fade> = Vec::new();
        let live = vec![arrow(
            Origin::Card(71),
            TargetRef::Card(81),
            ArrowKind::Spell,
        )];
        advance(&mut fades, &live, NO_CHAIN, FADE_SECS / 4.0);
        assert_eq!(fades.len(), 1);
        assert!((fades[0].alpha - 0.25).abs() < 1e-4);
        advance(&mut fades, &live, NO_CHAIN, FADE_SECS);
        assert_eq!(fades[0].alpha, 1.0);
        advance(&mut fades, &live, NO_CHAIN, FADE_SECS);
        assert_eq!(fades[0].alpha, 1.0);
        advance(&mut fades, &[], NO_CHAIN, FADE_SECS / 2.0);
        assert_eq!(fades.len(), 1);
        assert!((fades[0].alpha - 0.5).abs() < 1e-4);
        advance(&mut fades, &[], NO_CHAIN, FADE_SECS);
        assert!(fades.is_empty());
    }

    #[test]
    fn a_refolded_arrow_keeps_its_key_and_its_alpha_instead_of_blinking() {
        let mut fades: Vec<Fade> = Vec::new();
        let spell = arrow(Origin::Card(71), TargetRef::Card(81), ArrowKind::Spell);
        advance(&mut fades, &[spell], NO_CHAIN, FADE_SECS);
        let combat = arrow(Origin::Card(50), TargetRef::Card(60), ArrowKind::Combat);
        advance(&mut fades, &[spell, combat], NO_CHAIN, 0.0);
        assert_eq!(fades.len(), 2);
        assert_eq!(fades[0].alpha, 1.0);
        assert_eq!(fades[0].key, key(&spell, NO_CHAIN));
        assert_eq!(fades[1].alpha, 0.0);
        let recoloured = arrow(Origin::Card(71), TargetRef::Card(81), ArrowKind::Counter);
        advance(&mut fades, &[recoloured], NO_CHAIN, 0.0);
        assert_eq!(fades[0].alpha, 1.0);
        assert_eq!(fades[0].kind, ArrowKind::Counter);
    }

    #[test]
    fn a_plan_is_drawn_per_resolvable_arrow_and_skipped_where_an_endpoint_is_offscreen() {
        let anchors = anchors();
        let mut fades: Vec<Fade> = Vec::new();
        let live = vec![
            arrow(Origin::Card(71), TargetRef::Card(81), ArrowKind::Spell),
            arrow(Origin::Item(4), TargetRef::Seat(1), ArrowKind::Ability),
            arrow(Origin::Card(999), TargetRef::Card(81), ArrowKind::Spell),
            arrow(Origin::Card(71), TargetRef::Card(404), ArrowKind::Spell),
        ];
        advance(&mut fades, &live, NO_CHAIN, FADE_SECS);
        let owner = |_: u32| None;
        let plans = plan(&fades, &anchors, &owner, NO_CHAIN);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].key.from, Origin::Card(71));
        assert_eq!(plans[0].kind, ArrowKind::Spell);
        assert_eq!(plans[0].alpha, 1.0);
        assert_eq!(
            plans[0].shaft.first().copied(),
            Some(Vec2::new(100.0, 400.0))
        );
        assert_eq!(
            plans[0].shaft.last().copied(),
            Some(Vec2::new(500.0, 400.0))
        );
        assert_eq!(plans[0].head[0], Vec2::new(500.0, 400.0));
        assert_eq!(plans[1].key.from, Origin::Item(4));
        assert_eq!(plans[1].head[0], Vec2::new(400.0, 60.0));
    }

    #[test]
    fn a_zone_arrow_lands_on_the_sources_own_copy_of_a_per_seat_zone() {
        let anchors = anchors();
        let mut fades: Vec<Fade> = Vec::new();
        advance(
            &mut fades,
            &[arrow(
                Origin::Card(71),
                TargetRef::Zone(12),
                ArrowKind::Spell,
            )],
            NO_CHAIN,
            FADE_SECS,
        );
        let mine = plan(&fades, &anchors, &|_| Some(1), NO_CHAIN);
        assert_eq!(mine[0].head[0], Vec2::new(200.0, 120.0));
        let theirs = plan(&fades, &anchors, &|_| Some(0), NO_CHAIN);
        assert_eq!(theirs[0].head[0], Vec2::new(200.0, 640.0));
    }

    #[test]
    fn a_half_faded_arrow_is_still_planned_and_a_dead_one_is_not() {
        let anchors = anchors();
        let fades = vec![
            Fade {
                key: Key {
                    from: Origin::Card(71),
                    to: TargetRef::Card(81),
                },
                kind: ArrowKind::Spell,
                from: Origin::Card(71),
                to: TargetRef::Card(81),
                alpha: 0.4,
                live: false,
            },
            Fade {
                key: Key {
                    from: Origin::Card(71),
                    to: TargetRef::Card(60),
                },
                kind: ArrowKind::Attack,
                from: Origin::Card(71),
                to: TargetRef::Card(60),
                alpha: 0.0,
                live: true,
            },
        ];
        let plans = plan(&fades, &anchors, &|_| None, NO_CHAIN);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].alpha, 0.4);
    }

    #[test]
    fn every_arrow_kind_paints_its_own_colour() {
        let kinds = [
            ArrowKind::Spell,
            ArrowKind::Ability,
            ArrowKind::Attack,
            ArrowKind::Counter,
            ArrowKind::Combat,
        ];
        let mut seen: Vec<[u8; 3]> = Vec::new();
        for kind in kinds {
            let shade = tint(kind);
            assert!(!seen.contains(&shade), "{kind:?} repeats a colour");
            seen.push(shade);
        }
        assert_eq!(opacity(0.0), 0);
        assert_eq!(opacity(1.0), 255);
        assert_eq!(opacity(2.0), 255);
        assert_eq!(opacity(-1.0), 0);
        let faded = color(ArrowKind::Spell, 0.5);
        assert_eq!(faded.a(), 128);
    }

    #[test]
    fn the_arrows_a_pending_spell_with_two_targets_makes_are_two_separate_keys() {
        let arrows = vec![
            arrow(Origin::Card(71), TargetRef::Card(81), ArrowKind::Spell),
            arrow(Origin::Card(71), TargetRef::Card(60), ArrowKind::Spell),
        ];
        let mut fades: Vec<Fade> = Vec::new();
        advance(&mut fades, &arrows, NO_CHAIN, FADE_SECS);
        assert_eq!(fades.len(), 2);
        assert_ne!(key(&arrows[0], NO_CHAIN), key(&arrows[1], NO_CHAIN));
        let plans = plan(&fades, &anchors(), &|_| None, NO_CHAIN);
        assert_eq!(plans.len(), 2);
        assert_ne!(plans[0].head[0], plans[1].head[0]);
    }
}
