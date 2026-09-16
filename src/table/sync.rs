use super::*;
use bevy::asset::AssetId;

pub(super) fn sync_cards(
    mut commands: Commands,
    generation: Res<DealGeneration>,
    table: Res<GameTable>,
    my_seat: Res<MySeat>,
    players: Res<PlayerCount>,
    tuning: Res<Tuning>,
    card_mesh: Res<CardMesh>,
    mut held: ResMut<Held>,
    existing: Query<(Entity, &CardView, &FaceKey)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut foil_materials: ResMut<Assets<FoilMaterial>>,
    mut art: ResMut<art::ArtCache>,
    mirror: Res<Mirror>,
) {
    if !generation.is_changed() && !table.is_changed() && !my_seat.is_changed() && !art.is_changed()
    {
        return;
    }
    let back_name = net::game_of_zones(&mirror.view.zones)
        .art_game()
        .map(|game| game.back_name());
    let back_image = back_name.and_then(|name| art.image(name, &mut images));
    let force = generation.is_changed();
    let mut kept: Vec<CardId> = Vec::new();
    for (entity, view, key) in &existing {
        let keep = !force
            && table.get(view.0).is_some_and(|card| {
                card_shown(card, my_seat.0)
                    && *key == face_key(&drawn_face(card, &mirror), &art, back_name)
            });
        if keep {
            kept.push(view.0);
        } else {
            commands.entity(entity).despawn();
            if held.card == Some(entity) {
                held.card = None;
                held.target = None;
            }
        }
    }

    for real in table.cards() {
        if !card_shown(real, my_seat.0) || kept.contains(&real.id) {
            continue;
        }
        let drawn = drawn_face(real, &mirror);
        let card = &*drawn;
        let art_handle = if card.face.is_hidden() {
            back_image.clone()
        } else {
            art.image(&card.face.name, &mut images)
        };
        let card_art = art_handle.clone().map(CardArt);
        let start = deal_origin(card.owner, players.0, &tuning);
        let start_yaw = seat_yaw(card.owner, players.0);

        let mut entity = if card.face.foil && art_handle.is_some() {
            let mut art_color = Color::WHITE;
            art_color.set_alpha(tuning.foil_alpha);
            let art_material = materials.add(StandardMaterial {
                base_color: art_color,
                base_color_texture: art_handle,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            let body = foil_materials.add(FoilMaterial {
                base: StandardMaterial {
                    base_color: Color::srgb(0.04, 0.045, 0.06),
                    metallic: 1.0,
                    perceptual_roughness: 0.12,
                    reflectance: 1.0,
                    ..default()
                },
                extension: FoilExtension {
                    strength: tuning.foil_strength,
                    frequency: tuning.foil_frequency,
                    spark_strength: tuning.foil_sparks,
                    cell_density: tuning.foil_spark_density,
                    ..default()
                },
            });
            let mut entity = commands.spawn((
                Mesh3d(card_mesh.0.clone()),
                MeshMaterial3d(body),
                Transform::from_translation(start),
                CardView(card.id),
                FoilBody,
                Slot {
                    position: start,
                    facing: Facing::Camera,
                    yaw: start_yaw,
                    rot: 0.0,
                },
            ));
            entity.with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Plane3d::default().mesh().size(dim::CARD_W, dim::CARD_H))),
                    MeshMaterial3d(art_material),
                    Transform::from_xyz(0.0, dim::CARD_THICK / 2.0 + 0.001, 0.0),
                    FoilArt,
                    Pickable::IGNORE,
                ));
            });
            entity
        } else {
            let material = match art_handle {
                Some(handle) => StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(handle),
                    unlit: true,
                    ..default()
                },
                None => StandardMaterial {
                    base_color: if card.face.is_hidden() {
                        CARD_BACK
                    } else {
                        let [r, g, b] = card.face.tint;
                        Color::srgb_u8(r, g, b)
                    },
                    perceptual_roughness: 0.55,
                    ..default()
                },
            };
            commands.spawn((
                Mesh3d(card_mesh.0.clone()),
                MeshMaterial3d(materials.add(material)),
                Transform::from_translation(start),
                CardView(card.id),
                Slot {
                    position: start,
                    facing: Facing::Camera,
                    yaw: start_yaw,
                    rot: 0.0,
                },
            ))
        };
        if let Some(card_art) = card_art {
            entity.insert(card_art);
        }
        if force {
            entity.insert(SnapToSlot);
        }
        entity.insert(face_key(card, &art, back_name));
        entity
            .observe(on_hover_card)
            .observe(on_unhover_card)
            .observe(on_click_card)
            .observe(on_right_click_card)
            .observe(on_drag_start)
            .observe(on_drag_end);
    }
}

pub fn shown_at(at_table: bool, back_of_viewed_seat: bool) -> Visibility {
    if !at_table || back_of_viewed_seat {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

pub(super) fn hide_viewed_hand(
    view: Res<ViewSeat>,
    menu: Option<Res<crate::menu::Menu>>,
    mut backs: Query<(&OpponentHand, &mut Visibility)>,
    mut furniture: Query<&mut Visibility, (With<ZoneDecor>, Without<OpponentHand>)>,
) {
    let at_table = menu.as_deref().is_none_or(crate::menu::Menu::at_table);
    for (hand, mut visibility) in &mut backs {
        let wanted = shown_at(at_table, hand.0 == view.0);
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    let wanted = shown_at(at_table, false);
    for mut visibility in &mut furniture {
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

pub(super) fn sync_player_count(
    info: Res<SessionInfo>,
    mut players: ResMut<PlayerCount>,
    mut view: ResMut<ViewSeat>,
) {
    if info.active() {
        return;
    }
    let count = SOLO_SEATS;
    if players.0 != count {
        players.0 = count;
        if view.0 .0 as usize >= count {
            view.0 = PlayerId(0);
        }
    }
}

pub(super) fn sync_seats(
    mut commands: Commands,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    extent: Res<camera::Extent>,
    tuning: Res<Tuning>,
    library: Res<playmat::PlaymatLibrary>,
    mut art: ResMut<art::ArtCache>,
    mut images: ResMut<Assets<Image>>,
    existing: Query<Entity, With<SeatDecor>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut painted: Local<Option<(Vec<u8>, String, Vec<Option<AssetId<Image>>>)>>,
) {
    let wanted: Vec<u8> = (0..players.0 as u8)
        .map(|seat| colors::color_of(&info.roster, &seat_colors, my_seat.0, PlayerId(seat)))
        .collect();
    let mats: Vec<Option<(Handle<Image>, Vec2)>> = (0..players.0 as u8)
        .map(|seat| {
            let name =
                playmat::mat_of_seat(PlayerId(seat), my_seat.0, &tuning, &info.roster, &library)?;
            let handle = art.image(&name, &mut images)?;
            let size = images.get(&handle).map(|image| image.size_f32())?;
            Some((handle, size))
        })
        .collect();
    let signature = (
        wanted.clone(),
        tuning.playmat.clone(),
        mats.iter()
            .map(|mat| mat.as_ref().map(|(handle, _)| handle.id()))
            .collect::<Vec<_>>(),
    );
    if !players.is_changed() && !extent.is_changed() && painted.as_ref() == Some(&signature) {
        return;
    }
    *painted = Some(signature);
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for seat in 0..players.0 as u8 {
        let felt = colors::felt(wanted[seat as usize]);
        let seat = PlayerId(seat);
        let material = match &mats[seat.0 as usize] {
            Some((handle, size)) => StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(handle.clone()),
                uv_transform: playmat::cover_uv(
                    (size.x, size.y),
                    (extent.quad_w, dim::QUAD_D),
                    seat_yaw(seat, players.0) != 0.0,
                ),
                perceptual_roughness: 0.9,
                reflectance: 0.08,
                ..default()
            },
            None => StandardMaterial {
                base_color: felt,
                perceptual_roughness: 0.85,
                reflectance: 0.12,
                ..default()
            },
        };
        commands
            .spawn((
                Mesh3d(meshes.add(Plane3d::default().mesh().size(extent.quad_w, dim::QUAD_D))),
                MeshMaterial3d(materials.add(material)),
                Transform::from_translation(seat_center_in(seat, players.0, extent.quad_w)),
                DropSeat(seat),
                SeatDecor,
            ))
            .observe(on_drag_over_surface)
            .observe(on_drop_on_surface);
    }
}

pub(super) fn sync_zones(
    mut commands: Commands,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    mirror: Res<Mirror>,
    extent: Res<camera::Extent>,
    existing: Query<Entity, With<ZoneDecor>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cached: Local<Option<(Vec<agni_sim::wire::ZoneDecl>, usize, usize, u32)>>,
) {
    if !players.is_changed() && !info.is_changed() && !mirror.is_changed() && !extent.is_changed() {
        return;
    }
    let battlefields = info.battlefields_in_play(players.0);
    let current = (
        mirror.view.zones.clone(),
        players.0,
        battlefields,
        extent.quad_w.to_bits(),
    );
    if cached.as_ref() == Some(&current) {
        return;
    }
    *cached = Some(current);
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let plain = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.22),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let contested = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.16, 0.17),
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        ..default()
    });
    for anchor in zones::anchors_in(&mirror.view.zones, players.0, battlefields, extent.quad_w) {
        let material = if anchor.seat.is_none() {
            contested.clone()
        } else {
            plain.clone()
        };
        commands
            .spawn((
                Mesh3d(meshes.add(Plane3d::default().mesh().size(anchor.size.x, anchor.size.y))),
                MeshMaterial3d(material),
                Transform::from_translation(anchor.position + Vec3::Y * 0.01)
                    .with_rotation(Quat::from_rotation_y(anchor.yaw)),
                DropZone {
                    zone: anchor.zone,
                    seat: anchor.seat.unwrap_or(PlayerId(0)),
                    yaw: anchor.yaw,
                },
                ZoneDecor,
            ))
            .observe(on_drag_over_surface)
            .observe(on_drop_on_zone);
    }
}

pub(super) fn sync_hand_backs(
    mut commands: Commands,
    players: Res<PlayerCount>,
    table: Res<GameTable>,
    my_seat: Res<MySeat>,
    info: Res<SessionInfo>,
    view: Res<ViewSeat>,
    extent: Res<camera::Extent>,
    existing: Query<Entity, With<OpponentHand>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut art: ResMut<art::ArtCache>,
    mirror: Res<Mirror>,
    mut assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>, bool)>>,
) {
    if !players.is_changed()
        && !table.is_changed()
        && !my_seat.is_changed()
        && !info.is_changed()
        && !art.is_changed()
        && !extent.is_changed()
    {
        return;
    }
    let back_image = net::game_of_zones(&mirror.view.zones)
        .art_game()
        .and_then(|game| art.image(game.back_name(), &mut images));
    let textured = back_image.is_some();
    if assets.as_ref().is_some_and(|(_, _, was)| *was != textured) {
        *assets = None;
    }
    let (back_mesh, back_material, _) = assets
        .get_or_insert_with(|| {
            let material = match back_image {
                Some(handle) => StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(handle),
                    unlit: true,
                    ..default()
                },
                None => StandardMaterial {
                    base_color: CARD_BACK,
                    perceptual_roughness: 0.6,
                    ..default()
                },
            };
            (
                meshes.add(Cuboid::new(dim::CARD_W, dim::CARD_THICK, dim::CARD_H)),
                materials.add(material),
                textured,
            )
        })
        .clone();
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for seat in 0..players.0 as u8 {
        let seat = PlayerId(seat);
        if seat == my_seat.0 {
            continue;
        }
        let count = if info.active() {
            table.in_area(seat, Zone::Hand).count()
        } else {
            7
        };
        if count == 0 {
            continue;
        }
        let center = seat_center_in(seat, players.0, extent.quad_w);
        let spin = Quat::from_rotation_y(seat_yaw(seat, players.0));
        let hidden = view.0 == seat;
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * 0.82;
            commands.spawn((
                Mesh3d(back_mesh.clone()),
                MeshMaterial3d(back_material.clone()),
                Transform::from_translation(
                    center + spin * Vec3::new(x, 0.9 - x * x * 0.012, dim::HAND_NEAR),
                )
                .with_rotation(spin * Quat::from_rotation_x(dim::HAND_TILT - 0.5)),
                OpponentHand(seat),
                Pickable::IGNORE,
                if hidden {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
            ));
        }
    }
}

pub(super) fn orient_cards(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    card_mesh: Res<CardMesh>,
    card_meshes: Res<CardMeshes>,
    cards: Query<(Entity, &CardArt, Option<&Landscape>, Option<&Children>), With<CardView>>,
    art_planes: Query<(), With<FoilArt>>,
) {
    for (entity, art, landscape, children) in &cards {
        let Some(image) = images.get(&art.0) else {
            continue;
        };
        let wide = image.width() > image.height();
        if wide == landscape.is_some() {
            continue;
        }
        let (body, plane) = if wide {
            (card_meshes.wide_body.clone(), card_meshes.wide_art.clone())
        } else {
            (card_mesh.0.clone(), card_meshes.art.clone())
        };
        let mut card = commands.entity(entity);
        card.insert(Mesh3d(body));
        if wide {
            card.insert(Landscape);
        } else {
            card.remove::<Landscape>();
        }
        for &child in children.into_iter().flatten() {
            if art_planes.contains(child) {
                commands.entity(child).insert(Mesh3d(plane.clone()));
            }
        }
    }
}

pub fn face_down_in(card: &agni_core::Card, view: &TableView) -> bool {
    !card.face.is_hidden()
        && !view.revealed.contains(&card.id.0)
        && agni_sim::wire::zone_visibility(&view.zones, card.zone)
            == Some(agni_sim::wire::ZoneVisibility::All)
}

pub fn face_down_for(card: &agni_core::Card, view: &TableView, me: agni_core::PlayerId) -> bool {
    (card.owner == me || view.peeked.contains(&card.id.0)) && face_down_in(card, view)
}

pub fn face_down_mine(card: &agni_core::Card, mirror: &Mirror, me: agni_core::PlayerId) -> bool {
    face_down_for(card, &mirror.view, me)
}

pub fn lies_facedown_in(card: &agni_core::Card, view: &TableView) -> bool {
    face_down_in(card, view)
        || view
            .card(card.id.0)
            .is_some_and(|held| held.badges.iter().any(|badge| badge.key == "hidden"))
}

pub fn lies_facedown(card: &agni_core::Card, mirror: &Mirror) -> bool {
    lies_facedown_in(card, &mirror.view)
}

pub fn peeked_face_in<'a>(
    card: &'a agni_core::Card,
    view: &TableView,
    me: agni_core::PlayerId,
) -> Option<&'a str> {
    face_down_for(card, view, me).then_some(card.face.name.as_str())
}

pub fn peeked_face<'a>(
    card: &'a agni_core::Card,
    mirror: &Mirror,
    me: agni_core::PlayerId,
) -> Option<&'a str> {
    peeked_face_in(card, &mirror.view, me)
}

pub fn drawn_face_in<'a>(
    card: &'a agni_core::Card,
    view: &TableView,
) -> std::borrow::Cow<'a, agni_core::Card> {
    if face_down_in(card, view) {
        std::borrow::Cow::Owned(agni_core::Card {
            face: agni_core::CardFace::hidden(),
            ..card.clone()
        })
    } else {
        std::borrow::Cow::Borrowed(card)
    }
}

pub fn drawn_face<'a>(
    card: &'a agni_core::Card,
    mirror: &Mirror,
) -> std::borrow::Cow<'a, agni_core::Card> {
    drawn_face_in(card, &mirror.view)
}

#[cfg(test)]
mod facedown_tests {
    use super::*;
    use agni_core::{CardFace, PlayerId, Table, Zone};

    fn zones() -> Vec<agni_sim::wire::ZoneDecl> {
        agni_riftbound::zone_table()
    }

    fn view(revealed: Vec<u32>) -> TableView {
        TableView {
            zones: zones(),
            revealed,
            ..Default::default()
        }
    }

    #[test]
    fn an_enforced_hide_lies_face_down_for_its_owner_too() {
        let mut table = Table::new();
        let battlefield = Zone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST);
        let mine = table.add(PlayerId(0), battlefield, "Consult the Past", [0; 3]);
        let theirs = table.add_face(PlayerId(1), battlefield, CardFace::hidden());
        let open = table.add(PlayerId(0), battlefield, "Vi - Piltover Enforcer", [0; 3]);
        let owner = view(vec![open.0]);
        let opponent = view(vec![open.0]);
        assert!(
            face_down_in(table.get(mine).unwrap(), &owner),
            "the hider's own replica must not show its face on the table"
        );
        assert!(
            !face_down_in(table.get(theirs).unwrap(), &opponent),
            "a face nobody knows is already drawn from its back, not flipped again"
        );
        assert!(
            !face_down_in(table.get(open).unwrap(), &owner),
            "a revealed card stays face up for both seats"
        );
        let drawn = drawn_face_in(table.get(mine).unwrap(), &owner);
        assert!(
            drawn.face.is_hidden(),
            "and the mesh it is drawn with carries no face"
        );
    }

    #[test]
    fn a_hand_card_shown_in_place_and_then_hidden_is_drawn_face_down_for_the_seat_that_saw_it() {
        use agni_sim::log::{fold_entry, LogAction, LogEntry, LogState, TableConfig};
        use agni_sim::view::{table_view, view_to_table};
        use std::collections::BTreeMap;
        let mut state = LogState::new();
        let hand = Zone::Plugin(agni_riftbound::ZONE_HAND);
        let battlefield = Zone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST);
        let entries = vec![
            LogEntry::new(
                0,
                0,
                LogAction::Genesis {
                    name: "rae".into(),
                    config: TableConfig {
                        engine: None,
                        plugin: None,
                        zones: zones(),
                        options: None,
                        counters: Vec::new(),
                        despawn_any: false,
                    },
                },
            ),
            LogEntry::new(1, 1, LogAction::Join { name: "ada".into() }),
            LogEntry::new(
                2,
                1,
                LogAction::Deal {
                    cards: vec![9],
                    to: hand,
                },
            ),
            LogEntry::new(
                3,
                1,
                LogAction::Reveal {
                    card: 9,
                    face: CardFace::named("Smoke and Mirrors"),
                },
            ),
        ];
        for entry in &entries {
            fold_entry(&mut state, entry).unwrap();
        }
        let mut known = BTreeMap::new();
        known.insert(9u32, CardFace::named("Smoke and Mirrors"));
        let shown = table_view(&state, 0);
        assert!(shown.revealed.contains(&9));
        let seen = view_to_table(&shown, &known);
        assert!(
            !face_down_in(seen.get(agni_core::CardId(9)).unwrap(), &shown),
            "a card revealed in the opponent's hand is read face up"
        );
        fold_entry(
            &mut state,
            &LogEntry::new(
                4,
                1,
                LogAction::Move {
                    card: 9,
                    to: battlefield,
                    seat: 0,
                    index: 0,
                    hidden: true,
                },
            ),
        )
        .unwrap();
        let hidden = table_view(&state, 0);
        assert!(
            !hidden.revealed.contains(&9),
            "the hide takes the face back"
        );
        let stale = view_to_table(&hidden, &known);
        let card = stale.get(agni_core::CardId(9)).unwrap();
        assert!(
            face_down_in(card, &hidden),
            "even a replica still holding the face lays the card face down"
        );
        assert!(
            !face_down_for(card, &hidden, PlayerId(0)),
            "and seat 0 may not peek at it"
        );
        assert!(face_down_for(card, &hidden, PlayerId(1)));
        assert!(drawn_face_in(card, &hidden).face.is_hidden());
        let forgotten = view_to_table(&hidden, &BTreeMap::new());
        assert!(forgotten
            .get(agni_core::CardId(9))
            .unwrap()
            .face
            .is_hidden());
    }

    #[test]
    fn only_the_owner_of_a_face_down_card_may_peek_at_it() {
        let mut table = Table::new();
        let battlefield = Zone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST);
        let mine = table.add(PlayerId(0), battlefield, "Consult the Past", [0; 3]);
        let blank = table.add_face(PlayerId(1), battlefield, CardFace::hidden());
        let theirs = table.add(PlayerId(1), battlefield, "Tideturner", [0; 3]);
        let seen = view(Vec::new());
        assert_eq!(
            peeked_face_in(table.get(mine).unwrap(), &seen, PlayerId(0)),
            Some("Consult the Past"),
            "the owner holds the private face, so the hover preview may show it"
        );
        assert_eq!(
            peeked_face_in(table.get(blank).unwrap(), &seen, PlayerId(0)),
            None,
            "a seat that never learned the face has nothing to preview"
        );
        assert_eq!(
            peeked_face_in(table.get(theirs).unwrap(), &seen, PlayerId(0)),
            None,
            "408.3 · a real face on a card the viewer does not own is never peeked at, \
             however it reached this replica"
        );
        assert!(!face_down_for(
            table.get(theirs).unwrap(),
            &seen,
            PlayerId(0)
        ));
        assert!(face_down_for(
            table.get(theirs).unwrap(),
            &seen,
            PlayerId(1)
        ));
        let public = view(vec![mine.0]);
        assert_eq!(
            peeked_face_in(table.get(mine).unwrap(), &public, PlayerId(0)),
            None,
            "once it is revealed the preview is the ordinary art, not a peek"
        );
        let mut allowed = view(Vec::new());
        allowed.peeked = vec![theirs.0];
        assert_eq!(
            peeked_face_in(table.get(theirs).unwrap(), &allowed, PlayerId(0)),
            Some("Tideturner"),
            "a seat the rules let look at that facedown card previews its face"
        );
        assert!(face_down_for(
            table.get(theirs).unwrap(),
            &allowed,
            PlayerId(0)
        ));
        assert_eq!(
            peeked_face_in(table.get(blank).unwrap(), &allowed, PlayerId(0)),
            None
        );
    }

    #[test]
    fn the_plugins_own_badge_keeps_a_shown_card_face_down_in_its_zone() {
        let mut table = Table::new();
        let battlefield = Zone::Plugin(agni_riftbound::ZONE_BATTLEFIELD_FIRST);
        let mine = table.add(PlayerId(0), battlefield, "Consult the Past", [0; 3]);
        let mut shown = view(vec![mine.0]);
        shown.cards = vec![agni_sim::view::ViewCard {
            id: mine.0,
            zone: battlefield,
            seat: 0,
            owner: 0,
            face_visible: true,
            badges: vec![agni_sim::view::Badge {
                key: "hidden".into(),
                value: serde_bytes::ByteBuf::from(vec![9u8, 0]),
            }],
        }];
        let card = table.get(mine).unwrap();
        assert!(
            !face_down_in(card, &shown),
            "a voluntary reveal makes the face public"
        );
        assert!(
            lies_facedown_in(card, &shown),
            "737.1 · but the card still lies in its facedown zone, so it still plays from there"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OppSeat {
    pub seat: u8,
    pub name: String,
    pub color: [u8; 3],
    pub hand: usize,
    pub deck: usize,
    pub trash: usize,
    pub runes_ready: usize,
    pub runes_total: usize,
    pub legend: Option<String>,
    pub champion: Option<String>,
}

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct OppStrip {
    pub seats: Vec<OppSeat>,
}

fn zone_named(zones: &[agni_sim::wire::ZoneDecl], name: &str) -> Option<Zone> {
    zones
        .iter()
        .find(|decl| decl.name == name)
        .map(|decl| Zone::Plugin(decl.id))
}

pub fn opp_seats(
    table: &Table,
    mirror: &Mirror,
    players: usize,
    me: PlayerId,
    label: &dyn Fn(u8) -> (String, [u8; 3], bool),
) -> Vec<OppSeat> {
    let zones = &mirror.view.zones;
    let hand = zones::hand_zone(zones);
    let deck = zone_named(zones, agni_riftbound::ZONE_NAME_MAIN_DECK)
        .or_else(|| plate::deck_zone(zones).map(Zone::Plugin));
    let trash = zone_named(zones, agni_riftbound::ZONE_NAME_TRASH);
    let runes = zone_named(zones, agni_riftbound::ZONE_NAME_RUNE_POOL);
    let legend = zone_named(zones, agni_riftbound::ZONE_NAME_LEGEND);
    let champion = zone_named(zones, agni_riftbound::ZONE_NAME_CHAMPION);
    let count = |seat: PlayerId, zone: Option<Zone>| {
        zone.map_or(0, |zone| table.in_area(seat, zone).count())
    };
    let face = |seat: PlayerId, zone: Option<Zone>| {
        zone.and_then(|zone| table.in_area(seat, zone).last())
            .map(|card| card.face.name.clone())
            .filter(|name| !name.is_empty())
    };
    (0..players as u8)
        .map(PlayerId)
        .filter(|seat| *seat != me)
        .map(|seat| {
            let (name, color, _) = label(seat.0);
            let hand_count = table.in_area(seat, hand).count()
                + if hand != Zone::Hand {
                    table.in_area(seat, Zone::Hand).count()
                } else {
                    0
                };
            let pool: Vec<u32> = runes
                .map(|zone| table.in_area(seat, zone).map(|card| card.id.0).collect())
                .unwrap_or_default();
            let ready = pool.iter().filter(|card| !mirror.rotated(**card)).count();
            OppSeat {
                seat: seat.0,
                name,
                color,
                hand: hand_count,
                deck: count(seat, deck),
                trash: count(seat, trash),
                runes_ready: ready,
                runes_total: pool.len(),
                legend: face(seat, legend),
                champion: face(seat, champion),
            }
        })
        .collect()
}

pub(super) fn refresh_opp_strip(
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    my_seat: Res<MySeat>,
    info: Res<SessionInfo>,
    seat_colors: Res<colors::SeatColors>,
    mut strip: ResMut<OppStrip>,
) {
    if !table.is_changed()
        && !mirror.is_changed()
        && !players.is_changed()
        && !my_seat.is_changed()
        && !info.is_changed()
        && !seat_colors.is_changed()
    {
        return;
    }
    let label =
        |seat: u8| colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(seat));
    let next = opp_seats(&table, &mirror, players.0, my_seat.0, &label);
    if strip.seats != next {
        strip.seats = next;
    }
}

#[cfg(test)]
mod opp_tests {
    use super::*;
    use agni_core::CardFace;

    #[test]
    fn the_opp_strip_counts_what_the_far_seat_hides_and_names_its_faces() {
        let zones = agni_riftbound::zone_table();
        let mut mirror = Mirror::default();
        mirror.view.zones = zones.clone();
        let zone =
            |name: &str| Zone::Plugin(zones.iter().find(|decl| decl.name == name).unwrap().id);
        let mut table = Table::new();
        let far = PlayerId(1);
        for _ in 0..5 {
            table.add_face(
                far,
                zone(agni_riftbound::ZONE_NAME_HAND),
                CardFace::named("x"),
            );
        }
        for _ in 0..24 {
            table.add_face(
                far,
                zone(agni_riftbound::ZONE_NAME_MAIN_DECK),
                CardFace::named("deck"),
            );
        }
        for _ in 0..2 {
            table.add_face(
                far,
                zone(agni_riftbound::ZONE_NAME_TRASH),
                CardFace::named("t"),
            );
        }
        let mut runes = Vec::new();
        for _ in 0..6 {
            runes.push(table.add_face(
                far,
                zone(agni_riftbound::ZONE_NAME_RUNE_POOL),
                CardFace::named("Rune"),
            ));
        }
        table.add_face(
            far,
            zone(agni_riftbound::ZONE_NAME_LEGEND),
            CardFace::named("Lillia"),
        );
        table.add_face(
            far,
            zone(agni_riftbound::ZONE_NAME_CHAMPION),
            CardFace::named("Lillia, Bashful Bloom"),
        );
        table.add_face(
            PlayerId(0),
            zone(agni_riftbound::ZONE_NAME_HAND),
            CardFace::named("mine"),
        );
        mirror.solo_exhausted.insert(runes[0].0);
        mirror.solo_exhausted.insert(runes[1].0);
        let label = |seat: u8| (format!("seat {seat}"), [seat, 0, 0], seat == 0);
        let seats = opp_seats(&table, &mirror, 2, PlayerId(0), &label);
        assert_eq!(seats.len(), 1);
        let seat = &seats[0];
        assert_eq!(seat.seat, 1);
        assert_eq!(seat.name, "seat 1");
        assert_eq!(seat.hand, 5);
        assert_eq!(seat.deck, 24);
        assert_eq!(seat.trash, 2);
        assert_eq!((seat.runes_ready, seat.runes_total), (4, 6));
        assert_eq!(seat.legend.as_deref(), Some("Lillia"));
        assert_eq!(seat.champion.as_deref(), Some("Lillia, Bashful Bloom"));
        assert!(opp_seats(&table, &mirror, 1, PlayerId(0), &label).is_empty());
        let empty = Mirror::default();
        let bare = opp_seats(&table, &empty, 2, PlayerId(0), &label);
        assert_eq!(bare[0].deck, 0);
        assert_eq!(bare[0].legend, None);
    }
}
