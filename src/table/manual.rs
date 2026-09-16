use super::*;
use agni_net::session::WireIntent;
use agni_plugin_sdk::manual::{Command, CONTROLS};
use agni_sim::view::{TableView, ViewCard};
use agni_sim::wire::{CounterScope, CounterTarget, PluginView, ZoneOwner, ZoneVisibility};

#[derive(Message, Debug, Clone)]
pub struct Requested(pub WireIntent);

#[derive(bevy::ecs::system::SystemParam)]
pub struct Inputs<'w> {
    pub panel: ResMut<'w, Panel>,
    pub mirror: Res<'w, Mirror>,
    pub table: Res<'w, GameTable>,
    pub me: Res<'w, MySeat>,
    pub requests: MessageWriter<'w, Requested>,
    pub tokens: ResMut<'w, tokens::TokenPanel>,
}

#[derive(Resource)]
pub struct Panel {
    selected: Option<u32>,
    search: String,
    destination: Option<u16>,
    seat: u8,
    index: u32,
    hidden: bool,
    look_count: u32,
    label: String,
    turn_number: Option<(u32, u16)>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            selected: None,
            search: String::new(),
            destination: None,
            seat: 0,
            index: 0,
            hidden: false,
            look_count: 5,
            label: String::new(),
            turn_number: None,
        }
    }
}

impl Panel {
    fn select(&mut self, card: u32, me: u8) {
        if self.selected != Some(card) {
            self.destination = None;
            self.seat = me;
            self.index = 0;
            self.hidden = false;
            self.label.clear();
        }
        self.selected = Some(card);
    }

    fn retain_selection(&mut self, view: &TableView, me: u8) {
        if !self.selected.is_some_and(|card| {
            view.card(card)
                .is_some_and(|card| accessible(card, view, me))
        }) {
            self.selected = None;
            self.destination = None;
        }
        if self
            .destination
            .is_some_and(|zone| !view.zones.iter().any(|decl| decl.id == zone))
        {
            self.destination = None;
        }
    }
}

pub fn available(view: &PluginView, role: SessionRole) -> bool {
    matches!(role, SessionRole::Host | SessionRole::Client)
        && view
            .affordances
            .iter()
            .any(|offer| offer.enabled && offer.label == CONTROLS)
}

pub fn accessible(card: &ViewCard, view: &TableView, me: u8) -> bool {
    card.seat == me
        || view.zones.iter().any(|zone| {
            card.zone == Zone::Plugin(zone.id)
                && (zone.owner == ZoneOwner::Shared || zone.visibility == ZoneVisibility::All)
        })
}

fn game(command: Command) -> WireIntent {
    WireIntent::Game {
        data: command.encode().into(),
    }
}

fn button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(label).min_size(egui::vec2(0.0, hud::TOUCH_MIN)))
        .clicked()
}

fn bounded(value: i32, min: Option<i32>, max: Option<i32>) -> i32 {
    value.clamp(min.unwrap_or(i32::MIN), max.unwrap_or(i32::MAX))
}

fn seat_name(roster: &[agni_net::session::SeatInfo], view: &TableView, seat: u8) -> String {
    roster
        .iter()
        .find(|info| info.seat == seat)
        .map(|info| info.name.trim())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            view.seats
                .iter()
                .find(|info| info.seat == seat)
                .map(|info| info.name.trim())
                .filter(|name| !name.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("Player {}", seat + 1))
}

fn counter_rows(
    ui: &mut egui::Ui,
    view: &TableView,
    target: CounterTarget,
    out: &mut Vec<WireIntent>,
) {
    let scope = match target {
        CounterTarget::Table => CounterScope::Table,
        CounterTarget::Seat(_) => CounterScope::Seat,
        CounterTarget::Card(_) => CounterScope::Card,
    };
    for decl in view.counter_table.iter().filter(|decl| decl.scope == scope) {
        let value = view.counter(target, decl.id).unwrap_or(decl.start);
        ui.push_id((format!("{target:?}"), decl.id), |ui| {
            ui.label(&decl.label);
            ui.horizontal_wrapped(|ui| {
                let mut next = value;
                if button(ui, "−") {
                    next = bounded(value.saturating_sub(decl.step), decl.min, decl.max);
                }
                ui.add(
                    egui::DragValue::new(&mut next)
                        .range(decl.min.unwrap_or(i32::MIN)..=decl.max.unwrap_or(i32::MAX)),
                );
                if button(ui, "+") {
                    next = bounded(value.saturating_add(decl.step), decl.min, decl.max);
                }
                if button(ui, "reset") {
                    next = decl.start;
                }
                next = bounded(next, decl.min, decl.max);
                if next != value {
                    if let Some(delta) = next.checked_sub(value) {
                        out.push(WireIntent::Counter {
                            target,
                            counter: decl.id,
                            delta,
                        });
                    }
                }
            });
        });
    }
}

pub fn body(
    ui: &mut egui::Ui,
    panel: &mut Panel,
    view: &TableView,
    table: &Table,
    plugin: &PluginView,
    me: u8,
    roster: &[agni_net::session::SeatInfo],
) -> Vec<WireIntent> {
    let mut out = Vec::new();
    ui.label("Rules enforcement is disabled for this game. Resolve costs, effects, scoring and turns manually.");
    panel.retain_selection(view, me);
    egui::CollapsingHeader::new("scores and player counters")
        .default_open(true)
        .show(ui, |ui| {
            for seat in &view.seats {
                ui.push_id(seat.seat, |ui| {
                    ui.strong(seat_name(roster, view, seat.seat));
                    counter_rows(ui, view, CounterTarget::Seat(seat.seat), &mut out);
                });
            }
            ui.strong("table counters");
            counter_rows(ui, view, CounterTarget::Table, &mut out);
        });
    egui::CollapsingHeader::new("turn and battlefield control")
        .default_open(true)
        .show(ui, |ui| {
            if let Some(turn) = &plugin.turn {
                if panel
                    .turn_number
                    .is_none_or(|(live, _)| live != turn.number)
                {
                    panel.turn_number =
                        Some((turn.number, u16::try_from(turn.number).unwrap_or(u16::MAX)));
                }
                let number = &mut panel.turn_number.as_mut().unwrap().1;
                ui.horizontal_wrapped(|ui| {
                    ui.label("turn");
                    ui.add(egui::DragValue::new(number).range(1..=u16::MAX));
                    for seat in &view.seats {
                        if button(ui, &seat_name(roster, view, seat.seat)) {
                            out.push(game(Command::Turn {
                                seat: seat.seat,
                                number: *number,
                            }));
                        }
                    }
                });
            }
            for zone in view.zones.iter().filter(|zone| {
                zone.owner == ZoneOwner::Shared
                    && zone.kind == agni_sim::wire::ZoneKind::Battlefield
            }) {
                ui.push_id(zone.id, |ui| {
                    ui.label(&zone.label);
                    ui.horizontal_wrapped(|ui| {
                        if button(ui, "uncontrolled") {
                            out.push(game(Command::Control {
                                zone: zone.id,
                                seat: None,
                            }));
                        }
                        for seat in &view.seats {
                            if button(ui, &seat_name(roster, view, seat.seat)) {
                                out.push(game(Command::Control {
                                    zone: zone.id,
                                    seat: Some(seat.seat),
                                }));
                            }
                        }
                    });
                });
            }
        });
    egui::CollapsingHeader::new("decks · draw, look, search and shuffle")
        .default_open(true)
        .show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("look at top");
            ui.add(egui::DragValue::new(&mut panel.look_count).range(1..=1000));
        });
        for zone in view.zones.iter().filter(|zone| zone.kind == agni_sim::wire::ZoneKind::Deck) {
            let cards: Vec<_> = view.cards.iter().filter(|card| card.zone == Zone::Plugin(zone.id) && card.seat == me).collect();
            ui.push_id(zone.id, |ui| {
                ui.label(format!("{} · {} cards", zone.label, cards.len()));
                ui.horizontal_wrapped(|ui| {
                    if button(ui, "draw") {
                        if let (Some(card), Some(hand)) = (cards.last(), view.zones.iter().find(|zone| zone.kind == agni_sim::wire::ZoneKind::Hand)) {
                            out.push(WireIntent::Move { card: card.id, to: Zone::Plugin(hand.id), seat: me, index: u32::MAX });
                        }
                    }
                    if button(ui, "look") { out.push(game(Command::Look { zone: zone.id, count: panel.look_count.max(1) })); }
                    if button(ui, "search deck") { out.push(game(Command::Look { zone: zone.id, count: u32::MAX })); }
                    if button(ui, "finish looking") {
                        out.extend(cards.iter().filter(|card| view.peeked.contains(&card.id) || view.revealed.contains(&card.id)).map(|card| game(Command::Conceal { card: card.id })));
                    }
                    if button(ui, "shuffle") {
                        out.push(game(Command::Shuffle { zone: zone.id, seed: u64::from_le_bytes(crate::os::entropy::secret()) }));
                    }
                });
            });
        }
        ui.label("Look and search reveal faces only to you. Select a card below to move it or change its order. Shuffle after searching when required.");
    });
    ui.separator();
    ui.strong("cards in every zone");
    ui.add(
        egui::TextEdit::singleline(&mut panel.search)
            .hint_text("find a card or zone")
            .desired_width(ui.available_width()),
    );
    let search = panel.search.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("manual cards")
        .max_height(190.0)
        .show(ui, |ui| {
            for card in view
                .cards
                .iter()
                .rev()
                .filter(|card| accessible(card, view, me))
            {
                let name = card_name(card, table);
                let zone = match card.zone {
                    Zone::Plugin(id) => plugin_ui::zone_label(&view.zones, id),
                    _ => format!("{:?}", card.zone),
                };
                let label = format!("{name} · {zone} · {}", seat_name(roster, view, card.owner));
                if !label.to_lowercase().contains(&search) {
                    continue;
                }
                if ui
                    .add(
                        egui::Button::selectable(panel.selected == Some(card.id), label)
                            .wrap()
                            .min_size(egui::vec2(ui.available_width(), hud::TOUCH_MIN)),
                    )
                    .clicked()
                {
                    panel.select(card.id, me);
                }
            }
        });
    if let Some(card) = panel
        .selected
        .and_then(|id| view.card(id))
        .filter(|card| accessible(card, view, me))
    {
        ui.separator();
        ui.strong(card_name(card, table));
        card_controls(ui, panel, view, card, me, roster, &mut out);
        counter_rows(ui, view, CounterTarget::Card(card.id), &mut out);
    }
    out
}

fn card_name(card: &ViewCard, table: &Table) -> String {
    table
        .get(CardId(card.id))
        .filter(|_| card.face_visible)
        .map(|card| card.face.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("face-down card #{}", card.id))
}

fn card_controls(
    ui: &mut egui::Ui,
    panel: &mut Panel,
    view: &TableView,
    card: &ViewCard,
    me: u8,
    roster: &[agni_net::session::SeatInfo],
    out: &mut Vec<WireIntent>,
) {
    egui::ComboBox::from_id_salt("manual destination")
        .selected_text(
            panel
                .destination
                .map(|id| plugin_ui::zone_label(&view.zones, id))
                .unwrap_or_else(|| "move to zone".into()),
        )
        .show_ui(ui, |ui| {
            for zone in &view.zones {
                ui.selectable_value(&mut panel.destination, Some(zone.id), &zone.label);
            }
        });
    if let Some(zone) = panel
        .destination
        .and_then(|id| view.zones.iter().find(|zone| zone.id == id))
    {
        if zone.visibility != ZoneVisibility::All {
            panel.seat = me;
        }
        if zone.owner == ZoneOwner::PerSeat && zone.visibility == ZoneVisibility::All {
            ui.horizontal_wrapped(|ui| {
                for seat in &view.seats {
                    ui.selectable_value(
                        &mut panel.seat,
                        seat.seat,
                        seat_name(roster, view, seat.seat),
                    );
                }
            });
        }
        ui.checkbox(&mut panel.hidden, "move face down");
        ui.horizontal_wrapped(|ui| {
            for (label, index) in [("to top / end", u32::MAX), ("to bottom / start", 0)] {
                if button(ui, label) {
                    out.push(move_card(card.id, zone.id, panel.seat, index, panel.hidden));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("position from bottom (0 = bottom)");
            ui.add(egui::DragValue::new(&mut panel.index));
            if button(ui, "move") {
                out.push(move_card(
                    card.id,
                    zone.id,
                    panel.seat,
                    panel.index,
                    panel.hidden,
                ));
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        if card.owner == me {
            if button(ui, "reveal") {
                out.push(game(Command::Reveal { card: card.id }));
            }
            if button(ui, "turn face down") {
                out.push(game(Command::Conceal { card: card.id }));
            }
            if button(ui, "remove token") {
                out.push(game(Command::RemoveToken { card: card.id }));
            }
        }
    });
    egui::CollapsingHeader::new("ready, exhausted and status marks")
        .default_open(true)
        .show(ui, |ui| {
            for key in [
                "exhausted",
                "stunned",
                "attacker",
                "defender",
                "attached",
                "temporary",
                "empowered",
            ] {
                let mut on = card.badge(key).is_some();
                if ui.checkbox(&mut on, key).changed() {
                    out.push(WireIntent::Annotate {
                        card: card.id,
                        key: key.into(),
                        value: on.then(|| vec![1].into()),
                    });
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut panel.label)
                    .hint_text("custom label")
                    .desired_width(ui.available_width()),
            );
            if button(ui, "set label") {
                out.push(WireIntent::Annotate {
                    card: card.id,
                    key: "label".into(),
                    value: (!panel.label.is_empty())
                        .then(|| panel.label.as_bytes().to_vec().into()),
                });
            }
            for badge in &card.badges {
                ui.horizontal_wrapped(|ui| {
                    ui.label(&badge.key);
                    if badge.key == "label" {
                        ui.label(String::from_utf8_lossy(&badge.value));
                    }
                    if button(ui, "clear") {
                        out.push(WireIntent::Annotate {
                            card: card.id,
                            key: badge.key.clone(),
                            value: None,
                        });
                    }
                });
            }
        });
}

fn move_card(card: u32, zone: u16, seat: u8, index: u32, hidden: bool) -> WireIntent {
    let to = Zone::Plugin(zone);
    if hidden {
        WireIntent::MoveHidden {
            card,
            to,
            seat,
            index,
        }
    } else {
        WireIntent::Move {
            card,
            to,
            seat,
            index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_controls_require_a_live_session_and_explicit_plugin_support() {
        let mut view = PluginView::default();
        assert!(!available(&view, SessionRole::Host));
        view.affordances.push(agni_sim::wire::Affordance {
            label: CONTROLS.into(),
            hotkey: None,
            data: Command::Disable.encode().into(),
            enabled: true,
            card: None,
            kind: agni_sim::wire::AffordanceKind::Plain,
        });
        assert!(available(&view, SessionRole::Client));
        assert!(!available(&view, SessionRole::Ended));
    }

    #[test]
    fn a_manual_move_preserves_destination_order_and_hidden_intent() {
        assert_eq!(
            move_card(42, 3, 1, 7, true),
            WireIntent::MoveHidden {
                card: 42,
                to: Zone::Plugin(3),
                seat: 1,
                index: 7
            }
        );
    }

    #[test]
    fn manual_panel_defaults_to_a_useful_look_count_and_resets_card_fields() {
        let mut panel = Panel::default();
        assert_eq!(panel.look_count, 5);
        panel.destination = Some(3);
        panel.seat = 4;
        panel.index = 8;
        panel.hidden = true;
        panel.label = "marked".into();
        panel.select(12, 1);
        assert_eq!(panel.selected, Some(12));
        assert_eq!(panel.destination, None);
        assert_eq!(panel.seat, 1);
        assert_eq!(panel.index, 0);
        assert!(!panel.hidden);
        assert!(panel.label.is_empty());
    }

    #[test]
    fn manual_counter_adjustments_stay_inside_declared_bounds() {
        assert_eq!(bounded(-3, Some(0), Some(8)), 0);
        assert_eq!(bounded(12, Some(0), Some(8)), 8);
        assert_eq!(bounded(4, Some(0), Some(8)), 4);
    }

    #[test]
    fn manual_labels_prefer_roster_names_and_have_safe_fallbacks() {
        let roster = vec![agni_net::session::SeatInfo {
            seat: 1,
            name: "  Mira  ".into(),
            host: false,
            connected: true,
            color: 0,
            playmat: None,
        }];
        let view = TableView {
            seats: vec![agni_sim::log::Seat {
                seat: 2,
                name: "Kai".into(),
            }],
            ..Default::default()
        };
        assert_eq!(seat_name(&roster, &view, 1), "Mira");
        assert_eq!(seat_name(&roster, &view, 2), "Kai");
        assert_eq!(seat_name(&roster, &view, 3), "Player 4");
    }

    #[test]
    fn stale_manual_selection_is_dropped_before_controls_render() {
        let mut panel = Panel {
            selected: Some(42),
            destination: Some(3),
            ..Default::default()
        };
        panel.retain_selection(&TableView::default(), 0);
        assert_eq!(panel.selected, None);
        assert_eq!(panel.destination, None);
    }

    #[test]
    fn the_manual_browser_keeps_opponents_private_zones_and_unknown_faces_hidden() {
        let view = TableView {
            zones: agni_riftbound::zone_table(),
            ..Default::default()
        };
        let card = ViewCard {
            id: 0,
            zone: Zone::Plugin(agni_riftbound::ZONE_HAND),
            seat: 1,
            owner: 1,
            face_visible: false,
            badges: Vec::new(),
        };
        assert!(!accessible(&card, &view, 0));
        assert!(accessible(&card, &view, 1));
        let public = ViewCard {
            zone: Zone::Plugin(agni_riftbound::ZONE_BASE),
            ..card
        };
        assert!(accessible(&public, &view, 0));
        let mut table = Table::new();
        table.add_face(
            PlayerId(1),
            public.zone,
            agni_core::CardFace::named("secret"),
        );
        assert!(!card_name(&public, &table).contains("secret"));
    }
}
