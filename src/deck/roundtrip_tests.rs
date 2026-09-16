use super::editor::{Draft, Origin};
use super::history::{self, kept, snapshot};
use super::import::{self, ImportedDeck, ResolvedImport};
use agni_importers::riftbound::catalog::StaticCatalog;
use agni_importers::riftbound::link;
use agni_importers::riftbound::query::{DeckQuery, Fetch};
use std::collections::BTreeMap;

const TESTDATA: &str = "../../../agni/importers/src/riftbound/testdata";
const RIFTDECKS_PAGE: &str = include_str!(concat!(
    "../../../agni/importers/src/riftbound/testdata/",
    "riftdecks-lillia-285276.html"
));
const LILLIA_TEXT: &str =
    include_str!("../../../agni/importers/src/riftbound/testdata/deck-lillia.txt");
const OTHER_LISTS: [(&str, &str); 3] = [
    (
        "master-yi",
        include_str!("../../../agni/importers/src/riftbound/testdata/deck-master-yi.txt"),
    ),
    (
        "nasus",
        include_str!("../../../agni/importers/src/riftbound/testdata/deck-nasus.txt"),
    ),
    (
        "kha-zix",
        include_str!("../../../agni/importers/src/riftbound/testdata/deck-kha-zix.txt"),
    ),
];
const RIFTDECKS_URL: &str =
    "https://riftdecks.com/riftbound-metagame/deck-lillia-bashful-bloom-285276";
const PILTOVER_PAGE_URL: &str =
    "https://piltoverarchive.com/decks/view/lillia-bashful-bloom-285276";
const PUBLISHED_CODE: &str = "CMAAAAAAAAAQCAAAFIAACAIAABMQAAYGAAAC2LR2HRPWOAIDAASAEBAAIVHAGAQAAAVV2AIDAAVACBAAKMBQEAAANF5QIAYAEA25OAOZAEBAIAF5AHIQCAACAEBQAIACAUACQPIDAIAAA5D3AEBQASQBAQAFGAIEABJA";

struct Pages {
    pages: BTreeMap<String, String>,
    requests: Vec<String>,
}

impl Pages {
    fn none() -> Self {
        Self {
            pages: BTreeMap::new(),
            requests: Vec::new(),
        }
    }

    fn one(url: &str, body: &str) -> Self {
        let mut pages = BTreeMap::new();
        pages.insert(url.to_string(), body.to_string());
        Self {
            pages,
            requests: Vec::new(),
        }
    }
}

impl Fetch for Pages {
    fn get(&mut self, url: &str) -> Result<String, String> {
        self.requests.push(url.to_string());
        self.pages
            .get(url)
            .cloned()
            .ok_or_else(|| format!("no fixture page for {url}"))
    }
}

fn catalog() -> StaticCatalog {
    StaticCatalog::new(super::catalog::fixtures::dump_cards())
}

struct Source {
    name: &'static str,
    query: DeckQuery,
    pages: Pages,
    fetches: usize,
}

fn sources() -> Vec<Source> {
    let piltover_page = format!(
        "<html><body><a class=\"btn\" href=\"/deckbuilder?code={PUBLISHED_CODE}\">Open in builder</a></body></html>"
    );
    vec![
        Source {
            name: "piltover archive deck code",
            query: DeckQuery::Code(PUBLISHED_CODE.into()),
            pages: Pages::none(),
            fetches: 0,
        },
        Source {
            name: "piltover archive deckbuilder link",
            query: DeckQuery::Url(link::piltover_url(PUBLISHED_CODE)),
            pages: Pages::none(),
            fetches: 0,
        },
        Source {
            name: "piltover archive deck page",
            query: DeckQuery::Url(PILTOVER_PAGE_URL.into()),
            pages: Pages::one(PILTOVER_PAGE_URL, &piltover_page),
            fetches: 1,
        },
        Source {
            name: "riftdecks.com deck page",
            query: DeckQuery::Url(RIFTDECKS_URL.into()),
            pages: Pages::one(RIFTDECKS_URL, RIFTDECKS_PAGE),
            fetches: 1,
        },
        Source {
            name: "rift atlas link",
            query: DeckQuery::Url(link::riftatlas_url(PUBLISHED_CODE)),
            pages: Pages::none(),
            fetches: 0,
        },
        Source {
            name: "text list",
            query: DeckQuery::Text(LILLIA_TEXT.into()),
            pages: Pages::none(),
            fetches: 0,
        },
    ]
}

fn resolve(source: &mut Source) -> ResolvedImport {
    let resolved = import::resolve_with(&source.query, &mut source.pages, &mut catalog())
        .unwrap_or_else(|error| panic!("{}: {error}", source.name));
    assert_eq!(
        source.pages.requests.len(),
        source.fetches,
        "{}: fetched {:?}",
        source.name,
        source.pages.requests
    );
    assert!(
        resolved.unresolved.is_empty(),
        "{}: {:?}",
        source.name,
        resolved.unresolved
    );
    resolved
}

fn identity(deck: &ImportedDeck) -> agni_deck::DeckIdentity {
    snapshot(deck).identity()
}

fn names(deck: &ImportedDeck) -> super::pool::NameIdentity {
    super::pool::name_identity(riftbound(deck))
}

fn keyed_by_print(name: &str) -> bool {
    name != "riftdecks.com deck page"
}

fn riftbound(deck: &ImportedDeck) -> &agni_riftbound::ResolvedDeck {
    match deck {
        ImportedDeck::Riftbound(deck) => deck,
        ImportedDeck::Mtg(_) => panic!("a Riftbound deck"),
    }
}

fn sources_baseline() -> ImportedDeck {
    import::resolve_with(
        &DeckQuery::Code(PUBLISHED_CODE.into()),
        &mut Pages::none(),
        &mut catalog(),
    )
    .unwrap()
    .deck
}

fn temp_store(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kai-roundtrip-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_lillia_list_resolves_to_one_deck_from_every_site_and_form() {
    let mut sources = sources();
    let baseline = resolve(&mut sources[0]);
    let base = riftbound(&baseline.deck);
    assert_eq!(base.legend.as_ref().unwrap().name, "Lillia - Bashful Bloom");
    assert_eq!(
        base.chosen_champion.as_ref().unwrap().name,
        "Lillia - Fae Fawn"
    );
    assert_eq!(agni_deck::total(&base.main_deck), 39);
    assert_eq!(agni_deck::total(&base.runes), 12);
    assert_eq!(base.battlefields.len(), 3);
    assert_eq!(agni_deck::total(&base.sideboard), 10);
    for source in sources.iter_mut().skip(1) {
        let resolved = resolve(source);
        assert_eq!(
            names(&resolved.deck),
            names(&baseline.deck),
            "{} names the same deck as the published code",
            source.name
        );
        if keyed_by_print(source.name) {
            assert_eq!(
                identity(&resolved.deck),
                identity(&baseline.deck),
                "{} resolves the same prints as the published code",
                source.name
            );
        } else {
            assert_ne!(
                identity(&resolved.deck),
                identity(&baseline.deck),
                "riftdecks lists its own prints (alternate art, overnumbered), so the print identity differs"
            );
        }
        if source.name == "riftdecks.com deck page" {
            assert_eq!(
                resolved.title.as_deref(),
                Some("Lillia, Bashful Bloom by Jonnynick"),
                "the page title becomes the deck's label"
            );
        } else {
            assert_eq!(resolved.title, None, "{}", source.name);
        }
    }
}

#[test]
fn every_other_real_list_resolves_fully() {
    for (name, text) in OTHER_LISTS {
        let mut source = Source {
            name,
            query: DeckQuery::Text(text.into()),
            pages: Pages::none(),
            fetches: 0,
        };
        let resolved = resolve(&mut source);
        let deck = riftbound(&resolved.deck);
        assert!(deck.legend.is_some(), "{name}");
        assert!(deck.chosen_champion.is_some(), "{name}");
        assert_eq!(agni_deck::total(&deck.runes), 12, "{name}");
        assert_eq!(deck.battlefields.len(), 3, "{name}");
        assert!(
            resolved.code.is_some(),
            "{name}: the reply carries a deck code"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn every_source_saves_to_the_native_store_and_comes_back_whole() {
    use super::history::store;
    let dir = temp_store("native");
    let game = agni_riftbound::GAME;
    let mut sources = sources();
    let mut first = None;
    let mut expected_rows = 0;
    for source in sources.iter_mut() {
        let resolved = resolve(source);
        let label = format!("{} · {}", resolved.label(), source.name);
        let ci = store::remember_as_in(&dir, &resolved.deck, source.name, &label)
            .unwrap_or_else(|error| panic!("{}: {error}", source.name));
        match first {
            None => {
                first = Some(ci);
                expected_rows = 1;
            }
            Some(first) if keyed_by_print(source.name) => {
                assert_eq!(ci, first, "{}: one deck, one identity", source.name)
            }
            Some(first) => {
                assert_ne!(ci, first, "{}: its own prints, its own row", source.name);
                expected_rows = 2;
            }
        }
        let rows = store::rows_in(&dir, game);
        assert_eq!(
            rows.len(),
            expected_rows,
            "{}: re-saving the same list relabels its row: {rows:?}",
            source.name
        );
        let row = rows.iter().find(|row| row.ci == ci).unwrap();
        assert_eq!(row.label, label);
        assert!(row.held);
        let back = store::recall_in(&dir, game, ci)
            .unwrap_or_else(|| panic!("{}: the bytes are held", source.name));
        assert_eq!(identity(&back), identity(&resolved.deck), "{}", source.name);
        assert_eq!(names(&back), names(&sources_baseline()), "{}", source.name);
        assert_eq!(
            riftbound(&back).sideboard.len(),
            riftbound(&resolved.deck).sideboard.len(),
            "{}: the sideboard survives the store",
            source.name
        );
    }
    for (name, text) in OTHER_LISTS {
        let resolved = import::resolve_with(
            &DeckQuery::Text(text.into()),
            &mut Pages::none(),
            &mut catalog(),
        )
        .unwrap();
        store::remember_as_in(&dir, &resolved.deck, "text", name).unwrap();
    }
    let rows = store::rows_in(&dir, game);
    assert_eq!(rows.len(), 5, "{rows:?}");
    let ci = first.unwrap();
    let renamed = store::rename_in(&dir, game, ci, "Lillia by Jonnynick").unwrap();
    assert_eq!(renamed, ci);
    assert!(store::rows_in(&dir, game)
        .iter()
        .any(|row| row.label == "Lillia by Jonnynick"));
    store::forget_in(&dir, game, ci).unwrap();
    assert_eq!(store::rows_in(&dir, game).len(), 4);
    assert!(store::recall_in(&dir, game, ci).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn an_imported_deck_edits_saves_and_reloads_through_the_editor_paths() {
    use super::exchange;
    use super::history::store;
    let dir = temp_store("editor");
    let game = agni_riftbound::GAME;
    let mut source = sources().remove(3);
    let resolved = resolve(&mut source);
    let deck = riftbound(&resolved.deck).clone();
    let mut draft = Draft::from_deck(
        deck.clone(),
        &resolved.label(),
        Origin::Import(format!("link:{RIFTDECKS_URL}")),
    );
    assert_eq!(draft.label, "Lillia, Bashful Bloom by Jonnynick");
    assert!(
        draft.report.meter.champion,
        "the champion rides in its slot"
    );
    draft.dirty = true;
    let ci = exchange::save_in(&dir, &mut draft, game).unwrap();
    assert_eq!(draft.origin, Origin::Saved(ci));
    assert!(!draft.dirty);
    let rows = store::rows_in(&dir, game);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Lillia, Bashful Bloom by Jonnynick");
    let back = store::recall_in(&dir, game, ci).unwrap();
    assert_eq!(
        identity(&back),
        identity(&ImportedDeck::Riftbound(deck.clone()))
    );

    let slot_dir = temp_store("slot");
    draft.rename("Lillia Tempo");
    assert!(draft.dirty);
    crate::os::drafts::store_in(&slot_dir, &draft).unwrap();
    let restored = crate::os::drafts::load_in(&slot_dir).unwrap();
    assert_eq!(restored.label, "Lillia Tempo");
    assert_eq!(restored.origin, Origin::Saved(ci));
    assert_eq!(restored.deck, draft.deck);
    assert!(restored.dirty);
    let _ = std::fs::remove_dir_all(&slot_dir);

    let renamed = exchange::save_in(&dir, &mut draft, game).unwrap();
    assert_eq!(renamed, ci, "a rename keeps the identity");
    assert_eq!(store::rows_in(&dir, game)[0].label, "Lillia Tempo");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_source_saves_to_the_web_store_and_comes_back_whole() {
    let game = agni_riftbound::GAME;
    let mut kv = kept::Memory::default();
    let mut sources = sources();
    let mut first = None;
    let mut expected_rows = 0;
    for source in sources.iter_mut() {
        let resolved = resolve(source);
        let label = format!("{} · {}", resolved.label(), source.name);
        let ci = kept::remember_as(&mut kv, &resolved.deck, source.name, &label)
            .unwrap_or_else(|error| panic!("{}: {error}", source.name));
        match first {
            None => {
                first = Some(ci);
                expected_rows = 1;
            }
            Some(first) if keyed_by_print(source.name) => {
                assert_eq!(ci, first, "{}: one deck, one identity", source.name)
            }
            Some(first) => {
                assert_ne!(ci, first, "{}: its own prints, its own row", source.name);
                expected_rows = 2;
            }
        }
        let rows = kept::rows(&kv, game);
        assert_eq!(
            rows.len(),
            expected_rows,
            "{}: re-saving relabels its row: {rows:?}",
            source.name
        );
        let row = rows.iter().find(|row| row.ci == ci).unwrap();
        assert_eq!(row.label, label);
        let back = kept::recall(&kv, game, ci).unwrap_or_else(|| panic!("{}", source.name));
        assert_eq!(identity(&back), identity(&resolved.deck), "{}", source.name);
        assert_eq!(names(&back), names(&sources_baseline()), "{}", source.name);
        assert_eq!(
            riftbound(&back).sideboard.len(),
            riftbound(&resolved.deck).sideboard.len(),
            "{}: the sideboard survives localStorage",
            source.name
        );
        assert_eq!(
            riftbound(&back).chosen_champion,
            riftbound(&resolved.deck).chosen_champion,
            "{}: the champion slot survives localStorage",
            source.name
        );
    }
    let ci = first.unwrap();
    let text = kv.get_text();
    assert!(
        text.contains(&ci.to_string()),
        "the row is keyed by its identity"
    );
    assert!(
        text.contains("Lillia - Fae Fawn"),
        "the snapshot keeps the names for the box"
    );
    for (name, text) in OTHER_LISTS {
        let resolved = import::resolve_with(
            &DeckQuery::Text(text.into()),
            &mut Pages::none(),
            &mut catalog(),
        )
        .unwrap();
        kept::remember_as(&mut kv, &resolved.deck, "text", name).unwrap();
    }
    assert_eq!(kept::rows(&kv, game).len(), 5);
    assert!(kept::rows(&kv, agni_mtg::GAME).is_empty());
    let renamed = kept::rename(&mut kv, game, ci, "Lillia by Jonnynick").unwrap();
    assert_eq!(renamed, ci);
    assert_eq!(
        kept::rows(&kv, game).len(),
        5,
        "a rename is one row, not two"
    );
    let mut changed = kept::recall(&kv, game, ci).unwrap();
    if let ImportedDeck::Riftbound(deck) = &mut changed {
        deck.sideboard.clear();
    }
    let fresh =
        kept::replace(&mut kv, game, ci, &changed, "editor", "Lillia by Jonnynick").unwrap();
    assert_ne!(fresh, ci);
    assert_eq!(
        kept::rows(&kv, game).len(),
        5,
        "replace forgets the old row"
    );
    assert!(kept::recall(&kv, game, ci).is_none());
    kept::forget(&mut kv, game, fresh).unwrap();
    assert_eq!(kept::rows(&kv, game).len(), 4);
    assert_eq!(
        kept::remember_as(&mut kv, &changed, "editor", "  ").unwrap_err(),
        "a saved deck needs a name"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_web_and_native_stores_agree_on_a_decks_identity() {
    let dir = temp_store("agree");
    let mut sources = sources();
    let resolved = resolve(&mut sources[0]);
    let snapshot = snapshot(&resolved.deck);
    let web = kept::identity_of(&snapshot).unwrap();
    let native = agni_importers::deck::history::identity_of(&snapshot).unwrap();
    assert_eq!(web, native);
    let stored = history::store::remember_as_in(&dir, &resolved.deck, "code", "Lillia").unwrap();
    assert_eq!(
        stored, native,
        "the store files the deck under the same hash"
    );
    let mut kv = kept::Memory::default();
    assert_eq!(
        kept::remember_as(&mut kv, &resolved.deck, "code", "Lillia").unwrap(),
        native
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_testdata_path_is_where_the_fixtures_come_from() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/deck");
    let testdata = here.join(TESTDATA);
    assert!(
        testdata.join("riftdecks-lillia-285276.html").is_file(),
        "{}",
        testdata.display()
    );
}
