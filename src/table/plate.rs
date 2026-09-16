use super::auto::Stops;
use super::hud::{self, Hud, GAP, INK, INK_WEAK, SURFACE_2, TOUCH_MIN};
use super::*;
use crate::viewport::{InputKind, Viewport, ViewportClass};
use agni_sim::wire::{CounterPlace, CounterScope, CounterTarget, PluginView, TurnInfo, Waiting};

pub const PULSE_SECS: f32 = 2.0;
pub const ENFORCED: &str = "rules enforced";
pub const FREE: &str = "free table";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub zone: u16,
    pub held: Option<u8>,
    pub contested: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusLine {
    Turn {
        number: u32,
        seat: u8,
        phase: String,
        mode: String,
    },
    Points {
        points: Vec<(u8, i32)>,
        xp: Vec<(u8, i32)>,
    },
    Winner {
        seat: u8,
        points: i32,
    },
    Control(Vec<Control>),
    Chain(String),
    Showdown {
        zone: u16,
        attacker: u8,
        defender: u8,
        combat: bool,
        focus: Option<u8>,
    },
    Focus {
        seat: u8,
        passes: String,
    },
    Combat {
        attackers: Vec<u32>,
        attackers_might: i32,
        defenders: Vec<u32>,
        defenders_might: i32,
    },
    Assign {
        seat: u8,
        remaining: i32,
    },
    Waiting {
        seat: Option<u8>,
        what: Option<String>,
    },
    Setup,
    Roll {
        round: Option<u8>,
    },
    RollMarks(String),
    RollWon {
        seat: Option<u8>,
    },
    Mode(String),
    Proposal {
        seat: Option<u8>,
    },
    Narration(String),
}

fn token<'a>(text: &'a str, kind: &str) -> Option<(u32, &'a str)> {
    let rest = text
        .strip_prefix('{')?
        .strip_prefix(kind)?
        .strip_prefix(' ')?;
    let end = rest.find('}')?;
    let value = rest[..end].parse().ok()?;
    Some((value, &rest[end + 1..]))
}

fn seat_token(text: &str) -> Option<(u8, &str)> {
    token(text, "seat").and_then(|(value, rest)| u8::try_from(value).ok().map(|seat| (seat, rest)))
}

fn zone_token(text: &str) -> Option<(u16, &str)> {
    token(text, "zone").and_then(|(value, rest)| u16::try_from(value).ok().map(|zone| (zone, rest)))
}

fn card_token(text: &str) -> Option<(u32, &str)> {
    token(text, "card")
}

fn seat_value(part: &str) -> Option<(u8, i32)> {
    let (seat, rest) = seat_token(part)?;
    let value = rest.trim().parse().ok()?;
    Some((seat, value))
}

fn points_line(line: &str) -> Option<StatusLine> {
    let rest = line.strip_prefix("points · ")?;
    let mut points = Vec::new();
    let mut xp = Vec::new();
    let mut in_xp = false;
    for part in rest.split(" · ") {
        let part = match part.strip_prefix("xp ") {
            Some(part) => {
                in_xp = true;
                part
            }
            None => part,
        };
        let value = seat_value(part)?;
        if in_xp {
            xp.push(value);
        } else {
            points.push(value);
        }
    }
    Some(StatusLine::Points { points, xp })
}

fn control_line(line: &str) -> Option<StatusLine> {
    let mut out = Vec::new();
    for part in line.split(" · ") {
        let (zone, rest) = zone_token(part)?;
        let mut control = Control {
            zone,
            held: None,
            contested: None,
        };
        for clause in rest.trim().split(", ") {
            if let Some(rest) = clause.strip_prefix("held by ") {
                control.held = Some(seat_token(rest)?.0);
                continue;
            }
            let rest = clause.strip_prefix("contested by ")?;
            control.contested = Some(seat_token(rest)?.0);
        }
        out.push(control);
    }
    (!out.is_empty()).then_some(StatusLine::Control(out))
}

fn showdown_line(line: &str) -> Option<StatusLine> {
    let (combat, rest) = match line.strip_prefix("showdown at ") {
        Some(rest) => (false, rest),
        None => (true, line.strip_prefix("combat at ")?),
    };
    let (zone, rest) = zone_token(rest)?;
    let rest = rest.strip_prefix(" · ")?;
    let (attacker, rest) = seat_token(rest)?;
    let rest = rest
        .strip_prefix(" against ")
        .or_else(|| rest.strip_prefix(" attacks "))?;
    let (defender, rest) = seat_token(rest)?;
    let focus = match rest.strip_prefix(" · focus ") {
        Some(rest) => Some(seat_token(rest)?.0),
        None => None,
    };
    Some(StatusLine::Showdown {
        zone,
        attacker,
        defender,
        combat,
        focus,
    })
}

fn cards_and_might(text: &str) -> Option<(Vec<u32>, i32)> {
    let open = text.rfind(" (")?;
    let might = text[open + 2..].strip_suffix(" might)")?.parse().ok()?;
    let names = &text[..open];
    let cards = if names == "none" {
        Vec::new()
    } else {
        names
            .split(", ")
            .map(|name| card_token(name).map(|(card, _)| card))
            .collect::<Option<Vec<u32>>>()?
    };
    Some((cards, might))
}

fn combat_line(line: &str) -> Option<StatusLine> {
    let rest = line.strip_prefix("attackers ")?;
    let (attackers, defenders) = rest.split_once(" · defenders ")?;
    let (attackers, attackers_might) = cards_and_might(attackers)?;
    let (defenders, defenders_might) = cards_and_might(defenders)?;
    Some(StatusLine::Combat {
        attackers,
        attackers_might,
        defenders,
        defenders_might,
    })
}

fn waiting_line(line: &str) -> Option<StatusLine> {
    let rest = line.strip_prefix("waiting for ")?;
    if rest.starts_with("every seat") {
        return Some(StatusLine::Waiting {
            seat: None,
            what: Some(rest.to_string()),
        });
    }
    let (seat, rest) = seat_token(rest)?;
    let what = rest.strip_prefix(": ").map(str::to_string);
    Some(StatusLine::Waiting {
        seat: Some(seat),
        what,
    })
}

fn seat_led(line: &str) -> Option<StatusLine> {
    let (seat, rest) = seat_token(line)?;
    if let Some(rest) = rest.strip_prefix(" wins with ") {
        let points = rest.strip_suffix(" points")?.parse().ok()?;
        return Some(StatusLine::Winner { seat, points });
    }
    if let Some(rest) = rest.strip_prefix(" assigns ") {
        let remaining = rest.strip_suffix(" damage")?.parse().ok()?;
        return Some(StatusLine::Assign { seat, remaining });
    }
    if rest == " proposes a free table" {
        return Some(StatusLine::Proposal { seat: Some(seat) });
    }
    if rest.starts_with(" won the roll") {
        return Some(StatusLine::RollWon { seat: Some(seat) });
    }
    if rest.starts_with(": ") {
        return Some(StatusLine::RollMarks(line.to_string()));
    }
    None
}

pub fn classify_status(line: &str) -> StatusLine {
    if let Some(rest) = line.strip_prefix("turn ") {
        let mut parts = rest.split(" · ");
        let turn = parts.next().and_then(|number| number.parse().ok());
        let seat = parts
            .next()
            .and_then(|seat| seat_token(seat).map(|(seat, _)| seat));
        let phase = parts.next();
        let mode = parts.next();
        if let (Some(number), Some(seat), Some(phase), Some(mode)) = (turn, seat, phase, mode) {
            return StatusLine::Turn {
                number,
                seat,
                phase: phase.to_string(),
                mode: mode.to_string(),
            };
        }
    }
    if let Some(parsed) = points_line(line) {
        return parsed;
    }
    if let Some(rest) = line.strip_prefix("chain: ") {
        return StatusLine::Chain(rest.to_string());
    }
    if let Some(parsed) = showdown_line(line) {
        return parsed;
    }
    if let Some(rest) = line.strip_prefix("focus: ") {
        if let Some((seat, rest)) = seat_token(rest) {
            return StatusLine::Focus {
                seat,
                passes: rest.strip_prefix(" · ").unwrap_or("").to_string(),
            };
        }
    }
    if let Some(parsed) = combat_line(line) {
        return parsed;
    }
    if let Some(parsed) = waiting_line(line) {
        return parsed;
    }
    if line == "setup · mulligans" {
        return StatusLine::Setup;
    }
    if let Some(rest) = line.strip_prefix("roll for first player") {
        let round = rest
            .strip_prefix(" · tie, round ")
            .and_then(|round| round.parse().ok());
        return StatusLine::Roll { round };
    }
    if line.starts_with("you won the roll") {
        return StatusLine::RollWon { seat: None };
    }
    if let Some(rest) = line.strip_prefix("mode: ") {
        return StatusLine::Mode(rest.to_string());
    }
    if line.starts_with("free table proposed") {
        return StatusLine::Proposal { seat: None };
    }
    if let Some(parsed) = control_line(line) {
        return parsed;
    }
    if let Some(parsed) = seat_led(line) {
        return parsed;
    }
    StatusLine::Narration(line.to_string())
}

pub fn mode_of(view: &PluginView) -> Option<Mode> {
    view.turn
        .as_ref()
        .and_then(|turn| Mode::of(&turn.mode))
        .or_else(|| mode_of_lines(&lines(view)))
}

pub fn turn_of(view: &PluginView) -> Option<TurnInfo> {
    if let Some(turn) = &view.turn {
        return Some(turn.clone());
    }
    lines(view).into_iter().find_map(|line| match line {
        StatusLine::Turn {
            number,
            seat,
            phase,
            mode,
        } => Some(TurnInfo {
            number,
            seat,
            phase,
            phases: Vec::new(),
            mode,
        }),
        _ => None,
    })
}

pub fn waiting_of(view: &PluginView) -> Option<Waiting> {
    if let Some(waiting) = &view.waiting {
        return Some(waiting.clone());
    }
    lines(view).into_iter().find_map(|line| match line {
        StatusLine::Waiting { seat, what } => Some(Waiting {
            seat,
            what: what.unwrap_or_default(),
        }),
        _ => None,
    })
}

fn mode_of_lines(lines: &[StatusLine]) -> Option<Mode> {
    lines
        .iter()
        .find_map(|line| match line {
            StatusLine::Turn { mode, .. } => Mode::of(mode),
            _ => None,
        })
        .or_else(|| {
            lines.iter().find_map(|line| match line {
                StatusLine::Mode(mode) => Mode::of(mode),
                _ => None,
            })
        })
}

pub fn lines(view: &PluginView) -> Vec<StatusLine> {
    view.status
        .iter()
        .map(|line| classify_status(line))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Enforced,
    Free,
}

impl Mode {
    pub fn of(label: &str) -> Option<Mode> {
        match label {
            ENFORCED => Some(Mode::Enforced),
            FREE => Some(Mode::Free),
            _ => None,
        }
    }

    pub fn chip(self) -> &'static str {
        match self {
            Mode::Enforced => "enforced",
            Mode::Free => "free",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnPlate {
    pub headline: String,
    pub detail: String,
    pub seat: Option<u8>,
    pub mode: Option<Mode>,
    pub pulse: bool,
}

pub fn pass_offered(view: &PluginView) -> bool {
    view.affordances.iter().any(|affordance| {
        affordance.enabled && affordance.hotkey.as_deref() == Some(plugin_ui::PASS_KEY)
    })
}

pub fn end_turn_offered(view: &PluginView) -> bool {
    view.affordances.iter().any(|affordance| {
        affordance.enabled && affordance.hotkey.as_deref() == Some(plugin_ui::ADVANCE_KEY)
    })
}

pub fn turn_plate(
    view: &PluginView,
    me: u8,
    seat_name: &dyn Fn(u8) -> String,
    zone_name: &dyn Fn(u16) -> String,
) -> Option<TurnPlate> {
    let lines = lines(view);
    if lines.is_empty() && view.affordances.is_empty() && view.turn.is_none() {
        return None;
    }
    let turn = turn_of(view);
    let mode = mode_of(view);
    let detail = turn
        .as_ref()
        .map(|turn| format!("turn {} · {}", turn.number, short_phase(&turn.phase)))
        .unwrap_or_default();
    let waiting = waiting_of(view).map(|waiting| waiting.seat);
    let showdown = lines.iter().find_map(|line| match line {
        StatusLine::Showdown {
            zone,
            combat,
            focus,
            ..
        } => Some((*zone, *combat, *focus)),
        _ => None,
    });
    let winner = lines.iter().find_map(|line| match line {
        StatusLine::Winner { seat, .. } => Some(*seat),
        _ => None,
    });
    let roll = lines
        .iter()
        .any(|line| matches!(line, StatusLine::Roll { .. }));
    let roll_won = lines.iter().find_map(|line| match line {
        StatusLine::RollWon { seat } => Some(*seat),
        _ => None,
    });
    let setup = lines.contains(&StatusLine::Setup);
    let prompt = view.prompt.as_ref().map(|summary| summary.seat);
    let turn_seat = turn.as_ref().map(|turn| turn.seat);
    let (headline, seat) = if let Some(winner) = view.winner.or(winner) {
        (format!("{} wins", seat_name(winner)), Some(winner))
    } else if roll || roll_won.is_some() {
        match roll_won {
            Some(None) => ("you won the roll".to_string(), Some(me)),
            Some(Some(seat)) => (format!("{} won the roll", seat_name(seat)), Some(seat)),
            None => ("roll for first player".to_string(), None),
        }
    } else if setup {
        ("setup · mulligans".to_string(), turn_seat)
    } else if prompt == Some(me) {
        ("your choice".to_string(), Some(me))
    } else if let Some(seat) = prompt {
        (format!("waiting for {}", seat_name(seat)), Some(seat))
    } else if let Some((zone, combat, focus)) = showdown.filter(|_| pass_offered(view)) {
        let what = if combat { "combat" } else { "showdown" };
        (format!("{what} at {}", zone_name(zone)), focus.or(Some(me)))
    } else if pass_offered(view) {
        ("respond or pass".to_string(), Some(me))
    } else if let Some(Some(seat)) = waiting {
        (format!("waiting for {}", seat_name(seat)), Some(seat))
    } else if let Some(None) = waiting {
        ("waiting for every seat".to_string(), None)
    } else if end_turn_offered(view) || turn_seat == Some(me) {
        ("your action".to_string(), Some(me))
    } else {
        let seat = turn_seat?;
        (format!("waiting for {}", seat_name(seat)), Some(seat))
    };
    Some(TurnPlate {
        headline,
        detail,
        seat,
        mode,
        pulse: highlight::acting(view) || prompt == Some(me),
    })
}

pub fn short_phase(phase: &str) -> &str {
    phase
        .strip_suffix(" phase")
        .or_else(|| phase.strip_suffix(" step"))
        .unwrap_or(phase)
}

pub fn pulse(seconds: f32) -> f32 {
    let phase = (seconds / PULSE_SECS) * std::f32::consts::TAU;
    0.5 + 0.5 * phase.sin()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatPlate {
    pub seat: u8,
    pub name: String,
    pub color: [u8; 3],
    pub is_me: bool,
    pub points: i32,
    pub victory: i32,
    pub xp: i32,
    pub hand: usize,
    pub deck: usize,
    pub runes: Option<(u32, u32)>,
    pub connected: Option<bool>,
}

pub fn victory_score(info: &SessionInfo, players: usize) -> i32 {
    info.options_in_play(players).victory_score
}

pub fn deck_zone(zones: &[agni_sim::wire::ZoneDecl]) -> Option<u16> {
    zones
        .iter()
        .find(|decl| decl.name == agni_riftbound::ZONE_NAME_MAIN_DECK)
        .or_else(|| {
            zones
                .iter()
                .find(|decl| decl.kind == agni_sim::wire::ZoneKind::Deck)
        })
        .map(|decl| decl.id)
}

pub fn seat_plates(
    view: &PluginView,
    mirror: &Mirror,
    table: &Table,
    info: &SessionInfo,
    players: usize,
    label: &dyn Fn(u8) -> (String, [u8; 3], bool),
) -> Vec<SeatPlate> {
    let status = lines(view);
    let points_line = status.iter().find_map(|line| match line {
        StatusLine::Points { points, xp } => Some((points.clone(), xp.clone())),
        _ => None,
    });
    let victory = victory_score(info, players);
    let hand = zones::hand_zone(&mirror.view.zones);
    let deck = deck_zone(&mirror.view.zones).map(Zone::Plugin);
    (0..players as u8)
        .map(|seat| {
            let (name, color, is_me) = label(seat);
            let told = view.seats.iter().find(|held| held.seat == seat);
            let counter = |id: u16| mirror.view.counter(CounterTarget::Seat(seat), id);
            let from_line = |values: &[(u8, i32)]| {
                values
                    .iter()
                    .find(|(held, _)| *held == seat)
                    .map(|(_, value)| *value)
            };
            let points = told
                .map(|told| told.points)
                .or_else(|| counter(agni_riftbound::COUNTER_POINTS))
                .or_else(|| {
                    points_line
                        .as_ref()
                        .and_then(|(points, _)| from_line(points))
                })
                .unwrap_or(0);
            let xp = told
                .map(|told| told.xp)
                .or_else(|| counter(agni_riftbound::COUNTER_XP))
                .or_else(|| points_line.as_ref().and_then(|(_, xp)| from_line(xp)))
                .unwrap_or(0);
            let seat_id = PlayerId(seat);
            let hand_count = told.map(|told| told.hand as usize).unwrap_or_else(|| {
                table.in_area(seat_id, hand).count()
                    + if hand != Zone::Hand {
                        table.in_area(seat_id, Zone::Hand).count()
                    } else {
                        0
                    }
            });
            let deck_count = told
                .map(|told| told.deck as usize)
                .unwrap_or_else(|| deck.map_or(0, |zone| table.in_area(seat_id, zone).count()));
            let connected = info
                .roster
                .iter()
                .find(|held| held.seat == seat)
                .map(|held| held.connected);
            SeatPlate {
                seat,
                name,
                color,
                is_me,
                points,
                victory: told.map(|told| told.victory).unwrap_or(victory),
                xp,
                hand: hand_count,
                deck: deck_count,
                runes: told.map(|told| (told.runes_ready, told.runes_total)),
                connected,
            }
        })
        .collect()
}

pub fn ordered(plates: Vec<SeatPlate>) -> Vec<SeatPlate> {
    let mut plates = plates;
    plates.sort_by_key(|plate| plate.is_me);
    plates
}

pub fn seat_switch_allowed(free: bool, players: usize) -> bool {
    free || players > 2
}

fn rgb(color: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(color[0], color[1], color[2])
}

pub fn turn_plate_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    time: Res<Time>,
    panel: Res<plugin_ui::PluginPanel>,
    mirror: Res<Mirror>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    menu: Res<crate::menu::Menu>,
    keys: Res<ButtonInput<KeyCode>>,
    mut auto: ResMut<auto::Auto>,
) -> Result {
    if !menu.at_table() {
        return Ok(());
    }
    let seat_name =
        |seat: u8| colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(seat)).0;
    let zone_name = |zone: u16| plugin_ui::zone_label(&mirror.view.zones, zone);
    let Some(plate) = turn_plate(&panel.view, my_seat.0 .0, &seat_name, &zone_name) else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let rect = hud.0.turn_plate;
    let color = plate
        .seat
        .map(|seat| {
            rgb(colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(seat)).1)
        })
        .unwrap_or(INK);
    let glow = if plate.pulse {
        pulse(time.elapsed_secs())
    } else {
        0.0
    };
    let stroke = egui::Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (90.0 + 165.0 * glow) as u8,
    );
    let phone = hud.0.class.is_phone();
    let chips = plate_chips(
        plate.mode.filter(|_| rect.width() >= hud::TURN_PLATE_W),
        auto::effective_hold(&auto.hold, auto::control_held(&keys)).chip(),
    );
    let mut long_pressed = false;
    hud::slot(&context, "turn plate", rect, |ui| {
        let hit = ui.interact(rect, egui::Id::new("turn plate hold"), egui::Sense::click());
        long_pressed = hit.long_touched();
        egui::Frame::new()
            .fill(hud::SURFACE)
            .stroke(egui::Stroke::new(
                if plate.pulse { 2.0 } else { 1.0 },
                stroke,
            ))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_min_width(rect.width() - 16.0);
                ui.set_max_width(rect.width() - 16.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.label(
                    egui::RichText::new(&plate.headline)
                        .size(if phone { 17.0 } else { 15.0 })
                        .strong()
                        .color(color),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if !plate.detail.is_empty() {
                        ui.label(
                            egui::RichText::new(&plate.detail)
                                .size(12.0)
                                .color(INK_WEAK),
                        );
                    }
                    for (text, tint) in &chips {
                        ui.label(
                            egui::RichText::new(*text)
                                .size(11.0)
                                .color(*tint)
                                .background_color(SURFACE_2),
                        );
                    }
                });
            });
    });
    if long_pressed {
        match auto.hold {
            auto::HoldFocus::Off => auto.hold_phase(&panel.view),
            auto::HoldFocus::ThisPhase(_) => auto.hold_held(),
            auto::HoldFocus::Held => auto.hold_held(),
        }
    }
    Ok(())
}

pub fn plate_chips(
    mode: Option<Mode>,
    hold: Option<&'static str>,
) -> Vec<(&'static str, egui::Color32)> {
    match (hold, mode) {
        (Some(hold), _) => vec![(hold, hud::AMBER)],
        (None, Some(mode)) => vec![(
            mode.chip(),
            match mode {
                Mode::Enforced => INK_WEAK,
                Mode::Free => hud::AMBER,
            },
        )],
        (None, None) => Vec::new(),
    }
}

pub const PHASE_DOT: f32 = 10.0;
pub const PHASE_MARK: f32 = 3.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseChip {
    pub phase: String,
    pub current: bool,
    pub stop_mine: bool,
    pub stop_theirs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseBar {
    pub chips: Vec<PhaseChip>,
    pub seat: u8,
    pub mine: bool,
}

pub fn phase_bar(view: &PluginView, me: u8, stops: &Stops) -> Option<PhaseBar> {
    let turn = view.turn.as_ref()?;
    if turn.phases.is_empty() {
        return None;
    }
    let chips = turn
        .phases
        .iter()
        .map(|phase| PhaseChip {
            phase: phase.clone(),
            current: *phase == turn.phase,
            stop_mine: stops.set(phase, true),
            stop_theirs: stops.set(phase, false),
        })
        .collect();
    Some(PhaseBar {
        chips,
        seat: turn.seat,
        mine: turn.seat == me,
    })
}

pub fn phase_bar_shown(class: ViewportClass, kind: InputKind) -> bool {
    !class.is_phone() && !kind.is_touch()
}

pub fn chip_clicked(stops: &mut Stops, bar: &PhaseBar, index: usize, other_turn: bool) -> bool {
    let Some(chip) = bar.chips.get(index) else {
        return false;
    };
    stops.toggle(&chip.phase, bar.mine != other_turn)
}

pub fn chip_tip(chip: &PhaseChip, mine_now: bool) -> String {
    let state = |on: bool| if on { "stop" } else { "pass" };
    let (first, second) = if mine_now {
        ("my turn", "their turn")
    } else {
        ("their turn", "my turn")
    };
    let (first_on, second_on) = if mine_now {
        (chip.stop_mine, chip.stop_theirs)
    } else {
        (chip.stop_theirs, chip.stop_mine)
    };
    format!(
        "{} · {first}: {} · {second}: {} · click sets a stop for {first}, right-click for {second}",
        short_phase(&chip.phase),
        state(first_on),
        state(second_on)
    )
}

#[allow(clippy::too_many_arguments)]
pub fn phase_bar_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    viewport: Res<Viewport>,
    kind: Res<InputKind>,
    panel: Res<plugin_ui::PluginPanel>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    menu: Res<crate::menu::Menu>,
    mut tuning: ResMut<Tuning>,
) -> Result {
    if !menu.at_table() || !phase_bar_shown(viewport.class, *kind) {
        return Ok(());
    }
    let Some(rect) = hud.0.phase_bar else {
        return Ok(());
    };
    let Some(bar) = phase_bar(&panel.view, my_seat.0 .0, &tuning.stops) else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    let seat_color =
        |seat: u8| rgb(colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(seat)).1);
    let turn_color = seat_color(bar.seat);
    let my_color = seat_color(my_seat.0 .0);
    let mut clicked = None;
    hud::slot(&context, "phase bar", rect, |ui| {
        let count = bar.chips.len().max(1) as f32;
        let pitch = rect.width() / count;
        let (area, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
        for (index, chip) in bar.chips.iter().enumerate() {
            let center = egui::pos2(area.min.x + pitch * (index as f32 + 0.5), area.center().y);
            let hit = egui::Rect::from_center_size(center, egui::vec2(pitch, rect.height()));
            let response = ui.interact(
                hit,
                egui::Id::new(("phase chip", index)),
                egui::Sense::click(),
            );
            let painter = ui.painter();
            let radius = PHASE_DOT / 2.0;
            if chip.current {
                painter.circle_filled(center, radius, turn_color);
            } else {
                painter.circle_stroke(center, radius - 0.5, egui::Stroke::new(1.0, INK_WEAK));
            }
            if response.hovered() {
                painter.circle_stroke(center, radius + 2.0, egui::Stroke::new(1.0, INK));
            }
            let mark = |painter: &egui::Painter, above: bool, color: egui::Color32| {
                let y = if above {
                    center.y - radius - PHASE_MARK - 1.0
                } else {
                    center.y + radius + 1.0
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(center.x - PHASE_DOT / 2.0, y),
                        egui::vec2(PHASE_DOT, PHASE_MARK),
                    ),
                    1.0,
                    color,
                );
            };
            if chip.stop_mine {
                mark(painter, true, my_color);
            }
            if chip.stop_theirs {
                mark(painter, false, hud::AMBER);
            }
            let response = response.on_hover_text(chip_tip(chip, bar.mine));
            if response.clicked() {
                clicked = Some((index, false));
            } else if response.secondary_clicked() {
                clicked = Some((index, true));
            }
        }
    });
    if let Some((index, other_turn)) = clicked {
        chip_clicked(&mut tuning.stops, &bar, index, other_turn);
    }
    Ok(())
}

fn pips(ui: &mut egui::Ui, filled: i32, total: i32, color: egui::Color32) {
    let total = total.clamp(1, 16) as usize;
    let filled = filled.clamp(0, total as i32) as usize;
    let size = 8.0;
    let gap = 3.0;
    let width = total as f32 * (size + gap) - gap;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, size), egui::Sense::hover());
    for index in 0..total {
        let center = egui::pos2(
            rect.min.x + index as f32 * (size + gap) + size / 2.0,
            rect.center().y,
        );
        if index < filled {
            ui.painter().circle_filled(center, size / 2.0, color);
        } else {
            ui.painter().circle_stroke(
                center,
                size / 2.0 - 0.5,
                egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn seat_plates_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    panel: Res<plugin_ui::PluginPanel>,
    mirror: Res<Mirror>,
    table: Res<GameTable>,
    info: Res<SessionInfo>,
    players: Res<PlayerCount>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    tools: Res<plugin_ui::Tools>,
    menu: Res<crate::menu::Menu>,
    tuning: Res<Tuning>,
    mut view_seat: ResMut<ViewSeat>,
    mut nudges: MessageWriter<counters::CounterNudged>,
) -> Result {
    if !menu.at_table() || mirror.view.zones.is_empty() {
        return Ok(());
    }
    let label =
        |seat: u8| colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(seat));
    let plates = ordered(seat_plates(
        &panel.view,
        &mirror,
        &table,
        &info,
        players.0,
        &label,
    ));
    let context = contexts.ctx_mut()?.clone();
    let rect = hud.0.seats;
    let plate_h = hud.0.plate_h;
    let compact = plate_h < hud::PLATE_H;
    let editable = counters::nudges_allowed(&tools, info.role);
    let switchable = seat_switch_allowed(tools.free, players.0);
    let seat_decls: Vec<agni_sim::wire::CounterDecl> = mirror
        .view
        .counter_table
        .iter()
        .filter(|decl| decl.scope == CounterScope::Seat && decl.place == CounterPlace::SeatPlate)
        .cloned()
        .collect();
    let mut switch_to = None;
    hud::slot(&context, "seat plates", rect, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for plate in &plates {
            let color = rgb(plate.color);
            let viewed = view_seat.0 == PlayerId(plate.seat);
            let response = egui::Frame::new()
                .fill(hud::SURFACE)
                .stroke(egui::Stroke::new(1.0, hud::HAIRLINE))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_min_width(rect.width() - 16.0);
                    ui.set_max_width(rect.width() - 16.0);
                    ui.set_min_height(plate_h - 8.0);
                    ui.set_max_height(plate_h - 8.0);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let header = |ui: &mut egui::Ui| {
                        colors::swatch(ui, plate.color, 12.0, tuning.colour_blind);
                        let mut name = plate.name.clone();
                        if plate.is_me {
                            name.push_str(" (you)");
                        }
                        ui.label(egui::RichText::new(name).strong().color(INK));
                        if viewed && switchable {
                            let (eye, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().circle_stroke(
                                eye.center(),
                                5.0,
                                egui::Stroke::new(1.5, INK_WEAK),
                            );
                            ui.painter().circle_filled(eye.center(), 2.0, INK_WEAK);
                        }
                        if let Some(connected) = plate.connected {
                            let dot = if connected { hud::GREEN } else { hud::DANGER };
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, dot);
                        }
                    };
                    let score = |ui: &mut egui::Ui| {
                        pips(ui, plate.points, plate.victory, color);
                        ui.label(
                            egui::RichText::new(format!("{}", plate.points))
                                .size(12.0)
                                .color(INK),
                        );
                        if plate.xp > 0 {
                            ui.label(
                                egui::RichText::new(format!("xp {}", plate.xp))
                                    .size(11.0)
                                    .color(INK)
                                    .background_color(SURFACE_2),
                            );
                        }
                    };
                    if compact {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            header(ui);
                            score(ui);
                        });
                        return;
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        header(ui);
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if editable {
                            for decl in &seat_decls {
                                let target = CounterTarget::Seat(plate.seat);
                                let value =
                                    mirror.view.counter(target, decl.id).unwrap_or(decl.start);
                                counters::nudge(ui, decl, target, value, true, &mut nudges);
                            }
                        } else {
                            score(ui);
                            if !plate.is_me {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "hand {} · deck {}",
                                        plate.hand, plate.deck
                                    ))
                                    .size(11.0)
                                    .color(INK_WEAK),
                                );
                            }
                            if let Some((ready, total)) =
                                plate.runes.filter(|(_, total)| *total > 0)
                            {
                                ui.label(
                                    egui::RichText::new(format!("runes {ready}/{total}"))
                                        .size(11.0)
                                        .color(INK_WEAK),
                                );
                            }
                        }
                    });
                })
                .response;
            if switchable && !viewed {
                let hit = ui.interact(
                    response.rect,
                    egui::Id::new(("seat plate", plate.seat)),
                    egui::Sense::click(),
                );
                if hit.on_hover_text("look from this seat").clicked() {
                    switch_to = Some(PlayerId(plate.seat));
                }
            }
        }
    });
    if let Some(seat) = switch_to {
        view_seat.0 = seat;
    }
    let _ = (GAP, TOUCH_MIN);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Affordance, AffordanceKind, PromptSummary, SeatInfo};
    use serde_bytes::ByteBuf;

    fn offer(label: &str, hotkey: Option<&str>) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![1]),
            card: None,
        }
    }

    fn view(lines: &[&str], affordances: Vec<Affordance>) -> PluginView {
        PluginView {
            status: lines.iter().map(|line| line.to_string()).collect(),
            affordances,
            ..Default::default()
        }
    }

    fn name(seat: u8) -> String {
        ["rae", "claude", "sokka", "toph"][seat as usize].to_string()
    }

    fn zone(zone: u16) -> String {
        format!("Battlefield {}", zone - 8)
    }

    #[test]
    fn the_hold_chip_replaces_the_mode_chip_so_it_is_never_truncated_away() {
        assert_eq!(
            plate_chips(Some(Mode::Enforced), None),
            [(Mode::Enforced.chip(), INK_WEAK)]
        );
        assert_eq!(
            plate_chips(Some(Mode::Enforced), Some(auto::HOLD_PHASE_CHIP)),
            [(auto::HOLD_PHASE_CHIP, hud::AMBER)]
        );
        assert_eq!(
            plate_chips(None, Some(auto::HOLD_CHIP)),
            [(auto::HOLD_CHIP, hud::AMBER)]
        );
        assert!(plate_chips(None, None).is_empty());
        let font = egui::FontId::proportional(11.0);
        let context = egui::Context::default();
        let mut widths = Vec::new();
        let mut full = context.run_ui(egui::RawInput::default(), |ui| {
            for chip in [auto::HOLD_PHASE_CHIP, auto::HOLD_CHIP] {
                let galley = ui.painter().layout_no_wrap(
                    format!("turn 3 · action  {chip}"),
                    font.clone(),
                    INK,
                );
                widths.push((chip, galley.size().x));
            }
        });
        full.textures_delta.clear();
        for (chip, width) in widths {
            assert!(
                width <= hud::TURN_PLATE_W - 16.0,
                "{chip} lays out untruncated in the plate: {width}"
            );
        }
    }

    #[test]
    fn the_presenters_lines_classify_verbatim() {
        let table: Vec<(&str, StatusLine)> = vec![
            (
                "turn 1 · {seat 0} · action phase · rules enforced",
                StatusLine::Turn {
                    number: 1,
                    seat: 0,
                    phase: "action phase".into(),
                    mode: "rules enforced".into(),
                },
            ),
            (
                "turn 12 · {seat 1} · beginning phase · free table",
                StatusLine::Turn {
                    number: 12,
                    seat: 1,
                    phase: "beginning phase".into(),
                    mode: "free table".into(),
                },
            ),
            (
                "points · {seat 0} 0 · {seat 1} 0",
                StatusLine::Points {
                    points: vec![(0, 0), (1, 0)],
                    xp: vec![],
                },
            ),
            (
                "points · {seat 0} 3 · {seat 1} 5 · xp {seat 0} 1 · {seat 1} 3",
                StatusLine::Points {
                    points: vec![(0, 3), (1, 5)],
                    xp: vec![(0, 1), (1, 3)],
                },
            ),
            (
                "{seat 1} wins with 8 points",
                StatusLine::Winner { seat: 1, points: 8 },
            ),
            (
                "{zone 9} held by {seat 0}",
                StatusLine::Control(vec![Control {
                    zone: 9,
                    held: Some(0),
                    contested: None,
                }]),
            ),
            (
                "{zone 9} held by {seat 0}, contested by {seat 1} · {zone 10} contested by {seat 1}",
                StatusLine::Control(vec![
                    Control {
                        zone: 9,
                        held: Some(0),
                        contested: Some(1),
                    },
                    Control {
                        zone: 10,
                        held: None,
                        contested: Some(1),
                    },
                ]),
            ),
            (
                "chain: {card 4} → {card 7} ability (top)",
                StatusLine::Chain("{card 4} → {card 7} ability (top)".into()),
            ),
            (
                "showdown at {zone 9} · {seat 0} against {seat 1} · focus {seat 1}",
                StatusLine::Showdown {
                    zone: 9,
                    attacker: 0,
                    defender: 1,
                    combat: false,
                    focus: Some(1),
                },
            ),
            (
                "combat at {zone 10} · {seat 1} against {seat 0}",
                StatusLine::Showdown {
                    zone: 10,
                    attacker: 1,
                    defender: 0,
                    combat: true,
                    focus: None,
                },
            ),
            (
                "showdown at {zone 9} · {seat 0} attacks {seat 1}",
                StatusLine::Showdown {
                    zone: 9,
                    attacker: 0,
                    defender: 1,
                    combat: false,
                    focus: None,
                },
            ),
            (
                "focus: {seat 1} · passes 1/2",
                StatusLine::Focus {
                    seat: 1,
                    passes: "passes 1/2".into(),
                },
            ),
            (
                "attackers {card 71}, {card 72} (5 might) · defenders none (0 might)",
                StatusLine::Combat {
                    attackers: vec![71, 72],
                    attackers_might: 5,
                    defenders: vec![],
                    defenders_might: 0,
                },
            ),
            (
                "{seat 0} assigns 3 damage",
                StatusLine::Assign {
                    seat: 0,
                    remaining: 3,
                },
            ),
            (
                "waiting for {seat 1}: respond or pass",
                StatusLine::Waiting {
                    seat: Some(1),
                    what: Some("respond or pass".into()),
                },
            ),
            (
                "waiting for {seat 1}: pass or respond at {zone 9}",
                StatusLine::Waiting {
                    seat: Some(1),
                    what: Some("pass or respond at {zone 9}".into()),
                },
            ),
            (
                "waiting for {seat 1}: their action phase",
                StatusLine::Waiting {
                    seat: Some(1),
                    what: Some("their action phase".into()),
                },
            ),
            (
                "waiting for {seat 1}: where does {card 70} enter?",
                StatusLine::Waiting {
                    seat: Some(1),
                    what: Some("where does {card 70} enter?".into()),
                },
            ),
            (
                "waiting for {seat 0}",
                StatusLine::Waiting {
                    seat: Some(0),
                    what: None,
                },
            ),
            (
                "waiting for every seat to roll",
                StatusLine::Waiting {
                    seat: None,
                    what: Some("every seat to roll".into()),
                },
            ),
            ("setup · mulligans", StatusLine::Setup),
            ("roll for first player", StatusLine::Roll { round: None }),
            (
                "roll for first player · tie, round 2",
                StatusLine::Roll { round: Some(2) },
            ),
            (
                "{seat 0}: 4 · {seat 1}: rolled",
                StatusLine::RollMarks("{seat 0}: 4 · {seat 1}: rolled".into()),
            ),
            (
                "you won the roll — who goes first?",
                StatusLine::RollWon { seat: None },
            ),
            (
                "{seat 1} won the roll and chooses the mode and who goes first",
                StatusLine::RollWon { seat: Some(1) },
            ),
            ("mode: rules enforced", StatusLine::Mode("rules enforced".into())),
            (
                "{seat 1} proposes a free table",
                StatusLine::Proposal { seat: Some(1) },
            ),
            (
                "free table proposed · waiting for another seat",
                StatusLine::Proposal { seat: None },
            ),
            (
                "{card 12} dies",
                StatusLine::Narration("{card 12} dies".into()),
            ),
            (
                "every deck must be dealt before the start: the first turn draws and channels at once",
                StatusLine::Narration(
                    "every deck must be dealt before the start: the first turn draws and channels at once".into(),
                ),
            ),
        ];
        for (line, wanted) in table {
            assert_eq!(classify_status(line), wanted, "{line}");
        }
    }

    #[test]
    fn the_turn_plate_says_whose_move_in_words() {
        let mine = view(
            &[
                "turn 3 · {seat 0} · action phase · rules enforced",
                "points · {seat 0} 0 · {seat 1} 0",
            ],
            vec![offer("end turn", Some("space"))],
        );
        let plate = turn_plate(&mine, 0, &name, &zone).unwrap();
        assert_eq!(plate.headline, "your action");
        assert_eq!(plate.detail, "turn 3 · action");
        assert_eq!(short_phase("awaken step"), "awaken");
        assert_eq!(short_phase("setup"), "setup");
        assert_eq!(plate.mode, Some(Mode::Enforced));
        assert_eq!(plate.seat, Some(0));
        assert!(plate.pulse);

        let theirs = view(
            &[
                "turn 3 · {seat 0} · action phase · rules enforced",
                "waiting for {seat 0}: their action phase",
            ],
            vec![],
        );
        let plate = turn_plate(&theirs, 1, &name, &zone).unwrap();
        assert_eq!(plate.headline, "waiting for rae");
        assert_eq!(plate.seat, Some(0));
        assert!(!plate.pulse);

        let respond = view(
            &["turn 3 · {seat 0} · action phase · rules enforced"],
            vec![offer("pass", Some("w"))],
        );
        assert_eq!(
            turn_plate(&respond, 1, &name, &zone).unwrap().headline,
            "respond or pass"
        );

        let showdown = view(
            &[
                "turn 3 · {seat 0} · action phase · rules enforced",
                "showdown at {zone 10} · {seat 0} against {seat 1} · focus {seat 1}",
            ],
            vec![offer("pass", Some("w"))],
        );
        let plate = turn_plate(&showdown, 1, &name, &zone).unwrap();
        assert_eq!(plate.headline, "showdown at Battlefield 2");
        assert_eq!(plate.seat, Some(1));

        let setup = view(
            &[
                "turn 1 · {seat 0} · setup · rules enforced",
                "setup · mulligans",
            ],
            vec![],
        );
        assert_eq!(
            turn_plate(&setup, 0, &name, &zone).unwrap().headline,
            "setup · mulligans"
        );

        let free = view(
            &["turn 2 · {seat 1} · action phase · free table"],
            vec![offer("end turn", Some("space"))],
        );
        let plate = turn_plate(&free, 1, &name, &zone).unwrap();
        assert_eq!(plate.mode, Some(Mode::Free));
        assert!(
            !plate.pulse,
            "a free table never pulses: acting() is enforced-only"
        );

        let roll = view(
            &["roll for first player", "mode: rules enforced"],
            vec![offer("roll", None)],
        );
        let plate = turn_plate(&roll, 0, &name, &zone).unwrap();
        assert_eq!(plate.headline, "roll for first player");
        assert_eq!(plate.mode, Some(Mode::Enforced));
        assert_eq!(plate.detail, "");

        let won = view(
            &[
                "roll for first player",
                "{seat 0}: 4 · {seat 1}: 2",
                "you won the roll — who goes first?",
            ],
            vec![offer("go first", None)],
        );
        assert_eq!(
            turn_plate(&won, 0, &name, &zone).unwrap().headline,
            "you won the roll"
        );

        let mut prompted = view(
            &[
                "turn 1 · {seat 0} · setup · rules enforced",
                "set aside up to 2 cards",
            ],
            vec![],
        );
        prompted.prompt = Some(PromptSummary {
            seat: 0,
            why: "set aside up to 2 cards".into(),
            min: 0,
            max: 2,
            picked: 0,
            optional: true,
        });
        assert_eq!(
            turn_plate(&prompted, 0, &name, &zone).unwrap().headline,
            "your choice"
        );
        assert_eq!(
            turn_plate(&prompted, 1, &name, &zone).unwrap().headline,
            "waiting for rae"
        );

        let mut won_game = view(
            &[
                "turn 9 · {seat 1} · action phase · rules enforced",
                "{seat 1} wins with 8 points",
            ],
            vec![],
        );
        won_game.winner = Some(1);
        assert_eq!(
            turn_plate(&won_game, 0, &name, &zone).unwrap().headline,
            "claude wins"
        );

        assert!(turn_plate(&PluginView::default(), 0, &name, &zone).is_none());
        assert!((pulse(0.0) - 0.5).abs() < 1e-5);
        assert!((pulse(PULSE_SECS / 4.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_plate_prefers_the_structured_fields_and_falls_back_to_the_parser() {
        let parsed = view(
            &[
                "turn 3 · {seat 0} · action phase · rules enforced",
                "waiting for {seat 0}: their action phase",
            ],
            vec![],
        );
        let fallback = turn_plate(&parsed, 1, &name, &zone).unwrap();
        assert_eq!(fallback.headline, "waiting for rae");
        assert_eq!(fallback.detail, "turn 3 · action");
        assert_eq!(fallback.mode, Some(Mode::Enforced));
        assert_eq!(turn_of(&parsed).unwrap().phases, Vec::<String>::new());
        assert_eq!(
            waiting_of(&parsed),
            Some(Waiting {
                seat: Some(0),
                what: "their action phase".into()
            })
        );

        let mut told = PluginView {
            status: vec!["something the parser does not know".into()],
            turn: Some(TurnInfo {
                number: 7,
                seat: 1,
                phase: "beginning phase".into(),
                phases: auto::PHASES.iter().map(|phase| phase.to_string()).collect(),
                mode: "free table".into(),
            }),
            waiting: Some(Waiting {
                seat: Some(1),
                what: "their beginning phase".into(),
            }),
            ..Default::default()
        };
        let plate = turn_plate(&told, 0, &name, &zone).unwrap();
        assert_eq!(plate.headline, "waiting for claude");
        assert_eq!(plate.detail, "turn 7 · beginning");
        assert_eq!(plate.mode, Some(Mode::Free));
        assert_eq!(plate.seat, Some(1));
        assert_eq!(mode_of(&told), Some(Mode::Free));
        told.status = vec!["turn 2 · {seat 0} · action phase · rules enforced".into()];
        assert_eq!(
            turn_of(&told).unwrap().number,
            7,
            "the field wins over a contradicting line"
        );
        assert_eq!(mode_of(&told), Some(Mode::Free));
        told.waiting = None;
        told.status = vec!["setup · mulligans".into()];
        assert_eq!(
            turn_plate(&told, 0, &name, &zone).unwrap().headline,
            "setup · mulligans"
        );

        let mut mine = told.clone();
        mine.turn.as_mut().unwrap().seat = 0;
        mine.turn.as_mut().unwrap().mode = "rules enforced".into();
        mine.status.clear();
        mine.affordances = vec![offer("end turn", Some("space"))];
        let plate = turn_plate(&mine, 0, &name, &zone).unwrap();
        assert_eq!(plate.headline, "your action");
        assert_eq!(plate.mode, Some(Mode::Enforced));
        assert!(plate.pulse, "acting() reads the mode from the turn field");
        mine.turn.as_mut().unwrap().mode = "free table".into();
        assert!(
            !turn_plate(&mine, 0, &name, &zone).unwrap().pulse,
            "a free table never pulses, by the field too"
        );
    }

    #[test]
    fn seat_plates_prefer_the_seats_field_over_the_counters_and_the_line() {
        let zones = agni_riftbound::zone_table();
        let mirror = Mirror {
            view: agni_sim::view::TableView {
                zones,
                counter_table: agni_riftbound::counter_table(),
                counters: vec![agni_sim::wire::CounterValue {
                    target: CounterTarget::Seat(1),
                    counter: agni_riftbound::COUNTER_POINTS,
                    value: 5,
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut view = view(
            &["points · {seat 0} 2 · {seat 1} 1 · xp {seat 0} 0 · {seat 1} 3"],
            vec![],
        );
        view.seats = vec![
            SeatInfo {
                seat: 0,
                points: 3,
                victory: 10,
                xp: 1,
                hand: 5,
                deck: 31,
                runes_ready: 3,
                runes_total: 6,
            },
            SeatInfo {
                seat: 1,
                points: 4,
                victory: 10,
                xp: 2,
                hand: 2,
                deck: 30,
                runes_ready: 0,
                runes_total: 2,
            },
        ];
        let info = SessionInfo::default();
        let label = |seat: u8| (name(seat), [seat, 0, 0], seat == 0);
        let plates = seat_plates(&view, &mirror, &Table::new(), &info, 2, &label);
        assert_eq!(plates[0].points, 3);
        assert_eq!(plates[0].victory, 10);
        assert_eq!(plates[0].xp, 1);
        assert_eq!((plates[0].hand, plates[0].deck), (5, 31));
        assert_eq!(plates[0].runes, Some((3, 6)));
        assert_eq!(plates[1].points, 4, "the field wins over the counter");
        assert_eq!(plates[1].xp, 2, "and over the line");
        assert_eq!(plates[1].runes, Some((0, 2)));
        view.seats.clear();
        let plates = seat_plates(&view, &mirror, &Table::new(), &info, 2, &label);
        assert_eq!(
            plates[1].points, 5,
            "without the field the counter is the source"
        );
        assert_eq!(plates[1].xp, 3);
        assert_eq!(plates[1].runes, None);
        assert_eq!(plates[0].victory, 8);
    }

    #[test]
    fn the_phase_bar_needs_the_phase_list_and_toggles_stops_that_persist() {
        let parsed = view(
            &["turn 3 · {seat 0} · action phase · rules enforced"],
            vec![],
        );
        assert_eq!(
            phase_bar(&parsed, 0, &Stops::default()),
            None,
            "the parser carries no phase list"
        );
        let told = PluginView {
            turn: Some(TurnInfo {
                number: 3,
                seat: 1,
                phase: "action phase".into(),
                phases: auto::PHASES.iter().map(|phase| phase.to_string()).collect(),
                mode: "rules enforced".into(),
            }),
            ..Default::default()
        };
        let mut stops = Stops::default();
        let bar = phase_bar(&told, 0, &stops).unwrap();
        assert_eq!(bar.chips.len(), 9);
        assert!(!bar.mine);
        assert_eq!(bar.seat, 1);
        let current: Vec<usize> = bar
            .chips
            .iter()
            .enumerate()
            .filter(|(_, chip)| chip.current)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(current, [5]);
        assert!(bar
            .chips
            .iter()
            .all(|chip| !chip.stop_mine && !chip.stop_theirs));
        assert!(chip_clicked(&mut stops, &bar, 2, false));
        assert!(
            stops.set("beginning phase", false),
            "a click on their turn stops their beginning phase"
        );
        assert!(chip_clicked(&mut stops, &bar, 5, true));
        assert!(
            stops.set("action phase", true),
            "a right-click on their turn stops my action phase"
        );
        assert!(
            !chip_clicked(&mut stops, &bar, 2, false),
            "a second click clears it"
        );
        assert!(!stops.set("beginning phase", false));
        assert!(!chip_clicked(&mut stops, &bar, 40, false));
        let bar = phase_bar(&told, 0, &stops).unwrap();
        assert!(bar.chips[5].stop_mine && !bar.chips[5].stop_theirs);
        assert!(
            chip_tip(&bar.chips[5], false).starts_with("action · their turn: pass · my turn: stop")
        );
        let mut mine = told.clone();
        mine.turn.as_mut().unwrap().seat = 0;
        let bar = phase_bar(&mine, 0, &stops).unwrap();
        assert!(bar.mine);
        assert!(
            chip_tip(&bar.chips[5], true).starts_with("action · my turn: stop · their turn: pass")
        );
        assert!(chip_clicked(&mut stops, &bar, 6, false));
        assert!(
            stops.set("ending step", true),
            "on my turn a click stops my phase"
        );

        let tuned = Tuning {
            stops: stops.clone(),
            ..Tuning::default()
        };
        let json = serde_json::to_string(&tuned).unwrap();
        let back: Tuning = serde_json::from_str(&json).unwrap();
        assert_eq!(back.stops, stops, "the stops persist with tuning.json");
        assert!(phase_bar_shown(ViewportClass::Desktop, InputKind::Pointer));
        assert!(phase_bar_shown(ViewportClass::Tablet, InputKind::Pointer));
        assert!(!phase_bar_shown(ViewportClass::Tablet, InputKind::Touch));
        assert!(!phase_bar_shown(
            ViewportClass::PhonePortrait,
            InputKind::Pointer
        ));
        assert!(!phase_bar_shown(
            ViewportClass::PhoneLandscape,
            InputKind::Touch
        ));
    }

    #[test]
    fn seat_plates_read_the_counters_first_and_the_points_line_second() {
        let mut table = Table::new();
        let zones = agni_riftbound::zone_table();
        let hand = Zone::Plugin(agni_riftbound::ZONE_HAND);
        let deck = Zone::Plugin(deck_zone(&zones).unwrap());
        for _ in 0..4 {
            table.add(PlayerId(1), hand, "x", [0; 3]);
        }
        for _ in 0..30 {
            table.add(PlayerId(1), deck, "y", [0; 3]);
        }
        table.add(PlayerId(0), hand, "z", [0; 3]);
        let mirror = Mirror {
            view: agni_sim::view::TableView {
                zones,
                counter_table: agni_riftbound::counter_table(),
                counters: vec![agni_sim::wire::CounterValue {
                    target: CounterTarget::Seat(1),
                    counter: agni_riftbound::COUNTER_POINTS,
                    value: 5,
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let view = view(
            &["points · {seat 0} 2 · {seat 1} 1 · xp {seat 0} 0 · {seat 1} 3"],
            vec![],
        );
        let info = SessionInfo::default();
        let label = |seat: u8| (name(seat), [seat, 0, 0], seat == 0);
        let plates = ordered(seat_plates(&view, &mirror, &table, &info, 2, &label));
        assert_eq!(plates[0].seat, 1, "the other seat first, mine last");
        assert_eq!(plates[0].points, 5, "the counter wins over the line");
        assert_eq!(plates[0].xp, 3, "no xp counter, so the line");
        assert_eq!(plates[0].hand, 4);
        assert_eq!(plates[0].deck, 30);
        assert_eq!(plates[0].victory, 8);
        assert!(!plates[0].is_me);
        assert_eq!(plates[1].seat, 0);
        assert_eq!(plates[1].points, 2);
        assert_eq!(plates[1].hand, 1);
        assert!(plates[1].is_me);
        assert_eq!(plates[0].connected, None);
        assert!(seat_switch_allowed(true, 2));
        assert!(!seat_switch_allowed(false, 2));
        assert!(seat_switch_allowed(false, 3));
    }

    #[test]
    fn xp_reads_from_the_counter_and_is_nudged_only_on_a_free_table() {
        let zones = agni_riftbound::zone_table();
        let mirror = Mirror {
            view: agni_sim::view::TableView {
                zones,
                counter_table: agni_riftbound::counter_table(),
                counters: vec![agni_sim::wire::CounterValue {
                    target: CounterTarget::Seat(0),
                    counter: agni_riftbound::COUNTER_XP,
                    value: 6,
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let view = view(
            &[
                "turn 3 · {seat 0} · action phase · rules enforced",
                "points · {seat 0} 2 · {seat 1} 1 · xp {seat 0} 1 · {seat 1} 3",
            ],
            vec![],
        );
        let info = SessionInfo::default();
        let label = |seat: u8| (name(seat), [seat, 0, 0], seat == 0);
        let plates = seat_plates(&view, &mirror, &Table::new(), &info, 2, &label);
        assert_eq!(plates[0].xp, 6, "the counter wins over the line");
        assert_eq!(plates[1].xp, 3, "the line when the counter is absent");
        let tools = plugin_ui::Tools::from(&view);
        assert!(!tools.free);
        assert!(
            !counters::nudges_allowed(&tools, SessionRole::Host),
            "under rules enforced the seat plate shows XP and offers no nudge"
        );
        let free = plugin_ui::Tools { free: true };
        assert!(counters::nudges_allowed(&free, SessionRole::Host));
        assert!(!counters::nudges_allowed(&free, SessionRole::Ended));
        let seat_decls: Vec<&agni_sim::wire::CounterDecl> = mirror
            .view
            .counter_table
            .iter()
            .filter(|decl| {
                decl.scope == CounterScope::Seat && decl.place == CounterPlace::SeatPlate
            })
            .collect();
        assert!(
            seat_decls
                .iter()
                .any(|decl| decl.id == agni_riftbound::COUNTER_XP),
            "xp is a seat-plate counter, so the free table's nudge row carries it"
        );
    }
}
