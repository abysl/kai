use super::import::ImportedDeck;
use agni_deck::{DeckEntry, Snapshot, SnapshotCard, SnapshotZone};
use agni_importers::riftbound::snapshot as riftbound;

pub const LEGEND: &str = riftbound::LEGEND;
pub const CHAMPION: &str = riftbound::CHAMPION;
pub const COMMANDER: &str = "commander";
pub const MAIN: &str = riftbound::MAIN;
pub const RUNES: &str = riftbound::RUNES;
pub const BATTLEFIELDS: &str = riftbound::BATTLEFIELDS;
pub const SIDEBOARD: &str = riftbound::SIDEBOARD;

fn mtg_card(card: &agni_mtg::ResolvedCard, count: u32) -> SnapshotCard {
    SnapshotCard {
        count,
        image_url: card.image_url.clone(),
        kind: None,
        key: card.name.clone(),
        name: card.name.clone(),
        energy: None,
        power: None,
        might: None,
        domain: Vec::new(),
        tags: Vec::new(),
        signature: false,
    }
}

fn mtg_zone(zone: &str, entries: &[agni_mtg::DeckEntry]) -> SnapshotZone {
    SnapshotZone::new(
        zone,
        entries
            .iter()
            .map(|entry| mtg_card(&entry.card, entry.count))
            .collect(),
    )
}

fn single(zone: &str, card: Option<SnapshotCard>) -> SnapshotZone {
    SnapshotZone::new(zone, card.into_iter().collect())
}

pub fn riftbound_snapshot(deck: &agni_riftbound::ResolvedDeck) -> Snapshot {
    riftbound::snapshot(deck)
}

pub fn snapshot(deck: &ImportedDeck) -> Snapshot {
    match deck {
        ImportedDeck::Riftbound(deck) => riftbound_snapshot(deck),
        ImportedDeck::Mtg(deck) => Snapshot::new(
            agni_mtg::GAME,
            vec![
                single(
                    COMMANDER,
                    deck.commander.as_ref().map(|card| mtg_card(card, 1)),
                ),
                mtg_zone(MAIN, &deck.main_deck),
                mtg_zone(SIDEBOARD, &deck.sideboard),
            ],
        ),
    }
}

fn mtg_entries(snapshot: &Snapshot, zone: &str) -> Vec<agni_mtg::DeckEntry> {
    snapshot
        .zone(zone)
        .map(|zone| {
            zone.cards
                .iter()
                .map(|card| DeckEntry {
                    card: agni_mtg::ResolvedCard {
                        name: card.name.clone(),
                        image_url: card.image_url.clone(),
                    },
                    count: card.count,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn imported(snapshot: &Snapshot) -> Option<ImportedDeck> {
    if let Some(deck) = riftbound::deck(snapshot) {
        return Some(ImportedDeck::Riftbound(deck));
    }
    if snapshot.game == agni_mtg::GAME {
        return Some(ImportedDeck::Mtg(agni_mtg::ResolvedDeck {
            commander: mtg_entries(snapshot, COMMANDER)
                .into_iter()
                .next()
                .map(|entry| entry.card),
            main_deck: mtg_entries(snapshot, MAIN),
            sideboard: mtg_entries(snapshot, SIDEBOARD),
        }));
    }
    None
}

pub fn label(deck: &ImportedDeck) -> String {
    let headline = match deck {
        ImportedDeck::Riftbound(deck) => crate::deck::pool::label_for(deck).or_else(|| {
            deck.legend
                .as_ref()
                .or(deck.chosen_champion.as_ref())
                .map(|card| card.name.clone())
        }),
        ImportedDeck::Mtg(deck) => deck.commander.as_ref().map(|card| card.name.clone()),
    };
    match headline {
        Some(name) => name,
        None => {
            let cards = snapshot(deck).total();
            format!("{} — {cards} cards", deck.game().label())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavedRow {
    pub ci: spirit_sdk::CiHash,
    pub label: String,
    pub signer: Option<String>,
    pub held: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub mod store {
    use super::{imported, label, snapshot, SavedRow};
    use crate::deck::import::ImportedDeck;
    use agni_importers::deck::history;
    use spirit_sdk::record::Tdr;
    use spirit_sdk::{identity, BlobStore, CiHash, Identity, Trust};

    fn open_in(dir: &std::path::Path) -> Option<(BlobStore, Identity)> {
        let dir = dir.to_path_buf();
        let store = BlobStore::open(&dir).ok()?;
        let identity = identity::load_or_create(&dir).ok()?;
        Some((store, identity))
    }

    fn trust_of(dir: &std::path::Path, identity: &Identity) -> Trust {
        Trust::load(dir).with_own(identity.dgid())
    }

    fn import_td(source: &str) -> Tdr {
        Tdr::new("kai-deck-import", &(source, env!("CARGO_PKG_VERSION")))
            .expect("the import transform encodes")
    }

    pub fn remember(deck: &ImportedDeck, source: &str) -> Result<CiHash, String> {
        remember_as(deck, source, &label(deck))
    }

    pub fn remember_as(deck: &ImportedDeck, source: &str, label: &str) -> Result<CiHash, String> {
        let dir = crate::os::paths::store_dir().ok_or("no spirit store on this device")?;
        remember_as_in(&dir, deck, source, label)
    }

    pub fn remember_as_in(
        dir: &std::path::Path,
        deck: &ImportedDeck,
        source: &str,
        label: &str,
    ) -> Result<CiHash, String> {
        let (store, identity) = open_in(dir).ok_or("no spirit store on this device")?;
        let label = label.trim();
        if label.is_empty() {
            return Err("a saved deck needs a name".into());
        }
        history::save(
            &store,
            &identity,
            &snapshot(deck),
            label,
            &import_td(source),
        )
    }

    pub fn replace(
        game: &str,
        old: CiHash,
        deck: &ImportedDeck,
        source: &str,
        label: &str,
    ) -> Result<CiHash, String> {
        let dir = crate::os::paths::store_dir().ok_or("no spirit store on this device")?;
        replace_in(&dir, game, old, deck, source, label)
    }

    pub fn replace_in(
        dir: &std::path::Path,
        game: &str,
        old: CiHash,
        deck: &ImportedDeck,
        source: &str,
        label: &str,
    ) -> Result<CiHash, String> {
        let fresh = remember_as_in(dir, deck, source, label)?;
        if fresh != old {
            forget_in(dir, game, old)?;
        }
        Ok(fresh)
    }

    pub fn rows(game: &str) -> Vec<SavedRow> {
        match crate::os::paths::store_dir() {
            Some(dir) => rows_in(&dir, game),
            None => Vec::new(),
        }
    }

    pub fn rows_in(dir: &std::path::Path, game: &str) -> Vec<SavedRow> {
        let Some((store, _)) = open_in(dir) else {
            return Vec::new();
        };
        history::list(&store, game)
            .into_iter()
            .map(|saved| SavedRow {
                ci: saved.ci,
                label: saved.label,
                signer: saved.signer.map(|dgid| dgid.short()),
                held: saved.held,
            })
            .collect()
    }

    pub fn recall(game: &str, ci: CiHash) -> Option<ImportedDeck> {
        recall_in(&crate::os::paths::store_dir()?, game, ci)
    }

    pub fn recall_in(dir: &std::path::Path, game: &str, ci: CiHash) -> Option<ImportedDeck> {
        let (store, identity) = open_in(dir)?;
        let trust = trust_of(dir, &identity);
        let mut deck = imported(&history::load(&store, &trust, game, ci)?)?;
        backfill(&mut deck, dir);
        Some(deck)
    }

    pub fn backfill(deck: &mut ImportedDeck, dir: &std::path::Path) -> usize {
        use agni_importers::riftbound::catalog::CardLookup;
        let ImportedDeck::Riftbound(deck) = deck else {
            return 0;
        };
        let needs = |card: &agni_riftbound::ResolvedCard| {
            card.kind.is_none() || card.kind.as_deref() == Some("Other") || card.tags.is_empty()
        };
        let wanted = deck.legend.iter().filter(|card| needs(card)).count()
            + deck
                .chosen_champion
                .iter()
                .filter(|card| needs(card))
                .count()
            + [
                &deck.main_deck,
                &deck.runes,
                &deck.battlefields,
                &deck.sideboard,
            ]
            .iter()
            .flat_map(|zone| zone.iter())
            .filter(|entry| needs(&entry.card))
            .count();
        if wanted == 0 {
            return 0;
        }
        let Ok(Some(mut catalog)) = agni_importers::riftbound::ingest::load_catalog(dir) else {
            return 0;
        };
        let mut filled = 0;
        let mut fill = |card: &mut agni_riftbound::ResolvedCard| {
            if !needs(card) {
                return;
            }
            if let Ok(Some(known)) = catalog.by_name(&card.name) {
                let mut changed = false;
                let kind_unknown = card.kind.is_none() || card.kind.as_deref() == Some("Other");
                if kind_unknown && !known.is_padded() {
                    card.kind = Some(known.kind.as_str().to_string());
                    card.energy = known.energy;
                    card.power = known.power;
                    card.might = known.might;
                    card.domain = known.domain.clone();
                    changed = true;
                }
                if card.tags.is_empty() && !known.tags.is_empty() {
                    card.tags = known.tags.clone();
                    card.signature = known.signature;
                    changed = true;
                }
                let canonical = agni_importers::riftbound::resolve::canonical_name(
                    &card.riftbound_id,
                    &card.name,
                );
                if canonical != card.name {
                    card.name = canonical;
                    changed = true;
                }
                if changed {
                    filled += 1;
                }
            }
        };
        deck.legend.iter_mut().for_each(&mut fill);
        deck.chosen_champion.iter_mut().for_each(&mut fill);
        for zone in [
            &mut deck.main_deck,
            &mut deck.runes,
            &mut deck.battlefields,
            &mut deck.sideboard,
        ] {
            zone.iter_mut().for_each(|entry| fill(&mut entry.card));
        }
        filled
    }

    pub fn forget(game: &str, ci: CiHash) -> Result<(), String> {
        let dir = crate::os::paths::store_dir().ok_or("no spirit store on this device")?;
        forget_in(&dir, game, ci)
    }

    pub fn forget_in(dir: &std::path::Path, game: &str, ci: CiHash) -> Result<(), String> {
        let (store, identity) = open_in(dir).ok_or("no spirit store on this device")?;
        history::forget(&store, &identity, game, ci)
    }

    pub fn rename(game: &str, ci: CiHash, label: &str) -> Result<CiHash, String> {
        let dir = crate::os::paths::store_dir().ok_or("no spirit store on this device")?;
        rename_in(&dir, game, ci, label)
    }

    pub fn rename_in(
        dir: &std::path::Path,
        game: &str,
        ci: CiHash,
        label: &str,
    ) -> Result<CiHash, String> {
        let deck = recall_in(dir, game, ci).ok_or("that deck's bytes are not held here")?;
        remember_as_in(dir, &deck, "rename", label)
    }
}

pub mod kept {
    use super::{imported, label, snapshot, SavedRow};
    use crate::deck::import::ImportedDeck;
    use agni_deck::Snapshot;
    use serde::{Deserialize, Serialize};
    use spirit_sdk::CiHash;
    use std::collections::BTreeMap;

    pub const WEB_KEY: &str = "kai.decks";

    pub trait KeyValue {
        fn get(&self, key: &str) -> Option<String>;
        fn set(&mut self, key: &str, text: &str) -> Result<(), String>;
    }

    #[derive(Default, Debug)]
    pub struct Memory(pub BTreeMap<String, String>);

    impl Memory {
        pub fn get_text(&self) -> String {
            self.0.get(WEB_KEY).cloned().unwrap_or_default()
        }
    }

    impl KeyValue for Memory {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set(&mut self, key: &str, text: &str) -> Result<(), String> {
            self.0.insert(key.to_string(), text.to_string());
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Kept {
        pub ci: String,
        pub game: String,
        pub label: String,
        pub source: String,
        pub snapshot: Snapshot,
    }

    pub fn read(kv: &dyn KeyValue) -> Vec<Kept> {
        kv.get(WEB_KEY)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn write(kv: &mut dyn KeyValue, kept: &[Kept]) -> Result<(), String> {
        let text = serde_json::to_string(kept).map_err(|error| error.to_string())?;
        kv.set(WEB_KEY, &text)
    }

    pub fn identity_of(snapshot: &Snapshot) -> Result<CiHash, String> {
        spirit_sdk::record::Cir::new("deck", &snapshot.identity())
            .and_then(|cir| cir.address())
            .map_err(|error| error.to_string())
    }

    pub fn remember(
        kv: &mut dyn KeyValue,
        deck: &ImportedDeck,
        source: &str,
    ) -> Result<CiHash, String> {
        remember_as(kv, deck, source, &label(deck))
    }

    pub fn remember_as(
        kv: &mut dyn KeyValue,
        deck: &ImportedDeck,
        source: &str,
        label: &str,
    ) -> Result<CiHash, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("a saved deck needs a name".into());
        }
        let snapshot = snapshot(deck);
        let ci = identity_of(&snapshot)?;
        let game = deck.game().id().unwrap_or_default().to_string();
        let mut kept = read(kv);
        kept.retain(|row| !(row.game == game && (row.ci == ci.to_string() || row.label == label)));
        kept.push(Kept {
            ci: ci.to_string(),
            game,
            label: label.to_string(),
            source: source.to_string(),
            snapshot,
        });
        write(kv, &kept)?;
        Ok(ci)
    }

    pub fn replace(
        kv: &mut dyn KeyValue,
        game: &str,
        old: CiHash,
        deck: &ImportedDeck,
        source: &str,
        label: &str,
    ) -> Result<CiHash, String> {
        let fresh = remember_as(kv, deck, source, label)?;
        if fresh != old {
            forget(kv, game, old)?;
        }
        Ok(fresh)
    }

    pub fn rows(kv: &dyn KeyValue, game: &str) -> Vec<SavedRow> {
        read(kv)
            .into_iter()
            .filter(|row| row.game == game)
            .filter_map(|row| {
                Some(SavedRow {
                    ci: CiHash::parse(&row.ci)?,
                    label: row.label,
                    signer: None,
                    held: true,
                })
            })
            .collect()
    }

    pub fn recall(kv: &dyn KeyValue, game: &str, ci: CiHash) -> Option<ImportedDeck> {
        read(kv)
            .into_iter()
            .find(|row| row.game == game && row.ci == ci.to_string())
            .and_then(|row| imported(&row.snapshot))
    }

    pub fn forget(kv: &mut dyn KeyValue, game: &str, ci: CiHash) -> Result<(), String> {
        let mut kept = read(kv);
        kept.retain(|row| !(row.game == game && row.ci == ci.to_string()));
        write(kv, &kept)
    }

    pub fn rename(
        kv: &mut dyn KeyValue,
        game: &str,
        ci: CiHash,
        label: &str,
    ) -> Result<CiHash, String> {
        let deck = recall(kv, game, ci).ok_or("that deck is not saved in this browser")?;
        remember_as(kv, &deck, "rename", label)
    }
}

#[cfg(target_arch = "wasm32")]
pub mod store {
    use super::kept::{self, KeyValue};
    use super::SavedRow;
    use crate::deck::import::ImportedDeck;
    use agni_deck::Snapshot;
    use spirit_sdk::CiHash;

    pub use super::kept::WEB_KEY;

    struct Local;

    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or("no window")?
            .local_storage()
            .map_err(|_| "localStorage unavailable")?
            .ok_or_else(|| "localStorage disabled".to_string())
    }

    impl KeyValue for Local {
        fn get(&self, key: &str) -> Option<String> {
            storage().ok()?.get_item(key).ok().flatten()
        }

        fn set(&mut self, key: &str, text: &str) -> Result<(), String> {
            storage()?
                .set_item(key, text)
                .map_err(|_| "localStorage refused the deck".to_string())
        }
    }

    pub fn identity_of(snapshot: &Snapshot) -> Result<CiHash, String> {
        kept::identity_of(snapshot)
    }

    pub fn remember(deck: &ImportedDeck, source: &str) -> Result<CiHash, String> {
        kept::remember(&mut Local, deck, source)
    }

    pub fn remember_as(deck: &ImportedDeck, source: &str, label: &str) -> Result<CiHash, String> {
        kept::remember_as(&mut Local, deck, source, label)
    }

    pub fn replace(
        game: &str,
        old: CiHash,
        deck: &ImportedDeck,
        source: &str,
        label: &str,
    ) -> Result<CiHash, String> {
        kept::replace(&mut Local, game, old, deck, source, label)
    }

    pub fn rows(game: &str) -> Vec<SavedRow> {
        kept::rows(&Local, game)
    }

    pub fn recall(game: &str, ci: CiHash) -> Option<ImportedDeck> {
        kept::recall(&Local, game, ci)
    }

    pub fn forget(game: &str, ci: CiHash) -> Result<(), String> {
        kept::forget(&mut Local, game, ci)
    }

    pub fn rename(game: &str, ci: CiHash, label: &str) -> Result<CiHash, String> {
        kept::rename(&mut Local, game, ci, label)
    }
}

static NOTE: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

pub fn set_note(note: impl Into<String>) {
    *NOTE.lock() = Some(note.into());
}

pub fn take_note() -> Option<String> {
    NOTE.lock().clone()
}

pub fn remember(deck: &crate::deck::import::ImportedDeck, source: &str) {
    match store::remember(deck, source) {
        Ok(_) => {}
        Err(error) => set_note(format!("deck not saved to history: {error}")),
    }
}

#[derive(bevy::prelude::Resource, Default)]
pub struct DeckHistory {
    pub game: Option<String>,
    pub rows: Vec<SavedRow>,
    pub library: Vec<(String, SavedRow)>,
    pub note: Option<String>,
    since_refresh: f32,
}

pub const LIBRARY_GAMES: [&str; 2] = [agni_riftbound::GAME, agni_mtg::GAME];

pub fn library_rows() -> Vec<(String, SavedRow)> {
    LIBRARY_GAMES
        .iter()
        .flat_map(|game| {
            store::rows(game)
                .into_iter()
                .map(move |row| (game.to_string(), row))
        })
        .collect()
}

pub const REFRESH_SECS: f32 = 2.0;

pub fn refresh_history(
    time: bevy::prelude::Res<bevy::prelude::Time>,
    info: bevy::prelude::Res<crate::table::SessionInfo>,
    mirror: bevy::prelude::Res<crate::table::Mirror>,
    choice: bevy::prelude::Res<crate::net::TableChoice>,
    mut panel: bevy::prelude::ResMut<DeckHistory>,
) {
    use bevy::prelude::*;
    panel.since_refresh += time.delta_secs();
    if panel.since_refresh < REFRESH_SECS {
        return;
    }
    panel.since_refresh = 0.0;
    let game = if info.active() {
        crate::net::game_of_zones(&mirror.view.zones)
    } else {
        choice.game
    };
    let tag = game.id().map(String::from);
    let rows = match &tag {
        Some(tag) => store::rows(tag),
        None => Vec::new(),
    };
    if panel.game != tag {
        panel.game = tag;
    }
    if panel.rows != rows {
        panel.rows = rows;
    }
    let library = library_rows();
    if panel.library != library {
        panel.library = library;
    }
    let note = take_note();
    if panel.note != note {
        panel.note = note;
    }
}

#[derive(bevy::prelude::Message, Debug, Clone)]
pub struct SeatSavedDeck {
    pub game: String,
    pub ci: spirit_sdk::CiHash,
}

#[derive(bevy::prelude::Message, Debug, Clone)]
pub struct ForgetSavedDeck {
    pub game: String,
    pub ci: spirit_sdk::CiHash,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riftbound_card_of(name: &str, id: &str) -> agni_riftbound::ResolvedCard {
        agni_riftbound::ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            image_url: Some(format!("https://art.example/{id}.png")),
            ..Default::default()
        }
    }

    fn riftbound_deck() -> ImportedDeck {
        ImportedDeck::Riftbound(agni_riftbound::ResolvedDeck {
            legend: Some(riftbound_card_of("Vanguard Sentinel", "ogn-201-298")),
            chosen_champion: Some(riftbound_card_of("Emberwing Scout", "ogn-007-298")),
            main_deck: vec![DeckEntry {
                card: riftbound_card_of("Ember Rune", "ogn-042-298"),
                count: 12,
            }],
            runes: Vec::new(),
            battlefields: Vec::new(),
            sideboard: vec![DeckEntry {
                card: riftbound_card_of("Spare Blade", "ogn-099-298"),
                count: 2,
            }],
        })
    }

    fn mtg_deck() -> ImportedDeck {
        ImportedDeck::Mtg(agni_mtg::ResolvedDeck {
            commander: Some(agni_mtg::ResolvedCard {
                name: "Serelith, Tidebound Oracle".into(),
                image_url: None,
            }),
            main_deck: vec![DeckEntry {
                card: agni_mtg::ResolvedCard {
                    name: "Thornspire Adept".into(),
                    image_url: Some("https://art.example/adept.jpg".into()),
                },
                count: 4,
            }],
            sideboard: Vec::new(),
        })
    }

    #[test]
    fn a_riftbound_deck_survives_the_round_trip() {
        let deck = riftbound_deck();
        let restored = imported(&snapshot(&deck)).unwrap();
        assert_eq!(restored, deck);
    }

    #[test]
    fn an_mtg_deck_survives_the_round_trip() {
        let deck = mtg_deck();
        let restored = imported(&snapshot(&deck)).unwrap();
        assert_eq!(restored, deck);
    }

    #[test]
    fn the_two_games_never_share_an_identity() {
        assert_ne!(
            snapshot(&riftbound_deck()).identity(),
            snapshot(&mtg_deck()).identity()
        );
    }

    #[test]
    fn art_that_lands_later_does_not_change_the_identity() {
        let bare = mtg_deck();
        let mut arted = mtg_deck();
        let ImportedDeck::Mtg(deck) = &mut arted else {
            panic!("an mtg deck");
        };
        deck.commander.as_mut().unwrap().image_url = Some("https://art.example/oracle.jpg".into());
        assert_eq!(snapshot(&bare).identity(), snapshot(&arted).identity());
        assert_ne!(snapshot(&bare), snapshot(&arted));
    }

    #[test]
    fn a_deck_is_labelled_by_its_headline_card() {
        assert_eq!(label(&riftbound_deck()), "Vanguard Sentinel");
        assert_eq!(label(&mtg_deck()), "Serelith, Tidebound Oracle");
    }

    #[test]
    fn a_pool_list_is_labelled_by_the_pool_not_the_legend() {
        let house = ImportedDeck::Riftbound(crate::deck::pool::deck("lillia-house").unwrap());
        let other = ImportedDeck::Riftbound(crate::deck::pool::deck("lillia-jonnynick").unwrap());
        assert_eq!(label(&house), "Lillia (house)");
        assert_eq!(label(&other), "Lillia (Jonnynick)");
        assert_ne!(snapshot(&house).identity(), snapshot(&other).identity());
    }

    #[test]
    fn a_headless_deck_falls_back_to_a_count() {
        let deck = ImportedDeck::Mtg(agni_mtg::ResolvedDeck {
            commander: None,
            main_deck: vec![DeckEntry {
                card: agni_mtg::ResolvedCard {
                    name: "Shock".into(),
                    image_url: None,
                },
                count: 4,
            }],
            sideboard: Vec::new(),
        });
        assert_eq!(label(&deck), "MTG — 4 cards");
    }

    #[test]
    fn an_unknown_game_does_not_restore() {
        let mut snapshot = snapshot(&mtg_deck());
        snapshot.game = "hearthstone".into();
        assert!(imported(&snapshot).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn saving_over_a_saved_deck_replaces_its_row_and_a_copy_keeps_both() {
        let dir = std::env::temp_dir().join(format!("kai-deck-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let game = agni_riftbound::GAME;
        let first = riftbound_deck();
        let ci = store::remember_as_in(&dir, &first, "editor", "Lillia Tempo").unwrap();
        assert_eq!(
            store::remember_as_in(&dir, &first, "editor", "  ").unwrap_err(),
            "a saved deck needs a name"
        );
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Lillia Tempo");
        let renamed = store::rename_in(&dir, game, ci, "Lillia Aggro").unwrap();
        assert_eq!(renamed, ci, "a rename keeps the identity");
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 1, "a rename is one row, not two");
        assert_eq!(rows[0].label, "Lillia Aggro");
        let mut changed = first.clone();
        if let ImportedDeck::Riftbound(deck) = &mut changed {
            deck.main_deck[0].count = 11;
        }
        let fresh = store::replace_in(&dir, game, ci, &changed, "editor", "Lillia Aggro").unwrap();
        assert_ne!(fresh, ci);
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 1, "the old row is forgotten");
        assert_eq!(rows[0].ci, fresh);
        let same =
            store::replace_in(&dir, game, fresh, &changed, "editor", "Lillia Aggro").unwrap();
        assert_eq!(same, fresh, "replacing with the same list is a no-op save");
        assert_eq!(store::rows_in(&dir, game).len(), 1);
        let copy = store::remember_as_in(&dir, &first, "editor", "Lillia Aggro (copy)").unwrap();
        assert_ne!(copy, fresh);
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 2, "save as copy keeps both");
        let restored = store::recall_in(&dir, game, fresh).unwrap();
        assert_eq!(restored, changed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
