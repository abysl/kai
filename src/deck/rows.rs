use crate::menu::TOUCH;
use crate::table::dim;
use crate::theme;
use agni_riftbound::legality::{Grade, Verdict};
use agni_riftbound::ResolvedCard;
use bevy_egui::egui;

pub const THUMB_W: f32 = 40.0;
pub const ROW_H: f32 = 56.0;
pub const FLAG_BAR_W: f32 = 2.0;
pub const COPY_LIMIT_NOTE: &str = "three copies is the limit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    Stepper { cap: u32 },
    Swap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowEvent {
    Plus,
    Minus,
    Open,
    Swap,
}

pub fn verdict_text(verdict: Verdict) -> String {
    match verdict {
        Verdict::Legal => "legal".into(),
        Verdict::Broken(1) => "1 problem".into(),
        Verdict::Broken(n) => format!("{n} problems"),
        Verdict::Unverified => "unverified".into(),
    }
}

pub fn verdict_color(tokens: &theme::Tokens, verdict: Verdict) -> egui::Color32 {
    match verdict {
        Verdict::Legal => tokens.green,
        Verdict::Broken(_) => tokens.danger,
        Verdict::Unverified => tokens.amber,
    }
}

pub fn grade_color(tokens: &theme::Tokens, grade: Grade) -> egui::Color32 {
    match grade {
        Grade::Break => tokens.danger,
        Grade::Unverified => tokens.amber,
        Grade::Advisory => tokens.ink_weak,
    }
}

pub fn grade_glyph(grade: Grade) -> &'static str {
    match grade {
        Grade::Break => "✖",
        Grade::Unverified => "?",
        Grade::Advisory => "·",
    }
}

pub fn verdict_chip(ui: &mut egui::Ui, verdict: Verdict) -> egui::Response {
    let tokens = theme::tokens(ui.ctx());
    let color = verdict_color(&tokens, verdict);
    ui.add(
        egui::Button::new(
            egui::RichText::new(verdict_text(verdict))
                .strong()
                .color(color),
        )
        .corner_radius(egui::CornerRadius::same(12))
        .stroke(egui::Stroke::new(1.0, color))
        .min_size(egui::vec2(TOUCH, 32.0)),
    )
}

pub fn stat_line(card: &ResolvedCard) -> String {
    let mut parts = Vec::new();
    if let Some(energy) = card.energy {
        parts.push(format!("{energy} energy"));
    }
    if let Some(might) = card.might {
        parts.push(format!("{might} might"));
    }
    if let Some(kind) = card.kind.as_deref().filter(|_| parts.is_empty()) {
        parts.push(kind.to_lowercase());
    }
    parts.join(" · ")
}

pub fn first_word(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

pub fn thumb(ui: &mut egui::Ui, texture: Option<egui::TextureId>, name: &str) -> egui::Rect {
    let size = egui::vec2(THUMB_W, THUMB_W * dim::CARD_H / dim::CARD_W);
    match texture {
        Some(texture) => ui.image(egui::load::SizedTexture::new(texture, size)).rect,
        None => {
            let tokens = theme::tokens(ui.ctx());
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, tokens.surface_2);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                first_word(name),
                egui::FontId::proportional(9.0),
                tokens.ink_weak,
            );
            rect
        }
    }
}

pub fn domain_pips(ui: &mut egui::Ui, domains: &[String]) {
    let tokens = theme::tokens(ui.ctx());
    for domain in domains {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 4.0, theme::domain_color(&tokens, domain));
        response.on_hover_text(domain);
    }
}

pub const STEPPER_BUTTON: f32 = 36.0;
pub const COUNT_W: f32 = 24.0;

pub fn controls_width(action: RowAction) -> f32 {
    match action {
        RowAction::Stepper { .. } => STEPPER_BUTTON * 2.0 + COUNT_W,
        RowAction::Swap => TOUCH + COUNT_W,
    }
}

pub fn card_row(
    ui: &mut egui::Ui,
    card: &ResolvedCard,
    count: u32,
    thumb_texture: Option<egui::TextureId>,
    flagged: Option<Grade>,
    action: RowAction,
) -> Option<RowEvent> {
    let tokens = theme::tokens(ui.ctx());
    let mut event = None;
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_H);
        if let Some(grade) = flagged {
            let (bar, _) =
                ui.allocate_exact_size(egui::vec2(FLAG_BAR_W, ROW_H), egui::Sense::hover());
            ui.painter()
                .rect_filled(bar, 0.0, grade_color(&tokens, grade));
        }
        thumb(ui, thumb_texture, &card.name);
        let controls_w = controls_width(action) + ui.spacing().item_spacing.x * 3.0;
        let middle_w = (ui.available_width() - controls_w).max(60.0);
        ui.vertical(|ui| {
            ui.set_max_width(middle_w);
            ui.spacing_mut().item_spacing.y = 2.0;
            let name = ui.add(
                egui::Button::new(egui::RichText::new(&card.name).color(tokens.ink))
                    .frame(false)
                    .truncate()
                    .min_size(egui::vec2(0.0, 24.0)),
            );
            if name.clicked() {
                event = Some(RowEvent::Open);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let line = stat_line(card);
                if !line.is_empty() {
                    ui.add(
                        egui::Label::new(egui::RichText::new(line).small().color(tokens.ink_weak))
                            .truncate(),
                    );
                }
                domain_pips(ui, &card.domain);
            });
        });
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match action {
                RowAction::Stepper { cap } => {
                    let plus = ui.add_enabled(
                        count < cap,
                        egui::Button::new("+").min_size(egui::vec2(STEPPER_BUTTON, STEPPER_BUTTON)),
                    );
                    if plus.clicked() {
                        event = Some(RowEvent::Plus);
                    }
                    if count >= cap {
                        plus.on_disabled_hover_text(COPY_LIMIT_NOTE);
                    }
                    ui.label(
                        egui::RichText::new(count.to_string())
                            .strong()
                            .color(tokens.ink),
                    );
                    if ui
                        .add(
                            egui::Button::new("−")
                                .min_size(egui::vec2(STEPPER_BUTTON, STEPPER_BUTTON)),
                        )
                        .clicked()
                    {
                        event = Some(RowEvent::Minus);
                    }
                }
                RowAction::Swap => {
                    if ui
                        .add(egui::Button::new("swap").min_size(egui::vec2(TOUCH, 40.0)))
                        .clicked()
                    {
                        event = Some(RowEvent::Swap);
                    }
                    ui.label(
                        egui::RichText::new(format!("{count}×"))
                            .strong()
                            .color(tokens.ink),
                    );
                }
            },
        );
    });
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verdict_pill_reads_legal_problems_or_unverified() {
        assert_eq!(verdict_text(Verdict::Legal), "legal");
        assert_eq!(verdict_text(Verdict::Broken(1)), "1 problem");
        assert_eq!(verdict_text(Verdict::Broken(3)), "3 problems");
        assert_eq!(verdict_text(Verdict::Unverified), "unverified");
        let tokens = theme::DARK;
        assert_eq!(verdict_color(&tokens, Verdict::Legal), tokens.green);
        assert_eq!(verdict_color(&tokens, Verdict::Broken(2)), tokens.danger);
        assert_eq!(verdict_color(&tokens, Verdict::Unverified), tokens.amber);
        assert_eq!(grade_color(&tokens, Grade::Break), tokens.danger);
        assert_eq!(grade_color(&tokens, Grade::Unverified), tokens.amber);
        assert_eq!(grade_glyph(Grade::Break), "✖");
    }

    #[test]
    fn the_stat_line_names_energy_and_might_and_falls_back_to_the_kind() {
        let card = ResolvedCard {
            energy: Some(3),
            might: Some(4),
            kind: Some("Unit".into()),
            ..Default::default()
        };
        assert_eq!(stat_line(&card), "3 energy · 4 might");
        let spell = ResolvedCard {
            energy: Some(2),
            kind: Some("Spell".into()),
            ..Default::default()
        };
        assert_eq!(stat_line(&spell), "2 energy");
        let rune = ResolvedCard {
            kind: Some("Rune".into()),
            ..Default::default()
        };
        assert_eq!(stat_line(&rune), "rune");
        assert_eq!(stat_line(&ResolvedCard::default()), "");
        assert_eq!(first_word("Lonely Poro"), "Lonely");
        assert_eq!(first_word(""), "");
    }

    #[test]
    fn a_row_lays_out_inside_a_phone_pane_without_an_event_until_tapped() {
        let context = egui::Context::default();
        let card = ResolvedCard {
            name: "Lonely Poro".into(),
            riftbound_id: "sfd-036-298".into(),
            energy: Some(1),
            domain: vec!["Calm".into()],
            ..Default::default()
        };
        let mut heights = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(328.0, 200.0),
                )),
                ..Default::default()
            };
            let output = context.run_ui(input, |ui| {
                let top = ui.cursor().min.y;
                let event = card_row(
                    ui,
                    &card,
                    3,
                    None,
                    Some(Grade::Break),
                    RowAction::Stepper { cap: 3 },
                );
                assert_eq!(event, None);
                heights.push(ui.cursor().min.y - top);
                let event = card_row(ui, &card, 1, None, None, RowAction::Swap);
                assert_eq!(event, None);
                assert!(
                    ui.min_rect().width() <= 328.0,
                    "the row never overflows the pane"
                );
            });
            output.drop_without_applying_deltas();
        }
        assert!(
            heights
                .iter()
                .all(|height| *height >= ROW_H && *height <= ROW_H + 16.0),
            "a row is one touch-high strip: {heights:?}"
        );
    }
}
