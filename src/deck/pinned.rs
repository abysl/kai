use crate::deck::history::{self, DeckHistory, SavedRow};
use crate::deck::import::{self, ImportPanel, SeatedDeck};
use crate::deck::pool::{self, PoolDeck};
use crate::render::art::ArtCache;
use crate::table::{GameTable, MySeat, SessionInfo, SessionRole};
use agni_core::PlayerId;
use bevy::prelude::*;

pub fn default_for(role: SessionRole) -> Option<String> {
    let pinnable = pool::pinnable();
    let pick = match role {
        SessionRole::Client => pinnable.get(1).or(pinnable.first()),
        _ => pinnable.first(),
    };
    pick.map(|deck| deck.slug.clone())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableLegends {
    pub mine: Option<String>,
    pub theirs: Vec<String>,
}

pub fn dealt_here(role: SessionRole, legend_on_table: bool) -> bool {
    matches!(role, SessionRole::Host | SessionRole::Client) && legend_on_table
}

pub fn legends_on_table(table: &agni_core::Table, me: PlayerId) -> TableLegends {
    let mut legends = TableLegends::default();
    let zone = agni_core::Zone::Plugin(agni_riftbound::ZONE_LEGEND);
    for card in table.cards().iter().filter(|card| card.zone == zone) {
        if card.seat == me {
            legends.mine = Some(card.face.name.clone());
        } else {
            legends.theirs.push(card.face.name.clone());
        }
    }
    legends
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Saved(String),
    Pool(String),
    Missing,
}

impl Source {
    pub fn note(&self, deck: &PoolDeck) -> String {
        match self {
            Source::Saved(label) if *label == deck.label => {
                format!("{} · your saved copy", deck.label)
            }
            Source::Saved(label) => format!("{} · your saved copy {label}", deck.label),
            Source::Pool(_) => format!("{} · the pool list", deck.label),
            Source::Missing => format!("{} · not here — import one", deck.label),
        }
    }
}

pub fn saved_row<'r>(rows: &'r [SavedRow], deck: &PoolDeck) -> Option<&'r SavedRow> {
    let identity = pool::identity(&deck.slug);
    let held = || rows.iter().filter(|row| row.held);
    held()
        .find(|row| identity.is_some_and(|ci| row.ci == ci))
        .or_else(|| held().find(|row| row.label.trim() == deck.label))
}

pub fn resolve(deck: &PoolDeck, rows: &[SavedRow]) -> (Option<import::ImportedDeck>, Source) {
    if let Some(row) = saved_row(rows, deck) {
        if let Some(saved) = history::store::recall(agni_riftbound::GAME, row.ci) {
            return (Some(saved), Source::Saved(row.label.clone()));
        }
    }
    match pool::deck(&deck.slug) {
        Ok(built) => (
            Some(import::ImportedDeck::Riftbound(built)),
            Source::Pool(deck.label.clone()),
        ),
        Err(_) => (None, Source::Missing),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seating {
    pub side: String,
    pub ci: Option<spirit_sdk::CiHash>,
    pub source: Source,
}

#[derive(Resource, Debug, Default)]
pub struct PinnedDeck {
    pub side: Option<String>,
    pub seating: Option<Seating>,
    role: SessionRole,
}

pub fn side_after(
    role: SessionRole,
    was: SessionRole,
    side: Option<String>,
    legends: &TableLegends,
) -> Option<String> {
    if role != SessionRole::Client || was == SessionRole::Client {
        return side;
    }
    if let Some(mine) = legends.mine.as_deref() {
        return pool::of_legend(mine).map(|deck| deck.slug);
    }
    let Some(theirs) = legends.theirs.first() else {
        return side;
    };
    let taken = pool::of_legend(theirs).map(|deck| deck.slug);
    match side {
        Some(chosen) if taken.as_deref() != Some(chosen.as_str()) => Some(chosen),
        _ => pool::another_than(theirs).map(|deck| deck.slug),
    }
}

pub fn seating_holds(
    seating: Option<&Seating>,
    slug: &str,
    saved: Option<spirit_sdk::CiHash>,
    seated_label: Option<&str>,
) -> bool {
    let Some(seating) = seating else {
        return false;
    };
    if seating.side != slug || seating.ci != saved {
        return false;
    }
    let pool_label = pool::of_slug(slug).map(|deck| deck.label);
    match &seating.source {
        Source::Missing => true,
        Source::Saved(label) => seated_label
            .is_some_and(|seated| seated == label || Some(seated) == pool_label.as_deref()),
        Source::Pool(label) => seated_label.is_some_and(|seated| seated == label),
    }
}

pub fn seating_holds_record(seating: Option<&Seating>, record: &import::SeatedDeckRecord) -> bool {
    let Some(seating) = seating else {
        return false;
    };
    let label = history::label(&record.deck);
    let pool_label = pool::of_slug(&seating.side).map(|deck| deck.label);
    match &seating.source {
        Source::Missing => false,
        Source::Saved(saved) => label == *saved || Some(label.as_str()) == pool_label.as_deref(),
        Source::Pool(pool) => label == *pool,
    }
}

pub fn pin_decks(
    info: Res<SessionInfo>,
    choice: Res<crate::net::TableChoice>,
    history: Res<DeckHistory>,
    my_seat: Res<MySeat>,
    table: Res<GameTable>,
    mut pinned: ResMut<PinnedDeck>,
    mut seated: ResMut<SeatedDeck>,
    mut cache: ResMut<ArtCache>,
    mut panel: ResMut<ImportPanel>,
) {
    let was = pinned.role;
    if was != info.role {
        let legends = legends_on_table(&table.0, my_seat.0);
        pinned.side = side_after(info.role, was, pinned.side.take(), &legends);
        pinned.role = info.role;
    }
    let chosen_by_hand = seated
        .bypass_change_detection()
        .0
        .as_ref()
        .is_some_and(|record| !seating_holds_record(pinned.seating.as_ref(), record));
    if !crate::net::rules_enforced(&info, &choice) || chosen_by_hand {
        if pinned.side.is_some() || pinned.seating.is_some() {
            pinned.side = None;
            pinned.seating = None;
        }
        return;
    }
    let slug = match pinned.side.clone() {
        Some(slug) => slug,
        None => {
            let Some(slug) = default_for(info.role) else {
                return;
            };
            pinned.side = Some(slug.clone());
            slug
        }
    };
    let Some(pool_deck) = pool::of_slug(&slug) else {
        pinned.side = None;
        return;
    };
    let saved = saved_row(&history.rows, &pool_deck).map(|row| row.ci);
    let seated_label = seated
        .bypass_change_detection()
        .0
        .as_ref()
        .map(|record| history::label(&record.deck));
    if seating_holds(
        pinned.seating.as_ref(),
        &slug,
        saved,
        seated_label.as_deref(),
    ) {
        return;
    }
    let (deck, source) = resolve(&pool_deck, &history.rows);
    if let Some(deck) = deck {
        let dealt = dealt_here(
            info.role,
            legends_on_table(&table.0, my_seat.0).mine.is_some(),
        );
        let faces = import::seat_deck(deck, my_seat.0, &mut seated, &mut cache, None);
        panel.auto_deal = !dealt;
        panel.error = None;
        panel.note = Some(if dealt {
            format!(
                "{} · {faces} faces staged · already on the table",
                source.note(&pool_deck)
            )
        } else {
            format!("{} · {faces} faces staged", source.note(&pool_deck))
        });
    }
    pinned.seating = Some(Seating {
        side: slug,
        ci: saved,
        source,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ci(byte: u8) -> spirit_sdk::CiHash {
        spirit_sdk::CiHash::from_hash(spirit_sdk::BlobHash::from_bytes([byte; 32]))
    }

    fn row(label: &str, held: bool, byte: u8) -> SavedRow {
        SavedRow {
            ci: ci(byte),
            label: label.into(),
            signer: None,
            held,
        }
    }

    fn legends(mine: Option<&str>, theirs: &[&str]) -> TableLegends {
        TableLegends {
            mine: mine.map(str::to_string),
            theirs: theirs.iter().map(|name| name.to_string()).collect(),
        }
    }

    fn slug(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    #[test]
    fn the_host_defaults_to_the_first_pool_deck_and_a_joiner_to_the_second() {
        assert_eq!(default_for(SessionRole::Solo), slug("lillia-house"));
        assert_eq!(default_for(SessionRole::Starting), slug("lillia-house"));
        assert_eq!(default_for(SessionRole::Host), slug("lillia-house"));
        assert_eq!(default_for(SessionRole::Client), slug("irelia-house"));
        let empty = legends(None, &[]);
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Joining,
                slug("lillia-house"),
                &empty
            ),
            slug("lillia-house"),
            "a joiner keeps the deck it pinned when the table has no deck dealt"
        );
        assert_eq!(
            side_after(SessionRole::Client, SessionRole::Joining, None, &empty),
            None,
            "nothing pinned and nothing dealt leaves the default to the pool"
        );
        assert_eq!(
            side_after(
                SessionRole::Host,
                SessionRole::Starting,
                slug("irelia-house"),
                &empty
            ),
            slug("irelia-house"),
            "a host keeps the deck it chose before hosting"
        );
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Client,
                slug("nasus-thundertrees"),
                &empty
            ),
            slug("nasus-thundertrees")
        );
    }

    #[test]
    fn a_joiner_takes_a_deck_the_host_has_not_dealt() {
        let host_on_irelia = legends(None, &["Irelia - Blade Dancer"]);
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Joining,
                slug("irelia-house"),
                &host_on_irelia
            ),
            slug("lillia-house")
        );
        let host_on_lillia = legends(None, &["Lillia - Bashful Bloom"]);
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Joining,
                None,
                &host_on_lillia
            ),
            slug("irelia-house")
        );
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Joining,
                slug("nasus-thundertrees"),
                &host_on_irelia
            ),
            slug("nasus-thundertrees"),
            "a pinned deck the host is not on stays pinned"
        );
        let host_off_pool = legends(None, &["Jinx - Rebel"]);
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Joining,
                slug("lillia-house"),
                &host_off_pool
            ),
            slug("lillia-house"),
            "a legend outside the pool leaves the pinned deck alone"
        );
        assert_eq!(
            side_after(
                SessionRole::Client,
                SessionRole::Joining,
                None,
                &host_off_pool
            ),
            slug("lillia-house"),
            "a legend outside the pool leaves the first pool deck"
        );
    }

    #[test]
    fn a_reconnecting_joiner_keeps_the_deck_already_dealt_for_it() {
        let dealt = legends(Some("Lillia - Bashful Bloom"), &["Irelia - Blade Dancer"]);
        for chosen in [slug("lillia-house"), slug("irelia-house"), None] {
            assert_eq!(
                side_after(
                    SessionRole::Client,
                    SessionRole::Joining,
                    chosen.clone(),
                    &dealt
                ),
                slug("lillia-house"),
                "{chosen:?}"
            );
        }
        let mut table = agni_core::Table::default();
        let legend = agni_core::Zone::Plugin(agni_riftbound::ZONE_LEGEND);
        table.add(PlayerId(0), legend, "Irelia - Blade Dancer", [0; 3]);
        table.add(
            PlayerId(1),
            agni_core::Zone::Plugin(agni_riftbound::ZONE_MAIN_DECK),
            "Defy",
            [0; 3],
        );
        assert_eq!(
            legends_on_table(&table, PlayerId(1)),
            legends(None, &["Irelia - Blade Dancer"])
        );
        table.add(PlayerId(1), legend, "Lillia - Bashful Bloom", [0; 3]);
        assert_eq!(
            legends_on_table(&table, PlayerId(1)),
            legends(Some("Lillia - Bashful Bloom"), &["Irelia - Blade Dancer"])
        );
        assert_eq!(
            legends_on_table(&table, PlayerId(0)),
            legends(Some("Irelia - Blade Dancer"), &["Lillia - Bashful Bloom"])
        );
        assert!(dealt_here(SessionRole::Host, true));
        assert!(dealt_here(SessionRole::Client, true));
        assert!(
            !dealt_here(SessionRole::Solo, true),
            "a legend left in the mirror by the last table is not dealt on the next one"
        );
        assert!(!dealt_here(SessionRole::Ended, true));
        assert!(!dealt_here(SessionRole::Host, false));
    }

    #[test]
    fn a_saved_deck_is_preferred_by_identity_then_by_exact_label_and_only_when_held() {
        let lillia = pool::of_slug("lillia-house").unwrap();
        let own_copy = pool::identity("lillia-house").unwrap();
        let rows = vec![
            row("Jinx - Rebel", true, 1),
            row("Lillia (house)", false, 2),
            row("Lillia - Bashful Bloom", true, 4),
            SavedRow {
                ci: own_copy,
                label: "my lillia".into(),
                signer: None,
                held: true,
            },
        ];
        assert_eq!(
            saved_row(&rows, &lillia).map(|row| row.label.as_str()),
            Some("my lillia"),
            "the same forty-plus cards under any label is the user's copy"
        );
        assert!(
            saved_row(&rows[..3], &lillia).is_none(),
            "a deck that merely shares the legend is not the pool deck, and an unheld row is skipped"
        );
        let labelled = vec![row("  Lillia (house) ", true, 5)];
        assert_eq!(saved_row(&labelled, &lillia).map(|row| row.ci), Some(ci(5)));
    }

    #[test]
    fn the_seating_holds_until_the_deck_the_saved_row_or_the_seated_deck_changes() {
        let seating = Seating {
            side: "lillia-house".into(),
            ci: None,
            source: Source::Pool("Lillia (house)".into()),
        };
        assert!(seating_holds(
            Some(&seating),
            "lillia-house",
            None,
            Some("Lillia (house)")
        ));
        assert!(!seating_holds(None, "lillia-house", None, Some("Lillia")));
        assert!(!seating_holds(
            Some(&seating),
            "irelia-house",
            None,
            Some("Lillia (house)")
        ));
        assert!(
            !seating_holds(
                Some(&seating),
                "lillia-house",
                Some(ci(9)),
                Some("Lillia (house)")
            ),
            "a saved deck appearing in history replaces the pool deck"
        );
        assert!(
            !seating_holds(
                Some(&seating),
                "lillia-house",
                None,
                Some("Lillia (Jonnynick)")
            ),
            "the other Lillia deck is not this one"
        );
        assert!(
            !seating_holds(Some(&seating), "lillia-house", None, Some("Jinx - Rebel")),
            "a deck seated from elsewhere is re-pinned"
        );
        assert!(!seating_holds(Some(&seating), "lillia-house", None, None));
        let saved = Seating {
            side: "lillia-house".into(),
            ci: Some(ci(9)),
            source: Source::Saved("my lillia".into()),
        };
        assert!(seating_holds(
            Some(&saved),
            "lillia-house",
            Some(ci(9)),
            Some("my lillia")
        ));
        assert!(
            seating_holds(
                Some(&saved),
                "lillia-house",
                Some(ci(9)),
                Some("Lillia (house)")
            ),
            "a saved copy of the pool list is labelled by the pool"
        );
        let missing = Seating {
            side: "irelia-house".into(),
            ci: None,
            source: Source::Missing,
        };
        assert!(
            seating_holds(Some(&missing), "irelia-house", None, None),
            "nothing to seat is not retried every frame"
        );
    }

    #[test]
    fn the_source_note_says_which_deck_is_in_use() {
        let lillia = pool::of_slug("lillia-house").unwrap();
        let irelia = pool::of_slug("irelia-house").unwrap();
        assert_eq!(
            Source::Saved("my lillia".into()).note(&lillia),
            "Lillia (house) · your saved copy my lillia"
        );
        assert_eq!(
            Source::Saved("Lillia (house)".into()).note(&lillia),
            "Lillia (house) · your saved copy"
        );
        assert_eq!(
            Source::Pool("Irelia (house)".into()).note(&irelia),
            "Irelia (house) · the pool list"
        );
        assert!(Source::Missing.note(&irelia).contains("import one"));
    }

    #[test]
    fn with_no_saved_deck_the_pool_builds_every_pinnable_deck() {
        let pinnable = pool::pinnable();
        assert!(pinnable.len() >= 4, "{pinnable:?}");
        for held in pinnable {
            let (deck, source) = resolve(&held, &[]);
            let deck = deck.expect("the pool has the deck");
            assert_eq!(source, Source::Pool(held.label.clone()));
            assert_eq!(history::label(&deck), held.label);
            let import::ImportedDeck::Riftbound(deck) = &deck else {
                panic!("{}: a Riftbound deck", held.slug);
            };
            let total = |entries: &[agni_riftbound::DeckEntry]| -> usize {
                entries.iter().map(|entry| entry.count as usize).sum()
            };
            assert_eq!(total(&deck.main_deck) + 1, agni_riftbound::MAIN_DECK_SIZE);
            assert_eq!(total(&deck.runes), agni_riftbound::RUNE_DECK_SIZE);
            assert_eq!(total(&deck.battlefields), agni_riftbound::BATTLEFIELD_COUNT);
            let legend = deck.legend.as_ref().expect("a legend");
            assert!(deck.chosen_champion.is_some(), "{}: a champion", held.slug);
            for entry in deck.main_deck.iter().chain(&deck.runes) {
                assert!(
                    pool::in_domain_identity(&entry.card.domain, &legend.domain),
                    "{}: {} is outside the domain identity",
                    held.slug,
                    entry.card.name
                );
            }
        }
    }
}
