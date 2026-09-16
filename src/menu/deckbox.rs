use super::opponent::{AiChoice, Opponent};
use super::{chip, decks, link, step_title, DeckSeat, Menu, Sheet};
use crate::deck::editor::{Draft, Origin};
use crate::deck::thumbs::Thumbs;
use crate::deck::{history, import, sideboard};
use crate::net::{self, TableGame};
use crate::settings::{DeckParams, NetParams, TableParams};
use crate::table::hud::{self, Side};
use crate::table::MySeat;
use crate::theme;
use crate::viewport::ViewportClass;
use agni_riftbound::DeckEntry;
use bevy_egui::{egui, EguiContexts};

pub const TILE_MIN_W: f32 = 140.0;
pub const EDITOR_LINK: &str = "open the deck editor";
pub const EDITOR_LINE: &str = "build, import, rename and share decks in the deck editor";

pub fn tile_columns(available_width: f32, gap: f32) -> usize {
    (((available_width + gap) / (TILE_MIN_W + gap)).floor() as usize).clamp(1, 4)
}

pub fn title(seat: DeckSeat) -> &'static str {
    match seat {
        DeckSeat::Mine => "your deck",
        DeckSeat::Ai => "the AI's deck",
    }
}

pub fn shows_history(_seat: DeckSeat, _enforced: bool) -> bool {
    true
}

pub fn shows_pool(game: TableGame) -> bool {
    game == TableGame::Riftbound
}

pub fn shows_battlefield_step() -> bool {
    false
}

pub fn offers_editor(seat: DeckSeat, game: TableGame) -> bool {
    seat == DeckSeat::Mine && game == TableGame::Riftbound
}

fn legend_entries(deck: &import::ImportedDeck) -> Vec<DeckEntry> {
    match deck {
        import::ImportedDeck::Riftbound(deck) => deck
            .legend
            .iter()
            .chain(deck.chosen_champion.iter())
            .map(|card| DeckEntry {
                card: card.clone(),
                count: 1,
            })
            .collect(),
        import::ImportedDeck::Mtg(_) => Vec::new(),
    }
}

pub fn stage_thumbs(
    contexts: &mut EguiContexts,
    thumbs: &mut Thumbs,
    decks: &mut DeckParams,
    opponent: &Opponent,
) {
    let DeckParams {
        seated,
        art,
        images,
        registry,
        ..
    } = decks;
    if let Some(record) = &seated.0 {
        let legends = legend_entries(&record.deck);
        if let import::ImportedDeck::Riftbound(deck) = &record.deck {
            thumbs.stage(
                contexts,
                registry,
                images,
                legends
                    .iter()
                    .chain(deck.battlefields.iter())
                    .chain(deck.main_deck.iter())
                    .chain(deck.sideboard.iter()),
                &record.faces,
                art,
            );
        }
    }
    if let Some((_, deck)) = &opponent.cached {
        let faces = import::face_map(deck);
        let legends = legend_entries(deck);
        if let import::ImportedDeck::Riftbound(deck) = deck {
            thumbs.stage(
                contexts,
                registry,
                images,
                legends.iter().chain(deck.battlefields.iter()),
                &faces,
                art,
            );
        }
    }
}

pub fn deck_tile(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    line: &str,
    chip_text: Option<&str>,
    selected: bool,
) -> bool {
    let text = match chip_text {
        Some(chip) => format!("{label}\n{line}\n{chip}"),
        None => format!("{label}\n{line}"),
    };
    ui.allocate_ui(egui::vec2(width, 72.0), |ui| {
        ui.set_max_width(width);
        ui.set_min_width(width);
        ui.add(
            egui::Button::new(egui::RichText::new(text).size(13.0))
                .wrap()
                .selected(selected)
                .min_size(egui::vec2(width, 72.0)),
        )
        .clicked()
    })
    .inner
}

#[allow(clippy::too_many_arguments)]
pub fn deckbox_ui(
    context: &egui::Context,
    class: ViewportClass,
    game: TableGame,
    menu: &mut Menu,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) {
    let Some(Sheet::DeckBox(seat)) = menu.sheet else {
        return;
    };
    let mut open = true;
    let mut outcome = BoxOutcome::default();
    hud::sheet(
        context,
        "deck box",
        class,
        Side::Right,
        title(seat),
        &mut open,
        |ui| outcome = deckbox_body(ui, seat, game, my_seat, table, net, decks, thumbs),
    );
    if !open {
        menu.sheet = None;
    }
    if outcome.editor {
        decks::go(menu);
    }
    if let Some(draft) = outcome.draft {
        decks::open_draft(menu, decks, draft);
    }
}

#[derive(Default)]
pub struct BoxOutcome {
    pub draft: Option<Draft>,
    pub editor: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn deckbox_body(
    ui: &mut egui::Ui,
    seat: DeckSeat,
    game: TableGame,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) -> BoxOutcome {
    let enforced = net::rules_enforced(&net.info, &net.choice);
    let _ = my_seat;
    let mut outcome = BoxOutcome::default();
    decks_step(ui, seat, game, enforced, net, decks);
    if seat == DeckSeat::Mine && game == TableGame::Riftbound {
        outcome.draft = sideboard_step(ui, table, net, decks, thumbs);
    }
    if offers_editor(seat, game) {
        ui.add_space(12.0);
        ui.separator();
        ui.label(
            egui::RichText::new(EDITOR_LINE)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
        outcome.editor = link(ui, EDITOR_LINK).clicked();
    }
    outcome
}

fn decks_step(
    ui: &mut egui::Ui,
    seat: DeckSeat,
    game: TableGame,
    enforced: bool,
    net: &mut NetParams,
    decks: &mut DeckParams,
) {
    step_title(ui, "your decks");
    let gap = ui.spacing().item_spacing.x;
    let columns = tile_columns(ui.available_width(), gap);
    let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    let _ = enforced;
    if seat == DeckSeat::Ai
        && shows_pool(game)
        && chip(ui, "let the AI pick", net.opponent.ai == AiChoice::Auto).clicked()
    {
        net.opponent.ai = AiChoice::Auto;
    }
    if !shows_history(seat, enforced) {
        return;
    }
    if decks.history.rows.is_empty() {
        ui.label(
            egui::RichText::new(EMPTY_SAVED)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
    let game_tag = decks.history.game.clone().unwrap_or_default();
    let rows = decks.history.rows.clone();
    let seated_label = decks
        .seated
        .0
        .as_ref()
        .map(|record| history::label(&record.deck));
    let mut seat_pick = None;
    for row in rows.chunks(columns) {
        ui.horizontal_top(|ui| {
            for saved in row {
                let selected = match seat {
                    DeckSeat::Mine => seated_label.as_deref() == Some(saved.label.as_str()),
                    DeckSeat::Ai => net.opponent.ai == AiChoice::Saved(saved.ci),
                };
                let line = if saved.held { "saved" } else { "bytes missing" };
                if deck_tile(ui, width, &saved.label, line, None, selected) && saved.held {
                    seat_pick = Some(saved.ci);
                }
            }
        });
    }
    if let Some(ci) = seat_pick {
        match seat {
            DeckSeat::Mine => {
                decks.seat_saved.write(history::SeatSavedDeck {
                    game: game_tag.clone(),
                    ci,
                });
            }
            DeckSeat::Ai => {
                net.opponent.ai = AiChoice::Saved(ci);
                net.opponent.ai_battlefield = 0;
            }
        }
    }
    if let Some(note) = &decks.history.note {
        ui.label(
            egui::RichText::new(note)
                .color(theme::tokens(ui.ctx()).ink_weak)
                .small(),
        );
    }
}

pub const EMPTY_SAVED: &str =
    "no saved decks yet — open the deck editor and import one from the clipboard";

fn sideboard_step(
    ui: &mut egui::Ui,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
    thumbs: &Thumbs,
) -> Option<Draft> {
    let riftbound_table = decks
        .mirror
        .view
        .zones
        .iter()
        .any(|decl| decl.name == agni_riftbound::ZONE_NAME_MAIN_DECK);
    let between_games = hud::between_games(&net.info, &table.panel.view);
    let active = net.info.active();
    let free = table.tools.free;
    let DeckParams {
        seated,
        sideboard: panel,
        reloads,
        ..
    } = decks;
    let record = seated.0.as_mut()?;
    let import::ImportedDeck::Riftbound(_) = &record.deck else {
        return None;
    };
    step_title(ui, "sideboard");
    let outcome = sideboard::sideboard_step(
        ui,
        sideboard::SideboardStep {
            panel,
            record,
            thumbs,
            active,
            riftbound_table,
            free,
            between_games,
        },
    );
    if outcome.reload {
        reloads.write(sideboard::ReloadDeckRequested);
    }
    if outcome.edit {
        if let import::ImportedDeck::Riftbound(deck) = &record.deck {
            let label = history::label(&record.deck);
            return Some(Draft::from_deck(deck.clone(), &label, Origin::Seated));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deck_tiles_fit_two_across_a_phone_sheet_and_never_more_than_four() {
        assert_eq!(tile_columns(312.0, 8.0), 2);
        assert_eq!(tile_columns(348.0, 8.0), 2);
        assert_eq!(tile_columns(100.0, 8.0), 1);
        assert_eq!(tile_columns(2000.0, 8.0), 4);
    }

    #[test]
    fn the_box_seats_decks_and_only_my_riftbound_seat_links_to_the_editor() {
        assert!(shows_history(DeckSeat::Mine, true));
        assert!(shows_history(DeckSeat::Mine, false));
        assert!(shows_history(DeckSeat::Ai, true));
        assert!(
            shows_pool(TableGame::Riftbound),
            "the AI may still pick from the pool"
        );
        assert!(!shows_pool(TableGame::Mtg));
        assert!(
            !shows_battlefield_step(),
            "the battlefield is chosen at the table"
        );
        assert!(offers_editor(DeckSeat::Mine, TableGame::Riftbound));
        assert!(!offers_editor(DeckSeat::Ai, TableGame::Riftbound));
        assert!(!offers_editor(DeckSeat::Mine, TableGame::Mtg));
        assert_eq!(title(DeckSeat::Mine), "your deck");
        assert_eq!(title(DeckSeat::Ai), "the AI's deck");
    }
}
