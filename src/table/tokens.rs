use super::*;
use agni_sim::wire::{TokenDecl, ZoneDecl, ZoneKind, ZoneOwner};

#[derive(Resource, Default, Debug)]
pub struct TokenPanel {
    pub open: bool,
    pub custom_name: String,
    pub custom_might: u8,
    pub placing: Option<Placing>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placing {
    pub name: String,
    pub face: agni_core::CardFace,
}

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct PluginTokens(pub Vec<TokenDecl>);

pub const PLACE_MIN: f32 = 48.0;
pub const PLACE_ALPHA: u8 = 60;
pub const PLACE_EDGE: u8 = 230;
pub const PLACE_WIDTH: f32 = 3.0;

pub fn spawn_targets(zones: &[ZoneDecl]) -> Vec<(u16, String, bool)> {
    zones
        .iter()
        .filter(|decl| decl.kind == ZoneKind::Battlefield)
        .map(|decl| (decl.id, decl.label.clone(), decl.owner == ZoneOwner::Shared))
        .collect()
}

pub fn refresh_tokens(mut tokens: ResMut<PluginTokens>) {
    let current = crate::engine::modules::tokens();
    if tokens.0 != current {
        tokens.0 = current;
    }
}

pub fn choices(
    tokens: &[TokenDecl],
    custom_name: &str,
    custom_might: u8,
) -> Vec<(String, Option<u8>, bool, agni_core::CardFace)> {
    let mut out: Vec<(String, Option<u8>, bool, agni_core::CardFace)> = tokens
        .iter()
        .map(|decl| (decl.name.clone(), decl.might, decl.temporary, decl.face()))
        .collect();
    let custom = custom_name.trim();
    if !custom.is_empty() {
        let face = agni_core::CardFace::named(custom.to_string())
            .with_kind(agni_riftbound::KIND_UNIT)
            .with_might(Some(custom_might));
        out.push((custom.to_string(), Some(custom_might), false, face));
    }
    out
}

pub fn placing_hint(placing: &Placing) -> String {
    format!("tap a lit zone to place {} · esc cancels", placing.name)
}

pub fn spawn_for(placing: &Placing, zone: u16, shared: bool, me: PlayerId) -> TokenSpawn {
    TokenSpawn {
        face: placing.face.clone(),
        to: Zone::Plugin(zone),
        seat: if shared { PlayerId(0) } else { me },
    }
}

pub fn inside(points: &[egui::Pos2], at: egui::Pos2) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0i8;
    for index in 0..n {
        let a = points[index];
        let b = points[(index + 1) % n];
        let cross = (b.x - a.x) * (at.y - a.y) - (b.y - a.y) * (at.x - a.x);
        let this = if cross > 0.0 {
            1
        } else if cross < 0.0 {
            -1
        } else {
            0
        };
        if this == 0 {
            continue;
        }
        if sign == 0 {
            sign = this;
        } else if sign != this {
            return false;
        }
    }
    true
}

pub fn tokens_tab(
    ui: &mut egui::Ui,
    panel: &mut TokenPanel,
    tokens: &PluginTokens,
    zones: &[ZoneDecl],
) -> Option<Placing> {
    let targets = spawn_targets(zones);
    if targets.is_empty() {
        ui.label("this table has nowhere to put a token");
        return None;
    }
    let mut start = None;
    ui.label(
        egui::RichText::new("pick a token, then tap the zone it lands on")
            .color(hud::INK_WEAK)
            .small(),
    );
    if let Some(placing) = &panel.placing {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("placing {}", placing.name)).color(hud::AMBER));
            if ui.small_button("cancel").clicked() {
                start = Some(None);
            }
        });
    }
    let choices = choices(&tokens.0, &panel.custom_name, panel.custom_might);
    for (name, might, temporary, face) in choices {
        let mut label = match might {
            Some(might) => format!("{name} · {might} might"),
            None => name.clone(),
        };
        if temporary {
            label.push_str(" · temporary");
        }
        let button = egui::Button::new(egui::RichText::new(label).color(hud::INK))
            .min_size(egui::vec2(ui.available_width(), PLACE_MIN));
        if ui.add(button).clicked() {
            start = Some(Some(Placing { name, face }));
        }
    }
    ui.separator();
    ui.label(egui::RichText::new("custom").color(hud::INK_WEAK).small());
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut panel.custom_name)
                .hint_text("token name")
                .desired_width(140.0),
        );
        ui.add(egui::DragValue::new(&mut panel.custom_might).range(0..=20));
        ui.label("might");
    });
    match start {
        Some(next) => {
            panel.placing = next.clone();
            next
        }
        None => None,
    }
}

pub fn leave_placement(
    mut panel: ResMut<TokenPanel>,
    menu: Res<crate::menu::Menu>,
    tools: Res<plugin_ui::Tools>,
) {
    if panel.placing.is_some() && (!menu.at_table() || !tools.free) {
        panel.placing = None;
    }
}

pub fn placement_ui(
    mut contexts: EguiContexts,
    mut panel: ResMut<TokenPanel>,
    hud: Res<hud::Hud>,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    time: Res<Time>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut spawns: MessageWriter<TokenSpawn>,
    mut armed: Local<bool>,
) -> Result {
    let Some(placing) = panel.placing.clone() else {
        *armed = false;
        return Ok(());
    };
    let accepting = *armed;
    *armed = true;
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let targets = spawn_targets(&mirror.view.zones);
    let wanted: Vec<u16> = targets.iter().map(|(zone, _, _)| *zone).collect();
    let anchors = zones::anchors(
        &mirror.view.zones,
        players.0,
        info.battlefields_in_play(players.0),
    );
    let ink = highlight::rim_color(highlight::RimKind::Answer);
    let alpha = highlight::pulse_alpha(time.elapsed_secs());
    let edge = highlight::faded(ink, alpha.min(PLACE_EDGE));
    let painter = context.layer_painter(egui::LayerId::background());
    let clicked_at = context.input(|input| {
        input
            .pointer
            .primary_released()
            .then(|| input.pointer.interact_pos())
            .flatten()
    });
    let mut landed: Option<u16> = None;
    for anchor in &anchors {
        if !highlight::tinted(
            anchor.seat.map(|seat| seat.0),
            my_seat.0 .0,
            anchor.zone,
            &wanted,
        ) {
            continue;
        }
        let Some(points) = highlight::projected(
            camera,
            camera_transform,
            &GlobalTransform::IDENTITY,
            highlight::zone_quad(anchor.position, anchor.yaw, anchor.size),
        ) else {
            continue;
        };
        painter.add(egui::Shape::convex_polygon(
            points.clone(),
            highlight::faded(ink, PLACE_ALPHA),
            egui::Stroke::new(PLACE_WIDTH, edge),
        ));
        if accepting && clicked_at.is_some_and(|at| inside(&points, at)) {
            landed = Some(anchor.zone);
        }
    }
    let stage = hud.0.stage;
    egui::Area::new(egui::Id::new("token placement"))
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(egui::pos2(stage.center().x, stage.min.y + hud::GAP))
        .order(egui::Order::Middle)
        .show(&context, |ui| {
            ui.set_max_width(stage.width());
            hud::panel_frame(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(placing_hint(&placing)).color(hud::INK));
                    if ui.small_button("cancel").clicked() {
                        panel.placing = None;
                    }
                });
            });
        });
    if let Some(zone) = landed {
        if let Some((_, _, shared)) = targets.iter().find(|(id, _, _)| *id == zone) {
            spawns.write(spawn_for(&placing, zone, *shared, my_seat.0));
            panel.placing = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{ZoneLayout, ZonePlace, ZoneVisibility};

    fn decl(id: u16, kind: ZoneKind, owner: ZoneOwner) -> ZoneDecl {
        ZoneDecl {
            id,
            name: format!("z{id}"),
            kind,
            owner,
            visibility: ZoneVisibility::All,
            layout: ZoneLayout::Row,
            place: ZonePlace::Inner,
            span: 1,
            label: format!("Zone {id}"),
        }
    }

    #[test]
    fn a_placement_lands_on_the_zone_under_the_release_and_nowhere_else() {
        let quad = [
            egui::pos2(100.0, 100.0),
            egui::pos2(200.0, 100.0),
            egui::pos2(220.0, 160.0),
            egui::pos2(80.0, 160.0),
        ];
        assert!(inside(&quad, egui::pos2(150.0, 130.0)));
        assert!(!inside(&quad, egui::pos2(50.0, 130.0)));
        assert!(!inside(&quad, egui::pos2(150.0, 170.0)));
        assert!(!inside(&quad[..2], egui::pos2(150.0, 100.0)));
        let placing = Placing {
            name: "Sand Soldier".into(),
            face: agni_core::CardFace::named("Sand Soldier".to_string()),
        };
        let shared = spawn_for(&placing, 9, true, PlayerId(1));
        assert_eq!(shared.seat, PlayerId(0));
        assert_eq!(shared.to, Zone::Plugin(9));
        let mine = spawn_for(&placing, 8, false, PlayerId(1));
        assert_eq!(mine.seat, PlayerId(1));
        assert_eq!(mine.face.name, "Sand Soldier");
        assert_eq!(
            placing_hint(&placing),
            "tap a lit zone to place Sand Soldier · esc cancels"
        );
        let listed = choices(&agni_riftbound::token_table(), " Big Rock ", 4);
        assert_eq!(
            listed
                .last()
                .map(|(name, might, _, _)| (name.as_str(), *might)),
            Some(("Big Rock", Some(4)))
        );
        assert_eq!(choices(&[], "", 0).len(), 0);
    }

    #[test]
    fn tokens_can_land_on_bases_and_battlefields_only() {
        let zones = vec![
            decl(0, ZoneKind::Hand, ZoneOwner::PerSeat),
            decl(8, ZoneKind::Battlefield, ZoneOwner::PerSeat),
            decl(9, ZoneKind::Battlefield, ZoneOwner::Shared),
            decl(12, ZoneKind::Stack, ZoneOwner::Shared),
        ];
        let targets = spawn_targets(&zones);
        assert_eq!(targets.len(), 2);
        assert_eq!((targets[0].0, targets[0].2), (8, false));
        assert_eq!((targets[1].0, targets[1].2), (9, true));
    }

    #[test]
    fn the_m9_tokens_are_placeable_units_whose_artless_faces_fetch_by_name() {
        let tokens = agni_riftbound::token_table();
        let names: Vec<&str> = tokens.iter().map(|decl| decl.name.as_str()).collect();
        for name in ["Sand Soldier", "Shadow Clone", "Tentacle"] {
            let decl = tokens
                .iter()
                .find(|decl| decl.name == name)
                .unwrap_or_else(|| panic!("{name} is in the token table: {names:?}"));
            let face = decl.face();
            assert_eq!(face.kind.as_deref(), Some(agni_riftbound::KIND_UNIT));
            assert_eq!(face.might, decl.might);
            assert!(!decl.temporary);
            assert!(decl.art.is_none(), "{name} has no print to fetch by id");
        }
        assert_eq!(
            tokens
                .iter()
                .find(|decl| decl.name == "Shadow Clone")
                .and_then(|decl| decl.might),
            Some(0)
        );
        let ids = crate::render::art::token_art_ids(&tokens);
        assert!(ids.contains_key(&crate::render::art::art_key("Sprite")));
        assert!(!ids.contains_key(&crate::render::art::art_key("Tentacle")));
        let mut table = agni_core::Table::new();
        table.add_face(
            PlayerId(0),
            Zone::Plugin(8),
            tokens
                .iter()
                .find(|decl| decl.name == "Tentacle")
                .unwrap()
                .face(),
        );
        let wanted = crate::render::art::missing_from_table(
            &table,
            crate::render::art::ArtGame::Riftbound,
            &crate::render::art::ArtCache::default(),
            &ids,
        );
        assert_eq!(wanted.len(), 1);
        assert_eq!(wanted[0].name, "Tentacle");
        assert_eq!(
            wanted[0].key(),
            crate::render::art::ArtRequest::by_name(
                crate::render::art::ArtGame::Riftbound,
                "Tentacle"
            )
            .key(),
            "no id: by name, and the named placeholder stands in when the source has none"
        );
    }
}
