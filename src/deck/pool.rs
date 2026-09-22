use agni_importers::riftbound::catalog::{CardKind, CatalogCard, StaticCatalog};
use agni_importers::riftbound::{resolve, text_list};
use agni_riftbound::ResolvedDeck;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

macro_rules! pool_file {
    ($slug:literal) => {
        (
            $slug,
            include_str!(concat!(
                "../../../agni/games/riftbound/rules/pool/",
                $slug,
                ".md"
            )),
        )
    };
}

pub const FILES: &[(&str, &str)] = &[
    pool_file!("lillia-house"),
    pool_file!("irelia-house"),
    pool_file!("lillia-jonnynick"),
    pool_file!("master-yi-akame"),
    pool_file!("nasus-thundertrees"),
    pool_file!("kha-zix-hotkee"),
    pool_file!("origins"),
    pool_file!("spiritforged"),
    pool_file!("unleashed"),
    pool_file!("vendetta"),
];

pub fn is_deck_file(text: &str) -> bool {
    text_list::deck_block(text).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolDeck {
    pub slug: String,
    pub label: String,
    pub legend: String,
    pub scripted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolError {
    NoSuchDeck(String),
    Ambiguous(String, Vec<String>),
    NoDeckBlock(String),
    Parse(String, String),
    Unresolved(String, Vec<String>),
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchDeck(prefix) => write!(f, "the pool has no deck starting with {prefix}"),
            Self::Ambiguous(prefix, slugs) => {
                write!(f, "pool:{prefix} could be any of {}", slugs.join(", "))
            }
            Self::NoDeckBlock(slug) => write!(f, "{slug}.md has no deck block"),
            Self::Parse(slug, error) => write!(f, "{slug}.md: {error}"),
            Self::Unresolved(slug, names) => {
                write!(
                    f,
                    "{slug}.md names cards its pool lacks: {}",
                    names.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for PoolError {}

fn header(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix(prefix))
        .map(|rest| rest.trim().to_string())
}

fn legend_of_block(block: &str) -> Option<String> {
    let mut lines = block.lines().map(str::trim);
    lines.find(|line| line.eq_ignore_ascii_case("legend"))?;
    let line = lines.find(|line| !line.is_empty())?;
    let name = match line.split_once(' ') {
        Some((count, name)) if count.chars().all(|c| c.is_ascii_digit()) => name,
        _ => line,
    };
    Some(name.trim().to_string())
}

pub fn decks() -> Vec<PoolDeck> {
    FILES
        .iter()
        .filter(|(_, text)| is_deck_file(text))
        .map(|(slug, text)| PoolDeck {
            slug: (*slug).to_string(),
            label: header(text, "# ").unwrap_or_else(|| (*slug).to_string()),
            legend: text_list::deck_block(text)
                .and_then(legend_of_block)
                .unwrap_or_default(),
            scripted: header(text, "Scripted:").is_some_and(|value| value == "complete"),
        })
        .collect()
}

pub fn pinnable() -> Vec<PoolDeck> {
    decks().into_iter().filter(|deck| deck.scripted).collect()
}

pub fn of_slug(slug: &str) -> Option<PoolDeck> {
    decks().into_iter().find(|deck| deck.slug == slug)
}

pub fn of_legend(name: &str) -> Option<PoolDeck> {
    let wanted = name.trim().to_ascii_lowercase();
    pinnable()
        .into_iter()
        .find(|deck| deck.legend.to_ascii_lowercase() == wanted)
}

pub fn another_than(legend: &str) -> Option<PoolDeck> {
    let pinnable = pinnable();
    pinnable
        .iter()
        .find(|deck| !deck.legend.eq_ignore_ascii_case(legend))
        .or_else(|| pinnable.get(1))
        .cloned()
}

pub fn find(prefix: &str) -> Result<PoolDeck, PoolError> {
    let wanted = prefix.trim().to_ascii_lowercase();
    let all = decks();
    if let Some(exact) = all.iter().find(|deck| deck.slug == wanted) {
        return Ok(exact.clone());
    }
    let candidates: Vec<&PoolDeck> = all
        .iter()
        .filter(|deck| deck.slug.starts_with(&wanted))
        .collect();
    match candidates.as_slice() {
        [] => Err(PoolError::NoSuchDeck(prefix.trim().to_string())),
        [one] => Ok((*one).clone()),
        many => match many.iter().find(|deck| deck.slug.ends_with("-house")) {
            Some(house) => Ok((*house).clone()),
            None => Err(PoolError::Ambiguous(
                prefix.trim().to_string(),
                many.iter().map(|deck| deck.slug.clone()).collect(),
            )),
        },
    }
}

fn card_line(line: &str) -> Option<CatalogCard> {
    let rest = line.strip_prefix("- **")?;
    let (name, rest) = rest.split_once("** (")?;
    let (head, text) = rest.split_once("):")?;
    let mut parts = head.split(';').map(str::trim);
    let id = parts.next()?.to_string();
    let kinds = parts.next()?;
    let kind = CardKind::parse(kinds.split('/').next()?.trim());
    let champion = kinds
        .split('/')
        .any(|token| token.trim().eq_ignore_ascii_case("champion"));
    let domain: Vec<String> = parts
        .next()?
        .split('/')
        .map(|domain| domain.trim().to_string())
        .filter(|domain| !domain.is_empty())
        .collect();
    let text = text.trim();
    let mut card = CatalogCard {
        name: name.to_string(),
        riftbound_id: id,
        kind,
        champion,
        domain,
        text: (!text.is_empty()).then(|| text.to_string()),
        ..Default::default()
    };
    for token in parts.next().unwrap_or("").split_whitespace() {
        let (digits, suffix) = token.split_at(token.len().saturating_sub(1));
        let value = digits.parse::<u8>().ok();
        match suffix {
            "E" => card.energy = value,
            "P" => card.power = value,
            "M" => card.might = value,
            _ => {}
        }
    }
    Some(card)
}

pub fn cards() -> Vec<CatalogCard> {
    let mut seen = BTreeMap::new();
    for (_, text) in FILES {
        for card in text.lines().filter_map(card_line) {
            seen.entry(card.name.to_ascii_lowercase()).or_insert(card);
        }
    }
    seen.into_values().collect()
}

pub fn scripted_names() -> &'static BTreeSet<String> {
    static NAMES: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        FILES
            .iter()
            .filter(|(_, text)| header(text, "Scripted:").is_some_and(|value| value == "complete"))
            .flat_map(|(_, text)| text.lines().filter_map(card_line))
            .map(|card| card.name.to_ascii_lowercase())
            .collect()
    })
}

pub fn coverage<'n>(
    names: impl IntoIterator<Item = (&'n str, u32)>,
    scripted: &BTreeSet<String>,
) -> (u32, u32) {
    names
        .into_iter()
        .fold((0, 0), |(hit, total), (name, count)| {
            let known = scripted.contains(&name.trim().to_ascii_lowercase());
            (hit + if known { count } else { 0 }, total + count)
        })
}

pub fn deck_coverage(deck: &ResolvedDeck, scripted: &BTreeSet<String>) -> (u32, u32) {
    let singles = deck
        .legend
        .iter()
        .chain(deck.chosen_champion.iter())
        .map(|card| (card.name.as_str(), 1));
    let entries = deck
        .main_deck
        .iter()
        .chain(deck.battlefields.iter())
        .map(|entry| (entry.card.name.as_str(), entry.count));
    coverage(singles.chain(entries), scripted)
}

pub fn coverage_chip((hit, total): (u32, u32)) -> String {
    if total == 0 {
        "no cards".into()
    } else if hit == total {
        "scripted".into()
    } else {
        format!("{hit} of {total} scripted — the rest play as vanilla")
    }
}

pub type NameIdentity = Vec<(&'static str, String, u32)>;

pub fn name_identity(deck: &ResolvedDeck) -> NameIdentity {
    let mut out: NameIdentity = Vec::new();
    for (zone, card) in [
        ("legend", &deck.legend),
        ("champion", &deck.chosen_champion),
    ] {
        if let Some(card) = card {
            out.push((zone, card.name.clone(), 1));
        }
    }
    for (zone, entries) in [
        ("main", &deck.main_deck),
        ("runes", &deck.runes),
        ("battlefields", &deck.battlefields),
        ("sideboard", &deck.sideboard),
    ] {
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for entry in entries {
            *counts.entry(entry.card.name.clone()).or_default() += entry.count;
        }
        out.extend(counts.into_iter().map(|(name, count)| (zone, name, count)));
    }
    out
}

fn identities() -> &'static Vec<(String, agni_deck::DeckIdentity, NameIdentity)> {
    static IDENTITIES: std::sync::OnceLock<Vec<(String, agni_deck::DeckIdentity, NameIdentity)>> =
        std::sync::OnceLock::new();
    IDENTITIES.get_or_init(|| {
        decks()
            .into_iter()
            .filter_map(|held| {
                let built = deck(&held.slug).ok()?;
                let names = name_identity(&built);
                let snapshot = crate::deck::history::snapshot(
                    &crate::deck::import::ImportedDeck::Riftbound(built),
                );
                Some((held.label, snapshot.identity(), names))
            })
            .collect()
    })
}

pub fn label_for(deck: &ResolvedDeck) -> Option<String> {
    let wanted = crate::deck::history::riftbound_snapshot(deck).identity();
    let names = name_identity(deck);
    identities()
        .iter()
        .find(|(_, identity, held)| *identity == wanted || *held == names)
        .map(|(label, _, _)| label.clone())
}

#[cfg(test)]
pub fn markdown() -> String {
    FILES
        .iter()
        .map(|(_, text)| *text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn in_domain_identity(card: &[String], legend: &[String]) -> bool {
    agni_riftbound::legality::fits_identity(legend, card)
}

pub fn deck(prefix: &str) -> Result<ResolvedDeck, PoolError> {
    let found = find(prefix)?;
    let text = FILES
        .iter()
        .find(|(slug, _)| *slug == found.slug)
        .map(|(_, text)| *text)
        .ok_or_else(|| PoolError::NoSuchDeck(found.slug.clone()))?;
    let block =
        text_list::deck_block(text).ok_or_else(|| PoolError::NoDeckBlock(found.slug.clone()))?;
    let parsed = text_list::parse_text(block)
        .map_err(|error| PoolError::Parse(found.slug.clone(), error.to_string()))?;
    let mut catalog = StaticCatalog::new(cards());
    let resolution = resolve::resolve(&parsed, &mut catalog)
        .map_err(|error| PoolError::Parse(found.slug.clone(), error.to_string()))?;
    if !resolution.unresolved.is_empty() {
        return Err(PoolError::Unresolved(
            found.slug,
            resolution
                .unresolved
                .iter()
                .map(|entry| entry.identifier.clone())
                .collect(),
        ));
    }
    Ok(resolution.deck)
}

#[cfg(target_arch = "wasm32")]
pub fn identity(_slug: &str) -> Option<spirit_sdk::CiHash> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn identity(slug: &str) -> Option<spirit_sdk::CiHash> {
    static IDENTITIES: std::sync::OnceLock<BTreeMap<String, spirit_sdk::CiHash>> =
        std::sync::OnceLock::new();
    IDENTITIES
        .get_or_init(|| {
            decks()
                .into_iter()
                .filter_map(|held| {
                    let deck = deck(&held.slug).ok()?;
                    let snapshot = crate::deck::history::snapshot(
                        &crate::deck::import::ImportedDeck::Riftbound(deck),
                    );
                    agni_importers::deck::history::identity_of(&snapshot)
                        .ok()
                        .map(|ci| (held.slug, ci))
                })
                .collect()
        })
        .get(slug)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pool_file_on_disk_is_in_the_table_with_a_unique_label_and_a_legend() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../agni/games/riftbound/rules/pool");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("the pool directory")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".md")
                    .map(str::to_string)
            })
            .collect();
        on_disk.sort();
        let mut listed: Vec<String> = FILES.iter().map(|(slug, _)| (*slug).to_string()).collect();
        listed.sort();
        assert_eq!(on_disk, listed);
        let decks = decks();
        assert_eq!(
            decks.len(),
            FILES.iter().filter(|(_, text)| is_deck_file(text)).count()
        );
        assert_eq!(
            decks.len() + 4,
            FILES.len(),
            "origins, spiritforged, unleashed and vendetta are sets, not decks"
        );
        assert!(decks.iter().all(|deck| {
            deck.slug != "origins"
                && deck.slug != "spiritforged"
                && deck.slug != "unleashed"
                && deck.slug != "vendetta"
        }));
        for set in ["origins", "spiritforged", "unleashed", "vendetta"] {
            assert_eq!(
                find(set),
                Err(PoolError::NoSuchDeck(set.into())),
                "a set file is never a deck to pin"
            );
        }
        let mut labels: Vec<&str> = decks.iter().map(|deck| deck.label.as_str()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), decks.len(), "labels are unique");
        for deck in &decks {
            assert!(!deck.legend.is_empty(), "{}: a legend", deck.slug);
            assert!(
                deck.label.contains('('),
                "{}: the label carries provenance",
                deck.slug
            );
        }
        assert_eq!(
            of_slug("lillia-house").unwrap().legend,
            "Lillia - Bashful Bloom"
        );
        assert_eq!(
            of_slug("lillia-jonnynick").unwrap().label,
            "Lillia (Jonnynick)"
        );
    }

    #[test]
    fn a_slug_prefix_finds_one_deck_and_a_shared_prefix_prefers_the_house_list() {
        assert_eq!(find("nasus").unwrap().slug, "nasus-thundertrees");
        assert_eq!(find("Kha").unwrap().slug, "kha-zix-hotkee");
        assert_eq!(find("master").unwrap().slug, "master-yi-akame");
        assert_eq!(find("lillia").unwrap().slug, "lillia-house");
        assert_eq!(find("lillia-jonny").unwrap().slug, "lillia-jonnynick");
        assert_eq!(find("zed"), Err(PoolError::NoSuchDeck("zed".into())));
    }

    #[test]
    fn every_scripted_deck_resolves_to_a_legal_list_with_a_champion_and_its_own_domains() {
        for held in pinnable() {
            let deck = deck(&held.slug).unwrap_or_else(|error| panic!("{}: {error}", held.slug));
            let total = |entries: &[agni_riftbound::DeckEntry]| -> usize {
                entries.iter().map(|entry| entry.count as usize).sum()
            };
            assert_eq!(
                total(&deck.main_deck) + usize::from(deck.chosen_champion.is_some()),
                agni_riftbound::MAIN_DECK_SIZE,
                "{}: the chosen champion is one of the forty",
                held.slug
            );
            assert_eq!(
                total(&deck.runes),
                agni_riftbound::RUNE_DECK_SIZE,
                "{}",
                held.slug
            );
            assert_eq!(
                total(&deck.battlefields),
                agni_riftbound::BATTLEFIELD_COUNT,
                "{}",
                held.slug
            );
            let legend = deck.legend.as_ref().expect("a legend");
            assert_eq!(legend.name, held.legend);
            assert!(deck.chosen_champion.is_some(), "{}: a champion", held.slug);
            for entry in deck.main_deck.iter().chain(&deck.runes) {
                assert!(
                    in_domain_identity(&entry.card.domain, &legend.domain),
                    "{}: {} is outside the domain identity",
                    held.slug,
                    entry.card.name
                );
                assert!(entry.card.kind.is_some());
            }
            let report =
                agni_riftbound::legality::check(&deck, agni_riftbound::legality::Mode::Standard);
            if held.slug == "kha-zix-hotkee" {
                assert_eq!(
                    deck.main_deck
                        .iter()
                        .find(|entry| entry.card.name == "Stacked Deck")
                        .map(|entry| entry.count),
                    Some(3)
                );
                assert_eq!(report.breaks(), 1, "{:?}", report.findings);
                assert_eq!(report.verdict, agni_riftbound::legality::Verdict::Broken(1));
                assert!(report.findings.iter().any(|finding| {
                    matches!(
                        &finding.rule,
                        agni_riftbound::legality::Rule::Banned { name } if name == "Stacked Deck"
                    )
                }));
                continue;
            }
            assert_eq!(report.breaks(), 0, "{}: {:?}", held.slug, report.findings);
            assert_eq!(
                report.verdict,
                agni_riftbound::legality::Verdict::Unverified,
                "{}: pool markdown carries no tags",
                held.slug
            );
            assert!(identity(&held.slug).is_some());
        }
        assert_ne!(identity("lillia-house"), identity("lillia-jonnynick"));
    }

    #[test]
    fn another_than_walks_to_a_different_legend_and_mirrors_when_there_is_none() {
        let first = pinnable().remove(0);
        assert_ne!(another_than(&first.legend).unwrap().legend, first.legend);
        assert_eq!(another_than("Jinx - Rebel").unwrap().slug, first.slug);
        assert_eq!(
            of_legend("Irelia - Blade Dancer").unwrap().slug,
            "irelia-house"
        );
        assert_eq!(
            of_legend("Lillia - Bashful Bloom").unwrap().slug,
            "lillia-house"
        );
        assert!(of_legend("Jinx - Rebel").is_none());
    }

    #[test]
    fn coverage_counts_the_scripted_faces_of_a_deck() {
        let scripted = scripted_names();
        assert!(scripted.contains("defy"));
        assert!(!scripted.contains("jinx - rebel"));
        assert_eq!(
            coverage([("Defy", 3), ("Jinx - Rebel", 1), ("  defy ", 1)], scripted),
            (4, 5)
        );
        assert_eq!(
            coverage_chip((4, 5)),
            "4 of 5 scripted — the rest play as vanilla"
        );
        assert_eq!(coverage_chip((5, 5)), "scripted");
        assert_eq!(coverage_chip((0, 0)), "no cards");
        for held in pinnable() {
            let deck = deck(&held.slug).unwrap();
            let (hit, total) = deck_coverage(&deck, scripted);
            assert_eq!(
                hit, total,
                "{}: every face of a pool deck is scripted",
                held.slug
            );
            assert!(
                total >= 44,
                "{}: legend, champion, forty and battlefields",
                held.slug
            );
        }
        let mut off_pool = deck("lillia-house").unwrap();
        off_pool.main_deck[0].card.name = "Jinx - Rebel".into();
        let (hit, total) = deck_coverage(&off_pool, scripted);
        assert_eq!(total - hit, off_pool.main_deck[0].count);
    }

    #[test]
    fn a_pool_deck_is_labelled_by_its_h1_and_the_two_lillias_do_not_collide() {
        let house = deck("lillia-house").unwrap();
        let jonnynick = deck("lillia-jonnynick").unwrap();
        assert_eq!(label_for(&house).as_deref(), Some("Lillia (house)"));
        assert_eq!(label_for(&jonnynick).as_deref(), Some("Lillia (Jonnynick)"));
        let mut edited = house.clone();
        edited.main_deck[0].count += 1;
        assert_eq!(
            label_for(&edited),
            None,
            "a changed list is no longer the pool deck"
        );
        let mut arted = house;
        arted.legend.as_mut().unwrap().image_url = Some("https://art.example/x.png".into());
        assert_eq!(
            label_for(&arted).as_deref(),
            Some("Lillia (house)"),
            "art does not change the identity"
        );
        let mut reprinted = deck("lillia-house").unwrap();
        for entry in &mut reprinted.main_deck {
            entry.card.riftbound_id = format!("{}a", entry.card.riftbound_id);
        }
        assert_eq!(
            label_for(&reprinted).as_deref(),
            Some("Lillia (house)"),
            "the same list resolved to other printings keeps the pool label"
        );
        assert_ne!(
            name_identity(&reprinted),
            name_identity(&jonnynick),
            "the two Lillias differ by name and count, not only by printing"
        );
    }

    #[test]
    fn the_card_union_reads_every_line_once_and_the_markdown_carries_every_file() {
        let cards = cards();
        assert!(cards.len() >= 350, "{}", cards.len());
        let origins = cards
            .iter()
            .filter(|card| card.riftbound_id.starts_with("ogn-"))
            .count();
        assert_eq!(origins, 298, "the Origins set file lists every base print");
        let spiritforged = cards
            .iter()
            .filter(|card| card.riftbound_id.starts_with("sfd-"))
            .count();
        assert_eq!(
            spiritforged, 222,
            "the Spiritforged set file lists every base print and the Gold token"
        );
        let unleashed = cards
            .iter()
            .filter(|card| card.riftbound_id.starts_with("unl-"))
            .count();
        assert_eq!(
            unleashed, 219,
            "the Unleashed set file lists every base print"
        );
        let vendetta = cards
            .iter()
            .filter(|card| card.riftbound_id.starts_with("ven-"))
            .count();
        assert_eq!(
            vendetta, 166,
            "the Vendetta set file lists every base print, runes and the sp reprints aside"
        );
        let cleave = cards
            .iter()
            .find(|card| card.name == "Cleave")
            .expect("Cleave");
        assert_eq!(cleave.kind, CardKind::Spell);
        assert_eq!((cleave.energy, cleave.power), (Some(1), None));
        assert_eq!(cleave.domain, ["Fury"]);
        let defy = cards.iter().find(|card| card.name == "Defy").expect("Defy");
        assert_eq!(defy.kind, CardKind::Spell);
        assert_eq!(defy.energy, Some(1));
        let champion = cards
            .iter()
            .find(|card| card.name == "Nasus, Ascended")
            .expect("the champion");
        assert!(champion.champion);
        assert_eq!(champion.kind, CardKind::Unit);
        let poro = cards
            .iter()
            .find(|card| card.name == "Lonely Poro")
            .expect("the poro");
        assert!(
            poro.text
                .as_deref()
                .is_some_and(|text| text.starts_with("[Deathknell]")),
            "the rules text after the head rides along: {:?}",
            poro.text
        );
        let markdown = markdown();
        for (_, text) in FILES {
            assert!(markdown.contains(text));
        }
    }
}
