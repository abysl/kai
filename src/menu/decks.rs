use super::deckbox::{deck_tile, tile_columns};
use super::{
    back_button, content_width, gear_button, link, margin, step_title, Menu, FOOTER_H, LOBBY_MAX_W,
    PRIMARY_H,
};
use crate::deck::editor::{self, Draft, Origin, NEW_LABEL};
use crate::deck::{exchange, history, import};
use crate::settings::{DeckParams, Settings};
use crate::theme;
use crate::viewport::ViewportClass;
use agni_riftbound::ResolvedDeck;
use bevy::prelude::Resource;
use bevy_egui::egui;
use spirit_sdk::CiHash;

pub const TITLE: &str = "deck editor";
pub const NEW_DECK: &str = "new deck";
pub const CONTINUE: &str = "continue editing";
pub const STATUS_SECS: f64 = 4.0;
pub const EMPTY_LIBRARY: &str =
    "no saved decks yet — open a new deck and import from the clipboard, or build one";

#[derive(Resource, Default)]
pub struct LibraryState {
    pub rename: Option<(String, CiHash, String)>,
    pub forget_armed: Option<CiHash>,
    pub fresh_armed: bool,
    pub pending: Option<Draft>,
    pub recalled: Option<(CiHash, ResolvedDeck)>,
    pub status: Option<(String, f64)>,
}

impl LibraryState {
    pub fn status(&mut self, text: impl Into<String>, now: f64) {
        self.status = Some((text.into(), now));
    }

    pub fn live_status(&mut self, now: f64) -> Option<&str> {
        if self
            .status
            .as_ref()
            .is_some_and(|(_, since)| now - since > STATUS_SECS)
        {
            self.status = None;
        }
        self.status.as_ref().map(|(text, _)| text.as_str())
    }
}

pub fn go(menu: &mut Menu) {
    menu.open_decks();
}

pub fn replaces_dirty_draft(editor: &editor::DeckEditor) -> bool {
    editor.draft.as_ref().is_some_and(|draft| draft.dirty)
}

pub fn open_draft(menu: &mut Menu, decks: &mut DeckParams, draft: Draft) {
    if replaces_dirty_draft(&decks.editor) {
        decks.library.pending = Some(draft);
        menu.open_decks();
    } else {
        editor::open(&mut decks.editor, menu, draft);
    }
}

pub fn continue_draft(menu: &mut Menu, decks: &mut DeckParams) {
    if decks.editor.draft.is_some() {
        editor::reopen(&mut decks.editor, menu);
    }
}

pub fn new_deck(menu: &mut Menu, decks: &mut DeckParams) {
    open_draft(menu, decks, Draft::new(NEW_LABEL, Origin::New));
}

pub fn edit_seated(menu: &mut Menu, decks: &mut DeckParams, deck: ResolvedDeck, label: &str) {
    open_draft(menu, decks, Draft::from_deck(deck, label, Origin::Seated));
}

pub fn draft_tile(draft: Option<&Draft>) -> Option<(String, &'static str)> {
    let draft = draft?;
    let line = if draft.dirty { CONTINUE } else { "open" };
    let label = if draft.dirty {
        format!("{} · draft", draft.label)
    } else {
        draft.label.clone()
    };
    Some((label, line))
}

pub fn copy_label(label: &str) -> String {
    format!("{label} (copy)")
}

pub fn saved_draft(game: &str, ci: CiHash, label: &str) -> Result<Draft, String> {
    match history::store::recall(game, ci) {
        Some(import::ImportedDeck::Riftbound(deck)) => {
            Ok(Draft::from_deck(deck, label, Origin::Saved(ci)))
        }
        Some(import::ImportedDeck::Mtg(_)) => {
            Err("the deck editor builds Riftbound decks only".into())
        }
        None => Err("that deck is in the list but its bytes are not held here yet".into()),
    }
}

pub fn game_label(game: &str) -> Option<&'static str> {
    match game {
        agni_riftbound::GAME => None,
        agni_mtg::GAME => Some("MTG"),
        _ => None,
    }
}

pub fn editable(game: &str, held: bool) -> bool {
    game == agni_riftbound::GAME && held
}

#[allow(clippy::large_enum_variant)]
enum Act {
    NewDeck,
    Continue,
    StartFresh,
    Replace,
    KeepDraft,
    Open(Draft),
    Rename(String, CiHash, String),
    Forget(String, CiHash),
    Status(String),
}

pub fn library_screen(
    ui: &mut egui::Ui,
    class: ViewportClass,
    menu: &mut Menu,
    settings: &mut Settings,
    decks: &mut DeckParams,
) {
    let phone = class.is_phone();
    let screen_w = ui.max_rect().width();
    let width = content_width(class, screen_w, LOBBY_MAX_W);
    let left = ui.max_rect().min.x + ((screen_w - width) / 2.0).max(margin(class));
    let rect = egui::Rect::from_min_size(
        egui::pos2(left, ui.max_rect().min.y + margin(class) / 2.0),
        egui::vec2(width, ui.max_rect().height() - margin(class)),
    );
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    column.set_max_width(width);
    column.set_min_width(width);
    let ui = &mut column;
    let mut acts = Vec::new();
    ui.horizontal(|ui| {
        if back_button(ui) {
            menu.back();
        }
        ui.label(
            egui::RichText::new(TITLE)
                .size(20.0)
                .strong()
                .color(theme::tokens(ui.ctx()).ink),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if gear_button(ui) {
                settings.open = true;
            }
            if !phone && new_deck_button(ui, 160.0) {
                acts.push(Act::NewDeck);
            }
        });
    });
    ui.separator();
    let footer = if phone { FOOTER_H } else { 8.0 };
    let body_h = (rect.bottom() - ui.cursor().top() - footer).max(crate::settings::MIN_INNER);
    egui::ScrollArea::vertical()
        .id_salt("library body")
        .max_height(body_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(width);
            body(ui, decks, &mut acts);
            ui.add_space(16.0);
        });
    if phone {
        ui.add_space(4.0);
        if new_deck_button(ui, width) {
            acts.push(Act::NewDeck);
        }
    }
    let now = ui.input(|input| input.time);
    for act in acts {
        run(act, now, menu, decks);
    }
}

fn new_deck_button(ui: &mut egui::Ui, width: f32) -> bool {
    let tokens = theme::tokens(ui.ctx());
    ui.add(
        egui::Button::new(
            egui::RichText::new(NEW_DECK)
                .size(16.0)
                .strong()
                .color(tokens.ink),
        )
        .fill(tokens.green)
        .corner_radius(egui::CornerRadius::same(28))
        .min_size(egui::vec2(width, PRIMARY_H)),
    )
    .clicked()
}

fn body(ui: &mut egui::Ui, decks: &mut DeckParams, acts: &mut Vec<Act>) {
    let tokens = theme::tokens(ui.ctx());
    let now = ui.input(|input| input.time);
    if let Some(status) = decks.library.live_status(now) {
        ui.label(egui::RichText::new(status).color(tokens.ink_weak).small());
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
    pending_step(ui, decks, acts);
    draft_step(ui, decks, acts);
    saved_step(ui, decks, acts);
}

fn pending_step(ui: &mut egui::Ui, decks: &mut DeckParams, acts: &mut Vec<Act>) {
    let Some(held) = decks.editor.draft.as_ref().map(|draft| draft.label.clone()) else {
        return;
    };
    let Some(incoming) = decks
        .library
        .pending
        .as_ref()
        .map(|draft| draft.label.clone())
    else {
        return;
    };
    let tokens = theme::tokens(ui.ctx());
    ui.label(
        egui::RichText::new(format!(
            "replace the {held} draft with {incoming}? the unsaved changes are lost"
        ))
        .color(tokens.amber)
        .small(),
    );
    ui.horizontal(|ui| {
        if ui.button("replace").clicked() {
            acts.push(Act::Replace);
        }
        if ui.button("keep the draft").clicked() {
            acts.push(Act::KeepDraft);
        }
    });
}

fn draft_step(ui: &mut egui::Ui, decks: &mut DeckParams, acts: &mut Vec<Act>) {
    let Some((label, line)) = draft_tile(decks.editor.draft.as_ref()) else {
        return;
    };
    let dirty = replaces_dirty_draft(&decks.editor);
    step_title(ui, "in progress");
    let gap = ui.spacing().item_spacing.x;
    let columns = tile_columns(ui.available_width(), gap);
    let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(width);
            ui.set_min_width(width);
            if deck_tile(ui, width, &label, line, None, false) {
                acts.push(Act::Continue);
            }
            if dirty {
                let verb = if decks.library.fresh_armed {
                    "start fresh · are you sure?"
                } else {
                    "start fresh"
                };
                if link(ui, verb).clicked() {
                    acts.push(Act::StartFresh);
                }
            }
        });
    });
}

fn saved_step(ui: &mut egui::Ui, decks: &mut DeckParams, acts: &mut Vec<Act>) {
    step_title(ui, "your decks");
    let tokens = theme::tokens(ui.ctx());
    let rows = decks.history.library.clone();
    if rows.is_empty() {
        ui.label(
            egui::RichText::new(EMPTY_LIBRARY)
                .color(tokens.ink_weak)
                .small(),
        );
    }
    let gap = ui.spacing().item_spacing.x;
    let columns = tile_columns(ui.available_width(), gap);
    let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    let open_origin = decks
        .editor
        .draft
        .as_ref()
        .and_then(|draft| match &draft.origin {
            Origin::Saved(ci) => Some(*ci),
            _ => None,
        });
    for row in rows.chunks(columns) {
        ui.horizontal_top(|ui| {
            for (game, saved) in row {
                ui.vertical(|ui| {
                    ui.set_max_width(width);
                    ui.set_min_width(width);
                    let line = if saved.held { "saved" } else { "bytes missing" };
                    let editable = editable(game, saved.held);
                    let selected = open_origin == Some(saved.ci);
                    if deck_tile(ui, width, &saved.label, line, game_label(game), selected) {
                        if editable {
                            match saved_draft(game, saved.ci, &saved.label) {
                                Ok(draft) => acts.push(Act::Open(draft)),
                                Err(error) => acts.push(Act::Status(error)),
                            }
                        } else if game == agni_mtg::GAME {
                            acts.push(Act::Status(
                                "the deck editor builds Riftbound decks only".into(),
                            ));
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if editable && link(ui, "edit").clicked() {
                            match saved_draft(game, saved.ci, &saved.label) {
                                Ok(draft) => acts.push(Act::Open(draft)),
                                Err(error) => acts.push(Act::Status(error)),
                            }
                        }
                        ui.menu_button("more", |ui| {
                            if ui.button("rename").clicked() {
                                decks.library.rename =
                                    Some((game.clone(), saved.ci, saved.label.clone()));
                                ui.close();
                            }
                            if editable {
                                share_saved(ui, game, saved.ci, decks, acts);
                            }
                            let armed = decks.library.forget_armed == Some(saved.ci);
                            let verb = if armed {
                                "delete · are you sure?"
                            } else {
                                "delete"
                            };
                            if ui.button(verb).clicked() {
                                if armed {
                                    acts.push(Act::Forget(game.clone(), saved.ci));
                                    decks.library.forget_armed = None;
                                    ui.close();
                                } else {
                                    decks.library.forget_armed = Some(saved.ci);
                                }
                            }
                        });
                    });
                });
            }
        });
    }
    if let Some((game, ci, label)) = decks.library.rename.as_mut() {
        let (game, ci) = (game.clone(), *ci);
        let mut done = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(label).desired_width(200.0));
            done = ui.button("save name").clicked();
            cancel = ui.button("cancel").clicked();
        });
        if done && !label.trim().is_empty() {
            acts.push(Act::Rename(game, ci, label.trim().to_string()));
            cancel = true;
        }
        if cancel {
            decks.library.rename = None;
        }
    }
    if let Some(note) = &decks.history.note {
        ui.label(egui::RichText::new(note).color(tokens.ink_weak).small());
    }
}

fn share_saved(
    ui: &mut egui::Ui,
    game: &str,
    ci: CiHash,
    decks: &mut DeckParams,
    acts: &mut Vec<Act>,
) {
    let cached = decks
        .library
        .recalled
        .as_ref()
        .filter(|(held, _)| *held == ci)
        .map(|(_, deck)| deck.clone());
    let deck = match cached {
        Some(deck) => Some(deck),
        None => match history::store::recall(game, ci) {
            Some(import::ImportedDeck::Riftbound(deck)) => {
                decks.library.recalled = Some((ci, deck.clone()));
                Some(deck)
            }
            _ => None,
        },
    };
    let Some(deck) = deck else {
        ui.add_enabled(false, egui::Button::new("share"))
            .on_disabled_hover_text("that deck's bytes are not held here");
        return;
    };
    if let Some(status) = exchange::share_menu(ui, &deck, &mut decks.import.qr) {
        acts.push(Act::Status(status));
        ui.close();
    }
}

fn run(act: Act, now: f64, menu: &mut Menu, decks: &mut DeckParams) {
    match act {
        Act::NewDeck => new_deck(menu, decks),
        Act::Continue => continue_draft(menu, decks),
        Act::StartFresh => {
            if decks.library.fresh_armed {
                decks.library.fresh_armed = false;
                decks.editor.draft = None;
                crate::os::drafts::clear();
                editor::open(&mut decks.editor, menu, Draft::new(NEW_LABEL, Origin::New));
            } else {
                decks.library.fresh_armed = true;
            }
        }
        Act::Replace => {
            decks.editor.draft = None;
            crate::os::drafts::clear();
            if let Some(parked) = decks.library.pending.take() {
                editor::open(&mut decks.editor, menu, parked);
            }
        }
        Act::KeepDraft => decks.library.pending = None,
        Act::Open(draft) => open_draft(menu, decks, draft),
        Act::Rename(game, ci, label) => match history::store::rename(&game, ci, &label) {
            Ok(_) => {
                if let Some(draft) = decks
                    .editor
                    .draft
                    .as_mut()
                    .filter(|draft| draft.origin == Origin::Saved(ci) && !draft.dirty)
                {
                    draft.label = label.clone();
                }
                decks.library.status(format!("renamed to {label}"), now);
            }
            Err(error) => decks.library.status(format!("rename failed: {error}"), now),
        },
        Act::Forget(game, ci) => match history::store::forget(&game, ci) {
            Ok(()) => {
                if let Some(draft) = decks
                    .editor
                    .draft
                    .as_mut()
                    .filter(|draft| draft.origin == Origin::Saved(ci))
                {
                    draft.origin = Origin::New;
                    draft.dirty = true;
                }
                decks.library.recalled = None;
                decks.library.status("deleted", now);
            }
            Err(error) => decks
                .library
                .status(format!("could not delete that deck: {error}"), now),
        },
        Act::Status(text) => decks.library.status(text, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draft_tile_names_a_dirty_draft_and_offers_to_continue() {
        assert_eq!(draft_tile(None), None);
        let mut draft = Draft::new("Lillia Aggro", Origin::New);
        assert_eq!(
            draft_tile(Some(&draft)),
            Some(("Lillia Aggro".to_string(), "open"))
        );
        draft.rename("Lillia Tempo");
        assert_eq!(
            draft_tile(Some(&draft)),
            Some(("Lillia Tempo · draft".to_string(), CONTINUE))
        );
        assert_eq!(copy_label("Lillia (house)"), "Lillia (house) (copy)");
    }

    #[test]
    fn an_entry_point_over_a_dirty_draft_waits_for_the_replace_confirmation() {
        let mut editor = editor::DeckEditor::default();
        assert!(!replaces_dirty_draft(&editor));
        editor.draft = Some(Draft::new("Lillia Aggro", Origin::New));
        assert!(
            !replaces_dirty_draft(&editor),
            "a clean draft is replaced silently"
        );
        let mut draft = Draft::new("Lillia Aggro", Origin::New);
        draft.rename("Lillia Tempo");
        editor.draft = Some(draft);
        assert!(replaces_dirty_draft(&editor));
    }

    #[test]
    fn only_a_held_riftbound_row_opens_in_the_editor() {
        assert!(editable(agni_riftbound::GAME, true));
        assert!(!editable(agni_riftbound::GAME, false));
        assert!(!editable(agni_mtg::GAME, true));
        assert_eq!(game_label(agni_mtg::GAME), Some("MTG"));
        assert_eq!(game_label(agni_riftbound::GAME), None);
    }

    #[test]
    fn the_library_status_lives_four_seconds() {
        let mut state = LibraryState::default();
        assert_eq!(state.live_status(1.0), None);
        state.status("saved x", 1.0);
        assert_eq!(state.live_status(2.0), Some("saved x"));
        assert_eq!(state.live_status(1.0 + STATUS_SECS + 0.1), None);
    }
}
