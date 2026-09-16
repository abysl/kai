use crate::table::{dim, seat_center_in, seat_yaw, Facing, Slot};
use agni_core::{CardId, PlayerId, Table, Zone};
use agni_sim::wire::{ZoneDecl, ZoneKind, ZoneLayout, ZoneOwner, ZonePlace, ZoneVisibility};
use bevy::math::{Quat, Vec2, Vec3};

pub const OUTER_Z: f32 = 0.625;
pub const INNER_Z: f32 = -1.175;
pub const ZONE_DEPTH: f32 = dim::CARD_H + 0.25;
pub const CENTER_DEPTH: f32 = dim::CARD_H + 0.15;
pub const ZONE_GAP: f32 = 0.12;
pub const SHARED_SLOT_MAX: f32 = 4.0;
pub const SPREAD_SPACING: f32 = 0.5;
pub const GRID_ROW_DEPTH: f32 = dim::CARD_H * 0.6;
pub const ATTACHED_PEEK: f32 = dim::CARD_H * 0.28;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneAnchor {
    pub zone: u16,
    pub seat: Option<PlayerId>,
    pub position: Vec3,
    pub yaw: f32,
    pub size: Vec2,
}

fn band(zones: &[ZoneDecl], place: ZonePlace, owner: ZoneOwner) -> Vec<&ZoneDecl> {
    zones
        .iter()
        .filter(|decl| decl.place == place && decl.owner == owner)
        .collect()
}

fn columns(band: &[&ZoneDecl], width: f32) -> Vec<(f32, f32)> {
    let total: f32 = band.iter().map(|decl| decl.span.max(1) as f32).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut cursor = -width / 2.0;
    band.iter()
        .map(|decl| {
            let slot = width * decl.span.max(1) as f32 / total;
            let center = cursor + slot / 2.0;
            cursor += slot;
            (center, slot)
        })
        .collect()
}

pub fn contested_in_play(zones: &[ZoneDecl], battlefields: usize) -> Vec<&ZoneDecl> {
    let band = band(zones, ZonePlace::Center, ZoneOwner::Shared);
    let declared = band
        .iter()
        .filter(|decl| decl.kind == ZoneKind::Battlefield)
        .count();
    let keep = battlefields.max(1).min(declared);
    let mut seen = 0usize;
    band.into_iter()
        .filter(|decl| {
            if decl.kind != ZoneKind::Battlefield {
                return true;
            }
            seen += 1;
            seen <= keep
        })
        .collect()
}

pub fn anchors(zones: &[ZoneDecl], players: usize, battlefields: usize) -> Vec<ZoneAnchor> {
    anchors_in(zones, players, battlefields, dim::quad_w())
}

pub fn anchors_in(
    zones: &[ZoneDecl],
    players: usize,
    battlefields: usize,
    quad_w: f32,
) -> Vec<ZoneAnchor> {
    let mut out = Vec::new();
    for (place, z) in [(ZonePlace::Inner, INNER_Z), (ZonePlace::Outer, OUTER_Z)] {
        let row = band(zones, place, ZoneOwner::PerSeat);
        let slots = columns(&row, quad_w);
        for seat in 0..players as u8 {
            let seat = PlayerId(seat);
            let center = seat_center_in(seat, players, quad_w);
            let yaw = seat_yaw(seat, players);
            let spin = Quat::from_rotation_y(yaw);
            for (decl, (x, slot_w)) in row.iter().zip(&slots) {
                out.push(ZoneAnchor {
                    zone: decl.id,
                    seat: Some(seat),
                    position: center + spin * Vec3::new(*x, 0.0, z),
                    yaw,
                    size: Vec2::new(slot_w - ZONE_GAP, ZONE_DEPTH),
                });
            }
        }
    }
    let contested = contested_in_play(zones, battlefields);
    let spans: f32 = contested.iter().map(|decl| decl.span.max(1) as f32).sum();
    let cols = players.div_ceil(2).max(1);
    let width = (quad_w * cols as f32).min(SHARED_SLOT_MAX * spans);
    for (decl, (x, slot_w)) in contested.iter().zip(columns(&contested, width)) {
        out.push(ZoneAnchor {
            zone: decl.id,
            seat: None,
            position: Vec3::new(x, 0.0, 0.0),
            yaw: 0.0,
            size: Vec2::new(slot_w - ZONE_GAP, CENTER_DEPTH),
        });
    }
    out
}

pub(crate) fn zone_slot(
    layout: ZoneLayout,
    anchor: &ZoneAnchor,
    i: usize,
    n: usize,
    rotated: bool,
) -> Slot {
    let spin = Quat::from_rotation_y(anchor.yaw);
    let rot = if rotated {
        std::f32::consts::FRAC_PI_2
    } else {
        0.0
    };
    let local = match layout {
        ZoneLayout::Pile | ZoneLayout::Fan => {
            Vec3::new(0.0, dim::BOARD_Y + i as f32 * dim::STACK_STEP, 0.0)
        }
        ZoneLayout::Row | ZoneLayout::Spread => {
            let preferred = if layout == ZoneLayout::Row {
                dim::BOARD_SPACING
            } else {
                SPREAD_SPACING
            };
            let spacing = if n > 1 {
                preferred.min((anchor.size.x - dim::CARD_W).max(0.0) / (n as f32 - 1.0))
            } else {
                0.0
            };
            Vec3::new(
                (i as f32 - (n as f32 - 1.0) / 2.0) * spacing,
                dim::BOARD_Y + i as f32 * dim::STACK_STEP,
                0.0,
            )
        }
        ZoneLayout::Grid => {
            let cols = ((n as f32).sqrt().ceil() as usize).max(1);
            let col_spacing = if cols > 1 {
                (dim::CARD_W * 1.05)
                    .min((anchor.size.x - dim::CARD_W).max(0.0) / (cols as f32 - 1.0))
            } else {
                0.0
            };
            let row = i / cols;
            let col = i % cols;
            Vec3::new(
                (col as f32 - (cols as f32 - 1.0) / 2.0) * col_spacing,
                dim::BOARD_Y + row as f32 * dim::STACK_STEP * 2.0,
                -(row as f32) * GRID_ROW_DEPTH,
            )
        }
    };
    Slot {
        position: anchor.position + spin * local,
        facing: Facing::Flat,
        yaw: anchor.yaw,
        rot,
    }
}

pub(crate) fn attached_slot(wearer: &Slot, i: usize, rotated: bool) -> Slot {
    let spin = Quat::from_rotation_y(wearer.yaw);
    let local = Vec3::new(
        0.0,
        -(i as f32 + 1.0) * dim::STACK_STEP,
        -(i as f32 + 1.0) * ATTACHED_PEEK,
    );
    Slot {
        position: wearer.position + spin * local,
        facing: wearer.facing,
        yaw: wearer.yaw,
        rot: if rotated {
            std::f32::consts::FRAC_PI_2
        } else {
            0.0
        },
    }
}

pub(crate) fn attachments(
    ids: &[CardId],
    attached_to: impl Fn(u32) -> Option<u32>,
) -> (Vec<CardId>, Vec<(CardId, CardId, usize)>) {
    let mut loose = Vec::with_capacity(ids.len());
    let mut worn = Vec::new();
    for id in ids {
        match attached_to(id.0).map(CardId) {
            Some(unit) if unit != *id && ids.contains(&unit) && attached_to(unit.0).is_none() => {
                let rank = worn.iter().filter(|(_, wearer, _)| *wearer == unit).count();
                worn.push((*id, unit, rank));
            }
            _ => loose.push(*id),
        }
    }
    (loose, worn)
}

pub fn fan_zone(zones: &[ZoneDecl]) -> Option<u16> {
    zones
        .iter()
        .find(|decl| decl.owner == ZoneOwner::PerSeat && decl.place == ZonePlace::Fan)
        .map(|decl| decl.id)
}

pub fn stack_zone(zones: &[ZoneDecl]) -> Option<u16> {
    zones
        .iter()
        .find(|decl| decl.kind == ZoneKind::Stack)
        .map(|decl| decl.id)
}

pub fn offstage_decl(zones: &[ZoneDecl]) -> Option<&ZoneDecl> {
    zones
        .iter()
        .find(|decl| decl.owner == ZoneOwner::PerSeat && decl.place == ZonePlace::Offstage)
}

pub fn offstage(zones: &[ZoneDecl], zone: Zone) -> bool {
    decl_of(zones, zone).is_some_and(|decl| decl.place == ZonePlace::Offstage)
}

pub fn swap_targets(zones: &[ZoneDecl]) -> Vec<&ZoneDecl> {
    zones
        .iter()
        .filter(|decl| {
            decl.owner == ZoneOwner::PerSeat
                && (decl.kind == ZoneKind::Deck || decl.place == ZonePlace::Fan)
        })
        .collect()
}

pub fn hand_zone(zones: &[ZoneDecl]) -> Zone {
    match fan_zone(zones) {
        Some(id) => Zone::Plugin(id),
        None => Zone::Hand,
    }
}

pub fn decl_of(zones: &[ZoneDecl], zone: Zone) -> Option<&ZoneDecl> {
    match zone {
        Zone::Plugin(id) => zones.iter().find(|decl| decl.id == id),
        _ => None,
    }
}

pub fn draw_move(
    zones: &[ZoneDecl],
    table: &Table,
    my_seat: PlayerId,
    hovered: CardId,
) -> Option<(CardId, Zone, usize)> {
    let card = table.get(hovered)?;
    let decl = decl_of(zones, card.zone)?;
    if decl.kind != ZoneKind::Deck {
        return None;
    }
    if decl.owner == ZoneOwner::PerSeat && card.seat != my_seat {
        return None;
    }
    let top = table.in_area(card.seat, card.zone).last()?.id;
    let dest = hand_zone(zones);
    let index = table.in_area(my_seat, dest).count();
    Some((top, dest, index))
}

pub fn trash_move(
    zones: &[ZoneDecl],
    table: &Table,
    my_seat: PlayerId,
    hovered: CardId,
) -> Option<(Zone, PlayerId, usize)> {
    let card = table.get(hovered)?;
    let decl = zones
        .iter()
        .find(|decl| decl.kind == ZoneKind::Discard && decl.owner == ZoneOwner::PerSeat)?;
    let dest = Zone::Plugin(decl.id);
    if card.zone == dest && card.seat == my_seat {
        return None;
    }
    let index = table.in_area(my_seat, dest).count();
    Some((dest, my_seat, index))
}

pub fn zone_line(decl: &ZoneDecl, count: usize) -> String {
    if decl.layout == ZoneLayout::Pile {
        format!("{} · {count}", decl.label)
    } else {
        decl.label.clone()
    }
}

pub fn deck_draw(
    zones: &[ZoneDecl],
    table: &Table,
    my_seat: PlayerId,
    prefer: Option<&str>,
) -> Option<(CardId, Zone, usize)> {
    let decks = zones
        .iter()
        .filter(|decl| decl.kind == ZoneKind::Deck && decl.owner == ZoneOwner::PerSeat);
    let decl = match prefer {
        Some(want) => decks.clone().find(|decl| decl.name == want),
        None => None,
    }
    .or_else(|| {
        decks
            .max_by_key(|decl| table.in_area(my_seat, Zone::Plugin(decl.id)).count())
            .filter(|decl| {
                table
                    .in_area(my_seat, Zone::Plugin(decl.id))
                    .next()
                    .is_some()
            })
    })?;
    let zone = Zone::Plugin(decl.id);
    let top = table.in_area(my_seat, zone).last()?.id;
    let dest = hand_zone(zones);
    let index = table.in_area(my_seat, dest).count();
    Some((top, dest, index))
}

pub fn is_hand(zones: &[ZoneDecl], zone: Zone) -> bool {
    match zone {
        Zone::Hand => true,
        Zone::Board => false,
        Zone::Plugin(_) => decl_of(zones, zone).is_some_and(|decl| decl.kind == ZoneKind::Hand),
    }
}

pub fn base_zone(zones: &[ZoneDecl]) -> Option<u16> {
    zones
        .iter()
        .find(|decl| decl.kind == ZoneKind::Battlefield && decl.owner == ZoneOwner::PerSeat)
        .map(|decl| decl.id)
}

pub fn is_public(zones: &[ZoneDecl], table: &Table, card: CardId) -> bool {
    table.get(card).is_some_and(|held| {
        agni_sim::wire::zone_visibility(zones, held.zone) == Some(ZoneVisibility::All)
    })
}

pub fn recycle_move(
    zones: &[ZoneDecl],
    table: &Table,
    my_seat: PlayerId,
    card: CardId,
) -> Option<(Zone, PlayerId, usize)> {
    let held = table.get(card)?;
    let from = decl_of(zones, held.zone)?;
    let rune_side = from.name.contains("rune");
    let decl = if rune_side {
        zones
            .iter()
            .find(|decl| decl.kind == ZoneKind::Deck && decl.name.contains("rune"))?
    } else {
        zones
            .iter()
            .find(|decl| decl.kind == ZoneKind::Discard && decl.owner == ZoneOwner::PerSeat)?
    };
    let dest = Zone::Plugin(decl.id);
    if held.zone == dest && held.seat == my_seat {
        return None;
    }
    let index = table.in_area(my_seat, dest).count();
    Some((dest, my_seat, index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::seat_center;
    use agni_sim::wire::ZoneVisibility;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn decl(id: u16, name: &str, kind: ZoneKind, owner: ZoneOwner, layout: ZoneLayout) -> ZoneDecl {
        let place = match owner {
            ZoneOwner::Shared => ZonePlace::Center,
            ZoneOwner::PerSeat if layout == ZoneLayout::Fan => ZonePlace::Fan,
            ZoneOwner::PerSeat => ZonePlace::Inner,
        };
        ZoneDecl {
            id,
            name: name.into(),
            kind,
            owner,
            visibility: ZoneVisibility::All,
            layout,
            place,
            span: 1,
            label: name.into(),
        }
    }

    fn anchor_for(anchors: &[ZoneAnchor], zone: u16, seat: Option<PlayerId>) -> ZoneAnchor {
        *anchors
            .iter()
            .find(|anchor| anchor.zone == zone && anchor.seat == seat)
            .unwrap()
    }

    fn seat_rows(zones: &[ZoneDecl]) -> usize {
        zones
            .iter()
            .filter(|decl| {
                decl.owner == ZoneOwner::PerSeat
                    && matches!(decl.place, ZonePlace::Inner | ZonePlace::Outer)
            })
            .count()
    }

    #[test]
    fn the_riftbound_table_places_every_zone_for_every_seat() {
        let zones = agni_riftbound::zone_table();
        for players in [2usize, 4] {
            let anchors = anchors(
                &zones,
                players,
                players.min(agni_riftbound::BATTLEFIELD_COUNT),
            );
            let per_seat = anchors
                .iter()
                .filter(|anchor| anchor.seat.is_some())
                .count();
            let shared = anchors
                .iter()
                .filter(|anchor| anchor.seat.is_none())
                .count();
            assert_eq!(seat_rows(&zones), 8, "seven zones and the banish pile");
            assert_eq!(per_seat, seat_rows(&zones) * players);
            assert_eq!(shared, players.min(agni_riftbound::BATTLEFIELD_COUNT));
            for anchor in &anchors {
                assert!(anchor.size.x > dim::CARD_W * 0.8);
            }
            assert!(anchors
                .iter()
                .all(|anchor| anchor.zone != agni_riftbound::ZONE_SIDEBOARD));
        }
    }

    #[test]
    fn the_riftbound_seat_stacks_each_deck_under_its_card() {
        let zones = agni_riftbound::zone_table();
        let anchors = anchors(&zones, 2, 2);
        let me = Some(PlayerId(0));
        let x = |zone: u16| anchor_for(&anchors, zone, me).position.x;
        let z = |zone: u16| anchor_for(&anchors, zone, me).position.z;
        let inner = [
            agni_riftbound::ZONE_BASE,
            agni_riftbound::ZONE_LEGEND,
            agni_riftbound::ZONE_CHAMPION,
        ];
        for pair in inner.windows(2) {
            assert!(x(pair[0]) < x(pair[1]));
        }
        let outer = [
            agni_riftbound::ZONE_TRASH,
            agni_riftbound::ZONE_BANISHMENT,
            agni_riftbound::ZONE_RUNE_POOL,
            agni_riftbound::ZONE_RUNE_DECK,
            agni_riftbound::ZONE_MAIN_DECK,
        ];
        for pair in outer.windows(2) {
            assert!(x(pair[0]) < x(pair[1]));
        }
        for far in inner {
            for near in outer {
                assert!(z(far) < z(near));
            }
        }
        let field = anchor_for(&anchors, agni_riftbound::ZONE_BATTLEFIELD_FIRST, None);
        for zone in inner {
            assert!(field.position.z < z(zone));
        }
        let base = anchor_for(&anchors, agni_riftbound::ZONE_BASE, me);
        let legend = anchor_for(&anchors, agni_riftbound::ZONE_LEGEND, me);
        let pool = anchor_for(&anchors, agni_riftbound::ZONE_RUNE_POOL, me);
        let rune_deck = anchor_for(&anchors, agni_riftbound::ZONE_RUNE_DECK, me);
        let main_deck = anchor_for(&anchors, agni_riftbound::ZONE_MAIN_DECK, me);
        let champion = anchor_for(&anchors, agni_riftbound::ZONE_CHAMPION, me);
        let trash = anchor_for(&anchors, agni_riftbound::ZONE_TRASH, me);
        let banished = anchor_for(&anchors, agni_riftbound::ZONE_BANISHMENT, me);
        assert!(base.size.x > legend.size.x);
        assert!(pool.size.x > rune_deck.size.x);
        assert!((trash.size.x - banished.size.x).abs() < 0.01);
        assert!(
            (rune_deck.position.x - legend.position.x).abs() < dim::CARD_W / 2.0,
            "the rune deck stacks under the legend: {} vs {}",
            rune_deck.position.x,
            legend.position.x
        );
        assert!(
            (main_deck.position.x - champion.position.x).abs() < dim::CARD_W / 2.0,
            "the main deck stacks under the champion: {} vs {}",
            main_deck.position.x,
            champion.position.x
        );
    }

    #[test]
    fn as_many_battlefields_are_laid_out_as_the_table_options_put_in_play() {
        let zones = agni_riftbound::zone_table();
        assert_eq!(
            contested_in_play(&zones, 9).len(),
            agni_riftbound::BATTLEFIELD_COUNT,
            "the contested band holds every declared battlefield and nothing else"
        );

        let in_play = |battlefields| contested_in_play(&zones, battlefields).len();
        assert_eq!(in_play(2), 2);
        assert_eq!(in_play(3), 3);
        assert_eq!(in_play(4), 3);
        assert_eq!(in_play(1), 1);
        assert_eq!(in_play(0), 1);
    }

    #[test]
    fn the_trimmed_battlefields_are_the_ones_declared_first() {
        let zones = agni_riftbound::zone_table();
        let kept = contested_in_play(&zones, 2);
        let names: Vec<&str> = kept.iter().map(|decl| decl.name.as_str()).collect();
        assert_eq!(names, vec!["battlefield-1", "battlefield-2"]);
    }

    #[test]
    fn a_two_seat_table_with_three_battlefields_lays_out_all_three() {
        let zones = agni_riftbound::zone_table();
        let contested = |players, battlefields| {
            anchors(&zones, players, battlefields)
                .into_iter()
                .filter(|anchor| anchor.seat.is_none())
                .count()
        };
        assert_eq!(contested(2, 2), 2);
        assert_eq!(contested(2, 3), 3);
        assert_eq!(contested(3, 3), 3);
        assert_eq!(contested(4, 3), 3);
        assert_eq!(contested(4, 2), 2);
        let info = crate::table::SessionInfo::default();
        assert_eq!(info.battlefields_in_play(2), 2);
        assert_eq!(info.battlefields_in_play(4), 3);
        let three = agni_riftbound::TableOptions {
            victory_score: 2,
            battlefields: 3,
        };
        let info = crate::table::SessionInfo {
            options: Some(three.encode()),
            ..Default::default()
        };
        assert_eq!(info.battlefields_in_play(2), 3);
        assert_eq!(
            contested(2, info.battlefields_in_play(2)),
            3,
            "a duel with three battlefields has a slot for each"
        );
        let third = anchors(&zones, 2, 3)
            .into_iter()
            .find(|anchor| anchor.zone == agni_riftbound::ZONE_BATTLEFIELD_FIRST + 2)
            .expect("the third battlefield is anchored");
        assert!(third.seat.is_none());
    }

    #[test]
    fn a_seats_own_battlefield_kind_zones_are_never_trimmed() {
        let zones = agni_riftbound::zone_table();
        for players in [2usize, 3, 4] {
            let per_seat = anchors(&zones, players, players)
                .into_iter()
                .filter(|anchor| anchor.seat.is_some())
                .count();
            assert_eq!(
                per_seat,
                seat_rows(&zones) * players,
                "base and the rune pool are per-seat, not contested battlefields"
            );
        }
    }

    #[test]
    fn the_two_seat_rows_never_overlap_each_other_or_the_contested_band() {
        let zones = agni_riftbound::zone_table();
        let anchors = anchors(&zones, 2, 2);
        let edge = |zone: u16, seat: Option<PlayerId>| {
            let anchor = anchor_for(&anchors, zone, seat);
            (
                anchor.position.z - anchor.size.y / 2.0,
                anchor.position.z + anchor.size.y / 2.0,
            )
        };
        let me = Some(PlayerId(0));
        let (inner_near, inner_far) = edge(agni_riftbound::ZONE_BASE, me);
        let (outer_near, _) = edge(agni_riftbound::ZONE_MAIN_DECK, me);
        let (_, field_far) = edge(agni_riftbound::ZONE_BATTLEFIELD_FIRST, None);
        assert!(inner_far < outer_near);
        assert!(field_far < inner_near);
    }

    #[test]
    fn an_offstage_zone_never_reaches_the_table() {
        let zones = agni_riftbound::zone_table();
        let sideboard = Zone::Plugin(agni_riftbound::ZONE_SIDEBOARD);
        assert!(offstage(&zones, sideboard));
        assert!(!offstage(&zones, Zone::Plugin(agni_riftbound::ZONE_BASE)));
        assert!(!offstage(&[], sideboard));
        assert_eq!(
            offstage_decl(&zones).map(|decl| decl.id),
            Some(agni_riftbound::ZONE_SIDEBOARD)
        );
        assert!(offstage_decl(&[]).is_none());
    }

    #[test]
    fn a_bare_draw_takes_from_the_deepest_deck_not_the_first_declared() {
        let zones = agni_riftbound::zone_table();
        let id = |name: &str| {
            zones
                .iter()
                .find(|decl| decl.name == name)
                .map(|decl| Zone::Plugin(decl.id))
                .unwrap()
        };
        let main = id(agni_riftbound::ZONE_NAME_MAIN_DECK);
        let rune = id(agni_riftbound::ZONE_NAME_RUNE_DECK);
        let mut table = Table::default();
        let mut put = |zone: Zone, count: usize| {
            for _ in 0..count {
                let id = agni_core::CardId(table.len() as u32);
                table.insert_card(agni_core::Card {
                    id,
                    owner: PlayerId(0),
                    seat: PlayerId(0),
                    zone,
                    face: agni_core::CardFace::hidden(),
                });
            }
        };
        put(rune, 12);
        put(main, 40);
        let (card, _, _) = deck_draw(&zones, &table, PlayerId(0), None).unwrap();
        assert_eq!(table.get(card).unwrap().zone, main);
        let (rune_card, _, _) = deck_draw(
            &zones,
            &table,
            PlayerId(0),
            Some(agni_riftbound::ZONE_NAME_RUNE_DECK),
        )
        .unwrap();
        assert_eq!(table.get(rune_card).unwrap().zone, rune);
    }

    #[test]
    fn swap_targets_are_the_seats_decks_and_its_fan() {
        let zones = agni_riftbound::zone_table();
        let ids: Vec<u16> = swap_targets(&zones).iter().map(|decl| decl.id).collect();
        assert!(ids.contains(&agni_riftbound::ZONE_HAND));
        assert!(ids.contains(&agni_riftbound::ZONE_MAIN_DECK));
        assert!(ids.contains(&agni_riftbound::ZONE_RUNE_DECK));
        assert!(!ids.contains(&agni_riftbound::ZONE_SIDEBOARD));
        assert!(!ids.contains(&agni_riftbound::ZONE_TRASH));
        assert!(swap_targets(&[]).is_empty());
    }

    #[test]
    fn opposing_seats_read_facing_each_other() {
        let zones = agni_riftbound::zone_table();
        for (players, far) in [(2usize, 1u8), (4, 2)] {
            let anchors = anchors(
                &zones,
                players,
                players.min(agni_riftbound::BATTLEFIELD_COUNT),
            );
            let near = anchor_for(&anchors, agni_riftbound::ZONE_TRASH, Some(PlayerId(0)));
            let far = anchor_for(&anchors, agni_riftbound::ZONE_TRASH, Some(PlayerId(far)));
            assert_eq!(near.yaw, 0.0);
            assert_eq!(far.yaw, PI);
            let near_local = near.position - seat_center(PlayerId(0), players);
            let far_local = far.position - seat_center(far.seat.unwrap(), players);
            assert!((near_local.x + far_local.x).abs() < 1e-5);
            assert!((near_local.z + far_local.z).abs() < 1e-5);
        }
    }

    #[test]
    fn shared_zones_hold_the_table_center() {
        let zones = agni_riftbound::zone_table();
        for players in [2usize, 4] {
            let anchors = anchors(
                &zones,
                players,
                players.min(agni_riftbound::BATTLEFIELD_COUNT),
            );
            let shared: Vec<ZoneAnchor> = anchors
                .iter()
                .filter(|anchor| anchor.seat.is_none())
                .copied()
                .collect();
            let mean: f32 =
                shared.iter().map(|anchor| anchor.position.x).sum::<f32>() / shared.len() as f32;
            assert!(mean.abs() < 1e-4);
            for anchor in &shared {
                assert_eq!(anchor.position.z, 0.0);
                assert_eq!(anchor.yaw, 0.0);
            }
        }
    }

    #[test]
    fn piles_stack_upward_in_place() {
        let anchor = ZoneAnchor {
            zone: 0,
            seat: Some(PlayerId(0)),
            position: Vec3::new(2.0, 0.0, 4.0),
            yaw: 0.0,
            size: Vec2::new(1.8, ZONE_DEPTH),
        };
        let bottom = zone_slot(ZoneLayout::Pile, &anchor, 0, 3, false);
        let top = zone_slot(ZoneLayout::Pile, &anchor, 2, 3, false);
        assert_eq!(bottom.position.x, top.position.x);
        assert_eq!(bottom.position.z, top.position.z);
        assert!(top.position.y > bottom.position.y);
    }

    #[test]
    fn rows_order_along_the_seat_axis_at_both_rotations() {
        let zones = vec![decl(
            0,
            "row",
            ZoneKind::Aux,
            ZoneOwner::PerSeat,
            ZoneLayout::Row,
        )];
        for players in [2usize, 4] {
            for seat in 0..players as u8 {
                let seat = PlayerId(seat);
                let anchors = anchors(
                    &zones,
                    players,
                    players.min(agni_riftbound::BATTLEFIELD_COUNT),
                );
                let anchor = anchor_for(&anchors, 0, Some(seat));
                let axis = Quat::from_rotation_y(seat_yaw(seat, players)) * Vec3::X;
                let ranks: Vec<f32> = (0..3)
                    .map(|i| {
                        zone_slot(ZoneLayout::Row, &anchor, i, 3, false)
                            .position
                            .dot(axis)
                    })
                    .collect();
                assert!(ranks[0] < ranks[1] && ranks[1] < ranks[2]);
            }
        }
    }

    #[test]
    fn spreads_compress_to_stay_inside_their_slot() {
        let anchor = ZoneAnchor {
            zone: 0,
            seat: None,
            position: Vec3::ZERO,
            yaw: 0.0,
            size: Vec2::new(4.0, ZONE_DEPTH),
        };
        for layout in [ZoneLayout::Row, ZoneLayout::Spread] {
            let n = 20;
            for i in 0..n {
                let slot = zone_slot(layout, &anchor, i, n, false);
                assert!(slot.position.x.abs() <= anchor.size.x / 2.0 + 1e-4);
            }
        }
    }

    #[test]
    fn grids_wrap_into_rows_that_step_inward() {
        let anchor = ZoneAnchor {
            zone: 0,
            seat: Some(PlayerId(0)),
            position: Vec3::new(0.0, 0.0, OUTER_Z),
            yaw: 0.0,
            size: Vec2::new(2.0, ZONE_DEPTH),
        };
        let n = 9;
        let first = zone_slot(ZoneLayout::Grid, &anchor, 0, n, false);
        let fourth = zone_slot(ZoneLayout::Grid, &anchor, 3, n, false);
        assert_eq!(first.position.x, fourth.position.x);
        assert!(fourth.position.z < first.position.z);
        let second = zone_slot(ZoneLayout::Grid, &anchor, 1, n, false);
        assert!(second.position.x > first.position.x);
    }

    #[test]
    fn an_exhausted_card_turns_ninety_degrees_in_place() {
        let anchor = ZoneAnchor {
            zone: 0,
            seat: None,
            position: Vec3::ZERO,
            yaw: 0.0,
            size: Vec2::new(4.0, ZONE_DEPTH),
        };
        let ready = zone_slot(ZoneLayout::Row, &anchor, 0, 1, false);
        let exhausted = zone_slot(ZoneLayout::Row, &anchor, 0, 1, true);
        assert_eq!(ready.position, exhausted.position);
        assert_eq!(ready.rot, 0.0);
        assert_eq!(exhausted.rot, FRAC_PI_2);
    }

    #[test]
    fn the_empty_zone_table_declares_nothing() {
        assert!(anchors(&[], 4, 3).is_empty());
        assert_eq!(hand_zone(&[]), Zone::Hand);
        assert_eq!(fan_zone(&[]), None);
    }

    #[test]
    fn the_riftbound_fan_is_the_hand_zone() {
        let zones = agni_riftbound::zone_table();
        assert_eq!(fan_zone(&zones), Some(agni_riftbound::ZONE_HAND));
        assert_eq!(hand_zone(&zones), Zone::Plugin(agni_riftbound::ZONE_HAND));
    }

    #[test]
    fn a_hovered_deck_draws_its_top_card_into_the_hand() {
        let zones = agni_riftbound::zone_table();
        let mut table = Table::new();
        let deck = Zone::Plugin(agni_riftbound::ZONE_MAIN_DECK);
        let me = PlayerId(0);
        let bottom = table.add(me, deck, "bottom", [128; 3]);
        let top = table.add(me, deck, "top", [128; 3]);
        table.add(
            me,
            Zone::Plugin(agni_riftbound::ZONE_HAND),
            "held",
            [128; 3],
        );
        let (card, dest, index) = draw_move(&zones, &table, me, bottom).unwrap();
        assert_eq!(card, top);
        assert_eq!(dest, Zone::Plugin(agni_riftbound::ZONE_HAND));
        assert_eq!(index, 1);
        let held = table
            .in_area(me, Zone::Plugin(agni_riftbound::ZONE_HAND))
            .next()
            .unwrap()
            .id;
        assert!(draw_move(&zones, &table, me, held).is_none());
        assert!(draw_move(&zones, &table, PlayerId(1), bottom).is_none());
    }

    #[test]
    fn a_hovered_card_routes_to_its_seats_trash() {
        let zones = agni_riftbound::zone_table();
        let mut table = Table::new();
        let me = PlayerId(0);
        let field = Zone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST);
        let played = table.add(me, field, "played", [128; 3]);
        table.add(
            me,
            Zone::Plugin(agni_riftbound::ZONE_TRASH),
            "gone",
            [128; 3],
        );
        let (dest, seat, index) = trash_move(&zones, &table, me, played).unwrap();
        assert_eq!(dest, Zone::Plugin(agni_riftbound::ZONE_TRASH));
        assert_eq!(seat, me);
        assert_eq!(index, 1);
        assert!(trash_move(&[], &table, me, played).is_none());
        let trashed = table.in_area(me, dest).next().unwrap().id;
        assert!(trash_move(&zones, &table, me, trashed).is_none());
    }

    #[test]
    fn the_banish_pile_sits_beside_the_trash_and_takes_no_hand_moves() {
        let zones = agni_riftbound::zone_table();
        let banished = zones
            .iter()
            .find(|decl| decl.id == agni_riftbound::ZONE_BANISHMENT)
            .unwrap();
        assert_eq!(banished.kind, ZoneKind::Discard);
        assert_eq!(zone_line(banished, 2), "Banished · 2");
        let anchors = anchors(&zones, 2, 2);
        for seat in [PlayerId(0), PlayerId(1)] {
            let trash = anchor_for(&anchors, agni_riftbound::ZONE_TRASH, Some(seat));
            let pile = anchor_for(&anchors, agni_riftbound::ZONE_BANISHMENT, Some(seat));
            assert!((trash.position.z - pile.position.z).abs() < 0.01);
            assert!((trash.position.x - pile.position.x).abs() > dim::CARD_W);
        }
        let mut table = Table::new();
        let me = PlayerId(0);
        let hand = table.add(me, Zone::Plugin(agni_riftbound::ZONE_HAND), "Defy", [0; 3]);
        let trash_zone = Zone::Plugin(agni_riftbound::ZONE_TRASH);
        assert_eq!(
            trash_move(&zones, &table, me, hand),
            Some((trash_zone, me, 0))
        );
        assert_eq!(
            recycle_move(&zones, &table, me, hand),
            Some((trash_zone, me, 0))
        );
        let gone = table.add(
            me,
            Zone::Plugin(agni_riftbound::ZONE_BANISHMENT),
            "Time Warp",
            [0; 3],
        );
        assert_eq!(
            trash_move(&zones, &table, me, gone),
            Some((trash_zone, me, 0)),
            "a banished card dragged by hand goes to the trash, never back into the pile"
        );
        assert!(is_public(&zones, &table, gone), "the pile shows its faces");
    }

    #[test]
    fn pile_lines_carry_the_count() {
        let deck = decl(
            0,
            "Runes",
            ZoneKind::Deck,
            ZoneOwner::PerSeat,
            ZoneLayout::Pile,
        );
        assert_eq!(zone_line(&deck, 12), "Runes · 12");
        let row = decl(
            1,
            "Legend",
            ZoneKind::Aux,
            ZoneOwner::PerSeat,
            ZoneLayout::Row,
        );
        assert_eq!(zone_line(&row, 1), "Legend");
    }

    #[test]
    fn attached_gear_leaves_the_row_and_tucks_in_under_its_wearer() {
        let ids: Vec<CardId> = [10, 11, 12, 13, 14].into_iter().map(CardId).collect();
        let attached_to = |card: u32| match card {
            12 => Some(10),
            13 => Some(10),
            14 => Some(99),
            _ => None,
        };
        let (loose, worn) = attachments(&ids, attached_to);
        assert_eq!(
            loose,
            [CardId(10), CardId(11), CardId(14)],
            "a gear whose wearer is elsewhere is laid out loose"
        );
        assert_eq!(
            worn,
            [(CardId(12), CardId(10), 0), (CardId(13), CardId(10), 1)]
        );
        let (loose, worn) = attachments(&ids, |card| match card {
            11 => Some(10),
            10 => Some(11),
            _ => None,
        });
        assert_eq!(loose.len(), 5, "a wearer that is itself worn wears nothing");
        assert!(worn.is_empty());

        let zones = agni_riftbound::zone_table();
        let anchors = anchors(&zones, 2, 2);
        let base = anchor_for(&anchors, agni_riftbound::ZONE_BASE, Some(PlayerId(1)));
        let wearer = zone_slot(ZoneLayout::Spread, &base, 0, 1, false);
        let first = attached_slot(&wearer, 0, false);
        let second = attached_slot(&wearer, 1, true);
        let spin = Quat::from_rotation_y(base.yaw);
        let expected = wearer.position + spin * Vec3::new(0.0, -dim::STACK_STEP, -ATTACHED_PEEK);
        assert!((first.position - expected).length() < 1e-5);
        assert!(
            first.position.y < wearer.position.y,
            "the gear lies under the unit"
        );
        assert_eq!(first.yaw, wearer.yaw);
        assert_eq!(first.facing, Facing::Flat);
        assert_eq!(first.rot, 0.0);
        assert_eq!(second.rot, std::f32::consts::FRAC_PI_2);
        let far = (second.position - wearer.position).length();
        let near = (first.position - wearer.position).length();
        assert!(far > near, "each further gear peeks out a step more");
        assert!(
            (second.position - wearer.position).length() < dim::CARD_H,
            "and stays under the wearer's silhouette"
        );
    }

    #[test]
    fn anchors_take_the_quad_width_as_a_parameter() {
        let zones = agni_riftbound::zone_table();
        let narrow = anchors_in(&zones, 2, 2, 8.0);
        let wide = anchors_in(&zones, 2, 2, 16.0);
        let trash_narrow = anchor_for(&narrow, agni_riftbound::ZONE_TRASH, Some(PlayerId(0)));
        let trash_wide = anchor_for(&wide, agni_riftbound::ZONE_TRASH, Some(PlayerId(0)));
        assert!((trash_wide.position.x - 2.0 * trash_narrow.position.x).abs() < 1e-4);
        assert!((trash_wide.size.x - 2.0 * trash_narrow.size.x - ZONE_GAP).abs() < 1e-4);
        assert_eq!(trash_narrow.position.z, trash_wide.position.z);
        let four = anchors_in(&zones, 4, 3, 8.0);
        let right = anchor_for(&four, agni_riftbound::ZONE_TRASH, Some(PlayerId(1)));
        let left = anchor_for(&four, agni_riftbound::ZONE_TRASH, Some(PlayerId(0)));
        assert!((right.position.x - left.position.x - 8.0).abs() < 1e-4);
    }
}
