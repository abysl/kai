use crate::table::{SeatDecor, Theme, Tuning};
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::window::{WindowTheme, WindowThemeChanged};
use bevy_egui::{egui, EguiContexts};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolved {
    #[default]
    Dark,
    Light,
}

impl Resolved {
    pub const BOTH: [Resolved; 2] = [Resolved::Dark, Resolved::Light];

    pub fn label(self) -> &'static str {
        match self {
            Resolved::Dark => "dark",
            Resolved::Light => "light",
        }
    }
}

pub fn resolve(theme: Theme, system: Option<Resolved>) -> Resolved {
    match theme {
        Theme::Dark => Resolved::Dark,
        Theme::Light => Resolved::Light,
        Theme::System => system.unwrap_or_default(),
    }
}

pub fn of_window_theme(theme: WindowTheme) -> Resolved {
    match theme {
        WindowTheme::Light => Resolved::Light,
        WindowTheme::Dark => Resolved::Dark,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tokens {
    pub surface: egui::Color32,
    pub surface_2: egui::Color32,
    pub ink: egui::Color32,
    pub ink_weak: egui::Color32,
    pub green: egui::Color32,
    pub amber: egui::Color32,
    pub grey: egui::Color32,
    pub danger: egui::Color32,
    pub scrim: egui::Color32,
    pub hairline: egui::Color32,
    pub domain: [egui::Color32; DOMAINS.len()],
    pub dark_mode: bool,
}

pub const DOMAINS: [&str; 6] = ["Fury", "Calm", "Mind", "Body", "Chaos", "Order"];
pub const COLORLESS: &str = "Colorless";

pub const DARK: Tokens = Tokens {
    surface: egui::Color32::from_rgba_premultiplied(26, 26, 32, 235),
    surface_2: egui::Color32::from_rgb(42, 42, 51),
    ink: egui::Color32::from_rgb(242, 242, 245),
    ink_weak: egui::Color32::from_rgb(169, 169, 181),
    green: egui::Color32::from_rgb(56, 160, 92),
    amber: egui::Color32::from_rgb(240, 178, 50),
    grey: egui::Color32::from_rgb(70, 70, 76),
    danger: egui::Color32::from_rgb(255, 107, 107),
    scrim: egui::Color32::from_rgba_premultiplied(0, 0, 0, 115),
    hairline: egui::Color32::from_rgba_premultiplied(255, 255, 255, 28),
    domain: [
        egui::Color32::from_rgb(232, 84, 72),
        egui::Color32::from_rgb(84, 184, 120),
        egui::Color32::from_rgb(92, 150, 240),
        egui::Color32::from_rgb(236, 158, 58),
        egui::Color32::from_rgb(170, 108, 230),
        egui::Color32::from_rgb(236, 214, 130),
    ],
    dark_mode: true,
};

pub const LIGHT: Tokens = Tokens {
    surface: egui::Color32::from_rgba_premultiplied(246, 245, 242, 240),
    surface_2: egui::Color32::from_rgb(233, 231, 225),
    ink: egui::Color32::from_rgb(26, 26, 31),
    ink_weak: egui::Color32::from_rgb(92, 92, 102),
    green: egui::Color32::from_rgb(20, 118, 68),
    amber: egui::Color32::from_rgb(199, 120, 0),
    grey: egui::Color32::from_rgb(184, 184, 192),
    danger: egui::Color32::from_rgb(179, 38, 30),
    scrim: egui::Color32::from_rgba_premultiplied(0, 0, 0, 77),
    hairline: egui::Color32::from_rgba_premultiplied(0, 0, 0, 36),
    domain: [
        egui::Color32::from_rgb(196, 44, 34),
        egui::Color32::from_rgb(28, 134, 74),
        egui::Color32::from_rgb(34, 96, 208),
        egui::Color32::from_rgb(204, 116, 12),
        egui::Color32::from_rgb(124, 62, 196),
        egui::Color32::from_rgb(172, 142, 28),
    ],
    dark_mode: false,
};

pub const INK_DARK: egui::Color32 = egui::Color32::from_rgb(26, 26, 31);
pub const INK_LIGHT: egui::Color32 = egui::Color32::from_rgb(242, 242, 245);
pub const AA_CONTRAST: f32 = 4.5;
pub const AMBER_FILL_ALPHA: u8 = 51;

impl Tokens {
    pub fn of(resolved: Resolved) -> Tokens {
        match resolved {
            Resolved::Dark => DARK,
            Resolved::Light => LIGHT,
        }
    }

    pub fn amber_fill(&self) -> egui::Color32 {
        let [r, g, b, _] = self.amber.to_array();
        egui::Color32::from_rgba_unmultiplied(r, g, b, AMBER_FILL_ALPHA)
    }

    pub fn surface_opaque(&self) -> egui::Color32 {
        let [r, g, b, _] = self.surface.to_array();
        egui::Color32::from_rgb(r, g, b)
    }

    pub fn chip_pairs(&self) -> Vec<(&'static str, egui::Color32, egui::Color32)> {
        vec![
            ("ink on a chip", self.ink, self.surface_2),
            ("weak ink on a chip", self.ink_weak, self.surface_2),
            ("ink on a panel", self.ink, self.surface_opaque()),
            ("weak ink on a panel", self.ink_weak, self.surface_opaque()),
            ("danger on a panel", self.danger, self.surface_opaque()),
            ("ink on the primary green", best_ink(self.green), self.green),
            ("ink on the primary amber", best_ink(self.amber), self.amber),
            ("ink on the primary grey", best_ink(self.grey), self.grey),
        ]
    }

    pub fn visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = self.surface_opaque();
        visuals.window_fill = self.surface_opaque();
        visuals.extreme_bg_color = self.surface_2;
        visuals.faint_bg_color = self.surface_2;
        visuals.window_stroke = egui::Stroke::new(1.0, self.hairline);
        visuals.widgets.noninteractive.bg_fill = self.surface_2;
        if !self.dark_mode {
            visuals.widgets.inactive.weak_bg_fill = self.surface_2;
            visuals.widgets.inactive.bg_fill = self.surface_2;
            visuals.widgets.hovered.weak_bg_fill = self.grey;
        }
        visuals.widgets.noninteractive.fg_stroke.color = self.ink;
        visuals.widgets.inactive.fg_stroke.color = self.ink;
        visuals.widgets.hovered.fg_stroke.color = self.ink;
        visuals.widgets.active.fg_stroke.color = self.ink;
        visuals.widgets.open.fg_stroke.color = self.ink;
        visuals.selection.bg_fill = self.green;
        visuals.selection.stroke = egui::Stroke::new(1.0, best_ink(self.green));
        visuals.hyperlink_color = self.green;
        visuals.error_fg_color = self.danger;
        visuals.warn_fg_color = self.amber;
        visuals
    }
}

pub fn domain_index(domain: &str) -> Option<usize> {
    DOMAINS
        .iter()
        .position(|name| name.eq_ignore_ascii_case(domain.trim()))
}

pub fn domain_color(tokens: &Tokens, domain: &str) -> egui::Color32 {
    match domain_index(domain) {
        Some(index) => tokens.domain[index],
        None => tokens.grey,
    }
}

pub fn channel(value: u8) -> f32 {
    let c = value as f32 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(color: egui::Color32) -> f32 {
    let [r, g, b, _] = color.to_array();
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

pub fn contrast(left: egui::Color32, right: egui::Color32) -> f32 {
    let (a, b) = (luminance(left), luminance(right));
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

pub fn best_ink(fill: egui::Color32) -> egui::Color32 {
    if contrast(INK_LIGHT, fill) >= contrast(INK_DARK, fill) {
        INK_LIGHT
    } else {
        INK_DARK
    }
}

pub fn felt_tint(resolved: Resolved) -> Color {
    match resolved {
        Resolved::Dark => Color::WHITE,
        Resolved::Light => Color::linear_rgb(1.9, 1.85, 1.7),
    }
}

pub fn clear_color(resolved: Resolved) -> Color {
    match resolved {
        Resolved::Dark => Color::srgb_u8(14, 14, 18),
        Resolved::Light => Color::srgb_u8(206, 202, 194),
    }
}

pub fn ambient_brightness(resolved: Resolved) -> f32 {
    match resolved {
        Resolved::Dark => 80.0,
        Resolved::Light => 220.0,
    }
}

pub fn tinted(original: Color, tint: Color) -> Color {
    let base = original.to_linear();
    let tint = tint.to_linear();
    Color::LinearRgba(LinearRgba::new(
        base.red * tint.red,
        base.green * tint.green,
        base.blue * tint.blue,
        base.alpha,
    ))
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemTheme(pub Option<Resolved>);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTheme {
    pub resolved: Resolved,
    pub tokens: Tokens,
}

impl Default for ActiveTheme {
    fn default() -> Self {
        Self {
            resolved: Resolved::Dark,
            tokens: DARK,
        }
    }
}

const TOKENS_KEY: &str = "kai theme tokens";

pub fn tokens(context: &egui::Context) -> Tokens {
    context
        .data(|data| data.get_temp::<Tokens>(egui::Id::new(TOKENS_KEY)))
        .unwrap_or(DARK)
}

pub fn dress(ui: &mut egui::Ui) -> Tokens {
    let tokens = tokens(ui.ctx());
    if ui.visuals().dark_mode != tokens.dark_mode
        || ui.visuals().panel_fill != tokens.surface_opaque()
    {
        ui.style_mut().visuals = tokens.visuals();
    }
    tokens
}

pub fn track_system_theme(
    mut changes: MessageReader<WindowThemeChanged>,
    mut system: ResMut<SystemTheme>,
) {
    for change in changes.read() {
        let next = Some(of_window_theme(change.theme));
        if system.0 != next {
            system.0 = next;
        }
    }
}

pub fn resolve_theme(
    tuning: Res<Tuning>,
    system: Res<SystemTheme>,
    mut active: ResMut<ActiveTheme>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut contexts: EguiContexts,
    mut published: Local<Option<Resolved>>,
) {
    let resolved = resolve(tuning.theme, system.0);
    if active.resolved != resolved {
        *active = ActiveTheme {
            resolved,
            tokens: Tokens::of(resolved),
        };
    }
    if *published == Some(resolved) {
        return;
    }
    clear.0 = clear_color(resolved);
    ambient.brightness = ambient_brightness(resolved);
    if let Ok(context) = contexts.ctx_mut() {
        context.data_mut(|data| data.insert_temp(egui::Id::new(TOKENS_KEY), active.tokens));
        *published = Some(resolved);
    }
}

pub fn tint_felt(
    active: Res<ActiveTheme>,
    felts: Query<&MeshMaterial3d<StandardMaterial>, With<SeatDecor>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut originals: Local<BTreeMap<AssetId<StandardMaterial>, (Color, Resolved)>>,
) {
    let live: Vec<AssetId<StandardMaterial>> = felts.iter().map(|handle| handle.id()).collect();
    originals.retain(|id, _| live.contains(id));
    for id in live {
        if originals
            .get(&id)
            .is_some_and(|(_, applied)| *applied == active.resolved)
        {
            continue;
        }
        let Some(mut material) = materials.get_mut(id) else {
            continue;
        };
        let original = originals
            .get(&id)
            .map(|(original, _)| *original)
            .unwrap_or(material.base_color);
        material.base_color = tinted(original, felt_tint(active.resolved));
        originals.insert(id, (original, active.resolved));
    }
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SystemTheme>()
            .init_resource::<ActiveTheme>()
            .add_systems(
                Update,
                (track_system_theme, resolve_theme, tint_felt).chain(),
            );
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Stroke {
        points: Vec<egui::Pos2>,
        closed: bool,
    },
    Fill(Vec<egui::Pos2>),
    Circle {
        center: egui::Pos2,
        radius: f32,
        filled: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub name: &'static str,
    pub marks: Vec<Mark>,
}

pub const ICON_BOX: f32 = 24.0;

pub const ICONS: [(&str, &str); 21] = [
    ("back", include_str!("../assets/icons/back.svg")),
    ("forward", include_str!("../assets/icons/forward.svg")),
    ("close", include_str!("../assets/icons/close.svg")),
    ("space", include_str!("../assets/icons/space.svg")),
    ("dot", include_str!("../assets/icons/dot.svg")),
    ("target", include_str!("../assets/icons/target.svg")),
    ("half", include_str!("../assets/icons/half.svg")),
    ("hex", include_str!("../assets/icons/hex.svg")),
    ("bolt", include_str!("../assets/icons/bolt.svg")),
    ("menu", include_str!("../assets/icons/menu.svg")),
    ("question", include_str!("../assets/icons/question.svg")),
    ("check", include_str!("../assets/icons/check.svg")),
    ("sword", include_str!("../assets/icons/sword.svg")),
    ("shield", include_str!("../assets/icons/shield.svg")),
    ("gear", include_str!("../assets/icons/gear.svg")),
    ("hourglass", include_str!("../assets/icons/hourglass.svg")),
    ("eye", include_str!("../assets/icons/eye.svg")),
    ("hand", include_str!("../assets/icons/hand.svg")),
    ("star", include_str!("../assets/icons/star.svg")),
    ("drag", include_str!("../assets/icons/drag.svg")),
    ("spinner", include_str!("../assets/icons/spinner.svg")),
];

fn attribute<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = element;
    while let Some(at) = rest.find(name) {
        let after = &rest[at + name.len()..];
        let preceded = at == 0
            || rest[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        if preceded && after.starts_with('=') {
            let after = after[1..].trim_start();
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let inner = &after[1..];
            let end = inner.find(quote)?;
            return Some(&inner[..end]);
        }
        rest = &rest[at + name.len()..];
    }
    None
}

fn numbers(text: &str) -> Result<Vec<f32>, String> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f32>()
                .map_err(|_| format!("not a number: {part}"))
        })
        .collect()
}

fn points_of(text: &str) -> Result<Vec<egui::Pos2>, String> {
    let values = numbers(text)?;
    if values.len() % 2 != 0 || values.is_empty() {
        return Err(format!("odd point list: {text}"));
    }
    Ok(values
        .chunks(2)
        .map(|pair| egui::pos2(pair[0], pair[1]))
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Command(char),
    Number(f32),
}

fn path_tokens(d: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    let flush = |buffer: &mut String, tokens: &mut Vec<Token>| -> Result<(), String> {
        if !buffer.is_empty() {
            let value = buffer
                .parse::<f32>()
                .map_err(|_| format!("not a number: {buffer}"))?;
            tokens.push(Token::Number(value));
            buffer.clear();
        }
        Ok(())
    };
    for c in d.chars() {
        if c.is_ascii_alphabetic() {
            flush(&mut buffer, &mut tokens)?;
            tokens.push(Token::Command(c));
        } else if c.is_whitespace() || c == ',' {
            flush(&mut buffer, &mut tokens)?;
        } else {
            buffer.push(c);
        }
    }
    flush(&mut buffer, &mut tokens)?;
    Ok(tokens)
}

fn flush_subpath(current: &mut Vec<egui::Pos2>, closed: bool, marks: &mut Vec<Mark>, filled: bool) {
    if current.len() >= 2 {
        let points = std::mem::take(current);
        marks.push(if filled {
            Mark::Fill(points)
        } else {
            Mark::Stroke { points, closed }
        });
    } else {
        current.clear();
    }
}

fn apply_command(
    command: Option<char>,
    args: &[f32],
    current: &mut Vec<egui::Pos2>,
    marks: &mut Vec<Mark>,
    filled: bool,
) -> Result<(), String> {
    match command {
        Some('M') => {
            flush_subpath(current, false, marks, filled);
            for pair in args.chunks(2) {
                if pair.len() == 2 {
                    current.push(egui::pos2(pair[0], pair[1]));
                }
            }
        }
        Some('L') => {
            for pair in args.chunks(2) {
                if pair.len() == 2 {
                    current.push(egui::pos2(pair[0], pair[1]));
                }
            }
        }
        Some('H') => {
            let y = current.last().map(|p| p.y).unwrap_or(0.0);
            for x in args {
                current.push(egui::pos2(*x, y));
            }
        }
        Some('V') => {
            let x = current.last().map(|p| p.x).unwrap_or(0.0);
            for y in args {
                current.push(egui::pos2(x, *y));
            }
        }
        Some('Z') => flush_subpath(current, true, marks, filled),
        Some('\0') | None => {}
        Some(other) => return Err(format!("unsupported path command {other}")),
    }
    Ok(())
}

fn path_marks(d: &str, filled: bool) -> Result<Vec<Mark>, String> {
    let mut marks = Vec::new();
    let mut current: Vec<egui::Pos2> = Vec::new();
    let mut command = None;
    let mut args: Vec<f32> = Vec::new();
    for token in path_tokens(d)?
        .into_iter()
        .chain(std::iter::once(Token::Command('\0')))
    {
        match token {
            Token::Number(value) => args.push(value),
            Token::Command(c) => {
                apply_command(command, &args, &mut current, &mut marks, filled)?;
                args.clear();
                command = Some(c);
            }
        }
    }
    flush_subpath(&mut current, false, &mut marks, filled);
    Ok(marks)
}

pub fn parse_svg(name: &'static str, text: &str) -> Result<Glyph, String> {
    let mut marks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            break;
        };
        let element = &after[..end];
        rest = &after[end + 1..];
        let tag = element
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('/');
        let filled = attribute(element, "fill") != Some("none");
        match tag {
            "path" => {
                let d = attribute(element, "d").ok_or_else(|| format!("{name}: path without d"))?;
                marks.extend(path_marks(d, filled)?);
            }
            "circle" => {
                let read = |key: &str| -> Result<f32, String> {
                    attribute(element, key)
                        .ok_or_else(|| format!("{name}: circle without {key}"))?
                        .parse::<f32>()
                        .map_err(|_| format!("{name}: circle {key} is not a number"))
                };
                marks.push(Mark::Circle {
                    center: egui::pos2(read("cx")?, read("cy")?),
                    radius: read("r")?,
                    filled,
                });
            }
            "polygon" | "polyline" => {
                let points = attribute(element, "points")
                    .ok_or_else(|| format!("{name}: {tag} without points"))?;
                let points = points_of(points)?;
                marks.push(if filled && tag == "polygon" {
                    Mark::Fill(points)
                } else {
                    Mark::Stroke {
                        points,
                        closed: tag == "polygon",
                    }
                });
            }
            _ => {}
        }
    }
    if marks.is_empty() {
        return Err(format!("{name}: no marks"));
    }
    Ok(Glyph { name, marks })
}

pub fn glyphs() -> &'static [Glyph] {
    static SET: OnceLock<Vec<Glyph>> = OnceLock::new();
    SET.get_or_init(|| {
        ICONS
            .iter()
            .filter_map(|(name, text)| parse_svg(name, text).ok())
            .collect()
    })
}

pub fn glyph(name: &str) -> Option<&'static Glyph> {
    glyphs().iter().find(|glyph| glyph.name == name)
}

pub fn paint_glyph(painter: &egui::Painter, rect: egui::Rect, glyph: &Glyph, ink: egui::Color32) {
    let side = rect.width().min(rect.height());
    let scale = side / ICON_BOX;
    let origin = rect.center() - egui::vec2(side, side) / 2.0;
    let map = |p: egui::Pos2| origin + egui::vec2(p.x, p.y) * scale;
    let stroke = egui::Stroke::new((side / 12.0).max(1.0), ink);
    for mark in &glyph.marks {
        match mark {
            Mark::Stroke { points, closed } => {
                let points: Vec<egui::Pos2> = points.iter().map(|p| map(*p)).collect();
                if *closed {
                    painter.add(egui::Shape::closed_line(points, stroke));
                } else {
                    painter.add(egui::Shape::line(points, stroke));
                }
            }
            Mark::Fill(points) => {
                let points: Vec<egui::Pos2> = points.iter().map(|p| map(*p)).collect();
                painter.add(egui::Shape::convex_polygon(points, ink, egui::Stroke::NONE));
            }
            Mark::Circle {
                center,
                radius,
                filled,
            } => {
                if *filled {
                    painter.circle_filled(map(*center), radius * scale, ink);
                } else {
                    painter.circle_stroke(map(*center), radius * scale, stroke);
                }
            }
        }
    }
}

pub fn icon(ui: &mut egui::Ui, name: &str, size: f32, ink: egui::Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if let Some(glyph) = glyph(name) {
        paint_glyph(ui.painter(), rect, glyph, ink);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_ink_meets_aa_contrast_in_both_themes() {
        for resolved in Resolved::BOTH {
            let tokens = Tokens::of(resolved);
            for (what, ink, fill) in tokens.chip_pairs() {
                let ratio = contrast(ink, fill);
                assert!(
                    ratio >= AA_CONTRAST,
                    "{what} in the {} theme is {ratio:.2}:1",
                    resolved.label()
                );
            }
        }
    }

    #[test]
    fn the_theme_setting_resolves_and_system_follows_the_window() {
        assert_eq!(resolve(Theme::Dark, Some(Resolved::Light)), Resolved::Dark);
        assert_eq!(resolve(Theme::Light, None), Resolved::Light);
        assert_eq!(
            resolve(Theme::System, Some(Resolved::Light)),
            Resolved::Light
        );
        assert_eq!(resolve(Theme::System, None), Resolved::Dark);
        assert_eq!(of_window_theme(WindowTheme::Light), Resolved::Light);
        assert!(Tokens::of(Resolved::Dark).visuals().dark_mode);
        assert!(!Tokens::of(Resolved::Light).visuals().dark_mode);
        assert_eq!(
            Tokens::of(Resolved::Light).visuals().panel_fill,
            LIGHT.surface_opaque()
        );
        assert_ne!(felt_tint(Resolved::Light), felt_tint(Resolved::Dark));
        assert_eq!(
            tinted(Color::WHITE, Color::WHITE).to_linear(),
            LinearRgba::WHITE
        );
        assert_ne!(clear_color(Resolved::Light), clear_color(Resolved::Dark));
    }

    #[test]
    fn dress_gives_a_menu_ui_the_published_tokens_and_their_visuals() {
        let context = egui::Context::default();
        let mut seen = Vec::new();
        for resolved in Resolved::BOTH {
            context
                .data_mut(|data| data.insert_temp(egui::Id::new(TOKENS_KEY), Tokens::of(resolved)));
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                assert_eq!(tokens(ui.ctx()), Tokens::of(resolved));
                let dressed = dress(ui);
                seen.push((
                    dressed,
                    ui.visuals().dark_mode,
                    ui.visuals().panel_fill,
                    ui.visuals().widgets.inactive.fg_stroke.color,
                ));
            });
            output.drop_without_applying_deltas();
        }
        assert_eq!(seen.len(), 2);
        for ((dressed, dark, fill, ink), resolved) in seen.into_iter().zip(Resolved::BOTH) {
            let expected = Tokens::of(resolved);
            assert_eq!(dressed, expected);
            assert_eq!(dark, expected.dark_mode);
            assert_eq!(fill, expected.surface_opaque());
            assert_eq!(ink, expected.ink);
        }
        assert_eq!(tokens(&egui::Context::default()), DARK);
    }

    #[test]
    fn every_domain_has_its_own_pip_and_colorless_falls_back_to_grey() {
        for tokens in [DARK, LIGHT] {
            let pips: std::collections::BTreeSet<[u8; 4]> = DOMAINS
                .iter()
                .map(|d| domain_color(&tokens, d).to_array())
                .collect();
            assert_eq!(pips.len(), DOMAINS.len(), "six distinct domain colors");
            assert_eq!(domain_color(&tokens, "fury"), tokens.domain[0]);
            assert_eq!(domain_color(&tokens, " Order "), tokens.domain[5]);
            assert_eq!(domain_color(&tokens, COLORLESS), tokens.grey);
            assert_eq!(domain_color(&tokens, ""), tokens.grey);
            for pip in DOMAINS.iter().map(|d| domain_color(&tokens, d)) {
                assert!(
                    contrast(pip, tokens.surface_opaque()) >= 2.0,
                    "a pip must stand off the panel in {} mode",
                    if tokens.dark_mode { "dark" } else { "light" }
                );
            }
        }
        assert_eq!(domain_index("Chaos"), Some(4));
        assert_eq!(domain_index("Void"), None);
    }

    #[test]
    fn best_ink_picks_the_readable_side() {
        assert_eq!(best_ink(DARK.surface_2), INK_LIGHT);
        assert_eq!(best_ink(LIGHT.surface_2), INK_DARK);
        assert_eq!(best_ink(DARK.amber), INK_DARK);
        let white = egui::Color32::WHITE;
        let black = egui::Color32::BLACK;
        assert!((contrast(white, black) - 21.0).abs() < 0.01);
        assert!((contrast(white, white) - 1.0).abs() < 0.001);
    }

    #[test]
    fn every_icon_parses_inside_its_box_with_a_unique_name() {
        let mut names = Vec::new();
        for (name, text) in ICONS {
            let glyph = parse_svg(name, text).unwrap_or_else(|e| panic!("{e}"));
            assert!(!glyph.marks.is_empty(), "{name} paints nothing");
            for mark in &glyph.marks {
                let points: Vec<egui::Pos2> = match mark {
                    Mark::Stroke { points, .. } | Mark::Fill(points) => points.clone(),
                    Mark::Circle { center, radius, .. } => vec![
                        egui::pos2(center.x - radius, center.y - radius),
                        egui::pos2(center.x + radius, center.y + radius),
                    ],
                };
                for point in points {
                    assert!(
                        (0.0..=ICON_BOX).contains(&point.x) && (0.0..=ICON_BOX).contains(&point.y),
                        "{name} leaves the box at {point:?}"
                    );
                }
            }
            assert!(!names.contains(&name), "{name} is listed twice");
            names.push(name);
        }
        assert_eq!(glyphs().len(), ICONS.len());
        assert!(glyph("bolt").is_some());
        assert!(glyph("emoji").is_none());
    }

    #[test]
    fn the_path_parser_reads_subpaths_closes_and_refuses_arcs() {
        let marks = path_marks("M5 5 L19 19 M19 5 L5 19", false).unwrap();
        assert_eq!(marks.len(), 2);
        let closed = path_marks("M6 4 L18 4 L12 12 Z", false).unwrap();
        assert!(matches!(closed[0], Mark::Stroke { closed: true, .. }));
        let filled = path_marks("M0 0 H24 V24 H0 Z", true).unwrap();
        assert!(matches!(&filled[0], Mark::Fill(points) if points.len() == 4));
        assert!(path_marks("M12 3 A9 9 0 0 0 12 21", false).is_err());
        assert!(parse_svg("empty", "<svg></svg>").is_err());
        assert_eq!(
            attribute(r#"circle cx="12" cy="3" r="9" fill="none""#, "r"),
            Some("9")
        );
        assert_eq!(attribute(r#"circle cx="12""#, "x"), None);
    }
}
