use super::hud::{DrawerState, Hud};
use super::*;
use crate::viewport::{Viewport, ViewportClass};
use agni_sim::wire::PluginView;
use bevy_egui::egui::Rect;

pub const PHONE_CARD_W: f32 = 96.0;
pub const PHONE_CARD_H: f32 = PHONE_CARD_W * dim::CARD_H / dim::CARD_W;
pub const PHONE_CARD_OVERLAP: f32 = 12.0;
pub const PHONE_TUCKED_ACROSS: usize = 5;
pub const PHONE_CARD_TOP_INSET: f32 = 8.0;
pub const SWIPE_TUCK: f32 = 24.0;
pub const DRAWER_GRAB_H: f32 = 38.0;
pub const DOUBLE_TAP_SECS: f64 = 0.3;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HandDrawer {
    pub state: DrawerState,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct DrawerScroll(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerEvent {
    TapDrawer,
    TapFelt,
    SwipeDown,
    SwipeUp,
    DragOut,
    PromptOnBoard,
    PromptInHand,
    TurnEnded,
}

pub fn drawer_next(_state: DrawerState, event: DrawerEvent) -> DrawerState {
    match event {
        DrawerEvent::TapDrawer | DrawerEvent::SwipeUp | DrawerEvent::PromptInHand => {
            DrawerState::Raised
        }
        DrawerEvent::TapFelt
        | DrawerEvent::SwipeDown
        | DrawerEvent::DragOut
        | DrawerEvent::PromptOnBoard
        | DrawerEvent::TurnEnded => DrawerState::Tucked,
    }
}

pub fn swipe_event(delta: Vec2, threshold: f32) -> Option<DrawerEvent> {
    if delta.y <= -threshold && delta.y.abs() > delta.x.abs() {
        Some(DrawerEvent::SwipeUp)
    } else if delta.y >= threshold && delta.y.abs() > delta.x.abs() {
        Some(DrawerEvent::SwipeDown)
    } else {
        None
    }
}

pub fn prompt_event(
    view: &PluginView,
    me: u8,
    hand: &std::collections::BTreeSet<u32>,
) -> Option<DrawerEvent> {
    let prompt = view.prompt.as_ref()?;
    if prompt.seat != me {
        return None;
    }
    let candidates = plugin_ui::highlighted(view);
    if candidates.is_empty() {
        return None;
    }
    if candidates.iter().any(|card| hand.contains(card)) {
        Some(DrawerEvent::PromptInHand)
    } else {
        Some(DrawerEvent::PromptOnBoard)
    }
}

pub fn turn_event(was_acting: bool, acting: bool) -> Option<DrawerEvent> {
    (was_acting && !acting).then_some(DrawerEvent::TurnEnded)
}

pub fn drag_out(class: ViewportClass, state: DrawerState, pointer_y: f32, drawer_top: f32) -> bool {
    class.is_phone() && state == DrawerState::Raised && pointer_y < drawer_top
}

pub fn card_pitch(class: ViewportClass, state: DrawerState, band_width: f32) -> f32 {
    if !class.is_phone() {
        return dim::CARD_W;
    }
    let raised = PHONE_CARD_W - PHONE_CARD_OVERLAP;
    match state {
        DrawerState::Raised => raised,
        DrawerState::Tucked => {
            let room = (band_width - 2.0 * hud::GAP - PHONE_CARD_W).max(0.0);
            raised.min(room / (PHONE_TUCKED_ACROSS as f32 - 1.0))
        }
    }
}

pub fn hand_visible(class: ViewportClass, band_width: f32, pitch: f32) -> usize {
    if !class.is_phone() {
        return dim::HAND_VISIBLE as usize;
    }
    let room = (band_width - 2.0 * hud::GAP - PHONE_CARD_W).max(0.0);
    if pitch <= 0.0 {
        return 1;
    }
    ((room / pitch).floor() as usize + 1).max(1)
}

pub fn scroll_clamp(scroll: f32, hand: usize, visible: usize) -> f32 {
    let max = hand.saturating_sub(visible) as f32;
    scroll.clamp(0.0, max.max(0.0))
}

pub fn swipe_scroll(scroll: f32, delta_x: f32, pitch: f32, hand: usize, visible: usize) -> f32 {
    if pitch <= 0.0 {
        return scroll;
    }
    scroll_clamp(scroll - delta_x / pitch, hand, visible)
}

pub fn drawer_card_center(
    band: Rect,
    state: DrawerState,
    pitch: f32,
    index: usize,
    scroll: f32,
) -> bevy_egui::egui::Pos2 {
    let rel = index as f32 - scroll;
    let x = band.min.x + hud::GAP + PHONE_CARD_W / 2.0 + rel * pitch;
    let y = match state {
        DrawerState::Tucked => band.min.y + PHONE_CARD_H / 2.0,
        DrawerState::Raised => band.min.y + PHONE_CARD_TOP_INSET + PHONE_CARD_H / 2.0,
    };
    bevy_egui::egui::pos2(x, y)
}

pub fn card_depth(window_h: f32, fov: f32) -> f32 {
    if window_h <= 0.0 {
        return 10.0;
    }
    dim::CARD_W * window_h / (2.0 * PHONE_CARD_W * (fov / 2.0).tan())
}

pub fn screen_ray(window: Vec2, fov: f32, point: Vec2) -> Vec3 {
    let aspect = if window.y > 0.0 {
        window.x / window.y
    } else {
        1.0
    };
    let ndc = Vec2::new(
        point.x / window.x * 2.0 - 1.0,
        1.0 - point.y / window.y * 2.0,
    );
    let half = (fov / 2.0).tan();
    Vec3::new(ndc.x * half * aspect, ndc.y * half, -1.0)
}

pub fn phone_hand_slot(
    camera: &Transform,
    window: Vec2,
    view_yaw: f32,
    center: bevy_egui::egui::Pos2,
    depth: f32,
    index: usize,
) -> Slot {
    let view_dir = screen_ray(window, dim::FOV, Vec2::new(center.x, center.y));
    let position = camera.translation
        + camera.rotation * (view_dir * (depth - index as f32 * dim::STACK_STEP));
    Slot {
        position,
        facing: Facing::Camera,
        yaw: view_yaw,
        rot: 0.0,
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct HandPlane {
    pub phone: bool,
}

pub(super) fn auto_drawer(
    viewport: Res<Viewport>,
    panel: Res<plugin_ui::PluginPanel>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    mut drawer: ResMut<HandDrawer>,
    mut last: Local<(Option<(u8, String)>, bool)>,
) {
    if !viewport.class.is_phone() {
        return;
    }
    let view = &panel.view;
    let prompt = view
        .prompt
        .as_ref()
        .map(|prompt| (prompt.seat, prompt.why.clone()));
    let acting = highlight::acting(view);
    let mut events = Vec::new();
    if prompt != last.0 {
        let hand: std::collections::BTreeSet<u32> = my_hand_ids(&table, &mirror, my_seat.0)
            .into_iter()
            .map(|id| id.0)
            .collect();
        events.extend(prompt_event(view, my_seat.0 .0, &hand));
        last.0 = prompt;
    }
    events.extend(turn_event(last.1, acting));
    last.1 = acting;
    for event in events {
        let next = drawer_next(drawer.state, event);
        if next != drawer.state {
            drawer.state = next;
        }
    }
}

pub(super) fn tuck_on_drag_out(
    viewport: Res<Viewport>,
    hud: Res<Hud>,
    held: Res<Held>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    slots: Query<&Slot, With<CardView>>,
    mut drawer: ResMut<HandDrawer>,
) {
    let Some(card) = held.card else {
        return;
    };
    if !slots
        .get(card)
        .is_ok_and(|slot| slot.facing == Facing::Camera)
    {
        return;
    }
    let Some(cursor) = windows.single().ok().and_then(Window::cursor_position) else {
        return;
    };
    if drag_out(viewport.class, drawer.state, cursor.y, hud.0.hand.min.y) {
        drawer.state = drawer_next(drawer.state, DrawerEvent::DragOut);
    }
}

pub fn double_tap(last: Option<f64>, now: f64) -> bool {
    last.is_some_and(|last| now - last <= DOUBLE_TAP_SECS)
}

pub fn under_the_drawer(class: ViewportClass, band: Rect, at: Vec2) -> bool {
    class.is_phone() && band.contains(bevy_egui::egui::pos2(at.x, at.y))
}

pub(super) fn on_tap_felt(
    event: On<Pointer<Click>>,
    felt: Query<(), Or<(With<DropSeat>, With<DropZone>)>>,
    viewport: Res<Viewport>,
    hud: Res<Hud>,
    time: Res<Time>,
    mut drawer: ResMut<HandDrawer>,
    mut tuning: ResMut<Tuning>,
    mut last: Local<Option<f64>>,
) {
    if event.button != PointerButton::Primary || !felt.contains(event.event_target()) {
        return;
    }
    if under_the_drawer(viewport.class, hud.0.hand, event.pointer_location.position) {
        return;
    }
    let now = time.elapsed_secs_f64();
    if viewport.class.is_phone() && drawer.state != DrawerState::Tucked {
        drawer.state = drawer_next(drawer.state, DrawerEvent::TapFelt);
    }
    if double_tap(*last, now) {
        tuning.pan_x = 0.0;
        tuning.pan_z = 0.0;
        tuning.zoom = 1.0;
        *last = None;
    } else {
        *last = Some(now);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct PhoneHand<'w, 's> {
    viewport: Res<'w, Viewport>,
    hud: Res<'w, Hud>,
    drawer: Res<'w, HandDrawer>,
    scroll: Res<'w, DrawerScroll>,
    camera: Query<'w, 's, (&'static Camera, &'static Transform), With<Camera3d>>,
    plane: ResMut<'w, HandPlane>,
}

pub(super) fn layout_cards(
    table: Res<GameTable>,
    tuning: Res<Tuning>,
    generation: Res<DealGeneration>,
    scroll: Res<HandScroll>,
    view: Res<ViewSeat>,
    my_seat: Res<MySeat>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    held: Res<Held>,
    mirror: Res<Mirror>,
    extent: Res<camera::Extent>,
    menu: Option<Res<crate::menu::Menu>>,
    mut phone_hand: PhoneHand,
    mut cards: Query<(Entity, Ref<CardView>, &mut Slot, &mut Visibility)>,
) {
    let phone = phone_hand.viewport.class.is_phone();
    if phone_hand.plane.phone != phone {
        phone_hand.plane.phone = phone;
    }
    let viewport = &phone_hand.viewport;
    let hud = &phone_hand.hud;
    let drawer = &phone_hand.drawer;
    let drawer_scroll = &phone_hand.scroll;
    let camera = &phone_hand.camera;
    if !table.is_changed()
        && !tuning.is_changed()
        && !generation.is_changed()
        && !scroll.is_changed()
        && !view.is_changed()
        && !my_seat.is_changed()
        && !mirror.is_changed()
        && !extent.is_changed()
        && !info.is_changed()
        && !hud.is_changed()
        && !drawer.is_changed()
        && !drawer_scroll.is_changed()
        && !viewport.is_changed()
        && !menu.as_ref().is_some_and(|menu| menu.is_changed())
        && !cards.iter().any(|(_, card, _, _)| card.is_added())
    {
        return;
    }
    let at_table = menu.as_deref().is_none_or(crate::menu::Menu::at_table);

    let zone_table = &mirror.view.zones;
    let fan = zones::fan_zone(zone_table).map(Zone::Plugin);
    let anchors = zones::anchors_in(
        zone_table,
        players.0,
        info.battlefields_in_play(players.0),
        extent.quad_w,
    );
    let mut placement: Vec<(CardId, Slot, bool)> = Vec::with_capacity(table.len());
    let hand_ids = my_hand_ids(&table, &mirror, my_seat.0);
    let hand_n = hand_ids.len();
    let band = hud.0.hand;
    let pitch = card_pitch(viewport.class, drawer.state, band.width());
    let visible = hand_visible(viewport.class, band.width(), pitch);
    let phone_rig = phone
        .then(|| camera.single().ok())
        .flatten()
        .map(|(_, transform)| (*transform, card_depth(viewport.logical.y, dim::FOV)));
    let view_yaw = seat_yaw(view.0, players.0);
    for (i, id) in hand_ids.into_iter().enumerate() {
        let (slot, shown) = match &phone_rig {
            Some((transform, depth)) => {
                let rel = i as f32 - drawer_scroll.0;
                let shown = rel > -0.5 && rel < visible as f32 - 0.5;
                let center = drawer_card_center(band, drawer.state, pitch, i, drawer_scroll.0);
                (
                    phone_hand_slot(transform, viewport.logical, view_yaw, center, *depth, i),
                    shown,
                )
            }
            None => {
                let rel = i as f32 - scroll.0;
                let shown = rel > -0.5 && rel < dim::HAND_VISIBLE - 0.5;
                (
                    hand_slot(i, hand_n, scroll.0, view.0, players.0, &tuning),
                    shown,
                )
            }
        };
        placement.push((id, slot, shown));
    }
    for seat in 0..players.0 as u8 {
        let seat = PlayerId(seat);
        let ids: Vec<CardId> = table.in_area(seat, Zone::Board).map(|c| c.id).collect();
        let n = ids.len();
        for (i, id) in ids.into_iter().enumerate() {
            let mut slot = board_slot(seat, i, n, players.0);
            if mirror.rotated(id.0) {
                slot.rot = std::f32::consts::FRAC_PI_2;
            }
            placement.push((id, slot, true));
        }
        if let Some(fan) = fan {
            if seat != my_seat.0 {
                let ids: Vec<CardId> = table.in_area(seat, fan).map(|c| c.id).collect();
                let n = ids.len();
                let shown = view.0 != seat;
                for (i, id) in ids.into_iter().enumerate() {
                    placement.push((id, fan_back_slot(seat, i, n, players.0), shown));
                }
            }
        }
    }
    for anchor in &anchors {
        let Some(decl) = zone_table.iter().find(|decl| decl.id == anchor.zone) else {
            continue;
        };
        let seat = anchor.seat.unwrap_or(PlayerId(0));
        let ids: Vec<CardId> = table
            .in_area(seat, Zone::Plugin(anchor.zone))
            .map(|c| c.id)
            .collect();
        let (loose, worn) = zones::attachments(&ids, |card| mirror.attached_to(card));
        let n = loose.len();
        let first = placement.len();
        for (i, id) in loose.into_iter().enumerate() {
            placement.push((
                id,
                zones::zone_slot(decl.layout, anchor, i, n, mirror.rotated(id.0)),
                true,
            ));
        }
        for (gear, wearer, rank) in worn {
            let Some((_, slot, _)) = placement[first..].iter().find(|(id, _, _)| *id == wearer)
            else {
                continue;
            };
            let slot = zones::attached_slot(slot, rank, mirror.rotated(gear.0));
            placement.push((gear, slot, true));
        }
    }
    for (entity, card_view, mut slot, mut visibility) in &mut cards {
        match placement.iter().find(|(id, _, _)| *id == card_view.0) {
            Some((_, new_slot, shown)) => {
                if *slot != *new_slot {
                    *slot = *new_slot;
                }
                let wanted = card_visibility(at_table, *shown || held.card == Some(entity));
                if *visibility != wanted {
                    *visibility = wanted;
                }
            }
            None => {
                if *visibility != Visibility::Hidden {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

pub fn card_visibility(at_table: bool, placed: bool) -> Visibility {
    if at_table && placed {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

pub(super) fn fan_back_slot(seat: PlayerId, i: usize, n: usize, players: usize) -> Slot {
    let center = seat_center(seat, players);
    let yaw = seat_yaw(seat, players);
    let spin = Quat::from_rotation_y(yaw);
    let x = (i as f32 - (n as f32 - 1.0) / 2.0) * 0.82;
    Slot {
        position: center + spin * Vec3::new(x, 0.9 - x * x * 0.012, dim::HAND_NEAR),
        facing: Facing::Back,
        yaw,
        rot: 0.0,
    }
}

pub(super) fn hand_slot(
    i: usize,
    n: usize,
    scroll: f32,
    view: PlayerId,
    players: usize,
    tuning: &Tuning,
) -> Slot {
    let anchor = seat_center(view, players);
    let yaw = seat_yaw(view, players);
    let visible = dim::HAND_VISIBLE.min(n as f32);
    let rel = i as f32 - scroll - (visible - 1.0) / 2.0;
    let x = rel * tuning.hand_spacing();
    let curve_x = x.clamp(-4.0 * tuning.hand_spacing(), 4.0 * tuning.hand_spacing());
    Slot {
        position: anchor
            + Quat::from_rotation_y(yaw)
                * Vec3::new(
                    x,
                    tuning.hand_y + i as f32 * dim::STACK_STEP
                        - curve_x * curve_x * tuning.hand_droop,
                    dim::HAND_NEAR + (tuning.hand_z - 3.4) + curve_x * curve_x * tuning.hand_curve,
                ),
        facing: Facing::Camera,
        yaw,
        rot: 0.0,
    }
}

pub(super) fn board_slot(seat: PlayerId, i: usize, n: usize, players: usize) -> Slot {
    let offset = (i as f32 - n.saturating_sub(1) as f32 / 2.0) * dim::BOARD_SPACING;
    let yaw = seat_yaw(seat, players);
    Slot {
        position: seat_center(seat, players)
            + Quat::from_rotation_y(yaw)
                * Vec3::new(offset, dim::BOARD_Y + i as f32 * dim::STACK_STEP, -0.8),
        facing: Facing::Flat,
        yaw,
        rot: 0.0,
    }
}

pub(super) fn rotation_for(
    facing: Facing,
    yaw: f32,
    rot: f32,
    position: Vec3,
    camera: Vec3,
) -> Quat {
    match facing {
        Facing::Flat => Quat::from_rotation_y(yaw + rot),
        Facing::Back => Quat::from_rotation_y(yaw) * Quat::from_rotation_x(dim::HAND_TILT - 0.5),
        Facing::Camera => {
            let normal = (camera - position).normalize();
            let axis = Quat::from_rotation_y(yaw) * Vec3::X;
            let right = axis.reject_from_normalized(normal).normalize();
            Quat::from_mat3(&Mat3::from_cols(right, normal, right.cross(normal)))
        }
    }
}

#[cfg(test)]
mod drawer_tests {
    use super::*;
    use agni_core::CardFace;
    use agni_sim::wire::{Affordance, AffordanceKind, PromptSummary};
    use serde_bytes::ByteBuf;
    use std::collections::BTreeSet;

    fn pick(card: u32) -> Affordance {
        Affordance {
            label: format!("card {card}"),
            hotkey: None,
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![card as u8]),
            card: Some(card),
        }
    }

    fn prompt(why: &str, seat: u8, cards: &[u32]) -> PluginView {
        PluginView {
            prompt: Some(PromptSummary {
                seat,
                why: why.into(),
                min: 0,
                max: 2,
                picked: 0,
                optional: true,
            }),
            affordances: cards.iter().map(|card| pick(*card)).collect(),
            ..Default::default()
        }
    }

    fn hand_of(ids: &[u32]) -> BTreeSet<u32> {
        ids.iter().copied().collect()
    }

    #[test]
    fn a_tap_raises_the_drawer_and_the_felt_swipe_and_drag_out_tuck_it() {
        use DrawerEvent::*;
        assert_eq!(
            drawer_next(DrawerState::Tucked, TapDrawer),
            DrawerState::Raised
        );
        assert_eq!(
            drawer_next(DrawerState::Raised, TapDrawer),
            DrawerState::Raised
        );
        assert_eq!(
            drawer_next(DrawerState::Raised, TapFelt),
            DrawerState::Tucked
        );
        assert_eq!(
            drawer_next(DrawerState::Raised, SwipeDown),
            DrawerState::Tucked
        );
        assert_eq!(
            drawer_next(DrawerState::Tucked, SwipeUp),
            DrawerState::Raised
        );
        assert_eq!(
            drawer_next(DrawerState::Raised, DragOut),
            DrawerState::Tucked
        );
        assert_eq!(
            drawer_next(DrawerState::Raised, TurnEnded),
            DrawerState::Tucked
        );
        assert_eq!(
            drawer_next(DrawerState::Tucked, TurnEnded),
            DrawerState::Tucked
        );
    }

    #[test]
    fn swipes_are_classified_by_their_vertical_travel() {
        assert_eq!(
            swipe_event(Vec2::new(4.0, -40.0), SWIPE_TUCK),
            Some(DrawerEvent::SwipeUp)
        );
        assert_eq!(
            swipe_event(Vec2::new(-6.0, 30.0), SWIPE_TUCK),
            Some(DrawerEvent::SwipeDown)
        );
        assert_eq!(swipe_event(Vec2::new(80.0, 30.0), SWIPE_TUCK), None);
        assert_eq!(swipe_event(Vec2::new(0.0, 10.0), SWIPE_TUCK), None);
    }

    #[test]
    fn the_mulligan_fixture_raises_the_drawer_and_the_target_fixture_tucks_it() {
        let mulligan = prompt("set aside up to 2 cards to redraw", 0, &[11, 12, 13]);
        let hand = hand_of(&[11, 12, 13, 14]);
        assert_eq!(
            prompt_event(&mulligan, 0, &hand),
            Some(DrawerEvent::PromptInHand)
        );
        let target = prompt("Vi: choose a unit to stun", 0, &[41, 42]);
        assert_eq!(
            prompt_event(&target, 0, &hand),
            Some(DrawerEvent::PromptOnBoard)
        );
        assert_eq!(prompt_event(&target, 1, &hand), None);
        let theirs = prompt("choose", 1, &[41]);
        assert_eq!(prompt_event(&theirs, 0, &hand), None);
        let faceless = PluginView {
            prompt: Some(PromptSummary {
                seat: 0,
                why: "pay 2 energy?".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            ..Default::default()
        };
        assert_eq!(prompt_event(&faceless, 0, &hand), None);
        assert_eq!(turn_event(true, false), Some(DrawerEvent::TurnEnded));
        assert_eq!(turn_event(false, true), None);
        assert_eq!(turn_event(true, true), None);
    }

    #[test]
    fn a_drag_past_the_drawer_top_tucks_only_a_raised_phone_drawer() {
        assert!(drag_out(
            ViewportClass::PhonePortrait,
            DrawerState::Raised,
            500.0,
            612.0
        ));
        assert!(!drag_out(
            ViewportClass::PhonePortrait,
            DrawerState::Raised,
            700.0,
            612.0
        ));
        assert!(!drag_out(
            ViewportClass::PhonePortrait,
            DrawerState::Tucked,
            500.0,
            728.0
        ));
        assert!(!drag_out(
            ViewportClass::Desktop,
            DrawerState::Raised,
            100.0,
            576.0
        ));
    }

    #[test]
    fn hand_visible_counts_the_cards_that_fit_the_band() {
        assert_eq!(hand_visible(ViewportClass::Desktop, 896.0, 1.06), 7);
        assert_eq!(hand_visible(ViewportClass::Tablet, 640.0, 1.06), 7);
        let raised = card_pitch(ViewportClass::PhonePortrait, DrawerState::Raised, 360.0);
        assert_eq!(raised, PHONE_CARD_W - PHONE_CARD_OVERLAP);
        assert_eq!(hand_visible(ViewportClass::PhonePortrait, 360.0, raised), 3);
        let tucked = card_pitch(ViewportClass::PhonePortrait, DrawerState::Tucked, 360.0);
        assert!(tucked < raised);
        assert_eq!(hand_visible(ViewportClass::PhonePortrait, 360.0, tucked), 5);
        let wide = card_pitch(ViewportClass::PhoneLandscape, DrawerState::Raised, 800.0);
        assert_eq!(hand_visible(ViewportClass::PhoneLandscape, 800.0, wide), 9);
        assert_eq!(hand_visible(ViewportClass::PhonePortrait, 100.0, raised), 1);
    }

    #[test]
    fn a_swipe_scrolls_the_drawer_by_card_pitch_and_never_past_the_hand() {
        let pitch = 84.0;
        let scrolled = swipe_scroll(0.0, -168.0, pitch, 9, 3);
        assert!((scrolled - 2.0).abs() < 1e-5);
        assert_eq!(swipe_scroll(5.0, -1000.0, pitch, 9, 3), 6.0);
        assert_eq!(swipe_scroll(1.0, 1000.0, pitch, 9, 3), 0.0);
        assert_eq!(scroll_clamp(4.0, 3, 5), 0.0);
        assert_eq!(swipe_scroll(1.0, 10.0, 0.0, 9, 3), 1.0);
        let band = Rect::from_min_max(
            bevy_egui::egui::pos2(0.0, 728.0),
            bevy_egui::egui::pos2(360.0, 800.0),
        );
        let first = drawer_card_center(band, DrawerState::Tucked, 62.0, 0, 0.0);
        let second = drawer_card_center(band, DrawerState::Tucked, 62.0, 1, 0.0);
        assert_eq!(second.x - first.x, 62.0);
        assert_eq!(first.y, 728.0 + PHONE_CARD_H / 2.0);
        assert_eq!(first.x, hud::GAP + PHONE_CARD_W / 2.0);
        let raised = drawer_card_center(band, DrawerState::Raised, 84.0, 0, 1.0);
        assert_eq!(raised.x, hud::GAP + PHONE_CARD_W / 2.0 - 84.0);
        assert!(raised.y > first.y - PHONE_CARD_H);
    }

    #[test]
    fn a_phone_hand_slot_lies_on_the_ray_through_its_screen_centre() {
        let window = Vec2::new(800.0, 360.0);
        let camera = Transform::from_xyz(0.0, 0.0, 0.0);
        let middle = phone_hand_slot(
            &camera,
            window,
            0.0,
            bevy_egui::egui::pos2(400.0, 180.0),
            5.0,
            0,
        );
        assert!((middle.position - Vec3::new(0.0, 0.0, -5.0)).length() < 1e-4);
        let right = phone_hand_slot(
            &camera,
            window,
            0.0,
            bevy_egui::egui::pos2(800.0, 180.0),
            5.0,
            0,
        );
        let half = (dim::FOV / 2.0).tan();
        assert!((right.position.x - 5.0 * half * (800.0 / 360.0)).abs() < 1e-4);
        assert!((right.position.z + 5.0).abs() < 1e-4);
        let bottom = phone_hand_slot(
            &camera,
            window,
            0.0,
            bevy_egui::egui::pos2(400.0, 360.0),
            5.0,
            0,
        );
        assert!((bottom.position.y + 5.0 * half).abs() < 1e-4);
        assert_eq!(middle.facing, Facing::Camera);
        let turned =
            Transform::from_xyz(1.0, 2.0, 3.0).looking_at(Vec3::new(1.0, 2.0, -10.0), Vec3::Y);
        let ahead = phone_hand_slot(
            &turned,
            window,
            0.0,
            bevy_egui::egui::pos2(400.0, 180.0),
            2.0,
            0,
        );
        assert!((ahead.position - Vec3::new(1.0, 2.0, 1.0)).length() < 1e-4);
    }

    #[test]
    fn a_tap_inside_the_drawer_band_is_not_a_felt_tap() {
        let band = Rect::from_min_max(
            bevy_egui::egui::pos2(0.0, 304.0),
            bevy_egui::egui::pos2(800.0, 360.0),
        );
        assert!(under_the_drawer(
            ViewportClass::PhoneLandscape,
            band,
            Vec2::new(400.0, 330.0)
        ));
        assert!(!under_the_drawer(
            ViewportClass::PhoneLandscape,
            band,
            Vec2::new(400.0, 200.0)
        ));
        assert!(!under_the_drawer(
            ViewportClass::Desktop,
            band,
            Vec2::new(400.0, 330.0)
        ));
    }

    #[test]
    fn the_phone_hand_depth_projects_a_card_at_96_dp() {
        let depth = card_depth(800.0, dim::FOV);
        let px = dim::CARD_W * 800.0 / (2.0 * depth * (dim::FOV / 2.0).tan());
        assert!((px - PHONE_CARD_W).abs() < 1e-3);
        assert_eq!(card_depth(0.0, dim::FOV), 10.0);
        assert!(double_tap(Some(1.0), 1.2));
        assert!(!double_tap(Some(1.0), 1.5));
        assert!(!double_tap(None, 1.0));
        let _ = CardFace::named("Sprite");
    }
}
