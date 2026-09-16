use super::*;
use crate::viewport::ViewportClass;

pub(crate) fn columns_of(players: usize) -> f32 {
    players.div_ceil(2).max(1) as f32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    pub distance: f32,
    pub focus_z: f32,
    pub quad_w: f32,
    pub card_pt: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Rig {
    pub pitch: f32,
    pub distance: f32,
    pub focus_z: f32,
    pub aspect: f32,
    pub fov: f32,
}

impl Rig {
    fn eye(&self) -> Vec3 {
        Vec3::new(
            0.0,
            self.pitch.sin() * self.distance,
            self.focus_z + self.pitch.cos() * self.distance,
        )
    }

    fn forward(&self) -> Vec3 {
        Vec3::new(0.0, -self.pitch.sin(), -self.pitch.cos())
    }

    fn up(&self) -> Vec3 {
        Vec3::new(0.0, self.pitch.cos(), -self.pitch.sin())
    }

    pub fn depth(&self, point: Vec3) -> f32 {
        (point - self.eye()).dot(self.forward())
    }

    pub fn ndc(&self, point: Vec3) -> Vec2 {
        let offset = point - self.eye();
        let depth = offset.dot(self.forward()).max(1e-3);
        let half = (self.fov / 2.0).tan();
        Vec2::new(
            offset.x / (depth * half * self.aspect),
            offset.dot(self.up()) / (depth * half),
        )
    }
}

pub(crate) fn frame_edges() -> (f32, f32) {
    (-dim::QUAD_D, dim::QUAD_D - dim::FRAME_NEAR_INSET)
}

pub const FAR_CROP_MARGIN: f32 = 0.1;
pub const MIN_CARD_PT_PHONE: f32 = 56.0;
pub const MIN_CARD_PT_DESKTOP: f32 = 44.0;
pub const PITCH_PORTRAIT: f32 = 72.0;
pub const SOLVE_ROUNDS: usize = 14;
pub const BISECT_STEPS: usize = 40;

pub fn min_card_pt(class: ViewportClass) -> f32 {
    if class.is_phone() {
        MIN_CARD_PT_PHONE
    } else {
        MIN_CARD_PT_DESKTOP
    }
}

pub fn pitch_for(class: ViewportClass, pitch_deg: f32) -> f32 {
    match class {
        ViewportClass::PhonePortrait => PITCH_PORTRAIT,
        _ => pitch_deg,
    }
}

pub(crate) fn depth_edges(class: ViewportClass) -> (f32, f32) {
    let (far, near) = frame_edges();
    let far_inner = -base_card_z() - dim::CARD_H / 2.0 - FAR_CROP_MARGIN;
    let near_outer = dim::QUAD_D / 2.0 + zones::OUTER_Z + zones::ZONE_DEPTH / 2.0 + FAR_CROP_MARGIN;
    match class {
        ViewportClass::PhoneLandscape => (far_inner, near_outer),
        ViewportClass::PhonePortrait => (far, near_outer),
        _ => (far, near),
    }
}

pub(crate) fn base_card_z() -> f32 {
    dim::QUAD_D / 2.0 + zones::INNER_Z
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageNdc {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl StageNdc {
    pub const FULL: Self = Self {
        top: 1.0,
        bottom: -1.0,
        left: -1.0,
        right: 1.0,
    };

    pub fn of(window: Vec2, stage: egui::Rect) -> Self {
        if window.x <= 0.0 || window.y <= 0.0 {
            return Self::FULL;
        }
        let x = |px: f32| (px / window.x * 2.0 - 1.0).clamp(-1.0, 1.0);
        let y = |px: f32| (1.0 - px / window.y * 2.0).clamp(-1.0, 1.0);
        let out = Self {
            top: y(stage.min.y),
            bottom: y(stage.max.y),
            left: x(stage.min.x),
            right: x(stage.max.x),
        };
        if out.top - out.bottom < 0.05 || out.right - out.left < 0.05 {
            return Self::FULL;
        }
        out
    }

    pub fn span_y(&self) -> f32 {
        self.top - self.bottom
    }

    pub fn span_x(&self) -> f32 {
        self.right - self.left
    }
}

impl Default for StageNdc {
    fn default() -> Self {
        Self::FULL
    }
}

fn bisect(lo: f32, hi: f32, mut above: impl FnMut(f32) -> bool) -> f32 {
    let (mut lo, mut hi) = (lo, hi);
    let mut mid = (lo + hi) / 2.0;
    for _ in 0..BISECT_STEPS {
        mid = (lo + hi) / 2.0;
        if above(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    mid
}

fn pin_near(rig: &mut Rig, far: f32, near: f32, bottom: f32) {
    rig.focus_z = bisect(far - 2.0 * dim::QUAD_D, near + 2.0 * dim::QUAD_D, |focus| {
        rig.focus_z = focus;
        rig.ndc(Vec3::new(0.0, 0.0, near)).y < bottom
    });
}

fn fit_depth(rig: &mut Rig, far: f32, near: f32, stage: StageNdc) {
    for _ in 0..SOLVE_ROUNDS {
        rig.distance = bisect(0.5, 400.0, |distance| {
            rig.distance = distance;
            let top = rig.ndc(Vec3::new(0.0, 0.0, far)).y;
            let bottom = rig.ndc(Vec3::new(0.0, 0.0, near)).y;
            top - bottom > stage.span_y()
        });
        pin_near(rig, far, near, stage.bottom);
    }
}

fn fit_width(rig: &mut Rig, far: f32, near: f32, half_width: f32, stage: StageNdc) {
    for _ in 0..SOLVE_ROUNDS {
        rig.distance = bisect(0.5, 400.0, |distance| {
            rig.distance = distance;
            let right = rig.ndc(Vec3::new(half_width, 0.0, near)).x;
            let left = rig.ndc(Vec3::new(-half_width, 0.0, near)).x;
            right - left > stage.span_x()
        });
        pin_near(rig, far, near, stage.bottom);
    }
}

pub(crate) fn card_pt(rig: &Rig, window_h: f32, z: f32) -> f32 {
    let top = rig
        .ndc(Vec3::new(0.0, dim::BOARD_Y, z - dim::CARD_H / 2.0))
        .y;
    let bottom = rig
        .ndc(Vec3::new(0.0, dim::BOARD_Y, z + dim::CARD_H / 2.0))
        .y;
    (top - bottom) / 2.0 * window_h
}

fn pin_far(rig: &mut Rig, far: f32, near: f32, top: f32) {
    rig.focus_z = bisect(far - 2.0 * dim::QUAD_D, near + 2.0 * dim::QUAD_D, |focus| {
        rig.focus_z = focus;
        rig.ndc(Vec3::new(0.0, 0.0, far)).y < top
    });
}

pub fn crops_near(class: ViewportClass) -> bool {
    class == ViewportClass::PhoneLandscape
}

fn raise_to_floor(
    rig: &mut Rig,
    class: ViewportClass,
    far: f32,
    near: f32,
    stage: StageNdc,
    window_h: f32,
    floor: f32,
) {
    if floor <= 0.0 || window_h <= 0.0 {
        return;
    }
    for _ in 0..SOLVE_ROUNDS {
        let have = card_pt(rig, window_h, base_card_z());
        if have >= floor - 0.01 {
            return;
        }
        rig.distance = (rig.distance * have / floor).max(0.5);
        if crops_near(class) {
            pin_far(rig, far, near, stage.top);
        } else {
            pin_near(rig, far, near, stage.bottom);
        }
    }
}

pub(crate) fn framing(players: usize, aspect: f32, pitch_deg: f32, zoom: f32) -> Framing {
    framing_for(
        ViewportClass::Desktop,
        players,
        Vec2::new(aspect * 1000.0, 1000.0),
        StageNdc::FULL,
        pitch_deg,
        zoom,
        0.0,
    )
}

pub(crate) fn framing_for(
    class: ViewportClass,
    players: usize,
    window: Vec2,
    stage: StageNdc,
    pitch_deg: f32,
    zoom: f32,
    floor_pt: f32,
) -> Framing {
    let pitch = pitch_for(class, pitch_deg).to_radians();
    let aspect = if window.y > 0.0 {
        window.x / window.y
    } else {
        16.0 / 9.0
    };
    let (far, near) = depth_edges(class);
    let mut rig = Rig {
        pitch,
        distance: 10.0,
        focus_z: (far + near) / 2.0,
        aspect,
        fov: dim::FOV,
    };
    match class {
        ViewportClass::PhonePortrait => {
            let half_width = dim::QUAD_W_MIN / 2.0 + dim::FRAME_NEAR_INSET;
            fit_width(&mut rig, far, near, half_width, stage);
        }
        _ => fit_depth(&mut rig, far, near, stage),
    }
    raise_to_floor(&mut rig, class, far, near, stage, window.y, floor_pt);
    let half = (dim::FOV / 2.0).tan();
    let width = 2.0 * rig.depth(Vec3::new(0.0, 0.0, near)) * aspect * half * stage.span_x() / 2.0;
    let width = match class {
        ViewportClass::PhonePortrait => width - 2.0 * dim::FRAME_NEAR_INSET,
        _ => width,
    };
    Framing {
        distance: rig.distance * zoom,
        focus_z: rig.focus_z,
        quad_w: (width / columns_of(players)).max(dim::QUAD_W_MIN),
        card_pt: card_pt(&rig, window.y, base_card_z()),
    }
}

#[cfg(test)]
pub(crate) fn rig_of(framing: &Framing, aspect: f32, pitch_deg: f32) -> Rig {
    Rig {
        pitch: pitch_deg.to_radians(),
        distance: framing.distance,
        focus_z: framing.focus_z,
        aspect,
        fov: dim::FOV,
    }
}

pub(crate) fn camera_pose(distance: f32, pitch_deg: f32, focus: Vec3, yaw: f32) -> Transform {
    camera_pose_panned(distance, pitch_deg, focus, yaw, Vec2::ZERO)
}

pub(crate) fn camera_pose_panned(
    distance: f32,
    pitch_deg: f32,
    focus: Vec3,
    yaw: f32,
    pan: Vec2,
) -> Transform {
    let pitch = pitch_deg.to_radians();
    let spin = Quat::from_rotation_y(yaw);
    let focus = focus + spin * Vec3::new(pan.x, 0.0, pan.y);
    let offset = spin * Vec3::new(0.0, pitch.sin() * distance, pitch.cos() * distance);
    let up = spin * Vec3::new(0.0, pitch.cos(), -pitch.sin());
    Transform::from_translation(focus + offset).looking_at(focus, up)
}

pub(super) fn setup_scene(
    players: Res<PlayerCount>,
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: dim::FOV,
            ..default()
        }),
        camera_pose(
            camera::Extent::default().distance,
            tuning.pitch_deg,
            Vec3::ZERO,
            seat_yaw(PlayerId(0), players.0),
        ),
        #[cfg(target_os = "android")]
        Msaa::Off,
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            shadow_maps_enabled: cfg!(not(target_os = "android")),
            ..default()
        },
        Transform::from_xyz(4.0, 12.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        PointLight {
            intensity: 400_000.0,
            range: 40.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-5.0, 6.0, 5.0),
    ));

    commands.insert_resource(CardMesh(meshes.add(Cuboid::new(
        dim::CARD_W,
        dim::CARD_THICK,
        dim::CARD_H,
    ))));
    commands.insert_resource(CardMeshes {
        art: meshes.add(Plane3d::default().mesh().size(dim::CARD_W, dim::CARD_H)),
        wide_body: meshes.add(Cuboid::new(dim::CARD_H, dim::CARD_THICK, dim::CARD_W)),
        wide_art: meshes.add(Plane3d::default().mesh().size(dim::CARD_H, dim::CARD_W)),
    });
}

pub fn wheel_over_hand(pointer: Option<egui::Pos2>, hand: egui::Rect, shift: bool) -> bool {
    shift || pointer.is_some_and(|at| hand.contains(at))
}

pub(super) fn zoom_camera(
    mut wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    hud: Res<hud::Hud>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    mut scroll: ResMut<HandScroll>,
    mut tuning: ResMut<Tuning>,
) {
    let scrolled: f32 = wheel
        .read()
        .map(|w| match w.unit {
            MouseScrollUnit::Line => w.y * dim::ZOOM_LINE_STEP,
            MouseScrollUnit::Pixel => w.y * dim::ZOOM_PIXEL_STEP,
        })
        .sum();
    if scrolled == 0.0 {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if context.egui_wants_pointer_input() {
        return;
    }
    let pointer = context.input(|input| input.pointer.latest_pos());
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if wheel_over_hand(pointer, hud.0.hand, shift) {
        let hand = my_hand_ids(&table, &mirror, my_seat.0).len();
        let step = -scrolled / dim::ZOOM_LINE_STEP;
        scroll.0 = ui::hand_scroll_step(scroll.0, step, hand, dim::HAND_VISIBLE);
    } else {
        let live = tuning.bypass_change_detection();
        live.zoom = wheel_zoom(live.zoom, scrolled);
    }
}

pub fn wheel_zoom(zoom: f32, scrolled: f32) -> f32 {
    (zoom - scrolled).clamp(dim::ZOOM_MIN, dim::ZOOM_MAX)
}

pub fn pinch_zoom(zoom: f32, zoom_delta: f32) -> f32 {
    if !zoom_delta.is_finite() || zoom_delta <= 0.0 {
        return zoom;
    }
    (zoom / zoom_delta).clamp(dim::ZOOM_MIN, dim::ZOOM_MAX)
}

pub fn pan_step(pan: Vec2, delta: Vec2, distance: f32) -> Vec2 {
    let scale = distance * dim::PAN_SPEED;
    Vec2::new(
        (pan.x - delta.x * scale).clamp(-dim::PAN_LIMIT, dim::PAN_LIMIT),
        (pan.y - delta.y * scale).clamp(-dim::PAN_LIMIT, dim::PAN_LIMIT),
    )
}

pub(super) fn touch_camera(
    mut contexts: EguiContexts,
    extent: Res<camera::Extent>,
    mut tuning: ResMut<Tuning>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let Some(touch) = context.multi_touch() else {
        return;
    };
    if context.egui_wants_pointer_input() {
        return;
    }
    let zoom_delta = touch.zoom_delta;
    let translation = Vec2::new(touch.translation_delta.x, touch.translation_delta.y);
    if (zoom_delta - 1.0).abs() > 1e-4 {
        let live = tuning.bypass_change_detection();
        live.zoom = pinch_zoom(live.zoom, zoom_delta);
    }
    if translation != Vec2::ZERO {
        let next = pan_step(
            Vec2::new(tuning.pan_x, tuning.pan_z),
            translation,
            extent.distance,
        );
        tuning.pan_x = next.x;
        tuning.pan_z = next.y;
    }
}

pub(super) fn pan_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut contexts: EguiContexts,
    extent: Res<camera::Extent>,
    mut tuning: ResMut<Tuning>,
) {
    let dragging = buttons.pressed(MouseButton::Middle);
    let delta: Vec2 = motion.read().map(|m| m.delta).sum();
    if keys.just_pressed(KeyCode::Home) {
        tuning.pan_x = 0.0;
        tuning.pan_z = 0.0;
        return;
    }
    if !dragging || delta == Vec2::ZERO {
        return;
    }
    if contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_pointer_input())
        .unwrap_or(false)
    {
        return;
    }
    let next = pan_step(
        Vec2::new(tuning.pan_x, tuning.pan_z),
        delta,
        extent.distance,
    );
    tuning.pan_x = next.x;
    tuning.pan_z = next.y;
}

#[cfg(test)]
mod wheel_tests {
    use super::*;

    #[test]
    fn the_wheel_scrolls_the_hand_only_over_the_fan_or_with_shift() {
        let hand = egui::Rect::from_min_max(egui::pos2(192.0, 576.0), egui::pos2(1088.0, 800.0));
        assert!(wheel_over_hand(Some(egui::pos2(600.0, 700.0)), hand, false));
        assert!(!wheel_over_hand(
            Some(egui::pos2(600.0, 300.0)),
            hand,
            false
        ));
        assert!(wheel_over_hand(Some(egui::pos2(600.0, 300.0)), hand, true));
        assert!(!wheel_over_hand(None, hand, false));
        assert_eq!(wheel_zoom(1.0, 0.1), 0.9);
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    use crate::table::hud::{layout, DrawerState, Insets};

    fn phone_stage(class: ViewportClass, w: f32, h: f32) -> (Vec2, StageNdc) {
        let window = Vec2::new(w, h);
        let rects = layout(
            class,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h)),
            Insets::default(),
            DrawerState::Tucked,
            1,
            2,
        );
        (window, StageNdc::of(window, rects.frame))
    }

    fn rig_for(class: ViewportClass, fit: &Framing, window: Vec2) -> Rig {
        Rig {
            pitch: pitch_for(class, dim::PITCH_ARENA).to_radians(),
            distance: fit.distance,
            focus_z: fit.focus_z,
            aspect: window.x / window.y,
            fov: dim::FOV,
        }
    }

    #[test]
    fn a_board_card_meets_the_56_pt_floor_on_both_phones() {
        for (class, w, h) in [
            (ViewportClass::PhoneLandscape, 800.0, 360.0),
            (ViewportClass::PhonePortrait, 360.0, 800.0),
        ] {
            let (window, stage) = phone_stage(class, w, h);
            let plain = framing_for(class, 2, window, stage, dim::PITCH_ARENA, 1.0, 0.0);
            let fit = framing_for(
                class,
                2,
                window,
                stage,
                dim::PITCH_ARENA,
                1.0,
                min_card_pt(class),
            );
            assert_eq!(min_card_pt(class), MIN_CARD_PT_PHONE);
            assert!(
                fit.card_pt >= MIN_CARD_PT_PHONE - 0.05,
                "{class:?}: a base card renders {} pt",
                fit.card_pt
            );
            assert!(fit.distance <= plain.distance + 1e-3);
            let rig = rig_for(class, &fit, window);
            let (far, near) = depth_edges(class);
            let px = |z: f32| (1.0 - rig.ndc(Vec3::new(0.0, 0.0, z)).y) / 2.0 * h;
            let stage_bottom = (1.0 - stage.bottom) / 2.0 * h;
            let stage_top = (1.0 - stage.top) / 2.0 * h;
            if crops_near(class) {
                assert!(
                    (px(far) - stage_top).abs() < 1.5,
                    "{class:?}: the far edge stays pinned to the stage top ({} vs {stage_top})",
                    px(far)
                );
                let far_base_near_edge = -base_card_z() + dim::CARD_H / 2.0;
                assert!(
                    px(far_base_near_edge) > stage_top + 8.0,
                    "{class:?}: the far base is below the top bar ({} vs {stage_top})",
                    px(far_base_near_edge)
                );
            } else {
                assert!(
                    (px(near) - stage_bottom).abs() < 1.5,
                    "{class:?}: the near edge stays pinned to the stage bottom ({} vs {stage_bottom})",
                    px(near)
                );
            }
        }
    }

    #[test]
    fn the_portrait_fit_forces_the_steep_pitch_and_fits_nine_units_across() {
        let class = ViewportClass::PhonePortrait;
        let (window, stage) = phone_stage(class, 360.0, 800.0);
        assert_eq!(pitch_for(class, dim::PITCH_ARENA), PITCH_PORTRAIT);
        assert_eq!(pitch_for(ViewportClass::Desktop, 55.0), 55.0);
        let fit = framing_for(class, 2, window, stage, dim::PITCH_ARENA, 1.0, 0.0);
        assert_eq!(fit.quad_w, dim::QUAD_W_MIN);
        let rig = rig_for(class, &fit, window);
        let (_, near) = depth_edges(class);
        let half = dim::QUAD_W_MIN / 2.0 + dim::FRAME_NEAR_INSET;
        let right = rig.ndc(Vec3::new(half, 0.0, near)).x;
        assert!(
            (right - stage.right).abs() < 1e-2,
            "nine units across: {right}"
        );
        let raised = framing_for(
            class,
            2,
            window,
            stage,
            dim::PITCH_ARENA,
            1.0,
            MIN_CARD_PT_PHONE,
        );
        assert_eq!(raised.quad_w, dim::QUAD_W_MIN);
    }

    #[test]
    fn the_landscape_fit_crops_the_far_outer_band_and_keeps_the_battlefields() {
        let class = ViewportClass::PhoneLandscape;
        let (far, near) = depth_edges(class);
        assert!(
            far > -dim::QUAD_D,
            "the far seat's outer band is cropped: {far}"
        );
        assert!(
            near > dim::QUAD_D - dim::FRAME_NEAR_INSET - 1.0,
            "the near edge sits at my outer band: {near}"
        );
        let (window, stage) = phone_stage(class, 800.0, 360.0);
        let fit = framing_for(
            class,
            2,
            window,
            stage,
            dim::PITCH_ARENA,
            1.0,
            MIN_CARD_PT_PHONE,
        );
        let rig = rig_for(class, &fit, window);
        let px = |z: f32| (1.0 - rig.ndc(Vec3::new(0.0, 0.0, z)).y) / 2.0 * 360.0;
        let stage_top = (1.0 - stage.top) / 2.0 * 360.0;
        assert!(
            px(-zones::CENTER_DEPTH / 2.0) > stage_top + 8.0,
            "the battlefields' far edge is below the top bar"
        );
        let far_backs = rig
            .ndc(Vec3::new(0.0, 0.9, -(dim::QUAD_D / 2.0) - dim::HAND_NEAR))
            .y;
        assert!(
            far_backs > stage.top,
            "the far hand backs are off the top; the top bar carries the count"
        );
    }

    #[test]
    fn desktop_and_tablet_keep_the_window_fit() {
        for (class, w, h) in [
            (ViewportClass::Desktop, 1280.0, 800.0),
            (ViewportClass::Tablet, 1024.0, 768.0),
        ] {
            let window = Vec2::new(w, h);
            let fit = framing_for(
                class,
                2,
                window,
                StageNdc::FULL,
                dim::PITCH_ARENA,
                1.0,
                min_card_pt(class),
            );
            let plain = framing(2, w / h, dim::PITCH_ARENA, 1.0);
            assert!((fit.distance - plain.distance).abs() < 1e-3);
            assert!((fit.focus_z - plain.focus_z).abs() < 1e-3);
            assert!(fit.card_pt > MIN_CARD_PT_DESKTOP);
            assert_eq!(min_card_pt(class), MIN_CARD_PT_DESKTOP);
        }
    }

    #[test]
    fn the_stage_maps_to_ndc_and_degenerate_stages_fall_back_to_the_window() {
        let window = Vec2::new(800.0, 400.0);
        let stage = StageNdc::of(
            window,
            egui::Rect::from_min_max(egui::pos2(0.0, 100.0), egui::pos2(800.0, 300.0)),
        );
        assert_eq!(stage.top, 0.5);
        assert_eq!(stage.bottom, -0.5);
        assert_eq!(stage.left, -1.0);
        assert_eq!(stage.right, 1.0);
        assert_eq!(
            StageNdc::of(
                window,
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 2.0))
            ),
            StageNdc::FULL
        );
        assert_eq!(
            StageNdc::of(Vec2::ZERO, egui::Rect::EVERYTHING),
            StageNdc::FULL
        );
    }

    #[test]
    fn pinch_and_two_finger_pan_stay_inside_the_camera_limits() {
        assert!((pinch_zoom(1.0, 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(pinch_zoom(1.0, 0.1), dim::ZOOM_MAX);
        assert_eq!(pinch_zoom(1.0, 0.0), 1.0);
        assert_eq!(pinch_zoom(1.0, f32::NAN), 1.0);
        let panned = pan_step(Vec2::ZERO, Vec2::new(100.0, -50.0), 10.0);
        assert!(panned.x < 0.0 && panned.y > 0.0);
        let far = pan_step(Vec2::ZERO, Vec2::new(1.0e9, 0.0), 10.0);
        assert_eq!(far.x, -dim::PAN_LIMIT);
    }
}
