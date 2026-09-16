use super::hud::{self, Hud};
use super::*;
use crate::table::plugin_ui::PluginPanel;

#[derive(Resource, Default, Debug)]
pub struct WinnerDialog {
    dismissed: Option<(u32, u8)>,
}

pub enum Choice {
    Leave,
    NewGame,
}

pub fn headline(name: &str, points: Option<i32>) -> String {
    match points {
        Some(points) => format!("{name} wins · {points} points"),
        None => format!("{name} wins"),
    }
}

pub fn winner_points(view: &agni_sim::wire::PluginView, winner: u8) -> Option<i32> {
    plate::lines(view).into_iter().find_map(|line| match line {
        plate::StatusLine::Winner { seat, points } if seat == winner => Some(points),
        _ => None,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn winner_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    panel: Res<PluginPanel>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    seat_colors: Res<colors::SeatColors>,
    generation: Res<DealGeneration>,
    menu: Res<crate::menu::Menu>,
    mut dialog: ResMut<WinnerDialog>,
    mut choices: MessageWriter<WinnerChoice>,
) -> Result {
    let Some(winner) = panel.view.winner else {
        return Ok(());
    };
    if !menu.at_table() || dialog.dismissed == Some((generation.0, winner)) {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let (name, color, _) =
        colors::seat_label(&info.roster, &seat_colors, my_seat.0, PlayerId(winner));
    let text = headline(&name, winner_points(&panel.view, winner));
    let tint = egui::Color32::from_rgb(color[0], color[1], color[2]);
    let rect = hud.0.banner;
    let mut picked = None;
    let mut keep_looking = false;
    hud::slot(&context, "winner banner", rect, |ui| {
        hud::panel_frame(ui.style())
            .stroke(egui::Stroke::new(2.0, tint))
            .show(ui, |ui| {
                ui.set_min_width(rect.width() - 16.0);
                ui.set_max_width(rect.width() - 16.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&text).size(28.0).strong().color(tint));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text("keep looking").clicked() {
                            keep_looking = true;
                        }
                    });
                });
                ui.add_space(hud::GAP);
                ui.horizontal(|ui| {
                    let can_restart = matches!(info.role, SessionRole::Host | SessionRole::Client);
                    let again = egui::Button::new(egui::RichText::new("play again").strong())
                        .fill(hud::GREEN)
                        .min_size(egui::vec2(140.0, hud::TOUCH_MIN));
                    if ui
                        .add_enabled(can_restart, again)
                        .on_hover_text("the host resets the table and everyone deals in again")
                        .clicked()
                    {
                        picked = Some(Choice::NewGame);
                    }
                    let leave =
                        egui::Button::new("leave").min_size(egui::vec2(100.0, hud::TOUCH_MIN));
                    if ui.add(leave).clicked() {
                        picked = Some(Choice::Leave);
                    }
                });
            });
    });
    if keep_looking {
        dialog.dismissed = Some((generation.0, winner));
    }
    if let Some(choice) = picked {
        dialog.dismissed = Some((generation.0, winner));
        choices.write(WinnerChoice(choice));
    }
    Ok(())
}

#[derive(Message)]
pub struct WinnerChoice(pub Choice);

pub fn route_winner_choices(
    mut choices: MessageReader<WinnerChoice>,
    mut menu: ResMut<crate::menu::Menu>,
    mut info: ResMut<SessionInfo>,
    mut host: ResMut<crate::net::HostState>,
    mut client: ResMut<crate::net::ClientState>,
    mut redeals: MessageWriter<Redeal>,
) {
    for choice in choices.read() {
        match choice.0 {
            Choice::Leave => {
                crate::net::leave_session(&mut info, &mut host, &mut client);
                menu.screen = crate::menu::Screen::Games;
            }
            Choice::NewGame => match info.role {
                SessionRole::Host => {
                    redeals.write(Redeal);
                }
                SessionRole::Client => crate::net::ask_new_game(),
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::PluginView;

    #[test]
    fn the_banner_names_the_winner_and_the_score_from_the_presenters_line() {
        let view = PluginView {
            status: vec![
                "turn 9 · {seat 1} · action phase · rules enforced".into(),
                "points · {seat 0} 3 · {seat 1} 8".into(),
                "{seat 1} wins with 8 points".into(),
            ],
            winner: Some(1),
            ..Default::default()
        };
        assert_eq!(winner_points(&view, 1), Some(8));
        assert_eq!(winner_points(&view, 0), None);
        assert_eq!(headline("claude", Some(8)), "claude wins · 8 points");
        assert_eq!(headline("claude", None), "claude wins");
    }
}
