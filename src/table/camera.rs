use super::scene::{framing_for, min_card_pt, StageNdc};
use super::{dim, hud, seat_yaw, PlayerCount, Tuning, ViewSeat};
use crate::viewport::Viewport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

pub const EDGE_MARGIN: f32 = 28.0;

#[derive(Resource, Debug)]
pub struct CameraLock {
    pub locked: bool,
}

impl Default for CameraLock {
    fn default() -> Self {
        Self { locked: true }
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub quad_w: f32,
    pub aspect: f32,
    pub distance: f32,
    pub focus_z: f32,
    pub pitch_deg: f32,
    pub card_pt: f32,
}

impl Default for Extent {
    fn default() -> Self {
        let framing = super::scene::framing(2, 16.0 / 9.0, dim::PITCH_ARENA, 1.0);
        Self {
            quad_w: framing.quad_w,
            aspect: 16.0 / 9.0,
            distance: framing.distance,
            focus_z: framing.focus_z,
            pitch_deg: dim::PITCH_ARENA,
            card_pt: framing.card_pt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Arena,
    TopDown,
}

impl Preset {
    pub const ALL: [Preset; 2] = [Preset::Arena, Preset::TopDown];

    pub fn pitch_deg(self) -> f32 {
        match self {
            Preset::Arena => dim::PITCH_ARENA,
            Preset::TopDown => dim::PITCH_TOP_DOWN,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Arena => "arena",
            Preset::TopDown => "top-down",
        }
    }

    pub fn of(pitch_deg: f32) -> Preset {
        if (pitch_deg - dim::PITCH_TOP_DOWN).abs() < (pitch_deg - dim::PITCH_ARENA).abs() {
            Preset::TopDown
        } else {
            Preset::Arena
        }
    }

    pub fn apply(self, tuning: &mut Tuning) {
        tuning.pitch_deg = self.pitch_deg();
        tuning.pan_x = 0.0;
        tuning.pan_z = 0.0;
    }
}

pub fn fit_table(
    players: Res<PlayerCount>,
    tuning: Res<Tuning>,
    viewport: Res<Viewport>,
    hud: Res<hud::Hud>,
    mut extent: ResMut<Extent>,
) {
    let window = viewport.logical;
    if window.x <= 0.0 || window.y <= 0.0 {
        return;
    }
    let aspect = window.x / window.y;
    let class = viewport.class;
    let stage = if class.is_phone() {
        StageNdc::of(window, hud.0.frame)
    } else {
        StageNdc::of(
            window,
            bevy_egui::egui::Rect::from_min_size(
                hud.0.safe.min,
                bevy_egui::egui::vec2(hud.0.safe.width(), hud.0.safe.height()),
            ),
        )
    };
    let framing = framing_for(
        class,
        players.0,
        window,
        stage,
        tuning.pitch_deg,
        tuning.zoom,
        min_card_pt(class),
    );
    let next = Extent {
        quad_w: framing.quad_w,
        aspect,
        distance: framing.distance,
        focus_z: framing.focus_z,
        pitch_deg: super::scene::pitch_for(class, tuning.pitch_deg),
        card_pt: framing.card_pt,
    };
    let same = |a: f32, b: f32| (a - b).abs() < 1e-4;
    if same(extent.aspect, next.aspect)
        && same(extent.quad_w, next.quad_w)
        && same(extent.distance, next.distance)
        && same(extent.focus_z, next.focus_z)
        && same(extent.pitch_deg, next.pitch_deg)
    {
        return;
    }
    *extent = next;
    dim::set_quad_w(next.quad_w);
}

#[derive(Resource, Default, Debug)]
pub struct EdgeDrift {
    pub push: Vec2,
}

pub fn drift_push(cursor: Option<Vec2>, size: Vec2, margin: f32, last: Vec2) -> Vec2 {
    match cursor {
        Some(cursor) => edge_push(cursor, size, margin),
        None => last,
    }
}

pub fn edge_push(cursor: Vec2, size: Vec2, margin: f32) -> Vec2 {
    let mut push = Vec2::ZERO;
    if cursor.x <= margin {
        push.x = -(1.0 - cursor.x / margin).clamp(0.0, 1.0);
    } else if cursor.x >= size.x - margin {
        push.x = (1.0 - (size.x - cursor.x) / margin).clamp(0.0, 1.0);
    }
    if cursor.y <= margin {
        push.y = -(1.0 - cursor.y / margin).clamp(0.0, 1.0);
    } else if cursor.y >= size.y - margin {
        push.y = (1.0 - (size.y - cursor.y) / margin).clamp(0.0, 1.0);
    }
    push
}

pub fn toggle_camera_lock(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    mut lock: ResMut<CameraLock>,
    mut tuning: ResMut<Tuning>,
    menu: Res<crate::menu::Menu>,
) {
    if !menu.at_table() || !keys.just_pressed(KeyCode::KeyL) {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    lock.locked = !lock.locked;
    if lock.locked {
        tuning.pan_x = 0.0;
        tuning.pan_z = 0.0;
    }
}

pub fn edge_pan(
    time: Res<Time>,
    lock: Res<CameraLock>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut contexts: EguiContexts,
    mut drift: ResMut<EdgeDrift>,
    mut tuning: ResMut<Tuning>,
) {
    if lock.locked || tuning.camera_speed <= 0.0 {
        drift.push = Vec2::ZERO;
        return;
    }
    let Ok(window) = windows.single() else {
        drift.push = Vec2::ZERO;
        return;
    };
    if !window.focused {
        drift.push = Vec2::ZERO;
        return;
    }
    let cursor = window.cursor_position();
    if cursor.is_some()
        && contexts
            .ctx_mut()
            .map(|ctx| ctx.egui_wants_pointer_input())
            .unwrap_or(false)
    {
        drift.push = Vec2::ZERO;
        return;
    }
    let size = Vec2::new(window.width(), window.height());
    let push = drift_push(cursor, size, EDGE_MARGIN, drift.push);
    drift.push = push;
    if push == Vec2::ZERO {
        return;
    }
    let step = tuning.camera_speed * time.delta_secs();
    tuning.pan_x = (tuning.pan_x + push.x * step).clamp(-dim::PAN_LIMIT, dim::PAN_LIMIT);
    tuning.pan_z = (tuning.pan_z + push.y * step).clamp(-dim::PAN_LIMIT, dim::PAN_LIMIT);
}

pub fn apply_camera(
    players: Res<PlayerCount>,
    tuning: Res<Tuning>,
    view: Res<ViewSeat>,
    lock: Res<CameraLock>,
    extent: Res<Extent>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    if !tuning.is_changed()
        && !view.is_changed()
        && !players.is_changed()
        && !lock.is_changed()
        && !extent.is_changed()
    {
        return;
    }
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    let yaw = seat_yaw(view.0, players.0);
    let focus = Quat::from_rotation_y(yaw) * Vec3::new(0.0, 0.0, extent.focus_z);
    *transform = super::scene::camera_pose_panned(
        extent.distance,
        extent.pitch_deg,
        focus,
        yaw,
        Vec2::new(tuning.pan_x, tuning.pan_z),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_off_the_window_keeps_the_last_push() {
        let size = Vec2::new(1000.0, 800.0);
        let leaving = drift_push(Some(Vec2::new(0.0, 400.0)), size, 28.0, Vec2::ZERO);
        assert_eq!(leaving.x, -1.0);
        let gone = drift_push(None, size, 28.0, leaving);
        assert_eq!(gone, leaving);
        let still_gone = drift_push(None, size, 28.0, gone);
        assert_eq!(still_gone, leaving);
    }

    #[test]
    fn a_cursor_that_comes_back_inside_takes_over_from_the_drift() {
        let size = Vec2::new(1000.0, 800.0);
        let drifting = Vec2::new(-1.0, 0.0);
        let back = drift_push(Some(Vec2::new(500.0, 400.0)), size, 28.0, drifting);
        assert_eq!(back, Vec2::ZERO);
        let other_edge = drift_push(Some(Vec2::new(1000.0, 400.0)), size, 28.0, drifting);
        assert_eq!(other_edge.x, 1.0);
    }

    #[test]
    fn leaving_from_the_middle_drifts_nowhere() {
        let size = Vec2::new(1000.0, 800.0);
        let middle = drift_push(Some(Vec2::new(500.0, 400.0)), size, 28.0, Vec2::ZERO);
        assert_eq!(middle, Vec2::ZERO);
        assert_eq!(drift_push(None, size, 28.0, middle), Vec2::ZERO);
    }

    #[test]
    fn a_locked_camera_ignores_the_pan_offset() {
        let focus = Vec3::ZERO;
        let yaw = seat_yaw(agni_core::PlayerId(0), 2);
        let locked = super::super::scene::camera_pose(8.0, 45.0, focus, yaw);
        let panned =
            super::super::scene::camera_pose_panned(8.0, 45.0, focus, yaw, Vec2::new(3.0, -2.0));
        let unpanned = super::super::scene::camera_pose_panned(8.0, 45.0, focus, yaw, Vec2::ZERO);
        assert_eq!(locked.translation, unpanned.translation);
        assert_ne!(locked.translation, panned.translation);
    }

    #[test]
    fn the_cursor_pushes_only_at_the_edges() {
        let size = Vec2::new(1000.0, 800.0);
        assert_eq!(edge_push(Vec2::new(500.0, 400.0), size, 28.0), Vec2::ZERO);
        assert_eq!(edge_push(Vec2::new(0.0, 400.0), size, 28.0).x, -1.0);
        assert_eq!(edge_push(Vec2::new(1000.0, 400.0), size, 28.0).x, 1.0);
        assert_eq!(edge_push(Vec2::new(500.0, 0.0), size, 28.0).y, -1.0);
        assert_eq!(edge_push(Vec2::new(500.0, 800.0), size, 28.0).y, 1.0);
    }

    #[test]
    fn the_push_ramps_up_towards_the_very_edge() {
        let size = Vec2::new(1000.0, 800.0);
        let near = edge_push(Vec2::new(27.0, 400.0), size, 28.0).x;
        let nearer = edge_push(Vec2::new(14.0, 400.0), size, 28.0).x;
        let hard = edge_push(Vec2::new(0.0, 400.0), size, 28.0).x;
        assert!(near > -0.1, "just inside the margin barely pushes: {near}");
        assert!(nearer < near);
        assert!(hard < nearer);
        assert!(hard >= -1.0);
    }

    #[test]
    fn a_corner_pushes_on_both_axes() {
        let size = Vec2::new(1000.0, 800.0);
        let corner = edge_push(Vec2::new(0.0, 800.0), size, 28.0);
        assert_eq!(corner, Vec2::new(-1.0, 1.0));
    }

    #[test]
    fn the_two_presets_map_to_the_arena_and_top_down_pitches() {
        assert_eq!(Preset::Arena.pitch_deg(), dim::PITCH_ARENA);
        assert_eq!(Preset::TopDown.pitch_deg(), dim::PITCH_TOP_DOWN);
        assert_eq!(Preset::of(62.0), Preset::Arena);
        assert_eq!(Preset::of(88.0), Preset::TopDown);
        assert_eq!(Preset::ALL.len(), 2);
        assert_eq!(Preset::Arena.label(), "arena");
        let mut tuning = Tuning {
            pan_x: 3.0,
            pan_z: -1.0,
            ..Tuning::default()
        };
        Preset::TopDown.apply(&mut tuning);
        assert_eq!(tuning.pitch_deg, dim::PITCH_TOP_DOWN);
        assert_eq!((tuning.pan_x, tuning.pan_z), (0.0, 0.0));
    }
}
