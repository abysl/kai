use super::gesture::{Gesture, Tracker};
use super::*;
use crate::viewport::{back_pressed, consume_back, BackKey, InputKind};
use bevy::window::PrimaryWindow;

pub const HOVER_HYSTERESIS: f32 = 14.0;

pub fn hover_switches(cursor: Vec2, current: Vec2, candidate: Vec2, hysteresis: f32) -> bool {
    cursor.distance(candidate) + hysteresis < cursor.distance(current)
}

pub(super) fn on_hover_card(
    event: On<Pointer<Over>>,
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    slots: Query<&Slot>,
    hovered: Query<Entity, With<Hovered>>,
) {
    let target = event.event_target();
    let resting = |entity: Entity| -> Option<Vec2> {
        let slot = slots.get(entity).ok()?;
        let window = windows.single().ok()?;
        let (camera, camera_transform) = camera.single().ok()?;
        let _ = window;
        camera
            .world_to_viewport(camera_transform, slot.position)
            .ok()
    };
    let cursor = windows
        .single()
        .ok()
        .and_then(|window| window.cursor_position());
    for other in &hovered {
        if other == target {
            continue;
        }
        let same_layer = match (slots.get(other), slots.get(target)) {
            (Ok(a), Ok(b)) => a.facing == b.facing,
            _ => false,
        };
        if same_layer {
            if let (Some(cursor), Some(current), Some(candidate)) =
                (cursor, resting(other), resting(target))
            {
                if !hover_switches(cursor, current, candidate, HOVER_HYSTERESIS) {
                    return;
                }
            }
        }
        commands.entity(other).remove::<Hovered>();
    }
    commands.entity(target).insert(Hovered);
}

pub(super) fn on_unhover_card(
    event: On<Pointer<Out>>,
    mut commands: Commands,
    slots: Query<&Slot>,
    mut contexts: EguiContexts,
) {
    let in_hand = slots
        .get(event.event_target())
        .is_ok_and(|slot| slot.facing == Facing::Camera);
    if in_hand {
        return;
    }
    let egui_owns_the_pointer = contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_pointer_input())
        .unwrap_or(false);
    if egui_owns_the_pointer {
        return;
    }
    commands.entity(event.event_target()).remove::<Hovered>();
}

pub const HAND_HOVER_SLACK: f32 = 8.0;

pub fn hand_reach(corners: &[Vec2], window_height: f32, slack: f32) -> Option<Rect> {
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for corner in corners {
        min = min.min(*corner);
        max = max.max(*corner);
    }
    if corners.is_empty() || min.x > max.x {
        return None;
    }
    Some(Rect::new(
        min.x - slack,
        min.y - slack,
        max.x + slack,
        window_height.max(max.y + slack),
    ))
}

pub(super) fn settle_hand_hover(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    held: Res<Held>,
    input: Res<InputKind>,
    hovered: Query<(Entity, &Slot, &Transform), (With<Hovered>, With<CardView>)>,
    mut contexts: EguiContexts,
) {
    if input.is_touch() {
        return;
    }
    if contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_pointer_input())
        .unwrap_or(false)
    {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera.single() else {
        return;
    };
    for (entity, slot, transform) in &hovered {
        if slot.facing != Facing::Camera || held.card == Some(entity) {
            continue;
        }
        let corners: Vec<Vec2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .into_iter()
            .filter_map(|(sx, sz)| {
                let local = Vec3::new(sx * dim::CARD_W / 2.0, 0.0, sz * dim::CARD_H / 2.0);
                camera
                    .world_to_viewport(camera_transform, slot.position + transform.rotation * local)
                    .ok()
            })
            .collect();
        let inside = match (
            window.cursor_position(),
            hand_reach(&corners, window.height(), HAND_HOVER_SLACK),
        ) {
            (Some(cursor), Some(reach)) => reach.contains(cursor),
            _ => false,
        };
        if !inside {
            commands.entity(entity).remove::<Hovered>();
        }
    }
}

pub(super) fn mirror_touch_selection(
    mut commands: Commands,
    input: Res<InputKind>,
    selected: Res<Selected>,
    held: Res<Held>,
    hovered: Query<Entity, With<Hovered>>,
    cards: Query<(), With<CardView>>,
) {
    if !input.is_touch() || held.card.is_some() {
        return;
    }
    let wanted = selected.0.filter(|entity| cards.contains(*entity));
    for entity in &hovered {
        if Some(entity) != wanted {
            commands.entity(entity).remove::<Hovered>();
        }
    }
    if let Some(entity) = wanted {
        if !hovered.contains(entity) {
            commands.entity(entity).insert(Hovered);
        }
    }
}

pub(super) fn guarded_from_me(
    table: &Table,
    mirror: &Mirror,
    my_seat: PlayerId,
    card: CardId,
) -> bool {
    use agni_sim::wire::{ZoneOwner, ZoneVisibility};
    let Some(card) = table.get(card) else {
        return false;
    };
    let Some(decl) = zones::decl_of(&mirror.view.zones, card.zone) else {
        return false;
    };
    decl.visibility != ZoneVisibility::All
        && decl.owner == ZoneOwner::PerSeat
        && card.seat != my_seat
}

pub fn may_lift(free: bool, rims: &highlight::Rims, card: u32) -> bool {
    free || rims.kind(card).is_some()
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned(pub Option<Entity>);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingDrop {
    pub card: CardId,
    pub to: Zone,
    pub seat: PlayerId,
    pub index: usize,
    pub at: Vec3,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct DropChooser(pub Option<PendingDrop>);

pub const DROP_REACH: f32 = 0.35;

pub(super) fn on_press_card(
    event: On<Pointer<Press>>,
    time: Res<Time>,
    cards: Query<(), With<CardView>>,
    mut tracker: ResMut<Tracker>,
) {
    if event.button != PointerButton::Primary || !cards.contains(event.event_target()) {
        return;
    }
    tracker.press(
        event.event_target(),
        event.pointer_location.position,
        time.elapsed_secs_f64(),
    );
}

pub(super) fn on_release(
    event: On<Pointer<Release>>,
    time: Res<Time>,
    mut tracker: ResMut<Tracker>,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    tracker.settle(event.pointer_location.position, time.elapsed_secs_f64());
}

pub(super) fn on_cancel(
    _event: On<Pointer<Cancel>>,
    mut commands: Commands,
    mut tracker: ResMut<Tracker>,
    mut held: ResMut<Held>,
    mut swipe: ResMut<DrawerSwipe>,
    cards: Query<Entity, With<CardView>>,
) {
    tracker.cancel();
    swipe.0 = None;
    if held.card.is_some() {
        held.card = None;
        held.target = None;
        for entity in &cards {
            commands.entity(entity).insert(Pickable::default());
        }
    }
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct DrawerSwipe(pub Option<Swipe>);

#[derive(bevy::ecs::system::SystemParam)]
pub struct DrawerDrag<'w> {
    pub viewport: Res<'w, crate::viewport::Viewport>,
    pub hud: Res<'w, hud::Hud>,
    pub drawer: Res<'w, layout::HandDrawer>,
    pub scroll: ResMut<'w, layout::DrawerScroll>,
    pub swipe: ResMut<'w, DrawerSwipe>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Covers<'w> {
    pub menu: Res<'w, crate::menu::Menu>,
    pub settings: Res<'w, crate::settings::Settings>,
    pub table_menu: Res<'w, hud::TableMenu>,
    pub help: Res<'w, crate::help::HelpSheet>,
    pub chain_sheet: ResMut<'w, chain::ChainSheet>,
    pub pile_sheet: ResMut<'w, ui::PileSheet>,
}

impl Covers<'_> {
    pub fn covered(&self) -> bool {
        self.settings.open
            || self.menu.sheet.is_some()
            || self.table_menu.open
            || self.help.open
            || self.chain_sheet.open
            || self.pile_sheet.0.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swipe {
    pub card: Entity,
    pub scroll: f32,
    pub pitch: f32,
}

pub fn swipe_starts(class: crate::viewport::ViewportClass, in_drawer: bool, travel: Vec2) -> bool {
    class.is_phone() && in_drawer && travel.x.abs() > travel.y.abs()
}

pub(super) fn sync_hand_pickable(
    mut commands: Commands,
    viewport: Res<crate::viewport::Viewport>,
    drawer: Res<layout::HandDrawer>,
    held: Res<Held>,
    cards: Query<(Entity, &Slot, &Pickable), With<CardView>>,
) {
    if held.card.is_some() {
        return;
    }
    let tucked = viewport.class.is_phone() && drawer.state == hud::DrawerState::Tucked;
    for (entity, slot, pickable) in &cards {
        let wanted = if tucked && slot.facing == Facing::Camera {
            Pickable::IGNORE
        } else {
            Pickable::default()
        };
        if *pickable != wanted {
            commands.entity(entity).insert(wanted);
        }
    }
}

pub(super) fn on_drag_start(
    event: On<Pointer<DragStart>>,
    time: Res<Time>,
    cards: Query<(), With<CardView>>,
    mut tracker: ResMut<Tracker>,
) {
    let target = event.event_target();
    if event.button != PointerButton::Primary || !cards.contains(target) {
        return;
    }
    if tracker.pressed(target).is_none() {
        tracker.press(
            target,
            event.pointer_location.position,
            time.elapsed_secs_f64(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn on_drag(
    event: On<Pointer<Drag>>,
    mut commands: Commands,
    time: Res<Time>,
    input: Res<InputKind>,
    mut phone: DrawerDrag,
    mut tracker: ResMut<Tracker>,
    mut held: ResMut<Held>,
    mut chooser: ResMut<DropChooser>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    tools: Res<plugin_ui::Tools>,
    rims: Res<highlight::Rims>,
    cards: Query<Entity, With<CardView>>,
    views: Query<(&CardView, &Slot)>,
) {
    let target = event.event_target();
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok((view, slot)) = views.get(target) else {
        return;
    };
    let Some(press) = tracker.pressed(target) else {
        return;
    };
    let at = press.at + event.distance;
    let class = phone.viewport.class;
    let band_w = phone.hud.0.hand.width();
    let state = phone.drawer.state;
    let moved = tracker.moved(target, at, time.elapsed_secs_f64(), *input);
    match moved {
        Some(Gesture::DragStart) => {}
        Some(Gesture::Drag) => {
            if let Some(active) = phone.swipe.0.filter(|active| active.card == target) {
                let hand = my_hand_ids(&table, &mirror, my_seat.0).len();
                let visible = layout::hand_visible(class, band_w, active.pitch);
                let next = layout::scroll_clamp(
                    active.scroll - event.distance.x / active.pitch.max(1.0),
                    hand,
                    visible,
                );
                if next != phone.scroll.0 {
                    phone.scroll.0 = next;
                }
            }
            return;
        }
        _ => return,
    }
    let in_drawer = slot.facing == Facing::Camera && state == hud::DrawerState::Raised;
    if swipe_starts(class, in_drawer, event.distance) {
        phone.swipe.0 = Some(Swipe {
            card: target,
            scroll: phone.scroll.0,
            pitch: layout::card_pitch(class, state, band_w),
        });
        return;
    }
    if guarded_from_me(&table, &mirror, my_seat.0, view.0)
        || !may_lift(tools.free, &rims, view.0 .0)
    {
        return;
    }
    held.card = Some(target);
    held.target = None;
    if chooser.0.is_some() {
        chooser.0 = None;
    }
    for entity in &cards {
        commands.entity(entity).insert(Pickable::IGNORE);
    }
}

pub(super) fn on_drag_over_surface(event: On<Pointer<DragOver>>, mut held: ResMut<Held>) {
    if held.card.is_none() {
        return;
    }
    if let Some(position) = event.hit.position {
        held.target = Some(position);
    }
}

pub fn lit_zones(rims: &highlight::Rims, card: u32) -> Vec<u16> {
    let mut out: Vec<u16> = rims
        .destinations(card)
        .iter()
        .chain(rims.hides(card))
        .copied()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

pub fn snap_zone<'a>(
    position: Vec3,
    anchors: &'a [zones::ZoneAnchor],
    lit: &[u16],
    me: PlayerId,
    reach: f32,
) -> Option<&'a zones::ZoneAnchor> {
    let mut best: Option<(f32, &zones::ZoneAnchor)> = None;
    for anchor in anchors {
        if !lit.contains(&anchor.zone) || anchor.seat.is_some_and(|seat| seat != me) {
            continue;
        }
        let local = Quat::from_rotation_y(-anchor.yaw) * (position - anchor.position);
        let over_x = (local.x.abs() - anchor.size.x / 2.0).max(0.0);
        let over_z = (local.z.abs() - anchor.size.y / 2.0).max(0.0);
        if over_x > reach || over_z > reach {
            continue;
        }
        let overflow = over_x.max(over_z);
        if best.is_none_or(|(held, _)| overflow < held) {
            best = Some((overflow, anchor));
        }
    }
    best.map(|(_, anchor)| anchor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPlan {
    Drop { hidden: bool },
    Choose,
    Refuse,
}

pub fn drop_plan(rims: &highlight::Rims, free: bool, card: u32, zone: u16) -> DropPlan {
    let play = rims.destinations(card).contains(&zone);
    let hide = rims.hides(card).contains(&zone);
    match (play, hide) {
        (true, true) => DropPlan::Choose,
        (true, false) => DropPlan::Drop { hidden: false },
        (false, true) => DropPlan::Drop { hidden: true },
        (false, false) if free => DropPlan::Drop { hidden: false },
        (false, false) => DropPlan::Refuse,
    }
}

fn insertion_index(
    slots: &Query<(&CardView, &Slot)>,
    table: &Table,
    card: CardId,
    to: Zone,
    seat: PlayerId,
    axis: Vec3,
    position: Vec3,
) -> usize {
    slots
        .iter()
        .filter(|(v, _)| v.0 != card)
        .filter(|(v, _)| {
            table
                .get(v.0)
                .is_some_and(|c| c.zone == to && c.seat == seat)
        })
        .filter(|(_, s)| s.position.dot(axis) < position.dot(axis))
        .count()
}

#[allow(clippy::too_many_arguments)]
fn settle_drop(
    plan: DropPlan,
    card: CardId,
    to: Zone,
    seat: PlayerId,
    index: usize,
    position: Vec3,
    chooser: &mut DropChooser,
    dropped: &mut MessageWriter<CardDropped>,
) {
    match plan {
        DropPlan::Drop { hidden } => {
            dropped.write(CardDropped {
                card,
                to,
                seat,
                index,
                hidden,
            });
        }
        DropPlan::Choose => {
            chooser.0 = Some(PendingDrop {
                card,
                to,
                seat,
                index,
                at: position,
            });
        }
        DropPlan::Refuse => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn on_drop_on_surface(
    event: On<Pointer<DragDrop>>,
    view: Res<ViewSeat>,
    my_seat: Res<MySeat>,
    players: Res<PlayerCount>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    info: Res<SessionInfo>,
    held: Res<Held>,
    tools: Res<plugin_ui::Tools>,
    rims: Res<highlight::Rims>,
    mut chooser: ResMut<DropChooser>,
    seats: Query<&DropSeat>,
    slots: Query<(&CardView, &Slot)>,
    dragged: Query<&CardView>,
    mut dropped: MessageWriter<CardDropped>,
) {
    if held.card != Some(event.dropped) {
        return;
    }
    let Ok(card_view) = dragged.get(event.dropped) else {
        return;
    };
    let Ok(drop_seat) = seats.get(event.event_target()) else {
        return;
    };
    let Some(position) = event.hit.position else {
        return;
    };

    let view_center = seat_center(view.0, players.0);
    let outward = Quat::from_rotation_y(seat_yaw(view.0, players.0)) * Vec3::Z;
    let in_hand_strip = (position - view_center).dot(outward) > dim::HAND_STRIP;
    let hand = zones::hand_zone(&mirror.view.zones);
    let in_my_hand = table
        .get(card_view.0)
        .is_some_and(|card| card.seat == my_seat.0 && card.zone == hand);
    if drop_seat.0 == view.0 && in_hand_strip && (tools.free || in_my_hand) {
        let axis = Quat::from_rotation_y(seat_yaw(drop_seat.0, players.0)) * Vec3::X;
        let index = insertion_index(&slots, &table, card_view.0, hand, my_seat.0, axis, position);
        dropped.write(CardDropped {
            card: card_view.0,
            to: hand,
            seat: my_seat.0,
            index,
            hidden: false,
        });
        return;
    }
    if tools.free {
        let axis = Quat::from_rotation_y(seat_yaw(drop_seat.0, players.0)) * Vec3::X;
        let index = insertion_index(
            &slots,
            &table,
            card_view.0,
            Zone::Board,
            drop_seat.0,
            axis,
            position,
        );
        dropped.write(CardDropped {
            card: card_view.0,
            to: Zone::Board,
            seat: drop_seat.0,
            index,
            hidden: false,
        });
        return;
    }
    let anchors = zones::anchors(
        &mirror.view.zones,
        players.0,
        info.battlefields_in_play(players.0),
    );
    let lit = lit_zones(&rims, card_view.0 .0);
    let Some(anchor) = snap_zone(position, &anchors, &lit, my_seat.0, DROP_REACH) else {
        return;
    };
    let to = Zone::Plugin(anchor.zone);
    let seat = anchor.seat.unwrap_or(PlayerId(0));
    let axis = Quat::from_rotation_y(anchor.yaw) * Vec3::X;
    let index = insertion_index(&slots, &table, card_view.0, to, seat, axis, position);
    settle_drop(
        drop_plan(&rims, tools.free, card_view.0 .0, anchor.zone),
        card_view.0,
        to,
        seat,
        index,
        position,
        &mut chooser,
        &mut dropped,
    );
}

#[derive(Resource, Default, Debug)]
pub struct RecentDrag(pub Option<(Entity, f32)>);

pub(super) fn on_drag_end(
    event: On<Pointer<DragEnd>>,
    mut commands: Commands,
    mut held: ResMut<Held>,
    time: Res<Time>,
    mut tracker: ResMut<Tracker>,
    mut swipe: ResMut<DrawerSwipe>,
    mut recent_drag: ResMut<RecentDrag>,
    cards: Query<Entity, With<CardView>>,
) {
    let target = event.event_target();
    let gesture = tracker.release(
        target,
        event.pointer_location.position,
        time.elapsed_secs_f64(),
    );
    if swipe.0.is_some_and(|active| active.card == target) {
        swipe.0 = None;
    }
    if held.card == Some(target) {
        held.card = None;
        held.target = None;
    }
    if gesture != Some(Gesture::Drop) {
        return;
    }
    recent_drag.0 = Some((target, time.elapsed_secs()));
    for entity in &cards {
        commands.entity(entity).insert(Pickable::default());
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn on_drop_on_zone(
    event: On<Pointer<DragDrop>>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    held: Res<Held>,
    tools: Res<plugin_ui::Tools>,
    rims: Res<highlight::Rims>,
    mut chooser: ResMut<DropChooser>,
    targets: Query<&DropZone>,
    slots: Query<(&CardView, &Slot)>,
    dragged: Query<&CardView>,
    mut dropped: MessageWriter<CardDropped>,
) {
    use agni_sim::wire::{ZoneOwner, ZoneVisibility};
    if held.card != Some(event.dropped) {
        return;
    }
    let Ok(card_view) = dragged.get(event.dropped) else {
        return;
    };
    let Ok(target) = targets.get(event.event_target()) else {
        return;
    };
    let Some(position) = event.hit.position else {
        return;
    };
    let to = Zone::Plugin(target.zone);
    if let Some(decl) = zones::decl_of(&mirror.view.zones, to) {
        if decl.visibility != ZoneVisibility::All
            && decl.owner == ZoneOwner::PerSeat
            && target.seat != my_seat.0
        {
            return;
        }
    }
    let axis = Quat::from_rotation_y(target.yaw) * Vec3::X;
    let index = insertion_index(&slots, &table, card_view.0, to, target.seat, axis, position);
    settle_drop(
        drop_plan(&rims, tools.free, card_view.0 .0, target.zone),
        card_view.0,
        to,
        target.seat,
        index,
        position,
        &mut chooser,
        &mut dropped,
    );
}

pub(super) fn on_right_click_card(
    event: On<Pointer<Click>>,
    cards: Query<&CardView>,
    mut selected: ResMut<Selected>,
    mut pinned: ResMut<Pinned>,
) {
    if event.button != PointerButton::Secondary {
        return;
    }
    let target = event.event_target();
    if !cards.contains(target) {
        return;
    }
    if pinned.0 == Some(target) {
        pinned.0 = None;
        return;
    }
    pinned.0 = Some(target);
    if selected.0 != Some(target) {
        selected.0 = Some(target);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickPlan {
    Affordance,
    Ambiguous,
    Chip(chips::ChipAction),
    Exhaust { on: bool },
    First,
    ToChain,
    Inert,
}

pub const DOUBLE_CLICK_SECS: f32 = 0.35;

#[allow(clippy::too_many_arguments)]
pub fn click_plan(
    view: &agni_sim::wire::PluginView,
    rims: &highlight::Rims,
    table: &Table,
    mirror: &Mirror,
    me: PlayerId,
    card: CardId,
    free: bool,
    quick: bool,
) -> ClickPlan {
    match plugin_ui::card_affordances(view, card.0).as_slice() {
        [_] => return ClickPlan::Affordance,
        [_, _, ..] => return ClickPlan::Ambiguous,
        [] if view
            .prompt
            .as_ref()
            .is_some_and(|summary| summary.seat == me.0) =>
        {
            return ClickPlan::Inert
        }
        [] => {}
    }
    if !quick {
        return ClickPlan::First;
    }
    let zone_name = |zone: u16| plugin_ui::zone_label(&mirror.view.zones, zone);
    let offers = chips::offers(view, rims, table, mirror, me, card.0, &zone_name);
    if let Some(chip) = chips::default_action(&offers) {
        return ClickPlan::Chip(chip.action);
    }
    let held = table.get(card);
    let in_my_hand =
        held.is_some_and(|card| card.seat == me && zones::is_hand(&mirror.view.zones, card.zone));
    let my_facedown =
        held.is_some_and(|card| card.owner == me && sync::lies_facedown(card, mirror));
    if in_my_hand || my_facedown {
        return ClickPlan::ToChain;
    }
    if free && !guarded_from_me(table, mirror, me, card) {
        return ClickPlan::Exhaust {
            on: !mirror.rotated(card.0),
        };
    }
    ClickPlan::Inert
}

pub fn chain_drop(
    table: &Table,
    mirror: &Mirror,
    me: PlayerId,
    card: CardId,
) -> Option<CardDropped> {
    let chain = zones::stack_zone(&mirror.view.zones)?;
    let to = Zone::Plugin(chain);
    Some(CardDropped {
        card,
        to,
        seat: me,
        index: table.in_area(me, to).count(),
        hidden: false,
    })
}

pub(super) fn on_click_felt(
    event: On<Pointer<Click>>,
    felt: Query<(), Or<(With<DropSeat>, With<DropZone>)>>,
    mut selected: ResMut<Selected>,
    mut pinned: ResMut<Pinned>,
    mut chooser: ResMut<DropChooser>,
) {
    if event.button != PointerButton::Primary || !felt.contains(event.event_target()) {
        return;
    }
    if selected.0.is_some() {
        selected.0 = None;
    }
    if pinned.0.is_some() {
        pinned.0 = None;
    }
    if chooser.0.is_some() {
        chooser.0 = None;
    }
}

pub(super) fn expire_selection(
    mut selected: ResMut<Selected>,
    mut pinned: ResMut<Pinned>,
    menu: Res<crate::menu::Menu>,
    cards: Query<(), With<CardView>>,
) {
    let gone = |entity: Option<Entity>| {
        entity.is_some_and(|entity| !menu.at_table() || !cards.contains(entity))
    };
    if gone(selected.0) {
        selected.0 = None;
    }
    if gone(pinned.0) {
        pinned.0 = None;
    }
}

pub(super) fn tick_gestures(
    time: Res<Time>,
    input: Res<InputKind>,
    mut tracker: ResMut<Tracker>,
    mut selected: ResMut<Selected>,
    mut pinned: ResMut<Pinned>,
) {
    let Some((entity, Gesture::LongPress)) = tracker.tick(time.elapsed_secs_f64(), *input) else {
        return;
    };
    pinned.0 = Some(entity);
    if selected.0 != Some(entity) {
        selected.0 = Some(entity);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn on_click_card(
    event: On<Pointer<Click>>,
    time: Res<Time>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    recent_drag: Res<RecentDrag>,
    tracker: Res<Tracker>,
    panel: Res<plugin_ui::PluginPanel>,
    tools: Res<plugin_ui::Tools>,
    rims: Res<highlight::Rims>,
    cards: Query<&CardView>,
    mut selected: ResMut<Selected>,
    mut chooser: ResMut<DropChooser>,
    mut last: Local<Option<(Entity, f32)>>,
    mut act: chips::Act,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let target = event.event_target();
    let now = time.elapsed_secs();
    if tracker
        .pressed(target)
        .is_some_and(|press| press.lifted || press.long_fired)
    {
        return;
    }
    if recent_drag
        .0
        .is_some_and(|(entity, at)| entity == target && now - at < 0.3)
    {
        return;
    }
    let Ok(view) = cards.get(target) else {
        return;
    };
    if chooser.0.is_some() {
        chooser.0 = None;
    }
    if act.pinned.0.is_some_and(|pinned| pinned != target) {
        act.pinned.0 = None;
    }
    if selected.0 != Some(target) {
        selected.0 = Some(target);
    }
    let quick =
        matches!(*last, Some((entity, at)) if entity == target && now - at < DOUBLE_CLICK_SECS);
    let plan = click_plan(
        &panel.view,
        &rims,
        &table,
        &mirror,
        my_seat.0,
        view.0,
        tools.free,
        quick,
    );
    *last = match plan {
        ClickPlan::First => Some((target, now)),
        _ => None,
    };
    match plan {
        ClickPlan::Affordance => {
            if let Some(only) = plugin_ui::card_affordance(&panel.view, view.0 .0) {
                act.sender.fire(only);
            }
        }
        ClickPlan::Chip(action) => {
            let chip = chips::Chip {
                label: String::new(),
                reason: None,
                action,
                group: 1,
            };
            act.perform(
                &chip,
                &panel.view,
                &table,
                &mirror,
                my_seat.0,
                view.0,
                target,
            );
        }
        ClickPlan::Exhaust { on } => {
            act.exhaust.write(ExhaustToggled { card: view.0, on });
        }
        ClickPlan::ToChain => {
            if let Some(drop) = chain_drop(&table, &mirror, my_seat.0, view.0) {
                act.dropped.write(drop);
            }
        }
        ClickPlan::Ambiguous | ClickPlan::First | ClickPlan::Inert => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    Chooser,
    Drag,
    Prompt(usize),
    Selection,
    Pass,
}

pub fn escape_rung(
    covered: bool,
    chooser: bool,
    dragging: bool,
    cancel: Option<usize>,
    selected: bool,
    pinned: bool,
) -> Rung {
    if covered {
        Rung::Pass
    } else if chooser {
        Rung::Chooser
    } else if dragging {
        Rung::Drag
    } else if let Some(index) = cancel {
        Rung::Prompt(index)
    } else if selected || pinned {
        Rung::Selection
    } else {
        Rung::Pass
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn escape_ladder(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut back: ResMut<BackKey>,
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut covers: Covers,
    panel: Res<plugin_ui::PluginPanel>,
    my_seat: Res<MySeat>,
    mut chooser: ResMut<DropChooser>,
    mut held: ResMut<Held>,
    mut tracker: ResMut<Tracker>,
    mut selected: ResMut<Selected>,
    mut pinned: ResMut<Pinned>,
    mut sender: hud::Sender,
    cards: Query<Entity, With<CardView>>,
) {
    if !back_pressed(&keys, &back) || !covers.menu.at_table() {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    if covers.pile_sheet.0.is_some() {
        covers.pile_sheet.0 = None;
        consume_back(&mut keys, &mut back);
        return;
    }
    if covers.chain_sheet.open {
        covers.chain_sheet.open = false;
        consume_back(&mut keys, &mut back);
        return;
    }
    let covered = covers.covered();
    let mine = panel
        .view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == my_seat.0 .0);
    let cancel = mine.then(|| plugin_ui::cancel_index(&panel.view)).flatten();
    let rung = escape_rung(
        covered,
        chooser.0.is_some(),
        held.card.is_some(),
        cancel,
        selected.0.is_some(),
        pinned.0.is_some(),
    );
    match rung {
        Rung::Chooser => chooser.0 = None,
        Rung::Drag => {
            tracker.cancel();
            held.card = None;
            held.target = None;
            for entity in &cards {
                commands.entity(entity).insert(Pickable::default());
            }
        }
        Rung::Prompt(index) => sender.fire(&panel.view.affordances[index]),
        Rung::Selection => {
            selected.0 = None;
            pinned.0 = None;
        }
        Rung::Pass => return,
    }
    consume_back(&mut keys, &mut back);
}

pub fn cycle_order(view: &agni_sim::wire::PluginView, rims: &highlight::Rims) -> Vec<u32> {
    let mut out: Vec<u32> = plugin_ui::highlighted(view)
        .into_iter()
        .chain(rims.legal.keys().copied())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

pub fn step(order: &[u32], current: Option<u32>, forward: bool) -> Option<u32> {
    if order.is_empty() {
        return None;
    }
    let at = current.and_then(|card| order.iter().position(|held| *held == card));
    let next = match (at, forward) {
        (None, true) => 0,
        (None, false) => order.len() - 1,
        (Some(index), true) => (index + 1) % order.len(),
        (Some(index), false) => (index + order.len() - 1) % order.len(),
    };
    Some(order[next])
}

#[allow(clippy::too_many_arguments)]
pub(super) fn selection_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    menu: Res<crate::menu::Menu>,
    claimed: Res<plugin_ui::ClaimedKeys>,
    panel: Res<plugin_ui::PluginPanel>,
    rims: Res<highlight::Rims>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    hovered: Query<Entity, With<Hovered>>,
    cards: Query<(Entity, &CardView)>,
    mut selected: ResMut<Selected>,
    mut pinned: ResMut<Pinned>,
) {
    if !menu.at_table() {
        return;
    }
    let tab = keys.just_pressed(KeyCode::Tab) && plugin_ui::kai_key(&claimed, KeyCode::Tab);
    let left =
        keys.just_pressed(KeyCode::ArrowLeft) && plugin_ui::kai_key(&claimed, KeyCode::ArrowLeft);
    let right =
        keys.just_pressed(KeyCode::ArrowRight) && plugin_ui::kai_key(&claimed, KeyCode::ArrowRight);
    let pin = keys.just_pressed(KeyCode::KeyI) && plugin_ui::kai_key(&claimed, KeyCode::KeyI);
    if !tab && !left && !right && !pin {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    let current = selected
        .0
        .and_then(|entity| cards.get(entity).ok())
        .map(|(_, view)| view.0 .0);
    let entity_of = |card: u32| {
        cards
            .iter()
            .find(|(_, view)| view.0 .0 == card)
            .map(|(entity, _)| entity)
    };
    if pin {
        let focus = hovered.iter().next().or(selected.0);
        pinned.0 = match (pinned.0, focus) {
            (Some(held), Some(focus)) if held == focus => None,
            (_, focus) => focus,
        };
        if let Some(entity) = pinned.0 {
            if selected.0 != Some(entity) {
                selected.0 = Some(entity);
            }
        }
        return;
    }
    let next = if tab {
        let back = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        step(&cycle_order(&panel.view, &rims), current, !back)
    } else {
        let hand: Vec<u32> = my_hand_ids(&table, &mirror, my_seat.0)
            .into_iter()
            .map(|id| id.0)
            .collect();
        step(&hand, current, right)
    };
    if let Some(entity) = next.and_then(entity_of) {
        if selected.0 != Some(entity) {
            selected.0 = Some(entity);
        }
    }
}

#[cfg(test)]
mod hover_switch_tests {
    use super::*;

    #[test]
    fn the_hover_only_moves_to_a_card_that_is_clearly_nearer_at_rest() {
        let current = Vec2::new(100.0, 500.0);
        let candidate = Vec2::new(160.0, 500.0);
        let midpoint = Vec2::new(130.0, 500.0);
        assert!(!hover_switches(
            midpoint,
            current,
            candidate,
            HOVER_HYSTERESIS
        ));
        assert!(!hover_switches(
            Vec2::new(136.0, 500.0),
            current,
            candidate,
            HOVER_HYSTERESIS
        ));
        assert!(hover_switches(
            Vec2::new(138.0, 500.0),
            current,
            candidate,
            HOVER_HYSTERESIS
        ));
        assert!(hover_switches(
            Vec2::new(200.0, 500.0),
            current,
            candidate,
            HOVER_HYSTERESIS
        ));
        assert!(!hover_switches(
            Vec2::new(90.0, 500.0),
            current,
            candidate,
            HOVER_HYSTERESIS
        ));
    }

    #[test]
    fn a_lifted_card_does_not_hand_the_hover_to_the_neighbour_it_uncovers() {
        let current = Vec2::new(100.0, 500.0);
        let candidate = Vec2::new(160.0, 500.0);
        let over_the_current_card = Vec2::new(110.0, 480.0);
        assert!(!hover_switches(
            over_the_current_card,
            current,
            candidate,
            HOVER_HYSTERESIS
        ));
    }
}

#[cfg(test)]
mod drop_tests {
    use super::*;
    use agni_sim::wire::{Legal, LegalKind, PluginView};
    use std::collections::BTreeMap;

    fn anchor(zone: u16, seat: Option<u8>, x: f32, z: f32) -> zones::ZoneAnchor {
        zones::ZoneAnchor {
            zone,
            seat: seat.map(PlayerId),
            position: Vec3::new(x, 0.0, z),
            yaw: 0.0,
            size: Vec2::new(2.0, 1.6),
        }
    }

    #[test]
    fn a_release_near_a_lit_zone_snaps_to_it_and_far_from_every_zone_goes_nowhere() {
        let anchors = [
            anchor(9, None, 0.0, 0.0),
            anchor(10, None, 3.0, 0.0),
            anchor(12, Some(0), 0.0, 3.0),
            anchor(12, Some(1), 0.0, -3.0),
        ];
        let lit = [9u16, 12];
        let inside = snap_zone(Vec3::new(0.5, 0.0, 0.2), &anchors, &lit, PlayerId(0), 0.35);
        assert_eq!(inside.map(|a| a.zone), Some(9));
        let just_out = snap_zone(Vec3::new(1.3, 0.0, 0.0), &anchors, &lit, PlayerId(0), 0.35);
        assert_eq!(
            just_out.map(|a| a.zone),
            Some(9),
            "24 dp of reach past the edge"
        );
        let too_far = snap_zone(Vec3::new(1.4, 0.0, 0.0), &anchors, &lit, PlayerId(0), 0.35);
        assert!(too_far.is_none());
        let unlit = snap_zone(Vec3::new(3.0, 0.0, 0.0), &anchors, &lit, PlayerId(0), 0.35);
        assert!(unlit.is_none(), "zone 10 is not lit for this card");
        let theirs = snap_zone(Vec3::new(0.0, 0.0, -3.0), &anchors, &lit, PlayerId(0), 0.35);
        assert!(
            theirs.is_none(),
            "the other seat's copy of a per-seat zone never takes my drop"
        );
        let mine = snap_zone(Vec3::new(0.0, 0.0, 3.0), &anchors, &lit, PlayerId(0), 0.35);
        assert_eq!(mine.map(|a| a.seat), Some(Some(PlayerId(0))));
    }

    #[test]
    fn the_drop_plan_reads_play_and_hide_from_the_rows_and_a_free_table_takes_anything() {
        let bf1 = 9u16;
        let view = PluginView {
            legal: vec![
                Legal {
                    card: 7,
                    kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
                    zones: vec![bf1, 12],
                    hidden: vec![bf1],
                },
                Legal {
                    card: 8,
                    kinds: vec![LegalKind::March],
                    zones: vec![10],
                    hidden: Vec::new(),
                },
            ],
            ..Default::default()
        };
        let rims = highlight::rims(&view, 0, &[7].into_iter().collect(), &BTreeMap::new());
        assert_eq!(lit_zones(&rims, 7), [bf1, 12]);
        assert_eq!(lit_zones(&rims, 8), [10]);
        assert_eq!(drop_plan(&rims, false, 7, bf1), DropPlan::Choose);
        assert_eq!(
            drop_plan(&rims, false, 7, 12),
            DropPlan::Drop { hidden: false }
        );
        assert_eq!(
            drop_plan(&rims, false, 8, 10),
            DropPlan::Drop { hidden: false }
        );
        assert_eq!(drop_plan(&rims, false, 8, bf1), DropPlan::Refuse);
        assert_eq!(
            drop_plan(&rims, true, 8, bf1),
            DropPlan::Drop { hidden: false }
        );
        let only_hide = PluginView {
            legal: vec![Legal {
                card: 7,
                kinds: vec![LegalKind::Hide],
                zones: Vec::new(),
                hidden: vec![bf1],
            }],
            ..Default::default()
        };
        let rims = highlight::rims(&only_hide, 0, &BTreeSet::new(), &BTreeMap::new());
        assert_eq!(
            drop_plan(&rims, false, 7, bf1),
            DropPlan::Drop { hidden: true }
        );
    }

    #[test]
    fn the_escape_ladder_walks_chooser_drag_prompt_selection_then_passes_the_key_on() {
        assert_eq!(
            escape_rung(false, true, true, Some(2), true, true),
            Rung::Chooser
        );
        assert_eq!(
            escape_rung(false, false, true, Some(2), true, true),
            Rung::Drag
        );
        assert_eq!(
            escape_rung(false, false, false, Some(2), true, true),
            Rung::Prompt(2)
        );
        assert_eq!(
            escape_rung(false, false, false, None, true, false),
            Rung::Selection
        );
        assert_eq!(
            escape_rung(false, false, false, None, false, true),
            Rung::Selection
        );
        assert_eq!(
            escape_rung(false, false, false, None, false, false),
            Rung::Pass
        );
        assert_eq!(
            escape_rung(true, true, true, Some(2), true, true),
            Rung::Pass,
            "an open sheet or menu owns the key: nothing under it fires"
        );
    }

    #[test]
    fn a_swipe_across_the_raised_drawer_scrolls_instead_of_lifting() {
        use crate::viewport::ViewportClass;
        assert!(swipe_starts(
            ViewportClass::PhonePortrait,
            true,
            Vec2::new(12.0, 3.0)
        ));
        assert!(!swipe_starts(
            ViewportClass::PhonePortrait,
            true,
            Vec2::new(3.0, -12.0)
        ));
        assert!(!swipe_starts(
            ViewportClass::PhonePortrait,
            false,
            Vec2::new(12.0, 3.0)
        ));
        assert!(!swipe_starts(
            ViewportClass::Desktop,
            true,
            Vec2::new(12.0, 3.0)
        ));
    }

    #[test]
    fn tab_cycles_the_highlighted_cards_and_wraps() {
        let order = [3u32, 7, 9];
        assert_eq!(step(&order, None, true), Some(3));
        assert_eq!(step(&order, Some(3), true), Some(7));
        assert_eq!(step(&order, Some(9), true), Some(3));
        assert_eq!(step(&order, None, false), Some(9));
        assert_eq!(step(&order, Some(3), false), Some(9));
        assert_eq!(
            step(&order, Some(4), true),
            Some(3),
            "a card that left the set restarts"
        );
        assert_eq!(step(&[], Some(4), true), None);
        let view = PluginView {
            legal: vec![Legal {
                card: 9,
                kinds: vec![LegalKind::Play { accelerate: false }],
                zones: Vec::new(),
                hidden: Vec::new(),
            }],
            affordances: vec![agni_sim::wire::Affordance {
                label: "{card 3}".into(),
                enabled: true,
                card: Some(3),
                ..Default::default()
            }],
            ..Default::default()
        };
        let rims = highlight::rims(&view, 0, &BTreeSet::new(), &BTreeMap::new());
        assert_eq!(cycle_order(&view, &rims), [3, 9]);
    }
}

#[cfg(test)]
mod hover_tests {
    use super::*;

    #[test]
    fn the_hand_reach_runs_from_the_card_top_to_the_bottom_of_the_window() {
        let corners = [
            Vec2::new(400.0, 700.0),
            Vec2::new(500.0, 700.0),
            Vec2::new(500.0, 840.0),
            Vec2::new(400.0, 840.0),
        ];
        let reach = hand_reach(&corners, 800.0, 8.0).unwrap();
        assert!(reach.contains(Vec2::new(450.0, 799.0)));
        assert!(reach.contains(Vec2::new(392.0, 700.0)));
        assert!(!reach.contains(Vec2::new(380.0, 750.0)));
        assert!(!reach.contains(Vec2::new(450.0, 680.0)));
        assert!(reach.max.y >= 848.0);
        assert!(hand_reach(&[], 800.0, 8.0).is_none());
    }
}

#[cfg(test)]
mod click_tests {
    use super::*;
    use agni_sim::wire::{Affordance, AffordanceKind, Legal, LegalKind, PluginView};
    use serde_bytes::ByteBuf;

    struct Fixture {
        table: Table,
        mirror: Mirror,
        vi: CardId,
        cleave: CardId,
        theirs: CardId,
        lure: CardId,
    }

    fn m6_fixture() -> Fixture {
        let mut table = Table::new();
        let bf1 = Zone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST);
        let hand = Zone::Plugin(agni_riftbound::ZONE_HAND);
        let vi = table.add(PlayerId(0), bf1, "Vi - Piltover Enforcer", [0; 3]);
        let cleave = table.add(PlayerId(0), bf1, "Cleave", [0; 3]);
        let theirs = table.add(PlayerId(1), bf1, "Back Off", [0; 3]);
        let lure = table.add(PlayerId(0), hand, "Lure of the Depths", [0; 3]);
        let mirror = Mirror {
            view: TableView {
                zones: agni_riftbound::zone_table(),
                revealed: vec![vi.0],
                ..Default::default()
            },
            solo_exhausted: BTreeSet::new(),
        };
        Fixture {
            table,
            mirror,
            vi,
            cleave,
            theirs,
            lure,
        }
    }

    fn enforced_view(cleave: u32) -> PluginView {
        PluginView {
            status: vec!["turn 3 · {seat 1} · action phase · rules enforced".into()],
            legal: vec![Legal {
                card: cleave,
                kinds: vec![LegalKind::React],
                zones: vec![agni_riftbound::ZONE_CHAIN],
                hidden: Vec::new(),
            }],
            ..Default::default()
        }
    }

    fn free_view() -> PluginView {
        PluginView {
            status: vec!["turn 3 · {seat 1} · action phase · free table".into()],
            ..Default::default()
        }
    }

    fn plan(
        view: &PluginView,
        fixture: &Fixture,
        card: CardId,
        free: bool,
        quick: bool,
    ) -> ClickPlan {
        let rims = highlight::rims(
            view,
            0,
            &BTreeSet::new(),
            &highlight::owners(&GameTable(fixture.table.clone())),
        );
        click_plan(
            view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            card,
            free,
            quick,
        )
    }

    #[test]
    fn a_double_click_on_my_facedown_card_plays_it_to_the_chain() {
        let fixture = m6_fixture();
        let view = enforced_view(fixture.cleave.0);
        let me = PlayerId(0);
        assert!(sync::lies_facedown(
            fixture.table.get(fixture.cleave).unwrap(),
            &fixture.mirror
        ));
        assert_eq!(
            plan(&view, &fixture, fixture.cleave, false, false),
            ClickPlan::First,
            "the first click only selects"
        );
        assert_eq!(
            plan(&view, &fixture, fixture.cleave, false, true),
            ClickPlan::ToChain,
            "737.6 · the second click within 350 ms plays the hidden card"
        );
        let drop = chain_drop(&fixture.table, &fixture.mirror, me, fixture.cleave).unwrap();
        assert_eq!(drop.to, Zone::Plugin(agni_riftbound::ZONE_CHAIN));
        assert_eq!(drop.card, fixture.cleave);
        assert_eq!(drop.seat, me);
        assert!(!drop.hidden);
        assert_eq!(
            plan(&view, &fixture, fixture.lure, false, true),
            ClickPlan::ToChain,
            "a hand card double-clicks to the chain as it always did"
        );
        assert_eq!(
            plan(&view, &fixture, fixture.theirs, true, true),
            ClickPlan::Exhaust { on: true },
            "the other seat's face-down card is never mine to play, so a free table exhausts it"
        );
    }

    #[test]
    fn the_rules_enforced_fixture_leaves_every_free_verb_inert() {
        let fixture = m6_fixture();
        let view = enforced_view(fixture.cleave.0);
        let tools = plugin_ui::Tools::from(&view);
        assert!(!tools.free);
        assert!(plugin_ui::Tools::from(&free_view()).free);
        let pre_roll = PluginView {
            status: vec![
                "roll for first player".into(),
                "{seat 0}: waiting".into(),
                "mode: rules enforced".into(),
            ],
            ..Default::default()
        };
        assert!(
            !plugin_ui::Tools::from(&pre_roll).free,
            "the gate reads the mode line before the roll, like the plate chip"
        );
        assert_eq!(
            plate::mode_of(&pre_roll),
            Some(plate::Mode::Enforced),
            "one resolver serves the chip and the gate"
        );
        assert!(
            plugin_ui::Tools::from(&PluginView::default()).free,
            "an empty view is a free table"
        );
        assert_eq!(
            plan(&view, &fixture, fixture.vi, tools.free, false),
            ClickPlan::First,
            "the first click selects and nothing leaves it"
        );
        assert_eq!(
            plan(&view, &fixture, fixture.vi, tools.free, true),
            ClickPlan::Inert,
            "no ExhaustToggled leaves the second click either"
        );
        assert_eq!(
            plan(&view, &fixture, fixture.theirs, tools.free, true),
            ClickPlan::Inert
        );
        let rims = highlight::rims(
            &view,
            0,
            &BTreeSet::new(),
            &highlight::owners(&GameTable(fixture.table.clone())),
        );
        assert!(
            may_lift(tools.free, &rims, fixture.cleave.0),
            "the rimmed card lifts"
        );
        assert!(
            !may_lift(tools.free, &rims, fixture.vi.0),
            "a card without a rim does not"
        );
        assert!(
            may_lift(true, &rims, fixture.vi.0),
            "a free table lifts anything"
        );
        for key in [KeyCode::KeyE, KeyCode::KeyD, KeyCode::KeyT, KeyCode::KeyK] {
            assert!(!ui::key_allowed(&tools, key), "{key:?} is a free verb");
            assert!(ui::key_allowed(&plugin_ui::Tools::default(), key));
        }
        for key in [KeyCode::KeyH, KeyCode::KeyP, KeyCode::KeyR, KeyCode::Space] {
            assert!(
                ui::key_allowed(&tools, key),
                "{key:?} stays with the engine"
            );
        }
        assert!(!counters::nudges_allowed(&tools, SessionRole::Host));
        assert!(counters::nudges_allowed(
            &plugin_ui::Tools::default(),
            SessionRole::Host
        ));
        assert!(!counters::nudges_allowed(
            &plugin_ui::Tools::default(),
            SessionRole::Ended
        ));
        assert!(
            !ui::redeal_allowed(tools.free, false),
            "the sample deal is a free verb mid-game"
        );
        assert!(ui::redeal_allowed(tools.free, true));
        assert!(ui::redeal_allowed(true, false));
        assert!(!crate::deck::sideboard::reload_allowed(
            true, true, tools.free, false
        ));
        assert!(
            crate::deck::sideboard::reload_allowed(true, true, tools.free, true),
            "between games the reload is offered again"
        );
        assert!(crate::deck::sideboard::reload_allowed(
            true, true, true, false
        ));
        assert!(!crate::deck::sideboard::reload_allowed(
            false, true, true, true
        ));
    }

    #[test]
    fn a_free_table_exhausts_on_the_second_tap_and_the_affordance_click_wins() {
        let fixture = m6_fixture();
        let free = free_view();
        assert_eq!(
            plan(&free, &fixture, fixture.vi, true, false),
            ClickPlan::First,
            "one tap selects, on a free table too"
        );
        assert_eq!(
            plan(&free, &fixture, fixture.vi, true, true),
            ClickPlan::Exhaust { on: true }
        );
        let mut rotated = m6_fixture();
        rotated.mirror.solo_exhausted.insert(rotated.vi.0);
        assert_eq!(
            plan(&free, &rotated, rotated.vi, true, true),
            ClickPlan::Exhaust { on: false }
        );
        let asked = PluginView {
            affordances: vec![Affordance {
                label: "stun {card 0}".into(),
                hotkey: None,
                enabled: true,
                kind: AffordanceKind::Plain,
                data: ByteBuf::from(vec![1]),
                card: Some(fixture.vi.0),
            }],
            ..enforced_view(fixture.cleave.0)
        };
        assert_eq!(
            plan(&asked, &fixture, fixture.vi, false, false),
            ClickPlan::Affordance,
            "a prompt candidate answers the prompt on a single click"
        );
        let mut two = asked.clone();
        two.affordances.push(two.affordances[0].clone());
        assert_eq!(
            plan(&two, &fixture, fixture.vi, false, false),
            ClickPlan::Ambiguous
        );
        let open = PluginView {
            prompt: Some(agni_sim::wire::PromptSummary {
                seat: 0,
                why: "set aside up to 2 cards to redraw".into(),
                min: 0,
                max: 2,
                picked: 1,
                optional: false,
            }),
            ..asked.clone()
        };
        assert_eq!(
            plan(&open, &fixture, fixture.lure, false, true),
            ClickPlan::Inert,
            "while my prompt is open only the cards it names answer a click; nothing else is sent to be refused"
        );
        let theirs_open = PluginView {
            prompt: Some(agni_sim::wire::PromptSummary {
                seat: 1,
                ..open.prompt.clone().unwrap()
            }),
            ..asked.clone()
        };
        assert_eq!(
            plan(&theirs_open, &fixture, fixture.lure, false, true),
            ClickPlan::ToChain,
            "the other seat's question does not lock my cards"
        );
    }
}
