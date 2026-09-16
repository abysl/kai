use super::*;
use agni_sim::wire::ChainRow;

pub const BEAT_SECS: f32 = 0.45;
pub const BEAT_LIFT: f32 = 0.35;
pub const BEAT_SCALE: f32 = 0.12;
pub const BEAT_RING: f32 = 2.2;
pub const BEAT_STAGGER: f32 = 0.12;
pub const BEATS_KEPT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    pub card: u32,
    pub seat: u8,
    pub started: f32,
}

#[derive(Resource, Debug, Default)]
pub struct Beats {
    pub live: Vec<Beat>,
    pub chain: Vec<ChainRow>,
}

pub fn resolved_between(before: &[ChainRow], after: &[ChainRow]) -> Vec<ChainRow> {
    before
        .iter()
        .filter(|row| !after.iter().any(|kept| kept.item == row.item))
        .cloned()
        .collect()
}

pub fn beat_progress(started: f32, now: f32) -> Option<f32> {
    let age = now - started;
    (0.0..BEAT_SECS).contains(&age).then(|| age / BEAT_SECS)
}

pub fn beat_arc(progress: f32) -> f32 {
    (progress.clamp(0.0, 1.0) * std::f32::consts::PI).sin()
}

pub fn beat_lift(progress: f32) -> f32 {
    beat_arc(progress) * BEAT_LIFT
}

pub fn beat_scale(progress: f32) -> f32 {
    1.0 + beat_arc(progress) * BEAT_SCALE
}

impl Beats {
    pub fn push(&mut self, resolved: &[ChainRow], now: f32, fast: bool) {
        if fast {
            return;
        }
        let mut delay = 0.0;
        for row in resolved {
            let Some(card) = row.card else {
                continue;
            };
            self.live.push(Beat {
                card,
                seat: row.seat,
                started: now + delay,
            });
            delay += BEAT_STAGGER;
        }
        if self.live.len() > BEATS_KEPT {
            let extra = self.live.len() - BEATS_KEPT;
            self.live.drain(..extra);
        }
    }

    pub fn interrupt(&mut self) {
        self.live.clear();
    }

    pub fn prune(&mut self, now: f32) {
        self.live.retain(|beat| now - beat.started < BEAT_SECS);
    }

    pub fn progress(&self, card: u32, now: f32) -> Option<f32> {
        self.live
            .iter()
            .filter(|beat| beat.card == card)
            .find_map(|beat| beat_progress(beat.started, now))
    }

    pub fn is_idle(&self) -> bool {
        self.live.is_empty()
    }
}

pub(super) fn queue_beats(
    time: Res<Time>,
    tuning: Res<Tuning>,
    panel: Res<plugin_ui::PluginPanel>,
    mut beats: ResMut<Beats>,
) {
    let now = time.elapsed_secs();
    beats.prune(now);
    if !panel.is_changed() {
        return;
    }
    let resolved = resolved_between(&beats.chain, &panel.view.chain);
    if !resolved.is_empty() {
        beats.push(&resolved, now, tuning.fast_anim);
    }
    if beats.chain != panel.view.chain {
        beats.chain = panel.view.chain.clone();
    }
}

pub(super) fn interrupt_beats(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut beats: ResMut<Beats>,
) {
    if beats.is_idle() {
        return;
    }
    let pressed = keys.get_just_pressed().next().is_some()
        || buttons.get_just_pressed().next().is_some()
        || touches.any_just_pressed();
    if pressed {
        beats.interrupt();
    }
}

pub(super) fn beat_rings_ui(
    mut contexts: EguiContexts,
    time: Res<Time>,
    beats: Res<Beats>,
    seats: hud::Seats,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(&CardView, &GlobalTransform, &ViewVisibility)>,
) -> Result {
    if beats.is_idle() {
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let now = time.elapsed_secs();
    let painter = contexts
        .ctx_mut()?
        .layer_painter(egui::LayerId::background());
    for beat in &beats.live {
        let Some(progress) = beat_progress(beat.started, now) else {
            continue;
        };
        let Some((_, transform, _)) = cards
            .iter()
            .find(|(view, _, visible)| view.0 .0 == beat.card && visible.get())
        else {
            continue;
        };
        let Ok(centre) = camera.world_to_viewport(camera_transform, transform.translation()) else {
            continue;
        };
        let Ok(edge) = camera.world_to_viewport(
            camera_transform,
            transform.translation() + Vec3::X * dim::CARD_W / 2.0,
        ) else {
            continue;
        };
        let radius = (edge - centre).length() * (1.0 + progress * BEAT_RING);
        let [r, g, b] = seats.label(PlayerId(beat.seat)).1;
        let alpha = ((1.0 - progress) * 200.0) as u8;
        painter.circle_stroke(
            egui::pos2(centre.x, centre.y),
            radius,
            egui::Stroke::new(
                3.0 * (1.0 - progress) + 1.0,
                egui::Color32::from_rgba_unmultiplied(r, g, b, alpha),
            ),
        );
    }
    Ok(())
}

pub(super) fn animate_cards(
    mut commands: Commands,
    time: Res<Time>,
    held: Res<Held>,
    tuning: Res<Tuning>,
    view: Res<ViewSeat>,
    players: Res<PlayerCount>,
    refusals: Res<toast::Refusals>,
    beats: Res<Beats>,
    plane: Res<layout::HandPlane>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut cards: Query<
        (
            Entity,
            &CardView,
            &Slot,
            Option<&Hovered>,
            Has<SnapToSlot>,
            &mut Transform,
        ),
        With<CardView>,
    >,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation();
    let screen_up = camera.up().as_vec3();
    let screen_right = camera.right().as_vec3();
    let t = 1.0 - (-tuning.ease_rate * time.delta_secs()).exp();
    let now = time.elapsed_secs_f64();
    let beat_now = time.elapsed_secs();

    let view_yaw = seat_yaw(view.0, players.0);
    let hand_anchor = seat_center(view.0, players.0)
        + Quat::from_rotation_y(view_yaw)
            * Vec3::new(0.0, tuning.hand_y, dim::HAND_NEAR + (tuning.hand_z - 3.4));
    for (entity, card, slot, hovered, snap, mut transform) in &mut cards {
        let (target_pos, anchor, facing, yaw, rot) = if held.card == Some(entity) {
            let base = held.target.unwrap_or(transform.translation);
            let pos = base + Vec3::Y * tuning.lift;
            (pos, pos, Facing::Camera, view_yaw, 0.0)
        } else if hovered.is_some() && held.card.is_none() {
            let lift = match slot.facing {
                Facing::Camera => screen_up * tuning.hand_rise,
                _ => Vec3::Y * tuning.hover_rise,
            };
            (
                slot.position + lift,
                anchor_of(plane.phone, slot, hand_anchor),
                slot.facing,
                slot.yaw,
                slot.rot,
            )
        } else {
            (
                slot.position,
                anchor_of(plane.phone, slot, hand_anchor),
                slot.facing,
                slot.yaw,
                slot.rot,
            )
        };

        if snap {
            transform.translation = target_pos;
            transform.rotation = rotation_for(facing, yaw, rot, anchor, eye);
            commands.entity(entity).remove::<SnapToSlot>();
        } else {
            transform.translation = transform.translation.lerp(target_pos, t);
            transform.rotation = transform
                .rotation
                .slerp(rotation_for(facing, yaw, rot, anchor, eye), t);
        }
        if let Some(offset) = refusals.shaking(card.0 .0, now) {
            transform.translation += screen_right * offset;
        }
        match beats.progress(card.0 .0, beat_now) {
            Some(progress) => {
                transform.translation += Vec3::Y * beat_lift(progress);
                transform.scale = Vec3::splat(beat_scale(progress));
            }
            None if transform.scale != Vec3::ONE => transform.scale = Vec3::ONE,
            None => {}
        }
    }
}

pub fn anchor_of(phone: bool, slot: &Slot, hand_anchor: Vec3) -> Vec3 {
    if phone && slot.facing == Facing::Camera {
        slot.position
    } else {
        hand_anchor
    }
}

pub(super) fn apply_foil_alpha(
    tuning: Res<Tuning>,
    foil_art: Query<&MeshMaterial3d<StandardMaterial>, With<FoilArt>>,
    foil_bodies: Query<&MeshMaterial3d<FoilMaterial>, With<FoilBody>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foil_materials: ResMut<Assets<FoilMaterial>>,
) {
    if !tuning.is_changed() {
        return;
    }
    for handle in &foil_art {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.base_color.set_alpha(tuning.foil_alpha);
        }
    }
    for handle in &foil_bodies {
        if let Some(mut material) = foil_materials.get_mut(&handle.0) {
            material.extension.strength = tuning.foil_strength;
            material.extension.frequency = tuning.foil_frequency;
            material.extension.spark_strength = tuning.foil_sparks;
            material.extension.cell_density = tuning.foil_spark_density;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(item: u16, card: Option<u32>) -> ChainRow {
        ChainRow {
            item,
            card,
            seat: 1,
        }
    }

    #[test]
    fn a_chain_item_that_leaves_between_views_is_a_beat_and_fast_animations_skip_it() {
        let before = vec![row(1, Some(10)), row(2, Some(11)), row(3, None)];
        let after = vec![row(2, Some(11))];
        let resolved = resolved_between(&before, &after);
        assert_eq!(resolved, vec![row(1, Some(10)), row(3, None)]);
        assert!(
            resolved_between(&after, &before).is_empty(),
            "additions are not beats"
        );
        let mut beats = Beats::default();
        beats.push(&resolved, 5.0, false);
        assert_eq!(
            beats.live.len(),
            1,
            "an ability without a card has no face to beat"
        );
        assert_eq!(beats.live[0].card, 10);
        assert_eq!(beats.progress(10, 5.0), Some(0.0));
        assert!(beats.progress(10, 5.0 + BEAT_SECS / 2.0).unwrap() > 0.49);
        assert_eq!(beats.progress(10, 5.0 + BEAT_SECS + 0.01), None);
        assert_eq!(beats.progress(11, 5.0), None);
        let mut fast = Beats::default();
        fast.push(&resolved, 5.0, true);
        assert!(fast.is_idle());
    }

    #[test]
    fn beats_are_interruptible_staggered_and_pruned() {
        let mut beats = Beats::default();
        let rows = vec![row(1, Some(1)), row(2, Some(2))];
        beats.push(&rows, 0.0, false);
        assert_eq!(beats.live[1].started, BEAT_STAGGER);
        beats.prune(BEAT_STAGGER + BEAT_SECS + 0.01);
        assert!(beats.is_idle());
        beats.push(&rows, 0.0, false);
        beats.interrupt();
        assert!(beats.is_idle());
        assert_eq!(beat_progress(1.0, 0.5), None);
        assert_eq!(beat_lift(0.0), 0.0);
        assert!((beat_lift(0.5) - BEAT_LIFT).abs() < 1e-5);
        assert!((beat_scale(1.0) - 1.0).abs() < 1e-5);
        assert!(beat_scale(0.5) > 1.1);
    }
}
