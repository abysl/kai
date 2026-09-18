use super::{opponent_rating, storage, Entry, History, Outcome};
use bevy_egui::egui;

pub struct Panel {
    history: History,
    opponent: String,
    outcome: Option<Outcome>,
    load_error: Option<String>,
    message: String,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            history: History::default(),
            opponent: String::new(),
            outcome: None,
            load_error: None,
            message: String::new(),
        }
    }
}

impl Panel {
    pub fn load() -> Self {
        match storage::load() {
            Ok(history) => Self {
                history,
                ..Self::default()
            },
            Err(error) => Self {
                load_error: Some(error),
                ..Self::default()
            },
        }
    }

    fn commit_with(
        &mut self,
        next: History,
        save: impl FnOnce(&History) -> Result<(), String>,
    ) -> bool {
        match save(&next) {
            Ok(()) => {
                self.history = next;
                true
            }
            Err(error) => {
                self.message = format!("Not saved; estimate unchanged. {error}");
                false
            }
        }
    }

    fn record(&mut self, entry: Entry) {
        let mut next = self.history.clone();
        if next.record(entry.opponent, entry.outcome).is_ok()
            && self.commit_with(next, storage::store)
        {
            self.message = format!(
                "Saved {}: {} to {} ({:+}).",
                entry.outcome.label(),
                entry.before,
                entry.after,
                entry.delta()
            );
            self.outcome = None;
            self.opponent.clear();
        }
    }

    fn undo(&mut self) {
        let mut next = self.history.clone();
        if let Some(entry) = next.undo() {
            if self.commit_with(next, storage::store) {
                self.opponent = entry.opponent.to_string();
                self.outcome = Some(entry.outcome);
                self.message = format!("Undone. Estimate restored to {}. Adjust the entry above and record it again to correct it.", entry.before);
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("personal Elo");
        ui.label("An honor-based personal estimate. Record your own results using your opponent’s self-reported Elo. Only saved on this device; each player updates independently.");
        if let Some(error) = &self.load_error {
            ui.label(format!("Estimate unavailable: {error}"));
            ui.label("The saved history has been left untouched. Restore it or fix storage access, then retry.");
            if ui.button("retry loading").clicked() {
                *self = Self::load();
            }
            return;
        }
        ui.label(egui::RichText::new(format!("your estimate: {}", self.history.rating())).strong());
        ui.small("Starts at 1200 · K = 32 · change rounded to the nearest whole point (ties away from zero).");
        self.form(ui);
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        ui.add_space(8.0);
        if ui
            .add_enabled(
                !self.history.entries.is_empty(),
                egui::Button::new("undo latest result").min_size(egui::vec2(0.0, 36.0)),
            )
            .clicked()
        {
            self.undo();
        }
        egui::CollapsingHeader::new("recent results").show(ui, |ui| {
            ui.label("Latest 50 entries, newest first. To correct an entry, undo back to it, then re-enter it and any later results.");
            for entry in self.history.entries.iter().rev() {
                ui.label(format!("{} vs {}: {} to {} ({:+})", entry.outcome.label(), entry.opponent, entry.before, entry.after, entry.delta()));
            }
            ui.small(format!("Estimate before retained history: {}", self.history.baseline));
        });
    }

    fn form(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label("opponent’s self-reported Elo");
        ui.add_sized(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y.max(36.0)),
            egui::TextEdit::singleline(&mut self.opponent)
                .hint_text("e.g. 1200")
                .char_limit(16)
                .desired_width(ui.available_width()),
        );
        let opponent = opponent_rating(&self.opponent);
        if !self.opponent.is_empty() && opponent.is_err() {
            ui.label(super::INPUT_ERROR);
        }
        ui.horizontal_wrapped(|ui| {
            for outcome in [Outcome::Win, Outcome::Loss, Outcome::Draw] {
                if ui
                    .add(
                        egui::Button::new(outcome.label())
                            .selected(self.outcome == Some(outcome))
                            .min_size(egui::vec2(64.0, 36.0)),
                    )
                    .clicked()
                {
                    self.outcome = Some(outcome);
                }
            }
        });
        let preview = opponent
            .ok()
            .zip(self.outcome)
            .and_then(|(opponent, outcome)| {
                Entry::new(self.history.rating(), opponent, outcome).ok()
            });
        if let Some(entry) = preview {
            ui.label(format!(
                "{} to {} ({:+})",
                entry.before,
                entry.after,
                entry.delta()
            ));
        } else {
            ui.label("Enter an opponent rating and choose your result.");
        }
        if ui
            .add_enabled(
                preview.is_some(),
                egui::Button::new("record result locally").min_size(egui::vec2(0.0, 36.0)),
            )
            .clicked()
        {
            self.record(preview.unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        panel: &mut Panel,
        context: &egui::Context,
        width: f32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 1800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_max_width(width - 32.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                panel.show(ui);
                assert!(ui.min_rect().width() <= width, "panel overflow at {width}");
            },
        )
    }

    fn text_position(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(shape) if shape.galley.text() == text => {
                    Some(shape.pos + shape.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text: {text}"))
    }

    #[test]
    fn pointer_and_touch_select_a_result_at_phone_and_desktop_widths() {
        for width in [360.0, 1280.0] {
            for touch in [false, true] {
                let mut panel = Panel {
                    opponent: "1600".into(),
                    ..Default::default()
                };
                let context = egui::Context::default();
                frame(&mut panel, &context, width, vec![]).drop_without_applying_deltas();
                let output = frame(&mut panel, &context, width, vec![]);
                let pos = text_position(&output, "win");
                output.drop_without_applying_deltas();
                for pressed in [true, false] {
                    let mut events = vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::default(),
                        },
                    ];
                    if touch {
                        events.push(egui::Event::Touch {
                            device_id: egui::TouchDeviceId(1),
                            id: egui::TouchId(1),
                            phase: if pressed {
                                egui::TouchPhase::Start
                            } else {
                                egui::TouchPhase::End
                            },
                            pos,
                            force: None,
                        });
                    }
                    frame(&mut panel, &context, width, events).drop_without_applying_deltas();
                }
                assert_eq!(panel.outcome, Some(Outcome::Win));
                let output = frame(&mut panel, &context, width, vec![]);
                text_position(&output, "1200 to 1229 (+29)");
                output.drop_without_applying_deltas();
                panel.opponent = "NaN".into();
                let output = frame(&mut panel, &context, width, vec![]);
                text_position(&output, super::super::INPUT_ERROR);
                output.drop_without_applying_deltas();
                assert_eq!(panel.history.rating(), 1200);
            }
        }
    }

    #[test]
    fn failed_record_or_undo_does_not_change_the_estimate_or_history() {
        let mut panel = Panel::default();
        let mut next = panel.history.clone();
        next.record(1200, Outcome::Win).unwrap();
        assert!(!panel.commit_with(next.clone(), |_| Err("disk full".into())));
        assert_eq!(panel.history, History::default());
        assert!(panel.message.contains("unchanged"));
        assert!(panel.commit_with(next, |_| Ok(())));
        let original = panel.history.clone();
        let mut next = original.clone();
        next.undo();
        assert!(!panel.commit_with(next, |_| Err("read only".into())));
        assert_eq!(panel.history, original);
    }
}
