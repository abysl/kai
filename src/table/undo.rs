use super::*;
use crate::net::undo::Command;

#[path = "undo_batch.rs"]
mod batch;
use batch::Batch;

#[derive(Resource, Default)]
pub struct UndoUi {
    batch: Batch,
    sent_at: Option<f64>,
    sent_revision: u64,
    voted: Option<u64>,
}

pub fn undo_ui(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    menu: Res<crate::menu::Menu>,
    info: Res<SessionInfo>,
    me: Res<MySeat>,
    mut undo: ResMut<UndoUi>,
    mut commands: MessageWriter<Command>,
) -> Result {
    let context = contexts.ctx_mut()?;
    if !menu.at_table() || !matches!(info.role, SessionRole::Host | SessionRole::Client) {
        *undo = UndoUi::default();
        return Ok(());
    }
    let now = time.elapsed_secs_f64();
    let status = &info.undo;
    undo.batch.invalidate(status.revision);
    if status.proposal.is_some()
        || status.revision != undo.sent_revision
        || undo.sent_at.is_some_and(|sent| now - sent >= 5.0)
    {
        undo.sent_at = None;
    }
    if status.proposal.is_some() {
        undo.batch = Batch::default();
    }
    if status.proposal.is_none() {
        undo.voted = None;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let super_key = keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight);
    let shortcut = batch::shortcut(
        shift,
        control,
        alt || super_key,
        keys.just_pressed(KeyCode::Backspace),
        keys.just_pressed(KeyCode::KeyZ),
        context.egui_wants_keyboard_input(),
    );
    if shortcut
        && !context.egui_wants_keyboard_input()
        && status.proposal.is_none()
        && undo.sent_at.is_none()
    {
        undo.batch.press(now, status.revision, status.available);
    }
    if let Some((actions, revision)) = undo.batch.take_due(now) {
        commands.write(Command::Request { actions, revision });
        undo.sent_at = Some(now);
        undo.sent_revision = revision;
    }
    if undo.batch.count() > 0 {
        egui::Window::new("Undo queued")
            .default_width(300.0)
            .max_width((context.content_rect().width() - 24.0).max(120.0))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!("Request undo of {} action(s)…", undo.batch.count()));
                ui.label("Waiting one second after your last keypress.");
                if ui.button("Cancel").clicked() {
                    undo.batch = Batch::default();
                }
            });
    }
    if let Some(proposal) = &status.proposal {
        let waiting_on_me = proposal.waiting.contains(&me.0 .0);
        let requester = info
            .roster
            .iter()
            .find(|seat| seat.seat == proposal.requester)
            .map(|seat| seat.name.as_str())
            .unwrap_or("A player");
        egui::Window::new("Undo request")
            .default_width(300.0)
            .max_width((context.content_rect().width() - 24.0).max(120.0))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!(
                    "{requester} asks to roll back {} action(s).",
                    proposal.actions
                ));
                ui.label("This affects the whole table. Revealed information cannot be forgotten.");
                if waiting_on_me && undo.voted != Some(proposal.id) {
                    ui.horizontal(|ui| {
                        for (label, accept) in [("Agree", true), ("Decline", false)] {
                            if ui.button(label).clicked() {
                                commands.write(Command::Vote {
                                    id: proposal.id,
                                    accept,
                                });
                                undo.voted = Some(proposal.id);
                            }
                        }
                    });
                } else {
                    ui.label("Waiting for the other player(s) to agree.");
                }
                ui.label("Continuing play cancels this request.");
            });
    }
    Ok(())
}
