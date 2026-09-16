use super::*;
use std::f32::consts::PI;

#[test]
fn the_render_mirror_follows_folded_deltas() {
    use agni_sim::log::{fold_entry, LogAction, LogEntry, LogState, TableConfig};
    use agni_sim::view::{apply_deltas, diff_views, table_view};
    use agni_sim::wire::WireZone;
    use serde_bytes::ByteBuf;
    let mut state = LogState::new();
    let mut mirror = Mirror::default();
    let entries = vec![
        LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: None,
                    plugin: None,
                    zones: agni_riftbound::zone_table(),
                    options: None,
                    counters: Vec::new(),
                    despawn_any: false,
                },
            },
        ),
        LogEntry::new(
            1,
            0,
            LogAction::Deal {
                cards: vec![0, 1],
                to: WireZone::Plugin(agni_riftbound::ZONE_MAIN_DECK),
            },
        ),
        LogEntry::new(
            2,
            0,
            LogAction::Move {
                card: 0,
                to: WireZone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST),
                seat: 0,
                index: 0,
                hidden: false,
            },
        ),
        LogEntry::new(
            3,
            0,
            LogAction::Annotate {
                card: 0,
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![0xf5])),
            },
        ),
        LogEntry::new(
            4,
            0,
            LogAction::Annotate {
                card: 1,
                key: ATTACHED.into(),
                value: Some(ByteBuf::from(0u32.to_le_bytes().to_vec())),
            },
        ),
    ];
    for entry in &entries {
        let before = table_view(&state, 0);
        fold_entry(&mut state, entry).unwrap();
        let after = table_view(&state, 0);
        apply_deltas(&mut mirror.view, &diff_views(&before, &after));
    }
    assert_eq!(mirror.view.zones, agni_riftbound::zone_table());
    assert!(mirror.rotated(0));
    assert!(!mirror.rotated(1));
    let moved = mirror.view.cards.iter().find(|card| card.id == 0).unwrap();
    assert_eq!(
        moved.zone,
        WireZone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST)
    );
    let decked = mirror.view.cards.iter().find(|card| card.id == 1).unwrap();
    assert!(!decked.face_visible);
    assert_eq!(
        mirror.attached_to(1),
        Some(0),
        "the attached badge names the wearer in little-endian bytes"
    );
    assert_eq!(mirror.attached_to(0), None);
    assert_eq!(mirror.attached_to(7), None);
    mirror.view = TableView::default();
    assert_eq!(mirror.attached_to(1), None);
    assert!(!mirror.rotated(0));
    mirror.solo_exhausted.insert(0);
    assert!(mirror.rotated(0));
}

#[test]
fn artless_cards_carry_their_name_as_a_label() {
    let named = FaceKey {
        name: "Viktor, Herald of the Arcane".into(),
        foil: false,
        art: false,
    };
    assert!(wants_label(&named));
    assert_eq!(
        label_lines(&named.name),
        vec!["Viktor,", "Herald of", "the Arcane"]
    );
    let arted = FaceKey {
        name: "Viktor".into(),
        foil: false,
        art: true,
    };
    assert!(!wants_label(&arted));
    let hidden = FaceKey {
        name: String::new(),
        foil: false,
        art: false,
    };
    assert!(!wants_label(&hidden));
    assert_eq!(label_lines("Emberwing"), vec!["Emberwing"]);
    assert!(label_lines("").is_empty());
}

fn hand_center(tuning: &Tuning) -> Vec3 {
    seat_center(PlayerId(0), 2)
        + Vec3::new(0.0, tuning.hand_y, dim::HAND_NEAR + (tuning.hand_z - 3.4))
}

#[test]
fn the_frame_pins_the_far_edge_to_the_top_and_the_outer_band_to_the_bottom() {
    for pitch in [dim::PITCH_TOP_DOWN, dim::PITCH_ARENA, dim::PITCH_MIN] {
        let aspect = 16.0 / 9.0;
        let fit = framing(2, aspect, pitch, 1.0);
        let rig = rig_of(&fit, aspect, pitch);
        let (far, near) = frame_edges();
        let top = rig.ndc(Vec3::new(0.0, 0.0, far)).y;
        let bottom = rig.ndc(Vec3::new(0.0, 0.0, near)).y;
        assert!((top - 1.0).abs() < 1e-3, "pitch {pitch}: far edge at {top}");
        assert!(
            (bottom + 1.0).abs() < 1e-3,
            "pitch {pitch}: near band edge at {bottom}"
        );
        let near_corner = rig.ndc(Vec3::new(fit.quad_w / 2.0, 0.0, near)).x;
        if fit.quad_w > dim::QUAD_W_MIN {
            assert!(
                (near_corner - 1.0).abs() < 1e-3,
                "pitch {pitch}: near corner at {near_corner}"
            );
        }
        let far_corner = rig.ndc(Vec3::new(fit.quad_w / 2.0, 0.0, far)).x;
        if pitch < dim::PITCH_TOP_DOWN {
            assert!(
                far_corner < 0.98,
                "pitch {pitch}: the far side recedes ({far_corner})"
            );
        } else {
            assert!((far_corner - 1.0).abs() < 1e-3);
        }
    }
}

#[test]
fn the_arena_camera_hangs_the_hand_off_the_bottom_until_it_is_hovered() {
    let tuning = Tuning::default();
    let aspect = 16.0 / 9.0;
    let fit = framing(2, aspect, tuning.pitch_deg, 1.0);
    let rig = rig_of(&fit, aspect, tuning.pitch_deg);
    let resting = rig.ndc(hand_center(&tuning)).y;
    let pose = camera_pose(
        fit.distance,
        tuning.pitch_deg,
        Vec3::new(0.0, 0.0, fit.focus_z),
        0.0,
    );
    let raised = rig
        .ndc(hand_center(&tuning) + pose.up().as_vec3() * tuning.hand_rise)
        .y;
    assert!(
        resting < -0.55 && resting > -0.85,
        "resting hand centre at {resting}"
    );
    assert!(
        raised > -0.62 && raised < resting + 0.6,
        "hovered hand centre at {raised} from {resting} (distance {}, focus {})",
        fit.distance,
        fit.focus_z
    );
    let hand_depth = rig.depth(hand_center(&tuning));
    let board_depth = rig.depth(seat_center(PlayerId(0), 2));
    assert!(
        hand_depth < board_depth * 0.85,
        "the hand reads larger than the board"
    );
    let backs = seat_center(PlayerId(1), 2)
        + Quat::from_rotation_y(PI) * Vec3::new(0.0, 0.9, dim::HAND_NEAR);
    let top = rig.ndc(backs).y;
    assert!(
        top > 0.6 && top < 1.0,
        "opponent backs near the top at {top}"
    );
    assert!(
        rig.depth(backs) > board_depth,
        "the opponent's hand reads smaller"
    );
}

#[test]
fn the_far_seat_sees_the_same_table_spun_around() {
    let distance = 20.0;
    let near = camera_pose(
        distance,
        dim::PITCH_TOP_DOWN,
        Vec3::ZERO,
        seat_yaw(PlayerId(0), 2),
    );
    let far = camera_pose(
        distance,
        dim::PITCH_TOP_DOWN,
        Vec3::ZERO,
        seat_yaw(PlayerId(1), 2),
    );
    assert!((near.translation - far.translation).length() < 1e-3);
    assert!((near.up().as_vec3() + far.up().as_vec3()).length() < 1e-4);
    let tilted = camera_pose(distance, dim::PITCH_ARENA, Vec3::ZERO, 0.0);
    assert!(tilted.translation.z > 0.0 && tilted.translation.y > 0.0);
    assert!(tilted.forward().as_vec3().z < 0.0);
}

#[test]
fn a_narrow_window_keeps_the_mats_wide_enough() {
    let narrow = framing(2, 0.4, dim::PITCH_ARENA, 1.0);
    assert_eq!(narrow.quad_w, dim::QUAD_W_MIN);
    let wide = framing(2, 21.0 / 9.0, dim::PITCH_ARENA, 1.0);
    assert!(wide.quad_w > narrow.quad_w * 2.0);
}

#[test]
fn more_players_share_the_width_across_columns_and_zoom_scales_distance() {
    let aspect = 21.0 / 9.0;
    let two = framing(2, aspect, dim::PITCH_ARENA, 1.0);
    let four = framing(4, aspect, dim::PITCH_ARENA, 1.0);
    assert!(
        (four.quad_w * 2.0 - two.quad_w).abs() < 1e-3,
        "two {two:?} four {four:?}"
    );
    assert!(
        (four.distance - two.distance).abs() < 1e-3,
        "two {two:?} four {four:?}"
    );
    let backed = framing(2, aspect, dim::PITCH_ARENA, 1.5);
    assert!((backed.distance - two.distance * 1.5).abs() < 1e-3);
    assert_eq!(backed.focus_z, two.focus_z);
}

#[test]
fn seats_split_into_facing_rows() {
    for (players, near) in [(2, 1), (4, 2), (8, 4)] {
        for seat in 0..players {
            let yaw = seat_yaw(PlayerId(seat), players as usize);
            if (seat as usize) < near {
                assert_eq!(yaw, 0.0);
            } else {
                assert_eq!(yaw, PI);
            }
        }
    }
}

#[test]
fn far_row_board_mirrors_the_near_row() {
    let near = board_slot(PlayerId(0), 0, 2, 2);
    let far = board_slot(PlayerId(1), 0, 2, 2);
    let near_local = near.position - seat_center(PlayerId(0), 2);
    let far_local = far.position - seat_center(PlayerId(1), 2);
    assert!((near_local.x + far_local.x).abs() < 1e-5);
    assert!((near_local.z + far_local.z).abs() < 1e-5);
    assert!((near_local.y - far_local.y).abs() < 1e-5);
    let top = rotation_for(far.facing, far.yaw, far.rot, Vec3::ZERO, Vec3::ZERO) * Vec3::NEG_Z;
    assert!((top - Vec3::Z).length() < 1e-5);
}

#[test]
fn contested_slots_are_the_declared_battlefields_and_the_chain_is_offstage() {
    let zones = agni_riftbound::zone_table();
    let slots = contested_slots(&zones);
    assert_eq!(slots.len(), agni_riftbound::BATTLEFIELD_COUNT);
    assert_eq!(slots[0], agni_riftbound::ZONE_BATTLEFIELD_FIRST);
    assert!(!slots.contains(&agni_riftbound::ZONE_CHAIN));
    assert_eq!(zones::stack_zone(&zones), Some(agni_riftbound::ZONE_CHAIN));
    assert!(zones::anchors(&zones, 2, 2)
        .iter()
        .all(|anchor| anchor.zone != agni_riftbound::ZONE_CHAIN));
    assert!(contested_slots(&[]).is_empty());
}

#[test]
fn board_order_follows_the_seat_axis() {
    for players in [2usize, 4, 8] {
        for seat in 0..players as u8 {
            let seat = PlayerId(seat);
            let axis = Quat::from_rotation_y(seat_yaw(seat, players)) * Vec3::X;
            let ranks: Vec<f32> = (0..3)
                .map(|i| board_slot(seat, i, 3, players).position.dot(axis))
                .collect();
            assert!(ranks[0] < ranks[1] && ranks[1] < ranks[2]);
        }
    }
}

#[test]
fn cards_materialize_from_their_owners_side_of_the_table() {
    let tuning = Tuning::default();
    let near = deal_origin(PlayerId(0), 2, &tuning);
    let far = deal_origin(PlayerId(1), 2, &tuning);
    assert!(near.z > 0.0);
    assert!(far.z < 0.0);
    assert!((near.z + far.z).abs() < 1e-4);
    assert!((near.x - far.x).abs() < 1e-4);
    let hand = hand_slot(0, 1, 0.0, PlayerId(1), 2, &tuning);
    assert!((far - hand.position).length() < 2.0);
}

#[test]
fn billboards_stay_upright_from_the_far_seat() {
    for (players, view) in [(2usize, PlayerId(1)), (4, PlayerId(3)), (8, PlayerId(6))] {
        let focus = seat_center(view, players);
        let camera = camera_pose(8.0, 45.0, focus, seat_yaw(view, players));
        let slot = hand_slot(0, 1, 0.0, view, players, &Tuning::default());
        let rotation = rotation_for(
            slot.facing,
            slot.yaw,
            slot.rot,
            slot.position,
            camera.translation,
        );
        assert!((rotation * Vec3::X).dot(camera.rotation * Vec3::X) > 0.9);
        assert!((rotation * Vec3::NEG_Z).dot(camera.rotation * Vec3::Y) > 0.5);
    }
}
