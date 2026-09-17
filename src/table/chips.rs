use super::hud::{self, Hud};
use super::interaction::{DropChooser, Pinned};
use super::*;
use agni_sim::wire::{LegalKind, PluginView, ZoneOwner};

pub const CHIP_H: f32 = 40.0;
pub const CHIP_MIN_W: f32 = 48.0;
pub const CHIP_GAP: f32 = 6.0;
pub const MAX_SHOWN: usize = 4;
pub const ROW_SLACK: f32 = 8.0;
pub const NOT_NOW: &str = "not available right now";
pub const MORE: &str = "more…";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipAction {
    Affordance(usize),
    Play { zone: u16 },
    Move { zone: u16 },
    Hide { zone: u16 },
    PlayFromFacedown,
    Reveal,
    Inspect,
    Exhaust { on: bool },
    Trash,
    Recycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub label: String,
    pub reason: Option<String>,
    pub action: ChipAction,
    pub group: u8,
}

impl Chip {
    fn new(label: impl Into<String>, action: ChipAction, group: u8) -> Self {
        Self {
            label: label.into(),
            reason: None,
            action,
            group,
        }
    }

    pub fn enabled(&self) -> bool {
        self.reason.is_none()
    }

    pub fn is_offer(&self) -> bool {
        self.group <= 2
    }
}

pub fn offers(
    view: &PluginView,
    rims: &highlight::Rims,
    table: &Table,
    mirror: &Mirror,
    me: PlayerId,
    card: u32,
    zone_name: &dyn Fn(u16) -> String,
) -> Vec<Chip> {
    let mut out = Vec::new();
    for (index, affordance) in view.affordances.iter().enumerate() {
        if affordance.card != Some(card)
            || matches!(
                affordance.kind,
                agni_sim::wire::AffordanceKind::Reveal { .. }
            )
        {
            continue;
        }
        let mut chip = Chip::new(affordance.label.clone(), ChipAction::Affordance(index), 1);
        if !affordance.enabled {
            chip.reason = Some(NOT_NOW.into());
        }
        out.push(chip);
    }
    let held = table.get(CardId(card));
    let mine = held.is_some_and(|held| held.owner == me);
    let facedown = held.is_some_and(|held| sync::lies_facedown(held, mirror));
    let chain = zones::stack_zone(&mirror.view.zones);
    if mine && !facedown && view.prompt.is_none() {
        for row in view.legal.iter().filter(|row| row.card == card) {
            let plays = row
                .kinds
                .iter()
                .any(|kind| matches!(kind, LegalKind::Play { .. } | LegalKind::React));
            if plays && !row.zones.is_empty() {
                match chain.filter(|chain| row.zones.contains(chain)) {
                    Some(chain) => out.push(Chip::new("play", ChipAction::Play { zone: chain }, 1)),
                    None => {
                        for zone in &row.zones {
                            out.push(Chip::new(
                                format!("play › {}", zone_name(*zone)),
                                ChipAction::Play { zone: *zone },
                                1,
                            ));
                        }
                    }
                }
            }
            if row.kinds.contains(&LegalKind::March) {
                for zone in &row.zones {
                    out.push(Chip::new(
                        format!("move › {}", zone_name(*zone)),
                        ChipAction::Move { zone: *zone },
                        1,
                    ));
                }
            }
        }
    }
    for offer in plugin_ui::hidden_actions(view, rims, card, mine, facedown) {
        let action = match offer {
            plugin_ui::HiddenAction::Hide { zone } => ChipAction::Hide { zone },
            plugin_ui::HiddenAction::PlayFromFacedown => ChipAction::PlayFromFacedown,
            plugin_ui::HiddenAction::Reveal => ChipAction::Reveal,
        };
        out.push(Chip::new(offer.label(zone_name), action, 2));
    }
    if out.is_empty() && rims.greys(card) {
        out.push(Chip {
            label: "play".into(),
            reason: Some(plugin_ui::GREYED_HINT.into()),
            action: ChipAction::Play {
                zone: chain.unwrap_or(0),
            },
            group: 1,
        });
    }
    out
}

pub fn chips(mut offers: Vec<Chip>, free: bool, guarded: bool, exhausted: bool) -> Vec<Chip> {
    offers.push(Chip::new("inspect", ChipAction::Inspect, 3));
    if free && !guarded {
        offers.push(Chip::new(
            if exhausted { "ready" } else { "exhaust" },
            ChipAction::Exhaust { on: !exhausted },
            3,
        ));
        offers.push(Chip::new("trash", ChipAction::Trash, 3));
        offers.push(Chip::new("recycle", ChipAction::Recycle, 3));
    }
    offers
}

pub fn default_action(chips: &[Chip]) -> Option<&Chip> {
    let mut live = chips
        .iter()
        .filter(|chip| chip.is_offer() && chip.enabled());
    let first = live.next()?;
    live.next().is_none().then_some(first)
}

pub fn shown(chips: &[Chip], expanded: bool) -> (&[Chip], usize) {
    let has_location = chips.iter().any(|chip| {
        matches!(
            chip.action,
            ChipAction::Play { .. } | ChipAction::Move { .. } | ChipAction::Hide { .. }
        )
    });
    if expanded || chips.len() <= MAX_SHOWN || has_location {
        return (chips, 0);
    }
    (&chips[..MAX_SHOWN], chips.len() - MAX_SHOWN)
}

pub fn digit_key(key: KeyCode) -> Option<usize> {
    Some(match key {
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        _ => return None,
    })
}

pub fn by_key(chips: &[Chip], key: KeyCode) -> Option<&Chip> {
    let wanted = |chip: &&Chip| match key {
        KeyCode::KeyF => matches!(chip.action, ChipAction::Hide { .. }),
        KeyCode::KeyP => matches!(chip.action, ChipAction::PlayFromFacedown),
        KeyCode::KeyR => matches!(chip.action, ChipAction::Reveal),
        KeyCode::KeyE => matches!(chip.action, ChipAction::Exhaust { .. }),
        KeyCode::KeyT => matches!(chip.action, ChipAction::Trash),
        _ => false,
    };
    chips.iter().filter(|chip| chip.enabled()).find(wanted)
}

pub fn card_chip_text(digit: Option<usize>, chip: &Chip) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let ink = if chip.enabled() {
        hud::INK
    } else {
        hud::INK_WEAK
    };
    if let Some(digit) = digit {
        job.append(
            &format!("{digit} "),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(11.0),
                color: hud::INK_WEAK,
                ..Default::default()
            },
        );
    }
    job.append(
        &chip.label,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(14.0),
            color: ink,
            ..Default::default()
        },
    );
    if let Some(reason) = &chip.reason {
        job.append(
            &format!("\n{reason}"),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(11.0),
                color: hud::INK_WEAK,
                ..Default::default()
            },
        );
    }
    job
}

pub fn chip_widget(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>, enabled: bool) -> bool {
    let button = egui::Button::new(text)
        .fill(if enabled {
            hud::SURFACE_2
        } else {
            egui::Color32::from_rgba_premultiplied(30, 30, 36, 200)
        })
        .stroke(egui::Stroke::new(1.0, hud::HAIRLINE))
        .corner_radius(12.0)
        .min_size(egui::vec2(CHIP_MIN_W, CHIP_H));
    ui.add_enabled(enabled, button).clicked()
}

pub fn screen_rect(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &GlobalTransform,
    landscape: bool,
) -> Option<egui::Rect> {
    let (width, height) = if landscape {
        (dim::CARD_H, dim::CARD_W)
    } else {
        (dim::CARD_W, dim::CARD_H)
    };
    let mut rect = egui::Rect::NOTHING;
    for corner in plugin_ui::rim_corners(width, height) {
        let screen = camera
            .world_to_viewport(camera_transform, transform.transform_point(corner))
            .ok()?;
        rect.extend_with(egui::pos2(screen.x, screen.y));
    }
    (rect != egui::Rect::NOTHING).then_some(rect)
}

pub fn row_anchor(card: egui::Rect, above: bool) -> (egui::Pos2, egui::Align2) {
    if above {
        (
            egui::pos2(card.center().x, card.min.y - CHIP_GAP),
            egui::Align2::CENTER_BOTTOM,
        )
    } else {
        (
            egui::pos2(card.center().x, card.max.y + CHIP_GAP),
            egui::Align2::CENTER_TOP,
        )
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Act<'w> {
    pub dropped: MessageWriter<'w, CardDropped>,
    pub reveals: MessageWriter<'w, RevealRequested>,
    pub exhaust: MessageWriter<'w, ExhaustToggled>,
    pub sender: hud::Sender<'w>,
    pub pinned: ResMut<'w, Pinned>,
}

impl Act<'_> {
    #[allow(clippy::too_many_arguments)]
    pub fn perform(
        &mut self,
        chip: &Chip,
        view: &PluginView,
        table: &Table,
        mirror: &Mirror,
        me: PlayerId,
        card: CardId,
        entity: Entity,
    ) {
        let zones = &mirror.view.zones;
        let seat_for = |zone: u16| match agni_sim::wire::zone_owner(zones, Zone::Plugin(zone)) {
            Some(ZoneOwner::PerSeat) => me,
            _ => PlayerId(0),
        };
        let drop_to = |zone: u16, hidden: bool| {
            let to = Zone::Plugin(zone);
            let seat = seat_for(zone);
            CardDropped {
                card,
                to,
                seat,
                index: table.in_area(seat, to).count(),
                hidden,
            }
        };
        match chip.action {
            ChipAction::Affordance(index) => {
                if let Some(affordance) = view.affordances.get(index) {
                    self.sender.fire(affordance);
                }
            }
            ChipAction::Play { zone } => {
                let drop = match zones::stack_zone(zones) {
                    Some(chain) if chain == zone => {
                        interaction::chain_drop(table, mirror, me, card)
                    }
                    _ => Some(drop_to(zone, false)),
                };
                if let Some(drop) = drop {
                    self.dropped.write(drop);
                }
            }
            ChipAction::Move { zone } => {
                self.dropped.write(drop_to(zone, false));
            }
            ChipAction::Hide { zone } => {
                self.dropped.write(drop_to(zone, true));
            }
            ChipAction::PlayFromFacedown => {
                if let Some(drop) = interaction::chain_drop(table, mirror, me, card) {
                    self.dropped.write(drop);
                }
            }
            ChipAction::Reveal => {
                self.reveals.write(RevealRequested(card));
            }
            ChipAction::Inspect => {
                self.pinned.0 = Some(entity);
            }
            ChipAction::Exhaust { on } => {
                self.exhaust.write(ExhaustToggled { card, on });
            }
            ChipAction::Trash => {
                if let Some((to, seat, index)) = zones::trash_move(zones, table, me, card) {
                    self.dropped.write(CardDropped {
                        card,
                        to,
                        seat,
                        index,
                        hidden: false,
                    });
                }
            }
            ChipAction::Recycle => {
                if let Some((to, seat, index)) = zones::recycle_move(zones, table, me, card) {
                    self.dropped.write(CardDropped {
                        card,
                        to,
                        seat,
                        index,
                        hidden: false,
                    });
                }
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct CardChips<'w, 's> {
    pub panel: Res<'w, plugin_ui::PluginPanel>,
    pub rims: Res<'w, highlight::Rims>,
    pub tools: Res<'w, plugin_ui::Tools>,
    pub seats: hud::Seats<'w>,
    pub selected: Res<'w, Selected>,
    pub hovered: Query<'w, 's, Entity, With<Hovered>>,
    pub views: Query<'w, 's, &'static CardView>,
}

impl CardChips<'_, '_> {
    pub fn focus(&self) -> Option<Entity> {
        self.hovered
            .iter()
            .next()
            .or(self.selected.0)
            .filter(|entity| self.views.contains(*entity))
    }

    pub fn for_entity(&self, entity: Entity) -> Option<(CardId, Vec<Chip>)> {
        let view = self.views.get(entity).ok()?;
        let card = view.0;
        let table = &self.seats.table.0;
        let mirror = &*self.seats.mirror;
        let me = self.seats.me();
        let zone_name = |zone: u16| plugin_ui::zone_label(&mirror.view.zones, zone);
        let seat_name = |seat: u8| self.seats.name(seat);
        let card_name = |card: u32| plugin_ui::card_label(table, &mirror.view, me, card);
        let mut offers = offers(
            &self.panel.view,
            &self.rims,
            table,
            mirror,
            me,
            card.0,
            &zone_name,
        );
        for chip in &mut offers {
            chip.label = plugin_ui::expand(&chip.label, &seat_name, &zone_name, &card_name);
        }
        let guarded = interaction::guarded_from_me(table, mirror, me, card);
        let exhausted = mirror.rotated(card.0);
        Some((card, chips(offers, self.tools.free, guarded, exhausted)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShownRow {
    pub entity: Entity,
    pub rect: egui::Rect,
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct ChipRow(pub Option<ShownRow>);

#[allow(clippy::too_many_arguments)]
pub(super) fn card_chips_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    held: Res<Held>,
    menu: Res<crate::menu::Menu>,
    chooser: Res<DropChooser>,
    keys: Res<ButtonInput<KeyCode>>,
    claimed: Res<plugin_ui::ClaimedKeys>,
    input: CardChips,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(&GlobalTransform, &ViewVisibility, Has<Landscape>, &Slot), With<CardView>>,
    mut act: Act,
    mut last: Local<Option<ShownRow>>,
    mut expanded: Local<bool>,
    mut published: ResMut<ChipRow>,
) -> Result {
    let shown_now = card_chips_row(
        &mut contexts,
        &hud,
        &held,
        &menu,
        &chooser,
        &keys,
        &claimed,
        &input,
        &camera,
        &cards,
        &mut act,
        &mut last,
        &mut expanded,
    );
    if published.0 != *last {
        published.0 = *last;
    }
    shown_now
}

#[allow(clippy::too_many_arguments)]
fn card_chips_row(
    contexts: &mut EguiContexts,
    hud: &Hud,
    held: &Held,
    menu: &crate::menu::Menu,
    chooser: &DropChooser,
    keys: &ButtonInput<KeyCode>,
    claimed: &plugin_ui::ClaimedKeys,
    input: &CardChips,
    camera: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: &Query<(&GlobalTransform, &ViewVisibility, Has<Landscape>, &Slot), With<CardView>>,
    act: &mut Act,
    last: &mut Option<ShownRow>,
    expanded: &mut bool,
) -> Result {
    if !menu.at_table() || held.card.is_some() || chooser.0.is_some() {
        *last = None;
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let pointer = context.input(|input| input.pointer.latest_pos());
    let focus = input.focus().or_else(|| {
        let row = (*last)?;
        let inside = pointer.is_some_and(|at| row.rect.expand(ROW_SLACK).contains(at));
        (inside && input.views.contains(row.entity)).then_some(row.entity)
    });
    let Some(entity) = focus else {
        *last = None;
        *expanded = false;
        return Ok(());
    };
    if last.is_none_or(|row| row.entity != entity) {
        *expanded = false;
    }
    let Some((card, chips)) = input.for_entity(entity) else {
        *last = None;
        return Ok(());
    };
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let Ok((transform, visibility, landscape, slot)) = cards.get(entity) else {
        *last = None;
        return Ok(());
    };
    if !visibility.get() {
        *last = None;
        return Ok(());
    }
    let Some(rect) = screen_rect(camera, camera_transform, transform, landscape) else {
        return Ok(());
    };
    let above = slot.facing == Facing::Camera;
    let (anchor, pivot) = row_anchor(rect, above);
    let (visible, more) = shown(&chips, *expanded);
    let mut fired: Option<Chip> = None;
    let mut open_more = false;
    let response = egui::Area::new(egui::Id::new("card chips"))
        .fixed_pos(anchor)
        .pivot(pivot)
        .order(egui::Order::Middle)
        .constrain_to(hud.0.stage)
        .show(&context, |ui| {
            ui.set_max_width((hud.0.stage.width() - ROW_SLACK * 2.0).max(CHIP_MIN_W));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = CHIP_GAP;
                for (index, chip) in visible.iter().enumerate() {
                    let digit = (index < 9).then_some(index + 1);
                    if chip_widget(ui, card_chip_text(digit, chip), chip.enabled()) {
                        fired = Some(chip.clone());
                    }
                }
                if more > 0 && chip_widget(ui, MORE, true) {
                    open_more = true;
                }
            });
        });
    *last = Some(ShownRow {
        entity,
        rect: response.response.rect,
    });
    if open_more {
        *expanded = true;
    }
    if fired.is_none() && !context.egui_wants_keyboard_input() {
        let mine = input
            .panel
            .view
            .prompt
            .as_ref()
            .is_some_and(|summary| summary.seat == input.seats.me().0);
        for key in keys.get_just_pressed() {
            if claimed.taken(*key) {
                continue;
            }
            if *key == KeyCode::Enter {
                fired = default_action(&chips).cloned();
            } else if let Some(digit) = digit_key(*key) {
                if mine {
                    continue;
                }
                fired = visible
                    .get(digit - 1)
                    .filter(|chip| chip.enabled())
                    .cloned();
            }
        }
    }
    if let Some(chip) = fired {
        let table = &input.seats.table.0;
        act.perform(
            &chip,
            &input.panel.view,
            table,
            &input.seats.mirror,
            input.seats.me(),
            card,
            entity,
        );
    }
    Ok(())
}

pub fn strip_digits_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    claimed: Res<plugin_ui::ClaimedKeys>,
    panel: Res<plugin_ui::PluginPanel>,
    my_seat: Res<MySeat>,
    menu: Res<crate::menu::Menu>,
    mut sender: hud::Sender,
) -> Result {
    if !menu.at_table() {
        return Ok(());
    }
    let mine = panel
        .view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == my_seat.0 .0);
    if !mine || contexts.ctx_mut()?.egui_wants_keyboard_input() {
        return Ok(());
    }
    let chips = plugin_ui::strip_chips(&panel.view);
    for key in keys.get_just_pressed() {
        if claimed.taken(*key) {
            continue;
        }
        let indexed = chips
            .iter()
            .find(|index| plugin_ui::effective_hotkey(&panel.view, **index) == Some(*key))
            .copied()
            .or_else(|| digit_key(*key).and_then(|digit| chips.get(digit - 1).copied()));
        if let Some(index) = indexed {
            sender.fire(&panel.view.affordances[index]);
            return Ok(());
        }
    }
    Ok(())
}

pub fn drop_chooser_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    menu: Res<crate::menu::Menu>,
    mut chooser: ResMut<DropChooser>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut dropped: MessageWriter<CardDropped>,
) -> Result {
    let Some(pending) = chooser.0 else {
        return Ok(());
    };
    if !menu.at_table() {
        chooser.0 = None;
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let Ok(screen) = camera.world_to_viewport(camera_transform, pending.at) else {
        chooser.0 = None;
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let mut choice: Option<bool> = None;
    let response = egui::Area::new(egui::Id::new("drop chooser"))
        .fixed_pos(egui::pos2(screen.x, screen.y))
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Foreground)
        .constrain_to(hud.0.stage)
        .show(&context, |ui| {
            hud::panel_frame(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = CHIP_GAP;
                    if chip_widget(ui, "play here", true) {
                        choice = Some(false);
                    }
                    if chip_widget(ui, "hide here", true) {
                        choice = Some(true);
                    }
                });
            });
        });
    let outside = context.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|at| !response.response.rect.expand(ROW_SLACK).contains(at))
    });
    match choice {
        Some(hidden) => {
            dropped.write(CardDropped {
                card: pending.card,
                to: pending.to,
                seat: pending.seat,
                index: pending.index,
                hidden,
            });
            chooser.0 = None;
        }
        None if outside => chooser.0 = None,
        None => {}
    }
    Ok(())
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct MarchAll {
    pub zone: Option<u16>,
    pub sent_for: Option<Vec<String>>,
}

pub fn march_all_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    menu: Res<crate::menu::Menu>,
    panel: Res<plugin_ui::PluginPanel>,
    seats: hud::Seats,
    mut all: ResMut<MarchAll>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    let me = seats.me().0;
    let Some(plugin_ui::PromptKind::GroupMove { zone }) = plugin_ui::prompt_kind(&panel.view, me)
    else {
        if all.zone.is_some() {
            *all = MarchAll::default();
        }
        return Ok(());
    };
    if !menu.at_table() || all.zone == Some(zone) {
        return Ok(());
    }
    let others = plugin_ui::group_move_options(&panel.view);
    if others.is_empty() {
        return Ok(());
    }
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let players = seats.players.0;
    let anchor = zones::anchors(
        &seats.mirror.view.zones,
        players,
        seats.info.battlefields_in_play(players),
    )
    .into_iter()
    .find(|anchor| anchor.zone == zone && anchor.seat.is_none_or(|seat| seat.0 == me));
    let Some(anchor) = anchor else {
        return Ok(());
    };
    let Ok(screen) = camera.world_to_viewport(camera_transform, anchor.position) else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let mut pressed = false;
    egui::Area::new(egui::Id::new("march all"))
        .fixed_pos(egui::pos2(screen.x, screen.y))
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Middle)
        .constrain_to(hud.0.stage)
        .show(&context, |ui| {
            if chip_widget(ui, format!("all · move {}", others.len()), true) {
                pressed = true;
            }
        });
    if pressed {
        *all = MarchAll {
            zone: Some(zone),
            sent_for: None,
        };
    }
    Ok(())
}

pub fn drive_march_all(
    panel: Res<plugin_ui::PluginPanel>,
    my_seat: Res<MySeat>,
    mut all: ResMut<MarchAll>,
    mut sender: hud::Sender,
) {
    let Some(zone) = all.zone else {
        return;
    };
    let view = &panel.view;
    match plugin_ui::prompt_kind(view, my_seat.0 .0) {
        Some(plugin_ui::PromptKind::GroupMove { zone: open }) if open == zone => {}
        _ => {
            *all = MarchAll::default();
            return;
        }
    }
    let labels: Vec<String> = view
        .affordances
        .iter()
        .map(|affordance| affordance.label.clone())
        .collect();
    if all.sent_for.as_ref() == Some(&labels) {
        return;
    }
    let next = plugin_ui::group_move_options(view)
        .first()
        .copied()
        .or_else(|| primary::confirm_index(view));
    let Some(index) = next else {
        *all = MarchAll::default();
        return;
    };
    sender.fire(&view.affordances[index]);
    all.sent_for = Some(labels);
}

pub const MULLIGAN_FACE_W: f32 = 80.0;
pub const MULLIGAN_FACE_H: f32 = 112.0;
pub const MULLIGAN_FACE_MIN_W: f32 = 48.0;

pub fn mulligan_face(inner_w: f32, faces: usize, gap: f32) -> egui::Vec2 {
    let room = inner_w - gap * faces.saturating_sub(1) as f32;
    let width = if faces == 0 {
        MULLIGAN_FACE_W
    } else {
        (room / faces as f32).clamp(MULLIGAN_FACE_MIN_W, MULLIGAN_FACE_W)
    };
    egui::vec2(width, width * MULLIGAN_FACE_H / MULLIGAN_FACE_W)
}

pub fn mulligan_marks(
    hand: &[u32],
    offered: &BTreeSet<u32>,
    full: bool,
    tapped: &BTreeSet<u32>,
) -> BTreeSet<u32> {
    let mut marks: BTreeSet<u32> = tapped.iter().copied().collect();
    if !full {
        marks.extend(hand.iter().copied().filter(|card| !offered.contains(card)));
    }
    marks
}

#[allow(clippy::too_many_arguments)]
pub fn mulligan_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    menu: Res<crate::menu::Menu>,
    panel: Res<plugin_ui::PluginPanel>,
    seats: hud::Seats,
    mut art: hud::Art,
    intent: Res<toast::LastIntent>,
    mut sender: hud::Sender,
    mut tapped: Local<BTreeSet<u32>>,
) -> Result {
    let me = seats.me();
    if !menu.at_table()
        || plugin_ui::prompt_kind(&panel.view, me.0) != Some(plugin_ui::PromptKind::Mulligan)
    {
        if !tapped.is_empty() {
            tapped.clear();
        }
        return Ok(());
    }
    if intent.is_changed() {
        if let Some(card) = intent.card {
            tapped.insert(card);
        }
    }
    let table = &seats.table.0;
    let hand: Vec<u32> = my_hand_ids(table, &seats.mirror, me)
        .into_iter()
        .map(|id| id.0)
        .collect();
    let offered: BTreeSet<u32> = plugin_ui::highlighted(&panel.view);
    let summary = panel.view.prompt.as_ref();
    let full = summary.is_some_and(|summary| summary.picked >= summary.max);
    let marks = mulligan_marks(&hand, &offered, full, &tapped);
    let picked = summary.map_or(0, |summary| summary.picked);
    let max = summary.map_or(0, |summary| summary.max);
    let context = contexts.ctx_mut()?.clone();
    let rect = hud.0.banner;
    let mut fire: Option<usize> = None;
    let faces: Vec<(u32, Option<egui::TextureId>, String)> = hand
        .iter()
        .map(|card| {
            let name = table
                .get(CardId(*card))
                .map(|held| held.face.name.clone())
                .unwrap_or_default();
            let texture = art.texture(&mut contexts, &name);
            (*card, texture, name)
        })
        .collect();
    hud::slot(&context, "mulligan", rect, |ui| {
        hud::panel_frame(ui.style()).show(ui, |ui| {
            ui.set_max_width(rect.width() - 16.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("set aside up to {max} to redraw"))
                        .strong()
                        .color(hud::INK),
                );
                ui.label(
                    egui::RichText::new(format!("{picked} set aside"))
                        .size(11.0)
                        .color(hud::INK)
                        .background_color(hud::SURFACE_2),
                );
            });
            let size = mulligan_face(
                rect.width() - 32.0,
                faces.len(),
                ui.spacing().item_spacing.x + 4.0,
            );
            egui::ScrollArea::horizontal()
                .id_salt("mulligan faces")
                .show(ui, |ui| {
                    ui.spacing_mut().button_padding = egui::vec2(2.0, 2.0);
                    ui.horizontal_top(|ui| {
                        for (card, texture, name) in &faces {
                            let marked = marks.contains(card);
                            let option = panel.view.affordances.iter().position(|affordance| {
                                affordance.enabled && affordance.card == Some(*card)
                            });
                            let hit = match texture {
                                Some(texture) => ui.add_enabled(
                                    option.is_some(),
                                    egui::Button::image(egui::load::SizedTexture::new(
                                        *texture, size,
                                    )),
                                ),
                                None => {
                                    let (face, response) =
                                        ui.allocate_exact_size(size, egui::Sense::click());
                                    ui.painter().rect_filled(
                                        face,
                                        4.0,
                                        egui::Color32::from_gray(58),
                                    );
                                    ui.painter().text(
                                        face.center(),
                                        egui::Align2::CENTER_CENTER,
                                        name,
                                        egui::FontId::proportional(10.0),
                                        hud::INK,
                                    );
                                    response
                                }
                            };
                            if marked {
                                let corner = hit.rect.right_top() + egui::vec2(-10.0, 10.0);
                                ui.painter().circle_filled(corner, 9.0, hud::DANGER);
                                ui.painter().text(
                                    corner,
                                    egui::Align2::CENTER_CENTER,
                                    "×",
                                    egui::FontId::proportional(14.0),
                                    hud::INK,
                                );
                            }
                            if hit.clicked() {
                                fire = option;
                            }
                        }
                    });
                });
        });
    });
    if let Some(index) = fire {
        let affordance = &panel.view.affordances[index];
        if let Some(card) = affordance.card {
            tapped.insert(card);
        }
        sender.fire(affordance);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Affordance, AffordanceKind, Legal, PromptSummary};
    use serde_bytes::ByteBuf;
    use std::collections::BTreeMap;

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

    fn react_row(card: u32) -> Legal {
        Legal {
            card,
            kinds: vec![LegalKind::React],
            zones: vec![agni_riftbound::ZONE_CHAIN],
            hidden: Vec::new(),
        }
    }

    fn enforced(legal: Vec<Legal>, affordances: Vec<Affordance>) -> PluginView {
        PluginView {
            status: vec!["turn 3 · {seat 0} · action phase · rules enforced".into()],
            legal,
            affordances,
            ..Default::default()
        }
    }

    fn offer(label: &str, card: Option<u32>, enabled: bool) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: None,
            enabled,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![1]),
            card,
        }
    }

    fn zone_name(zone: u16) -> String {
        format!("zone {zone}")
    }

    fn labels(chips: &[Chip]) -> Vec<&str> {
        chips.iter().map(|chip| chip.label.as_str()).collect()
    }

    #[test]
    fn the_m6_hidden_fixture_lists_play_and_reveal_for_my_facedown_card_and_nothing_for_theirs() {
        let fixture = m6_fixture();
        let me = PlayerId(0);
        let view = enforced(
            vec![react_row(fixture.cleave.0), react_row(fixture.theirs.0)],
            Vec::new(),
        );
        let rims = highlight::rims(
            &view,
            0,
            &BTreeSet::new(),
            &highlight::owners(&GameTable(fixture.table.clone())),
        );
        let mine = offers(
            &view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            me,
            fixture.cleave.0,
            &zone_name,
        );
        assert_eq!(labels(&mine), ["play from hidden", "reveal"]);
        assert!(mine.iter().all(Chip::enabled));
        assert!(
            default_action(&mine).is_none(),
            "two offers leave the second tap alone"
        );
        let theirs = offers(
            &view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            me,
            fixture.theirs.0,
            &zone_name,
        );
        assert!(theirs.is_empty(), "{theirs:?}");
        let full = chips(theirs, false, true, false);
        assert_eq!(labels(&full), ["inspect"]);
        assert!(default_action(&full).is_none());
        let free = chips(Vec::new(), true, false, true);
        assert_eq!(labels(&free), ["inspect", "ready", "trash", "recycle"]);
        assert_eq!(free[1].action, ChipAction::Exhaust { on: false });
        let guarded = chips(Vec::new(), true, true, false);
        assert_eq!(labels(&guarded), ["inspect"]);
    }

    #[test]
    fn a_hand_card_with_a_react_row_has_one_play_chip_and_that_is_the_default_action() {
        let fixture = m6_fixture();
        let view = enforced(vec![react_row(fixture.lure.0)], Vec::new());
        let rims = highlight::rims(
            &view,
            0,
            &[fixture.lure.0].into_iter().collect(),
            &BTreeMap::new(),
        );
        let mine = offers(
            &view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            fixture.lure.0,
            &zone_name,
        );
        assert_eq!(labels(&mine), ["play"]);
        assert_eq!(
            default_action(&mine).map(|chip| chip.action),
            Some(ChipAction::Play {
                zone: agni_riftbound::ZONE_CHAIN
            })
        );
        let march = enforced(
            vec![Legal {
                card: fixture.vi.0,
                kinds: vec![LegalKind::March],
                zones: vec![agni_riftbound::ZONE_BATTLEFIELD_FIRST + 1, 9],
                hidden: Vec::new(),
            }],
            Vec::new(),
        );
        let rims = highlight::rims(&march, 0, &BTreeSet::new(), &BTreeMap::new());
        let moves = offers(
            &march,
            &rims,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            fixture.vi.0,
            &zone_name,
        );
        assert_eq!(
            labels(&moves),
            [
                format!("move › zone {}", agni_riftbound::ZONE_BATTLEFIELD_FIRST + 1),
                "move › zone 9".to_string()
            ]
        );
        assert!(default_action(&moves).is_none());
    }

    #[test]
    fn a_disabled_offer_keeps_its_place_with_a_reason_and_a_greyed_card_says_why() {
        let fixture = m6_fixture();
        let view = enforced(
            Vec::new(),
            vec![
                offer("Vi: stun a unit (exhaust)", Some(fixture.vi.0), false),
                offer("Vi: draw a card", Some(fixture.vi.0), true),
            ],
        );
        let rims = highlight::Rims::default();
        let mine = offers(
            &view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            fixture.vi.0,
            &zone_name,
        );
        assert_eq!(
            labels(&mine),
            ["Vi: stun a unit (exhaust)", "Vi: draw a card"]
        );
        assert_eq!(mine[0].reason.as_deref(), Some(NOT_NOW));
        assert!(mine[1].enabled());
        assert_eq!(
            default_action(&mine).map(|chip| chip.action),
            Some(ChipAction::Affordance(1)),
            "the one enabled offer is the default; the disabled one only explains itself"
        );
        let mut grey = highlight::Rims::default();
        grey.grey.insert(fixture.lure.0);
        let hinted = offers(
            &enforced(Vec::new(), Vec::new()),
            &grey,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            fixture.lure.0,
            &zone_name,
        );
        assert_eq!(labels(&hinted), ["play"]);
        assert_eq!(hinted[0].reason.as_deref(), Some(plugin_ui::GREYED_HINT));
        assert!(default_action(&hinted).is_none());
        let text = card_chip_text(Some(1), &hinted[0]);
        assert!(text.text.starts_with("1 play\n"));
    }

    #[test]
    fn four_chips_show_and_the_rest_hide_behind_more() {
        let many: Vec<Chip> = (0..6)
            .map(|index| Chip::new(format!("chip {index}"), ChipAction::Inspect, 3))
            .collect();
        let (visible, more) = shown(&many, false);
        assert_eq!(visible.len(), MAX_SHOWN);
        assert_eq!(more, 2);
        let (all, none) = shown(&many, true);
        assert_eq!(all.len(), 6);
        assert_eq!(none, 0);
        assert_eq!(digit_key(KeyCode::Digit1), Some(1));
        assert_eq!(digit_key(KeyCode::Digit9), Some(9));
        assert_eq!(digit_key(KeyCode::Digit0), None);
        let hide = Chip::new("hide", ChipAction::Hide { zone: 9 }, 2);
        let reveal = Chip::new("reveal", ChipAction::Reveal, 2);
        let row = vec![hide.clone(), reveal.clone()];
        assert_eq!(by_key(&row, KeyCode::KeyF), Some(&hide));
        assert_eq!(by_key(&row, KeyCode::KeyH), None);
        assert_eq!(by_key(&row, KeyCode::KeyR), Some(&reveal));
        assert_eq!(by_key(&row, KeyCode::KeyP), None);
        let (anchor, pivot) = row_anchor(
            egui::Rect::from_min_max(egui::pos2(100.0, 600.0), egui::pos2(160.0, 690.0)),
            true,
        );
        assert_eq!(anchor, egui::pos2(130.0, 600.0 - CHIP_GAP));
        assert_eq!(pivot, egui::Align2::CENTER_BOTTOM);
    }

    #[test]
    fn every_legal_play_and_hide_destination_stays_clickable() {
        let fixture = m6_fixture();
        let bf1 = agni_riftbound::ZONE_BATTLEFIELD_FIRST;
        let bf2 = bf1 + 1;
        let view = enforced(
            vec![Legal {
                card: fixture.lure.0,
                kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
                zones: vec![bf1, bf2],
                hidden: vec![bf1, bf2],
            }],
            Vec::new(),
        );
        let rims = highlight::rims(
            &view,
            0,
            &[fixture.lure.0].into_iter().collect(),
            &BTreeMap::new(),
        );
        let mine = offers(
            &view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            fixture.lure.0,
            &zone_name,
        );
        assert_eq!(
            mine.iter()
                .filter_map(|chip| match chip.action {
                    ChipAction::Play { zone } | ChipAction::Hide { zone } => Some(zone),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [bf1, bf2, bf1, bf2]
        );
        assert!(mine.iter().all(|chip| chip.enabled()));
        let all_chips = chips(mine, false, true, false);
        let (visible, more) = shown(&all_chips, false);
        assert_eq!(more, 0);
        assert_eq!(visible.len(), 5);
        let visible_destinations = visible
            .iter()
            .filter_map(|chip| match chip.action {
                ChipAction::Play { zone } | ChipAction::Hide { zone } => Some(zone),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(visible_destinations, [bf1, bf2, bf1, bf2]);
        let illegal = bf2 + 1;
        assert!(!visible.iter().any(|chip| {
            matches!(
                chip.action,
                ChipAction::Play { zone } | ChipAction::Hide { zone } if zone == illegal
            )
        }));
    }

    #[test]
    fn the_prompt_pick_stays_on_the_card_not_in_the_chips() {
        let fixture = m6_fixture();
        let view = PluginView {
            prompt: Some(PromptSummary {
                seat: 0,
                why: "choose a unit to stun".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            ..enforced(
                vec![Legal {
                    card: fixture.vi.0,
                    kinds: vec![LegalKind::Answer],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                }],
                vec![offer("{card 0}", Some(fixture.vi.0), true)],
            )
        };
        let rims = highlight::rims(&view, 0, &BTreeSet::new(), &BTreeMap::new());
        let mine = offers(
            &view,
            &rims,
            &fixture.table,
            &fixture.mirror,
            PlayerId(0),
            fixture.vi.0,
            &zone_name,
        );
        assert_eq!(labels(&mine), ["{card 0}"]);
        assert_eq!(
            default_action(&mine).map(|chip| chip.action),
            Some(ChipAction::Affordance(0))
        );
    }

    #[test]
    fn the_march_fixture_rims_the_other_ready_units_and_the_primary_reads_move_2() {
        let view = PluginView {
            status: vec!["turn 3 · {seat 0} · action phase · rules enforced".into()],
            affordances: vec![
                offer("{card 90}", Some(90), true),
                offer("{card 91}", Some(91), true),
                offer("done", None, true),
                offer("free table", None, true),
            ],
            prompt: Some(PromptSummary {
                seat: 0,
                why: "move others to {zone 10} too?".into(),
                min: 0,
                max: 3,
                picked: 1,
                optional: false,
            }),
            legal: vec![
                Legal {
                    card: 90,
                    kinds: vec![LegalKind::Answer],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                },
                Legal {
                    card: 91,
                    kinds: vec![LegalKind::Answer],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                },
            ],
            ..Default::default()
        };
        let rims = highlight::rims(&view, 0, &BTreeSet::new(), &BTreeMap::new());
        for unit in [90, 91] {
            assert_eq!(rims.kind(unit), Some(highlight::RimKind::Answer));
            assert!(rims.pulses(unit));
        }
        assert_eq!(
            rims.kind(92),
            None,
            "the unit already at the battlefield wears nothing"
        );
        let primary = primary::primary_of(&view).unwrap();
        assert_eq!(primary.label, "move 2");
        assert_eq!(primary.affordance, Some(2));
        assert_eq!(
            plugin_ui::prompt_kind(&view, 0),
            Some(plugin_ui::PromptKind::GroupMove { zone: 10 })
        );
        assert_eq!(plugin_ui::group_move_options(&view), [0, 1]);
        assert!(
            plugin_ui::strip_chips(&view).is_empty(),
            "the units are answered on the felt or through the all chip, never as strip chips"
        );
        let mut all = MarchAll {
            zone: Some(10),
            sent_for: None,
        };
        let labels: Vec<String> = view.affordances.iter().map(|a| a.label.clone()).collect();
        assert_ne!(all.sent_for.as_ref(), Some(&labels));
        all.sent_for = Some(labels.clone());
        assert_eq!(
            all.sent_for.as_ref(),
            Some(&labels),
            "one pick per view refresh"
        );
    }

    #[test]
    fn mulligan_marks_read_the_missing_options_and_remember_my_taps_once_full() {
        let hand = [70, 71, 72, 73];
        let offered: BTreeSet<u32> = [71, 72, 73].into_iter().collect();
        let marks = mulligan_marks(&hand, &offered, false, &BTreeSet::new());
        assert_eq!(marks.into_iter().collect::<Vec<_>>(), [70]);
        let tapped: BTreeSet<u32> = [70, 72].into_iter().collect();
        let full = mulligan_marks(&hand, &BTreeSet::new(), true, &tapped);
        assert_eq!(
            full.into_iter().collect::<Vec<_>>(),
            [70, 72],
            "when the prompt is full no option names the rest, so the taps are the record"
        );
    }
}
