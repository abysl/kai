use super::*;
use agni_sim::wire::ZoneKind;

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct PileSheet(pub Option<(u16, PlayerId)>);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct PileHover(pub Option<CardId>);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct PileSelected(pub Option<CardId>);

pub fn pile_target(hovered: Option<CardId>, selected: Option<CardId>) -> Option<CardId> {
    hovered.or(selected)
}

fn pile_preview_name<'a>(card: &'a agni_core::Card, view: &TableView) -> Option<&'a str> {
    let Zone::Plugin(zone) = card.zone else {
        return None;
    };
    let public_discard = view.zones.iter().any(|decl| {
        decl.id == zone
            && decl.kind == ZoneKind::Discard
            && decl.visibility == agni_sim::wire::ZoneVisibility::All
    });
    (public_discard && !card.face.is_hidden() && !sync::lies_facedown_in(card, view))
        .then_some(card.face.name.as_str())
}

fn pile_actions(view: &agni_sim::wire::PluginView, me: PlayerId, card: CardId) -> Vec<usize> {
    if view
        .prompt
        .as_ref()
        .is_some_and(|prompt| prompt.seat != me.0)
    {
        return Vec::new();
    }
    view.shown()
        .filter(|(index, affordance)| {
            affordance.enabled
                && affordance.card == Some(card.0)
                && matches!(affordance.kind, agni_sim::wire::AffordanceKind::Plain)
                && !plugin_ui::menu_only(view, *index)
        })
        .map(|(index, _)| index)
        .collect()
}

fn pile_preview_size(width: f32, height: f32) -> egui::Vec2 {
    let width = width.max(0.0).min(240.0).min(height.max(0.0) / 1.4);
    egui::vec2(width, width * 1.4)
}

pub(super) fn card_label_ui(
    mut contexts: EguiContexts,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(&FaceKey, &GlobalTransform, &ViewVisibility), With<CardView>>,
) -> Result {
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let painter = contexts
        .ctx_mut()?
        .layer_painter(egui::LayerId::background());
    for (key, transform, visibility) in &cards {
        if !visibility.get() || !wants_label(key) {
            continue;
        }
        let Ok(screen) = camera.world_to_viewport(camera_transform, transform.translation()) else {
            continue;
        };
        let lines = label_lines(&key.name);
        let font = egui::FontId::proportional(12.0);
        let step = 13.0;
        let top = screen.y - step * (lines.len() as f32 - 1.0) / 2.0;
        for (i, line) in lines.iter().enumerate() {
            let position = egui::pos2(screen.x, top + i as f32 * step);
            painter.text(
                position + egui::vec2(1.0, 1.0),
                egui::Align2::CENTER_CENTER,
                line,
                font.clone(),
                egui::Color32::from_black_alpha(200),
            );
            painter.text(
                position,
                egui::Align2::CENTER_CENTER,
                line,
                font.clone(),
                egui::Color32::from_rgb(235, 235, 235),
            );
        }
    }
    Ok(())
}

pub(super) fn empty_table_ui(
    mut contexts: EguiContexts,
    table: Res<GameTable>,
    hud: Res<hud::Hud>,
    mut menu: ResMut<crate::menu::Menu>,
) -> Result {
    if !table.0.is_empty() || !menu.at_table() {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let rect = hud.0.banner;
    let mut back = false;
    hud::slot(&context, "empty table", rect, |ui| {
        hud::panel_frame(ui.style()).show(ui, |ui| {
            ui.set_min_width(rect.width() - 16.0);
            ui.set_max_width(rect.width() - 16.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("nothing on the table yet")
                        .size(18.0)
                        .color(hud::INK),
                );
                ui.label(
                    egui::RichText::new("the deal happens once every seat has a deck")
                        .color(hud::INK_WEAK),
                );
                ui.add_space(hud::GAP);
                let button = egui::Button::new("back to the lobby")
                    .min_size(egui::vec2(168.0, hud::TOUCH_MIN));
                if ui.add(button).clicked() {
                    back = true;
                }
            });
        });
    });
    if back {
        menu.back();
    }
    Ok(())
}

pub fn redeal_allowed(free: bool, between_games: bool) -> bool {
    free || between_games
}

pub fn tuning_section(
    ui: &mut egui::Ui,
    tuning: &mut ResMut<Tuning>,
    redeal_allowed: bool,
    #[cfg(not(target_arch = "wasm32"))] redeal: &mut MessageWriter<Redeal>,
) {
    let live = tuning.bypass_change_detection();
    let before = live.clone();
    #[cfg(not(target_arch = "wasm32"))]
    if redeal_allowed {
        if ui.button("deal a sample hand").clicked() {
            redeal.write(Redeal);
        }
        ui.separator();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = redeal_allowed;
    ui.add(egui::Slider::new(&mut live.hand_gap, 0.0..=0.5).text("hand gap"));
    ui.add(egui::Slider::new(&mut live.hand_droop, 0.0..=0.1).text("droop"));
    ui.add(egui::Slider::new(&mut live.hand_curve, 0.0..=0.2).text("depth curve"));
    ui.add(egui::Slider::new(&mut live.hand_y, 0.0..=2.0).text("hand height"));
    ui.add(egui::Slider::new(&mut live.hand_z, 2.0..=5.0).text("hand distance"));
    ui.add(egui::Slider::new(&mut live.hover_rise, 0.0..=0.8).text("hover rise"));
    ui.add(egui::Slider::new(&mut live.hand_rise, 0.0..=2.0).text("hand hover lift"));
    ui.add(egui::Slider::new(&mut live.lift, 0.0..=1.5).text("drag lift"));
    ui.add(egui::Slider::new(&mut live.ease_rate, 4.0..=30.0).text("ease rate"));
    ui.add(egui::Slider::new(&mut live.zoom, dim::ZOOM_MIN..=dim::ZOOM_MAX).text("zoom"))
        .on_hover_text("1.0 frames the whole table exactly; below pulls in, above backs off");
    ui.add(
        egui::Slider::new(&mut live.pitch_deg, dim::PITCH_MIN..=dim::PITCH_TOP_DOWN)
            .text("camera tilt"),
    )
    .on_hover_text("90 looks straight down at the table; lower tilts toward your seat");
    ui.add(
        egui::Slider::new(&mut live.camera_speed, 0.0..=24.0).text("camera move speed"),
    )
    .on_hover_text(
        "how fast the table slides when the cursor sits at the edge of the screen — 0 turns edge panning off",
    );
    ui.add(egui::Slider::new(&mut live.preview_scale, 1.0..=5.0).text("hover preview size"));
    ui.add(egui::Slider::new(&mut live.foil_chance, 0.0..=1.0).text("foil chance"));
    ui.add(egui::Slider::new(&mut live.foil_alpha, 0.0..=1.0).text("foil art alpha"));
    ui.add(egui::Slider::new(&mut live.foil_strength, 0.0..=2.0).text("foil sheen"));
    ui.add(egui::Slider::new(&mut live.foil_frequency, 0.5..=8.0).text("foil bands"));
    ui.add(egui::Slider::new(&mut live.foil_sparks, 0.0..=3.0).text("foil sparks"));
    ui.add(egui::Slider::new(&mut live.foil_spark_density, 4.0..=40.0).text("spark density"));
    if ui.button("reset to defaults").clicked() {
        *live = Tuning::default();
    }
    if *live != before {
        tuning.set_changed();
    }
}

pub(super) fn save_tuning(time: Res<Time>, tuning: Res<Tuning>, mut pending: Local<Option<f32>>) {
    if tuning.is_changed() {
        *pending = Some(0.0);
        return;
    }
    if let Some(elapsed) = pending.as_mut() {
        *elapsed += time.delta_secs();
        if *elapsed >= dim::SAVE_DEBOUNCE_SECS {
            tuning.save();
            *pending = None;
        }
    }
}

pub fn hand_window(scroll: f32, hand: usize, visible: f32) -> Option<(usize, usize, usize)> {
    let visible = visible.max(1.0);
    if hand as f32 <= visible {
        return None;
    }
    let first = scroll.max(0.0) as usize + 1;
    let last = ((scroll + visible) as usize).min(hand);
    Some((first, last, hand))
}

pub fn hand_scroll_step(scroll: f32, scrolled: f32, hand: usize, visible: f32) -> f32 {
    let max = (hand as f32 - visible).max(0.0);
    (scroll + scrolled).clamp(0.0, max)
}

pub(super) fn hand_count_ui(
    mut contexts: EguiContexts,
    hud: Res<hud::Hud>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    menu: Res<crate::menu::Menu>,
    mut scroll: ResMut<HandScroll>,
) -> Result {
    if !menu.at_table() {
        return Ok(());
    }
    let hand = my_hand_ids(&table, &mirror, my_seat.0).len();
    let Some((first, last, total)) = hand_window(scroll.0, hand, dim::HAND_VISIBLE) else {
        if scroll.0 != 0.0 {
            scroll.0 = 0.0;
        }
        return Ok(());
    };
    let clamped = hand_scroll_step(scroll.0, 0.0, hand, dim::HAND_VISIBLE);
    if clamped != scroll.0 {
        scroll.0 = clamped;
    }
    let context = contexts.ctx_mut()?.clone();
    let band = hud.0.hand;
    let text = format!("{first}–{last} of {total}");
    let galley = context.fonts_mut(|fonts| {
        fonts.layout_no_wrap(text.clone(), egui::FontId::proportional(12.0), hud::INK)
    });
    let size = galley.size() + egui::vec2(16.0, 8.0);
    let rect = egui::Rect::from_min_size(
        egui::pos2(band.max.x - size.x, band.max.y - size.y - hud::GAP),
        size,
    );
    egui::Area::new(egui::Id::new("hand count"))
        .fixed_pos(rect.min)
        .order(egui::Order::Middle)
        .interactable(false)
        .show(&context, |ui| {
            let (chip, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(chip, size.y / 2.0, hud::SURFACE_2);
            ui.painter()
                .galley(chip.min + egui::vec2(8.0, 4.0), galley, hud::INK);
        });
    Ok(())
}

pub fn owner_prefix(shared: bool, mine: bool, owner: &str, label: &str) -> String {
    if shared || mine {
        label.to_string()
    } else {
        format!("{owner}'s {label}")
    }
}

pub fn pile_openable(kind: ZoneKind, count: usize) -> bool {
    kind == ZoneKind::Discard && count > 0
}

pub fn discard_piles(
    zones: &[agni_sim::wire::ZoneDecl],
    table: &Table,
    players: usize,
) -> Vec<(u16, PlayerId, usize)> {
    let mut piles = Vec::new();
    for decl in zones.iter().filter(|decl| decl.kind == ZoneKind::Discard) {
        let seats: Vec<PlayerId> = match decl.owner {
            agni_sim::wire::ZoneOwner::PerSeat => (0..players as u8).map(PlayerId).collect(),
            _ => vec![PlayerId(0)],
        };
        for seat in seats {
            let count = table.in_area(seat, Zone::Plugin(decl.id)).count();
            if count > 0 {
                piles.push((decl.id, seat, count));
            }
        }
    }
    piles
}

pub fn toggle_pile(
    current: Option<(u16, PlayerId)>,
    zone: u16,
    seat: PlayerId,
) -> Option<(u16, PlayerId)> {
    if current == Some((zone, seat)) {
        None
    } else {
        Some((zone, seat))
    }
}

pub fn control_of(view: &agni_sim::wire::PluginView, zone: u16) -> Option<plate::Control> {
    plate::lines(view).into_iter().find_map(|line| match line {
        plate::StatusLine::Control(controls) => {
            controls.into_iter().find(|control| control.zone == zone)
        }
        _ => None,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn zone_overlay_ui(
    mut contexts: EguiContexts,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    table: Res<GameTable>,
    panel: Res<plugin_ui::PluginPanel>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    menu: Res<crate::menu::Menu>,
    mut pile_sheet: ResMut<PileSheet>,
    mut pile_hover: ResMut<PileHover>,
    mut pile_selected: ResMut<PileSelected>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    if mirror.view.zones.is_empty() || !menu.at_table() {
        pile_hover.0 = None;
        pile_selected.0 = None;
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let painter = context.layer_painter(egui::LayerId::background());
    let seat_label =
        |seat: PlayerId| colors::seat_label(&info.roster, &seat_colors, my_seat.0, seat);
    for anchor in zones::anchors(
        &mirror.view.zones,
        players.0,
        info.battlefields_in_play(players.0),
    ) {
        let Some(decl) = mirror.view.zones.iter().find(|decl| decl.id == anchor.zone) else {
            continue;
        };
        let seat = anchor.seat.unwrap_or(PlayerId(0));
        let count = table.in_area(seat, Zone::Plugin(anchor.zone)).count();
        let spin = Quat::from_rotation_y(anchor.yaw);
        let edge = anchor.position + spin * Vec3::new(0.0, 0.0, anchor.size.y / 2.0 - 0.12);
        let Ok(screen) = camera.world_to_viewport(camera_transform, edge) else {
            continue;
        };
        let shared = anchor.seat.is_none();
        let owner = seat_label(seat);
        let line = owner_prefix(shared, owner.2, &owner.0, &zones::zone_line(decl, count));
        let hovered = if pile_openable(decl.kind, count) {
            let id = egui::Id::new(("pile sheet", anchor.zone, seat.0));
            let response = egui::Area::new(id)
                .fixed_pos(egui::pos2(screen.x, screen.y))
                .pivot(egui::Align2::CENTER_CENTER)
                .order(egui::Order::Middle)
                .show(&context, |ui| {
                    ui.allocate_exact_size(egui::vec2(96.0, 22.0), egui::Sense::click())
                        .1
                })
                .inner
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                pile_sheet.0 = toggle_pile(pile_sheet.0, anchor.zone, seat);
            }
            response.hovered()
        } else {
            false
        };
        let ink = if shared {
            egui::Color32::from_rgba_unmultiplied(235, 235, 235, 200)
        } else if hovered {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 220)
        } else {
            egui::Color32::from_rgba_unmultiplied(220, 220, 220, 130)
        };
        painter.text(
            egui::pos2(screen.x, screen.y),
            egui::Align2::CENTER_CENTER,
            line,
            egui::FontId::proportional(if shared { 13.0 } else { 11.0 }),
            ink,
        );
        if !shared {
            continue;
        }
        let Some(control) = control_of(&panel.view, anchor.zone) else {
            continue;
        };
        let near = anchor.position + spin * Vec3::new(0.0, 0.0, anchor.size.y / 2.0);
        let half = spin * Vec3::new(anchor.size.x / 2.0, 0.0, 0.0);
        let ends = [near - half, near + half].map(|point| {
            camera
                .world_to_viewport(camera_transform, point)
                .ok()
                .map(|at| egui::pos2(at.x, at.y))
        });
        let (Some(left), Some(right)) = (ends[0], ends[1]) else {
            continue;
        };
        if let Some(holder) = control.held {
            let color = seat_label(PlayerId(holder)).1;
            painter.line_segment(
                [left, right],
                egui::Stroke::new(4.0, egui::Color32::from_rgb(color[0], color[1], color[2])),
            );
        }
        if let Some(contester) = control.contested {
            let color = seat_label(PlayerId(contester)).1;
            let lift = egui::vec2(0.0, -6.0);
            painter.add(egui::Shape::dashed_line(
                &[left + lift, right + lift],
                egui::Stroke::new(3.0, egui::Color32::from_rgb(color[0], color[1], color[2])),
                8.0,
                5.0,
            ));
        }
    }
    Ok(())
}

pub(super) fn pile_sheet_ui(
    mut contexts: EguiContexts,
    hud: Res<hud::Hud>,
    mut pile_sheet: ResMut<PileSheet>,
    mut pile_hover: ResMut<PileHover>,
    mut pile_selected: ResMut<PileSelected>,
    mirror: Res<Mirror>,
    table: Res<GameTable>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    info: Res<SessionInfo>,
    menu: Res<crate::menu::Menu>,
    panel: Res<plugin_ui::PluginPanel>,
    mut art: hud::Art,
    mut sender: hud::Sender,
) -> Result {
    let Some((zone, seat)) = pile_sheet.0 else {
        pile_hover.0 = None;
        pile_selected.0 = None;
        return Ok(());
    };
    if !menu.at_table() || mirror.view.zones.is_empty() {
        pile_sheet.0 = None;
        pile_hover.0 = None;
        pile_selected.0 = None;
        return Ok(());
    }
    let Some(decl) = mirror.view.zones.iter().find(|decl| {
        decl.id == zone
            && decl.kind == ZoneKind::Discard
            && decl.visibility == agni_sim::wire::ZoneVisibility::All
    }) else {
        pile_sheet.0 = None;
        pile_hover.0 = None;
        pile_selected.0 = None;
        return Ok(());
    };
    let table = &table.0;
    let ids: Vec<CardId> = table
        .in_area(seat, Zone::Plugin(zone))
        .map(|c| c.id)
        .collect();
    let owner = colors::seat_label(&info.roster, &seat_colors, my_seat.0, seat);
    let title = owner_prefix(false, owner.2, &owner.0, &decl.label);
    let context = contexts.ctx_mut()?.clone();
    pile_hover.0 = None;
    pile_selected.0 = pile_selected.0.filter(|id| ids.contains(id));
    let mut open = true;
    let mut fire = None;
    hud::sheet(
        &context,
        "pile sheet",
        hud.0.class,
        hud::Side::Right,
        &format!("{title} · {}", ids.len()),
        &mut open,
        |ui| {
            if ids.is_empty() {
                ui.label(egui::RichText::new("empty").weak());
            }
            for (index, id) in ids.iter().rev().enumerate() {
                let label = plugin_ui::card_label(table, &mirror.view, my_seat.0, id.0);
                let text = if index == 0 {
                    format!("{label} · top")
                } else {
                    label
                };
                let response = ui.add_sized(
                    egui::vec2(ui.available_width(), hud::TOUCH_MIN),
                    egui::Button::new(text)
                        .selected(pile_selected.0 == Some(*id))
                        .wrap(),
                );
                if response.hovered() {
                    pile_hover.0 = Some(*id);
                }
                if response.clicked() {
                    pile_selected.0 = Some(*id);
                }
                if hud.0.class.is_phone() && pile_selected.0 == Some(*id) {
                    if let Some(name) = table
                        .get(*id)
                        .and_then(|card| pile_preview_name(card, &mirror.view))
                    {
                        if let Some(texture) = art.texture(&mut contexts, name) {
                            let size = pile_preview_size(
                                ui.available_width(),
                                context.content_rect().height() * 0.4,
                            );
                            ui.image(egui::load::SizedTexture::new(texture, size));
                        } else {
                            ui.label(egui::RichText::new("art unavailable").weak());
                        }
                    }
                }
                if info.role != SessionRole::Ended {
                    ui.horizontal_wrapped(|ui| {
                        for index in pile_actions(&panel.view, my_seat.0, *id) {
                            let label = plugin_ui::expand(
                                &plugin_ui::answer_label(&panel.view, my_seat.0 .0, index),
                                &|seat| {
                                    colors::seat_label(
                                        &info.roster,
                                        &seat_colors,
                                        my_seat.0,
                                        PlayerId(seat),
                                    )
                                    .0
                                },
                                &|zone| plugin_ui::zone_label(&mirror.view.zones, zone),
                                &|card| plugin_ui::card_label(table, &mirror.view, my_seat.0, card),
                            );
                            if ui
                                .add(
                                    egui::Button::new(label)
                                        .min_size(egui::vec2(hud::TOUCH_MIN, hud::TOUCH_MIN))
                                        .wrap(),
                                )
                                .clicked()
                            {
                                fire = Some(index);
                            }
                        }
                    });
                }
            }
        },
    );
    if !open {
        pile_sheet.0 = None;
        pile_hover.0 = None;
        pile_selected.0 = None;
    }
    if let Some(index) = fire {
        sender.fire(&panel.view.affordances[index]);
        pile_sheet.0 = None;
        pile_hover.0 = None;
        pile_selected.0 = None;
    }
    Ok(())
}

pub fn combat_summary(view: &agni_sim::wire::PluginView) -> Option<(u16, u8, u8, i32, i32)> {
    let lines = plate::lines(view);
    let (zone, attacker, defender) = lines.iter().find_map(|line| match line {
        plate::StatusLine::Showdown {
            zone,
            attacker,
            defender,
            combat: true,
            ..
        } => Some((*zone, *attacker, *defender)),
        _ => None,
    })?;
    let (attackers_might, defenders_might) = lines.iter().find_map(|line| match line {
        plate::StatusLine::Combat {
            attackers_might,
            defenders_might,
            ..
        } => Some((*attackers_might, *defenders_might)),
        _ => None,
    })?;
    Some((zone, attacker, defender, attackers_might, defenders_might))
}

pub(super) fn combat_plate_ui(
    mut contexts: EguiContexts,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    panel: Res<plugin_ui::PluginPanel>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    let Some((zone, attacker, defender, attack, defend)) = combat_summary(&panel.view) else {
        return Ok(());
    };
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let Some(anchor) = zones::anchors(
        &mirror.view.zones,
        players.0,
        info.battlefields_in_play(players.0),
    )
    .into_iter()
    .find(|anchor| anchor.zone == zone && anchor.seat.is_none()) else {
        return Ok(());
    };
    let Ok(screen) = camera.world_to_viewport(camera_transform, anchor.position) else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let chip = |seat: u8, might: i32| {
        let (name, color, _) =
            colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(seat));
        (
            format!("{name} · {might} might"),
            egui::Color32::from_rgb(color[0], color[1], color[2]),
        )
    };
    let (attack_text, attack_color) = chip(attacker, attack);
    let (defend_text, defend_color) = chip(defender, defend);
    egui::Area::new(egui::Id::new("combat plate"))
        .pivot(egui::Align2::CENTER_CENTER)
        .fixed_pos(egui::pos2(screen.x, screen.y))
        .order(egui::Order::Middle)
        .interactable(false)
        .show(&context, |ui| {
            hud::panel_frame(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(attack_text)
                            .strong()
                            .color(counters::readable_ink([
                                attack_color.r(),
                                attack_color.g(),
                                attack_color.b(),
                            ]))
                            .background_color(attack_color),
                    );
                    ui.label(egui::RichText::new("vs").color(hud::INK_WEAK));
                    ui.label(
                        egui::RichText::new(defend_text)
                            .strong()
                            .color(counters::readable_ink([
                                defend_color.r(),
                                defend_color.g(),
                                defend_color.b(),
                            ]))
                            .background_color(defend_color),
                    );
                });
            });
        });
    Ok(())
}

pub fn hide_target(
    rims: &crate::table::highlight::Rims,
    zones: &[agni_sim::wire::ZoneDecl],
    me: PlayerId,
    card: u32,
    enforced: bool,
) -> Option<(Zone, PlayerId)> {
    let zone = match rims.hides_at(card) {
        Some(zone) => zone,
        None if enforced => return None,
        None => zones::base_zone(zones)?,
    };
    let to = Zone::Plugin(zone);
    let seat = match agni_sim::wire::zone_owner(zones, to) {
        Some(agni_sim::wire::ZoneOwner::PerSeat) => me,
        _ => PlayerId(0),
    };
    Some((to, seat))
}

pub fn free_verb(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::KeyE | KeyCode::KeyD | KeyCode::KeyT | KeyCode::KeyK
    )
}

pub fn key_allowed(tools: &plugin_ui::Tools, key: KeyCode) -> bool {
    tools.free || !free_verb(key)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    tools: Res<plugin_ui::Tools>,
    claimed: Res<plugin_ui::ClaimedKeys>,
    focus: Focus,
    mut tokens: ResMut<crate::table::tokens::TokenPanel>,
    menu: Res<crate::menu::Menu>,
    rims: Res<crate::table::highlight::Rims>,
    panel: Res<crate::table::plugin_ui::PluginPanel>,
    selected: Res<Selected>,
    entities: Query<(Entity, &CardView)>,
    mut act: chips::Act,
) {
    if !menu.at_table() {
        return;
    }
    let pressed = |key: KeyCode| keys.just_pressed(key) && plugin_ui::kai_key(&claimed, key);
    let exhaust_key = pressed(KeyCode::KeyE);
    let draw_key = pressed(KeyCode::KeyD);
    let trash_key = pressed(KeyCode::KeyT);
    let play_key = pressed(KeyCode::KeyP);
    let hidden_key = pressed(KeyCode::KeyF);
    let reveal_key = pressed(KeyCode::KeyR);
    let tokens_key = pressed(KeyCode::KeyK);
    if !exhaust_key
        && !draw_key
        && !trash_key
        && !play_key
        && !hidden_key
        && !reveal_key
        && !tokens_key
    {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    if !keys.get_just_pressed().all(|key| key_allowed(&tools, *key)) {
        return;
    }
    if tokens_key {
        tokens.open = !tokens.open;
        return;
    }
    let hovered_card = focus.card();
    if draw_key && hovered_card.is_none() {
        if let Some((card, to, index)) =
            zones::deck_draw(&mirror.view.zones, &table, my_seat.0, None)
        {
            act.dropped.write(CardDropped {
                card,
                to,
                seat: my_seat.0,
                index,
                hidden: false,
            });
        }
        return;
    }
    let Some(view) = hovered_card.as_ref() else {
        return;
    };
    let entity = selected
        .0
        .filter(|entity| {
            entities
                .get(*entity)
                .is_ok_and(|(_, held)| held.0 == view.0)
        })
        .or_else(|| {
            entities
                .iter()
                .find(|(_, held)| held.0 == view.0)
                .map(|(entity, _)| entity)
        });
    let Some(entity) = entity else {
        return;
    };
    let zone_name = |zone: u16| plugin_ui::zone_label(&mirror.view.zones, zone);
    let offers = chips::offers(
        &panel.view,
        &rims,
        &table,
        &mirror,
        my_seat.0,
        view.0 .0,
        &zone_name,
    );
    let guarded = guarded_from_me(&table, &mirror, my_seat.0, view.0);
    let row = chips::chips(offers, tools.free, guarded, mirror.rotated(view.0 .0));
    let key = [
        (hidden_key, KeyCode::KeyF),
        (play_key, KeyCode::KeyP),
        (reveal_key, KeyCode::KeyR),
        (exhaust_key, KeyCode::KeyE),
        (trash_key, KeyCode::KeyT),
    ]
    .into_iter()
    .find(|(pressed, _)| *pressed)
    .map(|(_, key)| key);
    if let Some(chip) = key.and_then(|key| chips::by_key(&row, key)) {
        act.perform(
            chip,
            &panel.view,
            &table,
            &mirror,
            my_seat.0,
            view.0,
            entity,
        );
        return;
    }
    if play_key {
        if let Some(drop) = chain_drop(&table, &mirror, my_seat.0, view.0) {
            act.dropped.write(drop);
        }
        return;
    }
    if hidden_key && tools.free {
        if let Some((to, seat)) =
            hide_target(&rims, &mirror.view.zones, my_seat.0, view.0 .0, false)
        {
            if table
                .get(view.0)
                .is_some_and(|held| held.owner == my_seat.0)
            {
                act.dropped.write(CardDropped {
                    card: view.0,
                    to,
                    seat,
                    index: table.in_area(seat, to).count(),
                    hidden: true,
                });
            }
        }
        return;
    }
    if reveal_key {
        let mine = table
            .get(view.0)
            .is_some_and(|held| held.owner == my_seat.0);
        let unrevealed = !mirror.view.revealed.contains(&view.0 .0);
        if mine && unrevealed && zones::is_public(&mirror.view.zones, &table, view.0) {
            act.reveals.write(RevealRequested(view.0));
        }
        return;
    }
    if draw_key {
        if let Some((card, to, index)) =
            zones::draw_move(&mirror.view.zones, &table, my_seat.0, view.0)
        {
            act.dropped.write(CardDropped {
                card,
                to,
                seat: my_seat.0,
                index,
                hidden: false,
            });
        }
    }
}

#[cfg(test)]
mod hidden_tests {
    use super::*;
    use crate::table::highlight::Rims;
    use std::collections::BTreeMap;

    fn zones() -> Vec<agni_sim::wire::ZoneDecl> {
        agni_riftbound::zone_table()
    }

    fn offering(card: u32, zone: u16) -> Rims {
        let mut rims = Rims::default();
        rims.hides.insert(card, vec![zone]);
        rims.legal
            .insert(card, crate::table::highlight::RimKind::Hide);
        rims
    }

    #[test]
    fn the_hide_key_drops_on_the_battlefield_the_plugin_offers() {
        let bf1 = agni_riftbound::ZONE_BATTLEFIELD_FIRST;
        let rims = offering(7, bf1);
        assert_eq!(
            hide_target(&rims, &zones(), PlayerId(1), 7, true),
            Some((Zone::Plugin(bf1), PlayerId(0))),
            "737.1.b · a hide lands in the battlefield's shared area, not the hider's own"
        );
        assert_eq!(
            hide_target(&rims, &zones(), PlayerId(1), 8, true),
            None,
            "a card the plugin does not offer a hide for is not hidden anywhere"
        );
    }

    #[test]
    fn only_a_free_table_falls_back_to_the_base() {
        let base = agni_riftbound::ZONE_BASE;
        let nothing = Rims::default();
        assert!(nothing.is_empty());
        assert_eq!(
            hide_target(&nothing, &zones(), PlayerId(1), 7, false),
            Some((Zone::Plugin(base), PlayerId(1))),
            "a free table enforces nothing, so H is still the old face-down play"
        );
        assert_eq!(
            hide_target(&nothing, &zones(), PlayerId(1), 7, true),
            None,
            "an enforced table with an empty legal list — the other seat's turn — never guesses \
             the base, which it would only refuse"
        );
    }

    #[test]
    fn an_enforced_table_with_no_hide_on_offer_refuses_to_guess() {
        let mut rims = Rims::default();
        rims.legal
            .insert(9, crate::table::highlight::RimKind::March);
        rims.destinations = BTreeMap::from([(9, vec![agni_riftbound::ZONE_BATTLEFIELD_FIRST])]);
        assert!(!rims.is_empty());
        assert_eq!(
            hide_target(&rims, &zones(), PlayerId(0), 9, true),
            None,
            "the plugin is speaking and it did not offer a hide"
        );
    }

    #[test]
    fn the_mode_comes_from_the_plugins_own_turn_line() {
        use agni_sim::wire::PluginView;
        let enforced = PluginView {
            status: vec!["turn 2 · {seat 1} · action phase · rules enforced".into()],
            ..PluginView::default()
        };
        assert!(crate::table::plugin_ui::enforced(&enforced));
        let free = PluginView {
            status: vec!["turn 2 · {seat 1} · action phase · free table".into()],
            ..PluginView::default()
        };
        assert!(!crate::table::plugin_ui::enforced(&free));
        assert!(!crate::table::plugin_ui::enforced(&PluginView::default()));
    }
}

#[cfg(test)]
mod hud_tests {
    use super::*;
    use agni_sim::wire::PluginView;

    #[test]
    fn only_a_nonempty_discard_pile_opens_a_sheet() {
        assert!(pile_openable(ZoneKind::Discard, 1));
        assert!(!pile_openable(ZoneKind::Discard, 0));
        assert!(!pile_openable(ZoneKind::Deck, 3));
        assert!(!pile_openable(ZoneKind::Battlefield, 3));
    }

    #[test]
    fn clicking_the_open_piles_own_zone_and_seat_closes_it_again() {
        let trash = agni_riftbound::ZONE_TRASH;
        let banished = agni_riftbound::ZONE_BANISHMENT;
        assert_eq!(
            toggle_pile(None, trash, PlayerId(0)),
            Some((trash, PlayerId(0)))
        );
        assert_eq!(
            toggle_pile(Some((trash, PlayerId(0))), trash, PlayerId(0)),
            None,
            "clicking the same pile again is a close, not a reopen"
        );
        assert_eq!(
            toggle_pile(Some((trash, PlayerId(0))), trash, PlayerId(1)),
            Some((trash, PlayerId(1))),
            "clicking the other seat's copy of the same zone switches to it"
        );
        assert_eq!(
            toggle_pile(Some((trash, PlayerId(0))), banished, PlayerId(0)),
            Some((banished, PlayerId(0))),
            "clicking a different pile switches to it, never toggling closed"
        );
    }

    #[test]
    fn a_tapped_pile_card_stays_previewed_until_another_target_replaces_it() {
        let tapped = Some(CardId(7));
        assert_eq!(pile_target(None, tapped), tapped);
        assert_eq!(pile_target(Some(CardId(8)), tapped), Some(CardId(8)));
    }

    #[test]
    fn pile_previews_require_public_revealed_discard_faces_for_either_seat() {
        let mut table = Table::new();
        let mut view = TableView {
            zones: agni_riftbound::zone_table(),
            ..Default::default()
        };
        let trash = Zone::Plugin(agni_riftbound::ZONE_TRASH);
        for seat in [PlayerId(0), PlayerId(1)] {
            let id = table.add_face(seat, trash, agni_core::CardFace::named("Public spell"));
            let card = table.get(id).unwrap();
            assert_eq!(pile_preview_name(card, &view), None);
            view.revealed.push(id.0);
            assert_eq!(pile_preview_name(card, &view), Some("Public spell"));
            let concealed = agni_core::Card {
                face: agni_core::CardFace::hidden(),
                ..card.clone()
            };
            assert_eq!(pile_preview_name(&concealed, &view), None);
            let hand_card = agni_core::Card {
                zone: Zone::Plugin(agni_riftbound::ZONE_HAND),
                ..card.clone()
            };
            assert_eq!(pile_preview_name(&hand_card, &view), None);
        }
        view.zones
            .iter_mut()
            .find(|decl| decl.id == agni_riftbound::ZONE_TRASH)
            .unwrap()
            .visibility = agni_sim::wire::ZoneVisibility::Owner;
        for card in table.cards() {
            assert_eq!(pile_preview_name(card, &view), None);
        }
    }

    #[test]
    fn pile_actions_preserve_offered_bytes_and_exclude_unavailable_actions() {
        use agni_sim::wire::{Affordance, AffordanceKind, PromptSummary};
        let card = CardId(7);
        let offered = Affordance {
            label: "Reflow {card 7}".into(),
            card: Some(card.0),
            enabled: true,
            data: vec![12, 34, 56].into(),
            ..Default::default()
        };
        let mut view = PluginView {
            affordances: vec![
                offered.clone(),
                Affordance {
                    enabled: false,
                    ..offered.clone()
                },
                offered.clone(),
                Affordance {
                    card: Some(8),
                    ..offered.clone()
                },
                Affordance {
                    kind: AffordanceKind::Reveal { roll: 1 },
                    ..offered.clone()
                },
            ],
            hidden: vec![2],
            ..Default::default()
        };
        assert_eq!(pile_actions(&view, PlayerId(0), card), vec![0]);
        assert_eq!(
            view.affordances[pile_actions(&view, PlayerId(0), card)[0]],
            offered
        );
        view.prompt = Some(PromptSummary {
            seat: 0,
            ..Default::default()
        });
        assert_eq!(pile_actions(&view, PlayerId(0), card), vec![0]);
        assert!(pile_actions(&view, PlayerId(1), card).is_empty());
        view.affordances.clear();
        assert!(pile_actions(&view, PlayerId(0), card).is_empty());
    }

    #[test]
    fn inline_pile_preview_fits_portrait_and_short_landscape_phones() {
        for (width, height) in [(288.0, 256.0), (608.0, 128.0), (100.0, 256.0)] {
            let size = pile_preview_size(width, height);
            assert!(size.x > 0.0 && size.y > 0.0);
            assert!(size.x <= width && size.y <= height);
            assert!((size.y / size.x - 1.4).abs() < 0.001);
        }
    }

    #[test]
    fn the_hand_chip_counts_the_visible_window_and_the_wheel_scrolls_within_it() {
        assert_eq!(hand_window(0.0, 5, 7.0), None);
        assert_eq!(hand_window(0.0, 9, 7.0), Some((1, 7, 9)));
        assert_eq!(hand_window(2.0, 9, 7.0), Some((3, 9, 9)));
        assert_eq!(hand_scroll_step(0.0, 1.0, 9, 7.0), 1.0);
        assert_eq!(
            hand_scroll_step(1.5, 1.0, 9, 7.0),
            2.0,
            "clamped to the last window"
        );
        assert_eq!(hand_scroll_step(1.0, -3.0, 9, 7.0), 0.0);
        assert_eq!(
            hand_scroll_step(3.0, 0.0, 5, 7.0),
            0.0,
            "a hand that fits has no scroll"
        );
    }

    #[test]
    fn the_far_side_carries_its_owner_and_a_battlefield_its_control() {
        assert_eq!(
            owner_prefix(true, false, "claude", "Battlefield 1"),
            "Battlefield 1"
        );
        assert_eq!(owner_prefix(false, true, "rae", "Base"), "Base");
        assert_eq!(
            owner_prefix(false, false, "claude", "Base"),
            "claude's Base"
        );
        let view = PluginView {
            status: vec![
                "turn 3 · {seat 0} · action phase · rules enforced".into(),
                "{zone 9} held by {seat 0}, contested by {seat 1} · {zone 10} contested by {seat 1}".into(),
            ],
            ..Default::default()
        };
        assert_eq!(
            control_of(&view, 9),
            Some(plate::Control {
                zone: 9,
                held: Some(0),
                contested: Some(1)
            })
        );
        assert_eq!(control_of(&view, 10).unwrap().held, None);
        assert_eq!(control_of(&view, 11), None);
    }

    #[test]
    fn the_combat_plate_reads_the_two_might_totals_at_the_contested_battlefield() {
        let view = PluginView {
            status: vec![
                "turn 3 · {seat 0} · action phase · rules enforced".into(),
                "combat at {zone 10} · {seat 0} against {seat 1}".into(),
                "attackers {card 71}, {card 72} (5 might) · defenders {card 80} (2 might)".into(),
            ],
            ..Default::default()
        };
        assert_eq!(combat_summary(&view), Some((10, 0, 1, 5, 2)));
        let showdown = PluginView {
            status: vec![
                "showdown at {zone 10} · {seat 0} against {seat 1} · focus {seat 1}".into(),
            ],
            ..Default::default()
        };
        assert_eq!(
            combat_summary(&showdown),
            None,
            "a showdown without combat has no plate"
        );
    }
}
