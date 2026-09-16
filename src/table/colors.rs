use crate::table::{MySeat, SessionInfo, SessionRole};
use agni_core::PlayerId;
use agni_net::session::{default_seat_color, roster_color, SeatInfo, SEAT_PICKABLE_COLORS};
use bevy::prelude::*;
use bevy_egui::egui;

pub const SEAT_COLORS: [(&str, [u8; 3]); 8] = [
    ("blue", [72, 132, 236]),
    ("red", [216, 74, 68]),
    ("green", [76, 186, 108]),
    ("gold", [236, 189, 72]),
    ("purple", [166, 108, 226]),
    ("teal", [64, 196, 200]),
    ("white", [238, 238, 238]),
    ("pink", [236, 122, 182]),
];

pub fn seat_color_name(color: u8) -> &'static str {
    SEAT_COLORS[color as usize % SEAT_COLORS.len()].0
}

pub fn seat_color_rgb(color: u8) -> [u8; 3] {
    SEAT_COLORS[color as usize % SEAT_COLORS.len()].1
}

pub const SEAT_GLYPHS: [(&str, Glyph); 8] = [
    ("circle", Glyph::Circle),
    ("square", Glyph::Square),
    ("triangle", Glyph::Triangle),
    ("diamond", Glyph::Diamond),
    ("star", Glyph::Star),
    ("ring", Glyph::Ring),
    ("bar", Glyph::Bar),
    ("cross", Glyph::Cross),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Circle,
    Square,
    Triangle,
    Diamond,
    Star,
    Ring,
    Bar,
    Cross,
}

pub fn seat_glyph(color: u8) -> Glyph {
    SEAT_GLYPHS[color as usize % SEAT_GLYPHS.len()].1
}

pub fn seat_glyph_name(color: u8) -> &'static str {
    SEAT_GLYPHS[color as usize % SEAT_GLYPHS.len()].0
}

pub fn glyph_of_rgb(rgb: [u8; 3]) -> Option<Glyph> {
    SEAT_COLORS
        .iter()
        .position(|(_, held)| *held == rgb)
        .map(|index| seat_glyph(index as u8))
}

pub fn glyph_points(glyph: Glyph, rect: egui::Rect) -> Vec<egui::Pos2> {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    match glyph {
        Glyph::Square => vec![
            egui::pos2(c.x - r, c.y - r),
            egui::pos2(c.x + r, c.y - r),
            egui::pos2(c.x + r, c.y + r),
            egui::pos2(c.x - r, c.y + r),
        ],
        Glyph::Triangle => vec![
            egui::pos2(c.x, c.y - r),
            egui::pos2(c.x + r, c.y + r),
            egui::pos2(c.x - r, c.y + r),
        ],
        Glyph::Diamond => vec![
            egui::pos2(c.x, c.y - r),
            egui::pos2(c.x + r, c.y),
            egui::pos2(c.x, c.y + r),
            egui::pos2(c.x - r, c.y),
        ],
        Glyph::Star => (0..10)
            .map(|step| {
                let angle = -std::f32::consts::FRAC_PI_2 + step as f32 * std::f32::consts::PI / 5.0;
                let reach = if step % 2 == 0 { r } else { r * 0.45 };
                egui::pos2(c.x + reach * angle.cos(), c.y + reach * angle.sin())
            })
            .collect(),
        Glyph::Bar => vec![
            egui::pos2(c.x - r, c.y - r * 0.35),
            egui::pos2(c.x + r, c.y - r * 0.35),
            egui::pos2(c.x + r, c.y + r * 0.35),
            egui::pos2(c.x - r, c.y + r * 0.35),
        ],
        Glyph::Circle | Glyph::Ring | Glyph::Cross => Vec::new(),
    }
}

pub fn paint_glyph(painter: &egui::Painter, rect: egui::Rect, glyph: Glyph, ink: egui::Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    match glyph {
        Glyph::Circle => {
            painter.circle_filled(c, r, ink);
        }
        Glyph::Ring => {
            painter.circle_stroke(c, r * 0.8, egui::Stroke::new(r * 0.4, ink));
        }
        Glyph::Cross => {
            let stroke = egui::Stroke::new(r * 0.4, ink);
            painter.line_segment(
                [egui::pos2(c.x - r, c.y - r), egui::pos2(c.x + r, c.y + r)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - r, c.y + r), egui::pos2(c.x + r, c.y - r)],
                stroke,
            );
        }
        Glyph::Star => {
            let points = glyph_points(glyph, rect);
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points,
                closed: true,
                fill: ink,
                stroke: egui::epaint::PathStroke::NONE,
            }));
        }
        _ => {
            painter.add(egui::Shape::convex_polygon(
                glyph_points(glyph, rect),
                ink,
                egui::Stroke::NONE,
            ));
        }
    }
}

pub fn swatch(ui: &mut egui::Ui, rgb: [u8; 3], size: f32, colour_blind: bool) {
    let [r, g, b] = rgb;
    let ink = egui::Color32::from_rgb(r, g, b);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, size * 0.2, ink);
    if colour_blind {
        if let Some(glyph) = glyph_of_rgb(rgb) {
            let (mark, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            paint_glyph(
                ui.painter(),
                mark.shrink(size * 0.15),
                glyph,
                egui::Color32::from_rgb(242, 242, 245),
            );
        }
    }
}

pub fn pickable_seat_colors() -> impl Iterator<Item = u8> {
    0..SEAT_PICKABLE_COLORS
}

pub fn contested_color(slot: usize) -> u8 {
    let reserved = SEAT_COLORS.len() as u8 - SEAT_PICKABLE_COLORS;
    SEAT_PICKABLE_COLORS + (slot as u8 % reserved)
}

#[derive(Resource, Default)]
pub struct SeatColors {
    pub wanted: Option<u8>,
    pub asked: Option<u8>,
    pub refused: Option<u8>,
    pub sent_for: Option<u8>,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct ColorPicked(pub u8);

pub fn color_of(roster: &[SeatInfo], local: &SeatColors, my_seat: PlayerId, seat: PlayerId) -> u8 {
    if roster.is_empty() {
        return match (seat == my_seat, local.wanted) {
            (true, Some(wanted)) => wanted,
            _ => default_seat_color(seat.0),
        };
    }
    roster_color(roster, seat.0)
}

pub fn fallback_name(seat: PlayerId) -> String {
    format!("Player {}", seat.0 + 1)
}

pub fn seat_label(
    roster: &[SeatInfo],
    local: &SeatColors,
    me: PlayerId,
    seat: PlayerId,
) -> (String, [u8; 3], bool) {
    let name = roster
        .iter()
        .find(|info| info.seat == seat.0)
        .map(|info| info.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback_name(seat));
    let color = seat_color_rgb(color_of(roster, local, me, seat));
    (name, color, seat == me)
}

pub fn tint(color: u8) -> Color {
    let [r, g, b] = seat_color_rgb(color);
    Color::srgb_u8(r, g, b)
}

pub fn felt(color: u8) -> Color {
    let [r, g, b] = seat_color_rgb(color);
    let dim = |channel: u8| (channel as f32 / 255.0).powf(2.2) * 0.34 + 0.035;
    Color::srgb(dim(r), dim(g), dim(b))
}

pub fn egui_tint(color: u8) -> egui::Color32 {
    let [r, g, b] = seat_color_rgb(color);
    egui::Color32::from_rgb(r, g, b)
}

fn taken_by_others(roster: &[SeatInfo], my_seat: PlayerId) -> Vec<u8> {
    roster
        .iter()
        .filter(|info| info.seat != my_seat.0)
        .map(|info| info.color)
        .collect()
}

pub fn watch_seat_color(
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    mut colors: ResMut<SeatColors>,
    mut picked: MessageWriter<ColorPicked>,
) {
    if !info.is_changed() {
        return;
    }
    if !matches!(info.role, SessionRole::Host | SessionRole::Client) {
        colors.sent_for = None;
        colors.asked = None;
        colors.refused = None;
        return;
    }
    let seated = roster_color(&info.roster, my_seat.0 .0);
    if let Some(asked) = colors.asked {
        if seated == asked {
            colors.asked = None;
            colors.refused = None;
            colors.wanted = None;
        } else if info.roster.iter().any(|seat| seat.color == asked) {
            colors.asked = None;
            colors.refused = Some(asked);
            colors.wanted = None;
        }
        return;
    }
    if colors.sent_for == Some(my_seat.0 .0) {
        return;
    }
    colors.sent_for = Some(my_seat.0 .0);
    if let Some(wanted) = colors.wanted.filter(|wanted| *wanted != seated) {
        colors.asked = Some(wanted);
        picked.write(ColorPicked(wanted));
    }
}

pub fn color_section(
    ui: &mut egui::Ui,
    info: &SessionInfo,
    my_seat: &MySeat,
    colors: &mut SeatColors,
    picked: &mut MessageWriter<ColorPicked>,
) {
    let mine = color_of(&info.roster, colors, my_seat.0, my_seat.0);
    let taken = taken_by_others(&info.roster, my_seat.0);
    let mut wanted = None;
    {
        ui.label(format!("you are {}", seat_color_name(mine)));
        ui.horizontal_wrapped(|ui| {
            for color in pickable_seat_colors() {
                let claimed = taken.contains(&color);
                let label = egui::RichText::new(seat_color_name(color))
                    .color(if claimed {
                        egui::Color32::from_gray(110)
                    } else {
                        egui_tint(color)
                    })
                    .strong();
                let picked = ui
                    .add_enabled(
                        !claimed,
                        egui::Button::selectable(color == mine && !claimed, label),
                    )
                    .clicked();
                if picked && color != mine {
                    wanted = Some(color);
                }
            }
        });
        if let Some(asked) = colors.asked {
            ui.label(
                egui::RichText::new(format!("asking the host for {}…", seat_color_name(asked)))
                    .weak(),
            );
        }
        if let Some(refused) = colors.refused {
            ui.colored_label(
                egui::Color32::from_rgb(220, 180, 110),
                format!(
                    "{} was already taken — pick another",
                    seat_color_name(refused)
                ),
            );
        }
        ui.separator();
        for seat in &info.roster {
            ui.colored_label(
                egui_tint(seat.color),
                format!("{} — {}", seat_color_name(seat.color), seat.name),
            );
        }
        ui.label(egui::RichText::new("contested battlefields carry their own colours").weak());
    }
    if let Some(color) = wanted {
        colors.wanted = Some(color);
        colors.refused = None;
        if matches!(info.role, SessionRole::Host | SessionRole::Client) {
            colors.asked = Some(color);
            picked.write(ColorPicked(color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(seat: u8, color: u8) -> SeatInfo {
        SeatInfo {
            seat,
            name: format!("p{seat}"),
            host: seat == 0,
            connected: true,
            color,
            playmat: None,
        }
    }

    #[test]
    fn a_solo_table_still_names_every_seat_a_colour() {
        let untouched = SeatColors::default();
        assert_eq!(color_of(&[], &untouched, PlayerId(0), PlayerId(0)), 0);
        assert_eq!(color_of(&[], &untouched, PlayerId(0), PlayerId(1)), 1);
        let colors = SeatColors {
            wanted: Some(3),
            ..Default::default()
        };
        assert_eq!(color_of(&[], &colors, PlayerId(0), PlayerId(0)), 3);
        assert_eq!(color_of(&[], &colors, PlayerId(0), PlayerId(1)), 1);
    }

    #[test]
    fn a_roster_colour_always_wins_over_the_local_pick() {
        let roster = vec![seat(0, 4), seat(1, 2)];
        let colors = SeatColors {
            wanted: Some(3),
            ..Default::default()
        };
        assert_eq!(color_of(&roster, &colors, PlayerId(0), PlayerId(0)), 4);
        assert_eq!(color_of(&roster, &colors, PlayerId(0), PlayerId(1)), 2);
        assert_eq!(taken_by_others(&roster, PlayerId(0)), vec![2]);
    }

    #[test]
    fn a_seat_reads_as_its_roster_name_and_falls_back_to_player_n_never_a_colour_word() {
        let roster = vec![seat(0, 4), seat(1, 2)];
        let colors = SeatColors::default();
        assert_eq!(
            seat_label(&roster, &colors, PlayerId(0), PlayerId(0)),
            ("p0".to_string(), seat_color_rgb(4), true)
        );
        assert_eq!(
            seat_label(&roster, &colors, PlayerId(0), PlayerId(1)),
            ("p1".to_string(), seat_color_rgb(2), false)
        );
        assert_eq!(
            seat_label(&[], &colors, PlayerId(0), PlayerId(1)),
            ("Player 2".to_string(), seat_color_rgb(1), false)
        );
        let unnamed = vec![SeatInfo {
            name: "  ".into(),
            ..seat(1, 2)
        }];
        assert_eq!(
            seat_label(&unnamed, &colors, PlayerId(0), PlayerId(1)).0,
            "Player 2"
        );
    }

    #[test]
    fn contested_zones_never_borrow_a_seat_colour() {
        for slot in 0..agni_riftbound::BATTLEFIELD_COUNT {
            assert!(!pickable_seat_colors().any(|color| color == contested_color(slot)));
        }
        assert_ne!(contested_color(0), contested_color(1));
    }

    #[test]
    fn the_palette_names_every_seat_and_contested_zone_apart() {
        let seats: Vec<&str> = pickable_seat_colors().map(seat_color_name).collect();
        let fields: Vec<&str> = (0..agni_riftbound::BATTLEFIELD_COUNT)
            .map(|slot| seat_color_name(contested_color(slot)))
            .collect();
        for name in &fields {
            assert!(!seats.contains(name));
        }
        let all: std::collections::BTreeSet<&str> =
            seats.iter().chain(fields.iter()).copied().collect();
        assert_eq!(all.len(), seats.len() + fields.len());
        assert_eq!(seat_color_rgb(0), SEAT_COLORS[0].1);
        assert_eq!(seat_color_name(3), "gold");
    }

    #[test]
    fn every_seat_colour_pairs_with_its_own_glyph_for_the_colour_blind_palette() {
        let glyphs: std::collections::BTreeSet<&str> =
            (0..SEAT_COLORS.len() as u8).map(seat_glyph_name).collect();
        assert_eq!(glyphs.len(), SEAT_COLORS.len());
        assert_eq!(
            pickable_seat_colors()
                .map(seat_glyph_name)
                .collect::<Vec<_>>(),
            ["circle", "square", "triangle", "diamond", "star"]
        );
        assert_eq!(seat_glyph(0), Glyph::Circle);
        assert_eq!(seat_glyph(8), Glyph::Circle);
        assert_eq!(glyph_of_rgb(SEAT_COLORS[3].1), Some(Glyph::Diamond));
        assert_eq!(glyph_of_rgb([1, 2, 3]), None);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));
        assert_eq!(glyph_points(Glyph::Square, rect).len(), 4);
        assert_eq!(glyph_points(Glyph::Triangle, rect).len(), 3);
        assert_eq!(glyph_points(Glyph::Diamond, rect).len(), 4);
        assert_eq!(glyph_points(Glyph::Star, rect).len(), 10);
        assert!(glyph_points(Glyph::Circle, rect).is_empty());
        for glyph in SEAT_GLYPHS.map(|(_, glyph)| glyph) {
            for point in glyph_points(glyph, rect) {
                assert!(rect.contains(point), "{glyph:?} stays inside its box");
            }
        }
    }

    #[test]
    fn the_felt_stays_darker_than_the_label_tint() {
        for color in pickable_seat_colors() {
            let felt = felt(color).to_srgba();
            let tint = tint(color).to_srgba();
            assert!(felt.red <= tint.red + 1e-4);
            assert!(felt.green <= tint.green + 1e-4);
            assert!(felt.blue <= tint.blue + 1e-4);
        }
    }
}
