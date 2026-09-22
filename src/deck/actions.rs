use super::catalog::{Catalog, Filter};
use super::editor::{Draft, Edit, Origin, Zone};
use super::import::ImportedDeck;
use serde_json::{json, Value};

pub fn catalog() -> Catalog {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(Some(cards)) = super::catalog::platform::store_catalog() {
        if super::catalog::outranks_pool(&cards) {
            return cards;
        }
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(cards) = crate::net::gateway::riftbound_catalog() {
        return Catalog::from_cards(cards, super::catalog::Source::Gateway(0));
    }
    super::catalog::pool_catalog()
}

pub fn zone(name: &str) -> Result<Zone, String> {
    match name {
        "legend" => Ok(Zone::Legend),
        "champion" => Ok(Zone::Champion),
        "main" => Ok(Zone::Main),
        "runes" => Ok(Zone::Runes),
        "battlefields" => Ok(Zone::Battlefields),
        "sideboard" => Ok(Zone::Sideboard),
        _ => Err("Choose legend, champion, main, runes, battlefields or sideboard".into()),
    }
}

pub fn search_cards(cards: &Catalog, query: &str, offset: usize) -> Value {
    let results: Vec<_> = cards
        .filter(&Filter::default(), query, &[])
        .into_iter()
        .skip(offset.min(10000))
        .take(30)
        .map(|index| {
            let group = &cards.groups[index];
            let card = cards.card(group.default_print);
            json!({"id": card.riftbound_id, "name": card.name, "kind": card.kind.as_str(),
                "domain": card.domain, "energy": card.energy, "text": card.text})
        })
        .collect();
    json!({"catalog": cards.note(), "cards": results, "next_offset": offset + results.len()})
}

pub fn inspect(draft: &Draft) -> Value {
    let deck = ImportedDeck::Riftbound(draft.deck.clone());
    let report = deck
        .report()
        .expect("Riftbound decks have legality reports");
    json!({"label": draft.label, "summary": deck.summary(),
        "list": super::exchange::render(&draft.deck, super::exchange::Share::TextList).unwrap_or_default(),
        "validation": super::import::findings_text(&report), "unresolved": draft.unresolved})
}

pub fn edit(draft: &mut Option<Draft>, cards: &Catalog, args: &Value) -> Result<Value, String> {
    let action = args["action"].as_str().ok_or("Missing deck action")?;
    let text = |key: &str| args[key].as_str().unwrap_or_default();
    if action == "new" {
        *draft = Some(Draft::new(
            if text("label").is_empty() {
                "new deck"
            } else {
                text("label")
            },
            Origin::New,
        ));
    }
    let draft = draft
        .as_mut()
        .ok_or("Create, load or import a deck first")?;
    let card = || {
        let name = text("card");
        cards
            .cards
            .iter()
            .position(|c| c.riftbound_id == name)
            .or_else(|| cards.find_name(name).map(|g| cards.groups[g].default_print))
            .map(|i| cards.resolved(i))
            .ok_or_else(|| format!("Card not in this catalog: {name}. Search cards first."))
    };
    let edit = match action {
        "new" | "inspect" | "validate" => None,
        "rename" => {
            draft.rename(text("label"));
            None
        }
        "undo" => {
            draft.undo();
            None
        }
        "clear" => Some(Edit::Clear),
        "set_legend" => Some(Edit::SetLegend(card()?)),
        "clear_legend" => Some(Edit::ClearLegend),
        "set_champion" => Some(Edit::SetChampion(card()?)),
        "clear_champion" => Some(Edit::ClearChampion),
        "add" => Some(if text("zone").is_empty() {
            Edit::Add(card()?)
        } else {
            Edit::AddTo {
                zone: zone(text("zone"))?,
                card: card()?,
            }
        }),
        "set_count" => Some(Edit::SetCount {
            zone: zone(text("zone"))?,
            id: text("id").into(),
            count: args["count"]
                .as_u64()
                .filter(|n| *n <= 100)
                .ok_or("Count must be 0–100")? as u32,
        }),
        "to_sideboard" => Some(Edit::ToSideboard {
            id: text("id").into(),
        }),
        "to_main" => Some(Edit::ToMain {
            id: text("id").into(),
        }),
        "change_print" => Some(Edit::ChangePrint {
            zone: zone(text("zone"))?,
            id: text("id").into(),
            card: card()?,
        }),
        "fill_runes" => {
            Some(Edit::FillRunes(draft.rune_fill(|domain| {
                cards.basic_rune(domain).map(|i| cards.resolved(i))
            })?))
        }
        "save" => {
            let id = super::history::store::remember_as(
                &ImportedDeck::Riftbound(draft.deck.clone()),
                "editor",
                &draft.label,
            )?;
            return Ok(json!({"saved": id.to_string(), "label": draft.label}));
        }
        "export" => {
            let format = match text("format") {
                "code" => super::exchange::Share::DeckCode,
                "piltover" => super::exchange::Share::PiltoverLink,
                "text" | "" => super::exchange::Share::TextList,
                _ => return Err("Export format: text, code or piltover".into()),
            };
            return Ok(json!({"export": super::exchange::render(&draft.deck, format)?}));
        }
        _ => return Err("Unknown deck editor action".into()),
    };
    if let Some(edit) = edit {
        draft.apply(edit)?;
    }
    Ok(inspect(draft))
}

pub fn selection_allowed(view: &agni_sim::wire::PluginView) -> Result<(), String> {
    if view.turn.as_ref().is_some_and(|turn| turn.number > 0) && view.winner.is_none() {
        Err("A game is in progress. You may edit a draft, but start a new game before changing the seated deck.".into())
    } else {
        Ok(())
    }
}

pub fn selection_legal(verdict: agni_riftbound::legality::Verdict, enforced: bool) -> bool {
    !enforced || !matches!(verdict, agni_riftbound::legality::Verdict::Broken(_))
}

pub fn validate_deal(deck: &ImportedDeck, enforced: bool) -> Result<(), String> {
    let Some(report) = deck.report() else {
        return Ok(());
    };
    if selection_legal(report.verdict, enforced) {
        Ok(())
    } else {
        Err(super::import::findings_text(&report))
    }
}

pub fn validate_selection(draft: &Draft, enforced: bool, confirmed: bool) -> Result<(), String> {
    let deck = ImportedDeck::Riftbound(draft.deck.clone());
    validate_deal(&deck, enforced)?;
    let report = deck
        .report()
        .expect("Riftbound decks have legality reports");
    if matches!(report.verdict, agni_riftbound::legality::Verdict::Broken(_)) && !confirmed {
        return Err(
            "This free-table deck is invalid. Confirm explicitly to seat it anyway.".into(),
        );
    }
    Ok(())
}

pub fn load(
    draft: &mut Option<Draft>,
    deck: ImportedDeck,
    label: &str,
    unresolved: Vec<String>,
) -> Result<Value, String> {
    let ImportedDeck::Riftbound(deck) = deck else {
        return Err("The deck editor currently supports Riftbound".into());
    };
    let mut loaded = Draft::from_deck(deck, label, Origin::Import(label.into()));
    loaded.unresolved = unresolved;
    let result = inspect(&loaded);
    *draft = Some(loaded);
    Ok(result)
}

pub fn library() -> Value {
    let saved: Vec<_> = super::history::store::rows(agni_riftbound::GAME)
        .into_iter()
        .map(|row| row.label)
        .collect();
    let presets: Vec<_> = super::pool::pinnable()
        .into_iter()
        .map(|row| row.slug)
        .collect();
    json!({"saved": saved, "presets": presets})
}

pub fn saved(source: &str) -> Option<ImportedDeck> {
    let row = super::history::store::rows(agni_riftbound::GAME)
        .into_iter()
        .find(|row| row.label.eq_ignore_ascii_case(source));
    row.and_then(|row| super::history::store::recall(agni_riftbound::GAME, row.ci))
        .or_else(|| super::pool::deck(source).ok().map(ImportedDeck::Riftbound))
}

pub fn remote(args: &Value) -> Option<super::service::Request> {
    match args["action"].as_str()? {
        "search" => Some(super::service::Request::Search {
            site: args["site"].as_str().unwrap_or_default().into(),
            query: args["query"].as_str().unwrap_or_default().into(),
            page: args["page"].as_u64().unwrap_or(1).try_into().unwrap_or(0),
        }),
        "import" => Some(super::service::Request::Import {
            source: args["source"].as_str().unwrap_or_default().into(),
        }),
        _ => None,
    }
}

pub fn imported(draft: &mut Option<Draft>, reply: super::service::Reply) -> Result<Value, String> {
    match reply {
        super::service::Reply::Search(value) => Ok(value),
        super::service::Reply::Import(resolved) => {
            let label = resolved.label();
            load(
                draft,
                resolved.deck,
                &label,
                resolved
                    .unresolved
                    .into_iter()
                    .map(|(id, why)| format!("{id}: {why}"))
                    .collect(),
            )
        }
    }
}

pub fn local(draft: &mut Option<Draft>, args: &Value) -> Result<Value, String> {
    match args["action"].as_str().unwrap_or_default() {
        "list" => Ok(library()),
        "load" => {
            let source = args["source"]
                .as_str()
                .ok_or("Choose a saved deck name or preset slug")?;
            load(
                draft,
                saved(source).ok_or("No saved deck matches; use import_deck for URLs or lists")?,
                source,
                Vec::new(),
            )
        }
        "cards" => Ok(search_cards(
            &catalog(),
            args["query"].as_str().unwrap_or_default(),
            args["offset"].as_u64().unwrap_or(0) as usize,
        )),
        _ => edit(draft, &catalog(), args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_use_the_editor_validation_and_undo_stack() {
        let cards = super::super::catalog::fixtures::dump_catalog();
        let mut draft = None;
        edit(
            &mut draft,
            &cards,
            &json!({"action":"new", "label":"example"}),
        )
        .unwrap();
        assert!(edit(
            &mut draft,
            &cards,
            &json!({"action":"add", "card":"not a card"})
        )
        .is_err());
        edit(
            &mut draft,
            &cards,
            &json!({"action":"set_legend", "card":"Lillia, Bashful Bloom"}),
        )
        .unwrap();
        assert!(draft.as_ref().unwrap().deck.legend.is_some());
        edit(&mut draft, &cards, &json!({"action":"undo"})).unwrap();
        assert!(draft.as_ref().unwrap().deck.legend.is_none());
        assert!(!search_cards(&cards, "Lillia", 0)["cards"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    fn entry(name: &str, id: &str) -> agni_riftbound::DeckEntry {
        agni_riftbound::DeckEntry {
            card: agni_riftbound::ResolvedCard {
                name: name.into(),
                riftbound_id: id.into(),
                ..Default::default()
            },
            count: 1,
        }
    }

    #[test]
    fn selection_and_deals_recheck_banned_sideboards() {
        let mut deck = crate::deck::pool::deck("lillia-house").unwrap();
        deck.sideboard = vec![entry("Stacked Deck", "ogn-183-298")];
        let imported = ImportedDeck::Riftbound(deck.clone());
        let report = imported.report().unwrap();
        assert!(report.findings.iter().any(|finding| {
            matches!(
                &finding.rule,
                agni_riftbound::legality::Rule::Banned { name } if name == "Stacked Deck"
            ) && finding.zone == agni_riftbound::legality::Zone::Sideboard
        }));
        assert!(validate_deal(&imported, true).is_err());
        assert!(validate_deal(&imported, false).is_ok());

        let mut draft = Draft::from_deck(
            crate::deck::pool::deck("lillia-house").unwrap(),
            "stale",
            Origin::New,
        );
        draft.deck = deck;
        assert!(validate_selection(&draft, true, false).is_err());
        assert!(validate_selection(&draft, false, false).is_err());
        assert!(validate_selection(&draft, false, true).is_ok());
    }

    #[test]
    fn other_titles_stay_available_to_enforced_deals() {
        let mut deck = crate::deck::pool::deck("lillia-house").unwrap();
        deck.main_deck[0].card.name = "Ekko - Another Title".into();
        let imported = ImportedDeck::Riftbound(deck);
        assert!(imported.report().unwrap().findings.iter().all(|finding| {
            !matches!(&finding.rule, agni_riftbound::legality::Rule::Banned { .. })
        }));
        assert!(validate_deal(&imported, true).is_ok());
    }
}
