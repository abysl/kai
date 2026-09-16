use crate::deck::import::{ImportedDeck, SeatedDeck, SeatedDeckRecord};
use crate::deck::rows::{self, RowAction, RowEvent};
use crate::deck::thumbs::Thumbs;
use agni_riftbound::{DeckEntry, ResolvedDeck};
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::BTreeMap;

pub const EDIT_NOTE: &str =
    "swaps here last until the next import and never save — edit the whole deck to build and save";

#[derive(Resource, Default)]
pub struct SideboardPanel {
    pub imported: Option<ResolvedDeck>,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct ReloadDeckRequested;

pub fn card_pool(deck: &ResolvedDeck) -> BTreeMap<String, u32> {
    let mut pool: BTreeMap<String, u32> = BTreeMap::new();
    for entry in deck.main_deck.iter().chain(deck.sideboard.iter()) {
        *pool.entry(entry.card.riftbound_id.clone()).or_default() += entry.count;
    }
    pool
}

pub fn list_size(entries: &[DeckEntry]) -> u32 {
    agni_deck::total(entries)
}

pub fn shift_one(from: &mut Vec<DeckEntry>, to: &mut Vec<DeckEntry>, riftbound_id: &str) -> bool {
    let Some(index) = from
        .iter()
        .position(|entry| entry.card.riftbound_id == riftbound_id)
    else {
        return false;
    };
    let card = from[index].card.clone();
    from[index].count -= 1;
    if from[index].count == 0 {
        from.remove(index);
    }
    match to
        .iter_mut()
        .find(|entry| entry.card.riftbound_id == riftbound_id)
    {
        Some(entry) => entry.count += 1,
        None => to.push(DeckEntry { card, count: 1 }),
    }
    true
}

fn column(
    ui: &mut egui::Ui,
    heading: String,
    entries: &[DeckEntry],
    empty: &str,
    thumbs: &Thumbs,
) -> Option<String> {
    let mut moved = None;
    ui.label(egui::RichText::new(heading).strong());
    if entries.is_empty() {
        ui.label(egui::RichText::new(empty).weak());
    }
    for entry in entries {
        let art = thumbs.id(&entry.card.riftbound_id);
        if let Some(RowEvent::Swap | RowEvent::Open) =
            rows::card_row(ui, &entry.card, entry.count, art, None, RowAction::Swap)
        {
            moved = Some(entry.card.riftbound_id.clone());
        }
    }
    moved
}

pub fn reload_allowed(active: bool, riftbound: bool, free: bool, between_games: bool) -> bool {
    active && riftbound && (free || between_games)
}

pub fn reload_note(can_reload: bool, mid_game: bool) -> &'static str {
    if can_reload {
        "use this list next game — it clears your cards from the table and re-deals"
    } else if mid_game {
        "this list is used next game, once this one ends"
    } else {
        "the list deals itself when you sit at a riftbound table"
    }
}

pub fn sideboard_ui(mut panel: ResMut<SideboardPanel>, seated: Res<SeatedDeck>) {
    let deck = seated.0.as_ref().and_then(|record| match &record.deck {
        ImportedDeck::Riftbound(deck) => Some(deck),
        ImportedDeck::Mtg(_) => None,
    });
    let Some(deck) = deck else {
        if panel.imported.is_some() {
            panel.imported = None;
        }
        return;
    };
    let stale = panel
        .imported
        .as_ref()
        .map(|imported| card_pool(imported) != card_pool(deck))
        .unwrap_or(true);
    if stale {
        panel.imported = Some(deck.clone());
    }
}

pub struct SideboardStep<'a> {
    pub panel: &'a mut SideboardPanel,
    pub record: &'a mut SeatedDeckRecord,
    pub thumbs: &'a Thumbs,
    pub active: bool,
    pub riftbound_table: bool,
    pub free: bool,
    pub between_games: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SideboardOutcome {
    pub reload: bool,
    pub edit: bool,
}

pub fn sideboard_step(ui: &mut egui::Ui, step: SideboardStep<'_>) -> SideboardOutcome {
    let SeatedDeckRecord { deck, .. } = step.record;
    let ImportedDeck::Riftbound(deck) = deck else {
        ui.label(egui::RichText::new("this deck has no sideboard").weak());
        return SideboardOutcome::default();
    };
    if step
        .panel
        .imported
        .as_ref()
        .map(|imported| card_pool(imported) != card_pool(deck))
        .unwrap_or(true)
    {
        step.panel.imported = Some(deck.clone());
    }
    let can_reload = reload_allowed(
        step.active,
        step.riftbound_table,
        step.free,
        step.between_games,
    );
    let mid_game = step.active && step.riftbound_table && !can_reload;
    let main_size = list_size(&deck.main_deck);
    let bench_size = list_size(&deck.sideboard);
    let edited = step
        .panel
        .imported
        .as_ref()
        .is_some_and(|imported| imported.sideboard != deck.sideboard);
    let to_bench = column(
        ui,
        format!("main deck · {main_size}"),
        &deck.main_deck,
        "empty",
        step.thumbs,
    );
    ui.add_space(8.0);
    let to_main = column(
        ui,
        format!("sideboard · {bench_size}"),
        &deck.sideboard,
        "nothing benched",
        step.thumbs,
    );
    ui.add_space(8.0);
    let mut reload = false;
    let mut restore = false;
    let mut edit = false;
    ui.horizontal_wrapped(|ui| {
        reload = ui
            .add_enabled(can_reload, egui::Button::new("use this list next game"))
            .clicked();
        restore = ui
            .add_enabled(edited, egui::Button::new("reset to imported list"))
            .clicked();
        edit = crate::menu::link(ui, "edit the whole deck").clicked();
    });
    ui.label(egui::RichText::new(reload_note(can_reload, mid_game)).weak());
    ui.label(egui::RichText::new(EDIT_NOTE).weak().small());
    if let Some(id) = to_bench {
        shift_one(&mut deck.main_deck, &mut deck.sideboard, &id);
    }
    if let Some(id) = to_main {
        shift_one(&mut deck.sideboard, &mut deck.main_deck, &id);
    }
    if restore {
        if let Some(imported) = &step.panel.imported {
            deck.main_deck = imported.main_deck.clone();
            deck.sideboard = imported.sideboard.clone();
        }
    }
    SideboardOutcome { reload, edit }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_riftbound::ResolvedCard;

    fn card(name: &str, id: &str) -> ResolvedCard {
        ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            ..Default::default()
        }
    }

    fn deck() -> ResolvedDeck {
        ResolvedDeck {
            legend: Some(card("Legend", "l-1")),
            chosen_champion: None,
            main_deck: vec![
                DeckEntry {
                    card: card("Ambush", "a-1"),
                    count: 3,
                },
                DeckEntry {
                    card: card("Ledge Hopper", "h-1"),
                    count: 1,
                },
            ],
            runes: Vec::new(),
            battlefields: Vec::new(),
            sideboard: vec![DeckEntry {
                card: card("Spare Blade", "s-1"),
                count: 2,
            }],
        }
    }

    #[test]
    fn a_swap_moves_exactly_one_copy_each_way() {
        let mut deck = deck();
        assert!(shift_one(&mut deck.main_deck, &mut deck.sideboard, "a-1"));
        assert_eq!(deck.main_deck[0].count, 2);
        assert_eq!(deck.sideboard.len(), 2);
        assert_eq!(deck.sideboard[1].card.name, "Ambush");
        assert_eq!(deck.sideboard[1].count, 1);
        assert!(shift_one(&mut deck.sideboard, &mut deck.main_deck, "a-1"));
        assert_eq!(deck.main_deck[0].count, 3);
        assert_eq!(deck.sideboard.len(), 1);
    }

    #[test]
    fn the_last_copy_leaves_no_empty_row_behind() {
        let mut deck = deck();
        assert!(shift_one(&mut deck.main_deck, &mut deck.sideboard, "h-1"));
        assert_eq!(deck.main_deck.len(), 1);
        assert!(!shift_one(&mut deck.main_deck, &mut deck.sideboard, "h-1"));
    }

    #[test]
    fn sideboarding_never_changes_the_pool_but_an_import_does() {
        let before = deck();
        let mut after = deck();
        shift_one(&mut after.main_deck, &mut after.sideboard, "a-1");
        assert_eq!(card_pool(&before), card_pool(&after));
        assert_eq!(list_size(&before.main_deck), 4);
        assert_eq!(list_size(&after.main_deck), 3);
        assert_eq!(list_size(&after.sideboard), 3);
        let mut other = deck();
        other.main_deck.push(DeckEntry {
            card: card("Newcomer", "n-1"),
            count: 1,
        });
        assert_ne!(card_pool(&before), card_pool(&other));
    }
}
