use crate::deck::pool;
use crate::theme::{COLORLESS, DOMAINS};
use agni_importers::riftbound::card_code::{CardCode, Variant};
use agni_importers::riftbound::catalog::{normalize_name, CardKind, CatalogCard};
use agni_importers::riftbound::resolve::canonical_name;
use agni_riftbound::{legality, ResolvedCard};
use bevy::prelude::*;
use parking_lot::Mutex;
use std::collections::{BTreeSet, HashMap};

pub const KIND_ORDER: [CardKind; 7] = [
    CardKind::Legend,
    CardKind::Unit,
    CardKind::Spell,
    CardKind::Gear,
    CardKind::Rune,
    CardKind::Battlefield,
    CardKind::Other,
];

pub const DECK_KINDS: [CardKind; 3] = [CardKind::Unit, CardKind::Spell, CardKind::Gear];

pub const STORE_POLL_SECS: f32 = 2.0;

pub const NOTE_NONE: &str = "no catalog loaded";
pub const NOTE_POOL: &str =
    "offline catalog: Origins and the pool decks — download the full set in Settings › advanced";
pub const NOTE_POOL_WEB: &str =
    "offline catalog: Origins and the pool decks — your gateway holds no riftbound set yet";
pub const TAGS_NOTE_STORE: &str = "re-download to check champion tags";
pub const TAGS_NOTE_GATEWAY: &str =
    "your gateway's riftbound set predates champion tags — its owner re-runs the ingest";

static TAGS_NOTE: Mutex<Option<&'static str>> = Mutex::new(None);

pub fn tags_note() -> Option<&'static str> {
    *TAGS_NOTE.lock()
}

fn publish_tags_note(catalog: &Catalog) {
    let note = match catalog.source {
        Source::Store(_) if !catalog.tags_known => Some(TAGS_NOTE_STORE),
        Source::Gateway(_) if !catalog.tags_known => Some(TAGS_NOTE_GATEWAY),
        _ => None,
    };
    *TAGS_NOTE.lock() = note;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    None,
    Pool(usize),
    Store(usize),
    Gateway(usize),
}

impl Source {
    pub fn prints(self) -> usize {
        match self {
            Self::None => 0,
            Self::Pool(n) | Self::Store(n) | Self::Gateway(n) => n,
        }
    }

    pub fn is_fallback(self) -> bool {
        matches!(self, Self::None | Self::Pool(_))
    }

    pub fn with_count(self, count: usize) -> Self {
        match self {
            Self::None => Self::None,
            Self::Pool(_) => Self::Pool(count),
            Self::Store(_) => Self::Store(count),
            Self::Gateway(_) => Self::Gateway(count),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnergyBand {
    Low,
    Mid,
    High,
    Top,
}

impl EnergyBand {
    pub const ALL: [Self; 4] = [Self::Low, Self::Mid, Self::High, Self::Top];

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "0-1",
            Self::Mid => "2-3",
            Self::High => "4-5",
            Self::Top => "6+",
        }
    }

    pub fn holds(self, energy: u8) -> bool {
        match self {
            Self::Low => energy <= 1,
            Self::Mid => (2..=3).contains(&energy),
            Self::High => (4..=5).contains(&energy),
            Self::Top => energy >= 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filter {
    pub kinds: BTreeSet<CardKind>,
    pub domains: BTreeSet<String>,
    pub fits_identity: bool,
    pub energy: Option<EnergyBand>,
    pub champion_only: bool,
    pub signature_only: bool,
    pub sets: BTreeSet<String>,
}

impl Filter {
    pub fn only_kind(kind: CardKind) -> Self {
        Self {
            kinds: BTreeSet::from([kind]),
            ..Default::default()
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn toggle_kind(&mut self, kind: CardKind) {
        if !self.kinds.remove(&kind) {
            self.kinds.insert(kind);
        }
    }

    pub fn toggle_domain(&mut self, domain: &str) {
        if !self.domains.remove(domain) {
            self.domains.insert(domain.to_string());
        }
    }

    pub fn toggle_set(&mut self, set: &str) {
        if !self.sets.remove(set) {
            self.sets.insert(set.to_string());
        }
    }

    pub fn toggle_energy(&mut self, band: EnergyBand) {
        self.energy = if self.energy == Some(band) {
            None
        } else {
            Some(band)
        };
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub prints: Vec<usize>,
    pub default_print: usize,
    pub kind: CardKind,
    pub domain: Vec<String>,
    pub champion: bool,
    pub signature: bool,
    pub tags: Vec<String>,
    pub energy: Option<u8>,
    pub might: Option<u8>,
    pub power: Option<u8>,
    pub text_lower: String,
    pub name_lower: String,
    pub hidden: bool,
}

impl Group {
    pub fn in_deck_kind(&self) -> bool {
        DECK_KINDS.contains(&self.kind)
    }
}

#[derive(Resource, Default)]
pub struct Catalog {
    pub cards: Vec<CatalogCard>,
    pub groups: Vec<Group>,
    pub source: Source,
    pub tags_known: bool,
    pub generation: u32,
    by_id: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
    sets: Vec<String>,
    domains: Vec<&'static str>,
}

pub fn kind_rank(kind: CardKind) -> usize {
    KIND_ORDER
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(KIND_ORDER.len())
}

pub fn is_padded(card: &CatalogCard) -> bool {
    card.is_padded()
}

pub fn is_hidden(card: &CatalogCard) -> bool {
    is_padded(card) || CardCode::from_riftbound_id(&card.riftbound_id).is_err()
}

fn print_rank(card: &CatalogCard, canonical: &str) -> (bool, bool, bool, bool, String) {
    let code = CardCode::from_riftbound_id(&card.riftbound_id).ok();
    let variant = code.is_some_and(|code| code.variant != Variant::Base);
    let reprint = code.is_none_or(|code| code.set.reprints_another_set());
    (
        is_padded(card),
        reprint,
        variant,
        card.name != canonical,
        card.riftbound_id.to_ascii_lowercase(),
    )
}

fn energy_order(left: Option<u8>, right: Option<u8>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

impl Catalog {
    pub fn from_cards(cards: Vec<CatalogCard>, source: Source) -> Self {
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut canonical_names: Vec<String> = Vec::with_capacity(cards.len());
        for (index, card) in cards.iter().enumerate() {
            let canonical = canonical_name(&card.riftbound_id, &card.name);
            let key = normalize_name(&canonical);
            match by_key.get_mut(&key) {
                Some(prints) => prints.push(index),
                None => {
                    by_key.insert(key.clone(), vec![index]);
                    order.push(key);
                }
            }
            canonical_names.push(canonical);
        }
        let mut groups: Vec<Group> = order
            .iter()
            .map(|key| {
                let mut prints = by_key.remove(key).unwrap_or_default();
                let canonical = canonical_names[prints[0]].clone();
                prints.sort_by_cached_key(|print| print_rank(&cards[*print], &canonical));
                let default_print = prints[0];
                let lead = &cards[default_print];
                let text = prints
                    .iter()
                    .filter_map(|print| cards[*print].text.as_deref())
                    .next()
                    .unwrap_or_default();
                let tags = prints
                    .iter()
                    .map(|print| &cards[*print].tags)
                    .max_by_key(|tags| tags.len())
                    .cloned()
                    .unwrap_or_default();
                Group {
                    name_lower: canonical.to_lowercase(),
                    kind: lead.kind,
                    domain: lead.domain.clone(),
                    champion: lead.kind == CardKind::Unit
                        && prints.iter().any(|print| cards[*print].champion),
                    signature: lead.signature,
                    tags,
                    energy: lead.energy,
                    might: lead.might,
                    power: lead.power,
                    text_lower: text.to_lowercase(),
                    hidden: prints.iter().all(|print| is_hidden(&cards[*print])),
                    name: canonical,
                    prints,
                    default_print,
                }
            })
            .collect();
        groups.sort_by(|left, right| {
            kind_rank(left.kind)
                .cmp(&kind_rank(right.kind))
                .then_with(|| energy_order(left.energy, right.energy))
                .then_with(|| left.name_lower.cmp(&right.name_lower))
        });
        let mut by_id = HashMap::new();
        let mut by_name = HashMap::new();
        for (group_index, group) in groups.iter().enumerate() {
            by_name.insert(normalize_name(&group.name), group_index);
            for print in &group.prints {
                by_id.insert(cards[*print].riftbound_id.to_ascii_lowercase(), group_index);
            }
        }
        let tags_known = cards.iter().any(|card| !card.tags.is_empty());
        let sets = cards
            .iter()
            .filter_map(|card| card.set_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let domains = DOMAINS
            .iter()
            .copied()
            .filter(|name| {
                groups
                    .iter()
                    .any(|group| group.domain.iter().any(|d| d.eq_ignore_ascii_case(name)))
            })
            .collect();
        Self {
            source: source.with_count(cards.len()),
            tags_known,
            generation: 0,
            cards,
            groups,
            by_id,
            by_name,
            sets,
            domains,
        }
    }

    pub fn listed(&self) -> usize {
        self.groups
            .iter()
            .filter(|group| !group.hidden)
            .map(|group| group.prints.len())
            .sum()
    }

    pub fn card(&self, print: usize) -> &CatalogCard {
        &self.cards[print]
    }

    pub fn cards_at<'a>(&'a self, prints: &'a [usize]) -> impl Iterator<Item = &'a CatalogCard> {
        prints.iter().filter_map(|print| self.cards.get(*print))
    }

    pub fn group_of(&self, riftbound_id: &str) -> Option<usize> {
        self.by_id
            .get(&riftbound_id.trim().to_ascii_lowercase())
            .copied()
    }

    pub fn group_of_print(&self, print: usize) -> Option<usize> {
        self.cards
            .get(print)
            .and_then(|card| self.group_of(&card.riftbound_id))
    }

    pub fn find_name(&self, name: &str) -> Option<usize> {
        self.by_name.get(&normalize_name(name)).copied()
    }

    pub fn resolved(&self, print: usize) -> ResolvedCard {
        let mut card = self.cards[print].resolved();
        card.name = canonical_name(&card.riftbound_id, &card.name);
        card
    }

    pub fn basic_rune(&self, domain: &str) -> Option<usize> {
        let group = self.find_name(&format!("{} Rune", domain.trim()))?;
        let group = &self.groups[group];
        (group.kind == CardKind::Rune).then_some(group.default_print)
    }

    pub fn sets(&self) -> &[String] {
        &self.sets
    }

    pub fn filter(&self, filter: &Filter, search: &str, identity: &[String]) -> Vec<usize> {
        let query = search.trim().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut by_name = Vec::new();
        let mut by_text = Vec::new();
        for (index, group) in self.groups.iter().enumerate() {
            let name_hit = !words.is_empty()
                && words.iter().all(|word| {
                    group.name_lower.contains(word)
                        || group
                            .prints
                            .iter()
                            .any(|print| self.cards[*print].riftbound_id.starts_with(word))
                });
            if group.hidden {
                if name_hit {
                    by_name.push(index);
                }
                continue;
            }
            if !self.passes(group, filter, identity) {
                continue;
            }
            if words.is_empty() || name_hit {
                by_name.push(index);
            } else if words.iter().all(|word| group.text_lower.contains(word)) {
                by_text.push(index);
            }
        }
        by_name.extend(by_text);
        by_name
    }

    fn passes(&self, group: &Group, filter: &Filter, identity: &[String]) -> bool {
        if !filter.kinds.is_empty() && !filter.kinds.contains(&group.kind) {
            return false;
        }
        if !filter.domains.is_empty()
            && !group
                .domain
                .iter()
                .any(|domain| filter.domains.contains(domain))
        {
            return false;
        }
        if filter.fits_identity
            && !identity.is_empty()
            && !legality::fits_identity(identity, &group.domain)
        {
            return false;
        }
        if let Some(band) = filter.energy {
            match group.energy {
                Some(energy) if band.holds(energy) => {}
                _ => return false,
            }
        }
        if filter.champion_only && !group.champion {
            return false;
        }
        if filter.signature_only && !group.signature {
            return false;
        }
        if !filter.sets.is_empty()
            && !group.prints.iter().any(|print| {
                self.cards[*print]
                    .set_id
                    .as_ref()
                    .is_some_and(|set| filter.sets.contains(set))
            })
        {
            return false;
        }
        true
    }

    pub fn note(&self) -> String {
        match self.source {
            Source::None => NOTE_NONE.to_string(),
            Source::Pool(_) if cfg!(target_arch = "wasm32") => NOTE_POOL_WEB.to_string(),
            Source::Pool(_) => NOTE_POOL.to_string(),
            Source::Store(n) => format!("{n} prints · full set"),
            Source::Gateway(n) => format!("{n} prints · from your gateway"),
        }
    }

    pub fn replace_with(&mut self, fresh: Catalog) {
        let generation = self.generation.wrapping_add(1);
        *self = fresh;
        self.generation = generation;
        publish_tags_note(self);
    }

    pub fn domain_names(&self) -> &[&'static str] {
        &self.domains
    }
}

pub fn outranks_pool(fresh: &Catalog) -> bool {
    fresh.listed() >= pool_catalog().listed()
}

pub fn is_colorless(domains: &[String]) -> bool {
    domains.is_empty() || domains.iter().all(|d| d.eq_ignore_ascii_case(COLORLESS))
}

pub fn pool_catalog() -> Catalog {
    Catalog::from_cards(pool::cards(), Source::Pool(0))
}

pub fn register(app: &mut App) {
    app.init_resource::<Catalog>()
        .add_systems(Update, platform::load_catalog);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod platform {
    use super::{pool_catalog, Catalog, Source, STORE_POLL_SECS};
    use agni_importers::riftbound::ingest::{self, REF_NAME};
    use bevy::prelude::*;
    use spirit_core::{refs, BlobHash, BlobStore};

    pub fn store_ref() -> Option<BlobHash> {
        let dir = crate::os::paths::store_dir()?;
        let store = BlobStore::open(&dir).ok()?;
        refs::read(&store, REF_NAME)
    }

    pub fn store_catalog() -> Result<Option<Catalog>, String> {
        let Some(dir) = crate::os::paths::store_dir() else {
            return Ok(None);
        };
        let Some(manifest) = ingest::load_manifest(&dir).map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let cards = manifest.cards.iter().map(ingest::catalog_card).collect();
        Ok(Some(Catalog::from_cards(cards, Source::Store(0))))
    }

    #[derive(Default)]
    pub struct Watch {
        seen: Option<Option<BlobHash>>,
        since: f32,
    }

    pub fn load_catalog(time: Res<Time>, mut catalog: ResMut<Catalog>, mut watch: Local<Watch>) {
        watch.since += time.delta_secs();
        if watch.seen.is_some() && watch.since < STORE_POLL_SECS {
            return;
        }
        watch.since = 0.0;
        let current = store_ref();
        if watch.seen == Some(current) {
            return;
        }
        watch.seen = Some(current);
        let fresh = match store_catalog() {
            Ok(Some(fresh)) if super::outranks_pool(&fresh) => fresh,
            Ok(Some(partial)) => {
                info!(
                    "the store holds {} listed prints — fewer than the pool; the pool catalog stands in",
                    partial.listed()
                );
                if catalog.source != Source::None {
                    return;
                }
                pool_catalog()
            }
            Ok(None) => {
                if catalog.source != Source::None {
                    return;
                }
                pool_catalog()
            }
            Err(error) => {
                warn!("the riftbound catalog in the store does not load ({error}); the pool catalog stands in");
                if catalog.source != Source::None {
                    return;
                }
                pool_catalog()
            }
        };
        info!(
            "catalog: {} prints in {} names ({})",
            fresh.cards.len(),
            fresh.groups.len(),
            fresh.note()
        );
        catalog.replace_with(fresh);
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) mod platform {
    use super::{pool_catalog, Catalog, Source};
    use bevy::prelude::*;

    pub fn load_catalog(mut catalog: ResMut<Catalog>, mut seen: Local<Option<u32>>) {
        let generation = crate::net::gateway::riftbound_generation();
        if *seen == Some(generation) {
            return;
        }
        *seen = Some(generation);
        let fresh = match crate::net::gateway::riftbound_catalog() {
            Some(cards) => Catalog::from_cards(cards, Source::Gateway(0)),
            None if catalog.source == Source::None => pool_catalog(),
            None => return,
        };
        info!(
            "catalog: {} prints in {} names ({})",
            fresh.cards.len(),
            fresh.groups.len(),
            fresh.note()
        );
        catalog.replace_with(fresh);
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub const RIFTCODEX_ROWS: &str =
        include_str!("../../../agni/importers/src/riftbound/testdata/riftcodex-2026-09.tsv");

    fn optional(field: &str) -> Option<String> {
        (!field.is_empty()).then(|| field.to_string())
    }

    pub fn dump_cards() -> Vec<CatalogCard> {
        RIFTCODEX_ROWS
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                assert_eq!(fields.len(), 11, "{line}");
                let supertype = optional(fields[3]);
                let kind = CardKind::parse(fields[2]);
                CatalogCard {
                    name: fields[1].to_string(),
                    riftbound_id: fields[0].to_string(),
                    kind,
                    champion: supertype.as_deref() == Some("Champion"),
                    image_url: Some(format!("https://img.example/{}.png", fields[0])),
                    energy: fields[5].parse().ok(),
                    might: fields[6].parse().ok(),
                    power: fields[7].parse().ok(),
                    domain: fields[4]
                        .split('/')
                        .filter(|d| !d.is_empty())
                        .map(String::from)
                        .collect(),
                    tags: fields[9]
                        .split('/')
                        .filter(|t| !t.is_empty())
                        .map(String::from)
                        .collect(),
                    signature: supertype.as_deref() == Some("Signature"),
                    set_id: optional(fields[10]),
                    text: Some(format!("{} text of {}", kind.as_str(), fields[1])),
                }
            })
            .collect()
    }

    pub fn dump_catalog() -> Catalog {
        Catalog::from_cards(dump_cards(), Source::Store(0))
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{dump_cards, dump_catalog};
    use super::*;

    fn group<'a>(catalog: &'a Catalog, name: &str) -> &'a Group {
        &catalog.groups[catalog.find_name(name).expect(name)]
    }

    #[test]
    fn the_dump_folds_to_one_group_per_name_with_the_base_print_leading() {
        let catalog = dump_catalog();
        assert_eq!(catalog.cards.len(), 1451);
        assert_eq!(catalog.groups.len(), 938);
        assert_eq!(catalog.source, Source::Store(1451));
        assert!(catalog.tags_known);
        let lillia = group(&catalog, "Lillia - Bashful Bloom");
        assert_eq!(
            catalog.card(lillia.default_print).riftbound_id,
            "unl-189-219"
        );
        assert!(
            lillia.prints.len() >= 2,
            "signature and overnumbered prints fold in"
        );
        assert_eq!(lillia.kind, CardKind::Legend);
        assert_eq!(lillia.domain, ["Calm", "Mind"]);
        let poppy = group(&catalog, "Poppy - Paragon");
        assert_eq!(
            catalog.card(poppy.default_print).riftbound_id,
            "unl-116-219",
            "the alternate art print never leads its name"
        );
        assert!(poppy.champion);
        assert!(poppy.tags.iter().any(|t| t == "Poppy"));
        let nasus = group(&catalog, "Nasus - Curator of the Sands");
        assert!(
            nasus.prints.len() >= 2,
            "the alias table folds the VEN prints under one name: {:?}",
            nasus.prints
        );
        assert_eq!(
            catalog.group_of("UNL-116A-219"),
            catalog.find_name("poppy paragon")
        );
        let tokens = catalog.groups.iter().filter(|group| group.hidden).count();
        assert!(
            (1..10).contains(&tokens),
            "the token prints (SFD-t03 and friends) hide from the grid: {tokens}"
        );
        for group in catalog.groups.iter().filter(|group| !group.hidden) {
            let lead = catalog.card(group.default_print);
            let code = CardCode::from_riftbound_id(&lead.riftbound_id).unwrap();
            let has_base = group.prints.iter().any(|print| {
                CardCode::from_riftbound_id(&catalog.card(*print).riftbound_id)
                    .is_ok_and(|code| !code.set.reprints_another_set())
            });
            if has_base {
                assert!(
                    !code.set.reprints_another_set(),
                    "{} leads with a reprint {}",
                    group.name,
                    lead.riftbound_id
                );
            }
        }
    }

    #[test]
    fn groups_are_ordered_kind_then_energy_then_name() {
        let catalog = dump_catalog();
        let ranks: Vec<(usize, Option<u8>, String)> = catalog
            .groups
            .iter()
            .map(|g| (kind_rank(g.kind), g.energy, g.name_lower.clone()))
            .collect();
        let mut sorted = ranks.clone();
        sorted.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| energy_order(a.1, b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        assert_eq!(ranks, sorted);
        assert_eq!(catalog.groups[0].kind, CardKind::Legend);
        assert_eq!(catalog.groups.last().unwrap().kind, CardKind::Battlefield);
    }

    #[test]
    fn filters_narrow_by_kind_domain_identity_energy_and_flags() {
        let catalog = dump_catalog();
        let all = catalog.filter(&Filter::default(), "", &[]);
        let shown = catalog.groups.iter().filter(|g| !g.hidden).count();
        assert_eq!(all.len(), shown);
        let legends = catalog.filter(&Filter::only_kind(CardKind::Legend), "", &[]);
        assert!(!legends.is_empty());
        assert!(legends
            .iter()
            .all(|g| catalog.groups[*g].kind == CardKind::Legend));
        let mut filter = Filter::default();
        filter.toggle_kind(CardKind::Unit);
        filter.toggle_kind(CardKind::Spell);
        let units_spells = catalog.filter(&filter, "", &[]);
        assert!(units_spells
            .iter()
            .all(|g| matches!(catalog.groups[*g].kind, CardKind::Unit | CardKind::Spell)));
        filter.toggle_kind(CardKind::Unit);
        assert!(catalog
            .filter(&filter, "", &[])
            .iter()
            .all(|g| catalog.groups[*g].kind == CardKind::Spell));

        let mut fury = Filter::default();
        fury.toggle_domain("Fury");
        let fury_hits = catalog.filter(&fury, "", &[]);
        assert!(fury_hits
            .iter()
            .all(|g| catalog.groups[*g].domain.iter().any(|d| d == "Fury")));
        assert!(fury_hits.len() > 100);

        let identity = ["Calm".to_string(), "Mind".to_string()];
        let fitting = Filter {
            fits_identity: true,
            ..Default::default()
        };
        let fits = catalog.filter(&fitting, "", &identity);
        assert!(fits.iter().all(|g| {
            let domain = &catalog.groups[*g].domain;
            is_colorless(domain) || domain.iter().all(|d| d == "Calm" || d == "Mind")
        }));
        assert!(fits.len() < all.len());
        assert_eq!(
            catalog.filter(&fitting, "", &[]).len(),
            all.len(),
            "without a legend the identity chip filters nothing"
        );

        let mut high = Filter::default();
        high.toggle_energy(EnergyBand::High);
        let expensive = catalog.filter(&high, "", &[]);
        assert!(expensive
            .iter()
            .all(|g| matches!(catalog.groups[*g].energy, Some(4 | 5))));
        high.toggle_energy(EnergyBand::High);
        assert_eq!(high.energy, None);

        let champions = catalog.filter(
            &Filter {
                champion_only: true,
                ..Default::default()
            },
            "",
            &[],
        );
        assert!(champions
            .iter()
            .all(|g| catalog.groups[*g].champion && catalog.groups[*g].kind == CardKind::Unit));
        let signatures = catalog.filter(
            &Filter {
                signature_only: true,
                ..Default::default()
            },
            "",
            &[],
        );
        assert!(!signatures.is_empty());
        assert!(signatures.iter().all(|g| catalog.groups[*g].signature));

        let mut ven = Filter::default();
        ven.toggle_set("VEN");
        let ven_hits = catalog.filter(&ven, "", &[]);
        assert!(ven_hits
            .iter()
            .all(|g| catalog.groups[*g]
                .prints
                .iter()
                .any(|p| catalog.card(*p).set_id.as_deref() == Some("VEN"))));
        assert!(catalog.sets().contains(&"OGN".to_string()));
    }

    #[test]
    fn search_matches_name_then_text_and_reads_a_card_code() {
        let catalog = dump_catalog();
        let hits = catalog.filter(&Filter::default(), "lonely poro", &[]);
        assert_eq!(catalog.groups[hits[0]].name, "Lonely Poro");
        let by_code = catalog.filter(&Filter::default(), "unl-189", &[]);
        assert_eq!(catalog.groups[by_code[0]].name, "Lillia - Bashful Bloom");
        let text_hits = catalog.filter(&Filter::default(), "text of lonely", &[]);
        assert!(text_hits
            .iter()
            .any(|g| catalog.groups[*g].name == "Lonely Poro"));
        let name_first = catalog.filter(&Filter::default(), "poro", &[]);
        let first_text_only = name_first
            .iter()
            .position(|g| !catalog.groups[*g].name_lower.contains("poro"));
        let last_name = name_first
            .iter()
            .rposition(|g| catalog.groups[*g].name_lower.contains("poro"));
        if let (Some(text_at), Some(name_at)) = (first_text_only, last_name) {
            assert!(name_at < text_at, "name matches come before text matches");
        }
        assert!(catalog
            .filter(&Filter::default(), "zzzz no such card", &[])
            .is_empty());
        let filtered_search = catalog.filter(&Filter::only_kind(CardKind::Legend), "lillia", &[]);
        assert!(filtered_search
            .iter()
            .all(|g| catalog.groups[*g].kind == CardKind::Legend));
    }

    #[test]
    fn a_padded_record_hides_until_its_name_is_searched() {
        let mut cards = dump_cards();
        cards.push(CatalogCard {
            name: "Mystery Promo".into(),
            riftbound_id: "pr-099".into(),
            ..Default::default()
        });
        let catalog = Catalog::from_cards(cards, Source::Store(0));
        let mystery = catalog.find_name("Mystery Promo").unwrap();
        assert!(catalog.groups[mystery].hidden);
        assert!(!catalog
            .filter(&Filter::default(), "", &[])
            .contains(&mystery));
        assert!(!catalog
            .filter(&Filter::default(), "text of", &[])
            .contains(&mystery));
        assert!(catalog
            .filter(&Filter::default(), "mystery", &[])
            .contains(&mystery));
    }

    #[test]
    fn the_pool_fallback_is_flagged_and_still_finds_runes_and_legends() {
        let catalog = pool_catalog();
        assert!(matches!(catalog.source, Source::Pool(n) if n > 100));
        assert!(catalog.source.is_fallback());
        assert!(!catalog.tags_known, "pool markdown carries no tags");
        assert!(catalog.note().starts_with("offline catalog"));
        let rune = catalog.basic_rune("calm").expect("a Calm Rune");
        assert_eq!(catalog.card(rune).name, "Calm Rune");
        assert_eq!(catalog.card(rune).kind, CardKind::Rune);
        assert!(catalog.basic_rune("Void").is_none());
        let lillia = catalog
            .find_name("Lillia - Bashful Bloom")
            .expect("the pool legend");
        let resolved = catalog.resolved(catalog.groups[lillia].default_print);
        assert_eq!(resolved.kind.as_deref(), Some("Legend"));
        assert_eq!(resolved.name, "Lillia - Bashful Bloom");
        assert!(catalog.group_of("no-such-id").is_none());
        assert_eq!(
            catalog.group_of_print(catalog.groups[lillia].default_print),
            Some(lillia)
        );
        assert_eq!(Catalog::default().note(), NOTE_NONE);
        assert_eq!(Source::Store(7).prints(), 7);
    }

    #[test]
    fn replacing_the_catalog_bumps_the_generation_and_publishes_the_tags_note() {
        let mut catalog = Catalog::default();
        assert_eq!(catalog.generation, 0);
        let mut untagged = dump_cards();
        for card in &mut untagged {
            card.tags.clear();
        }
        catalog.replace_with(Catalog::from_cards(untagged, Source::Store(0)));
        assert_eq!(catalog.generation, 1);
        assert!(!catalog.tags_known);
        assert_eq!(tags_note(), Some(TAGS_NOTE_STORE));
        catalog.replace_with(dump_catalog());
        assert_eq!(catalog.generation, 2);
        assert!(catalog.tags_known);
        assert_eq!(tags_note(), None);
        catalog.replace_with(pool_catalog());
        assert_eq!(tags_note(), None, "the pool never asks for a re-download");
    }

    #[test]
    fn energy_bands_cover_every_cost_once() {
        for energy in 0..=12u8 {
            let bands: Vec<EnergyBand> = EnergyBand::ALL
                .into_iter()
                .filter(|band| band.holds(energy))
                .collect();
            assert_eq!(
                bands.len(),
                1,
                "energy {energy} sits in one band: {bands:?}"
            );
        }
        assert_eq!(EnergyBand::Top.label(), "6+");
        assert!(Filter::default().is_default());
    }

    #[test]
    fn the_domain_list_follows_the_theme_order_and_skips_absent_domains() {
        let catalog = dump_catalog();
        assert_eq!(catalog.domain_names(), DOMAINS.to_vec());
        let one = Catalog::from_cards(
            vec![CatalogCard {
                name: "Ember".into(),
                riftbound_id: "ogn-001-298".into(),
                kind: CardKind::Unit,
                domain: vec!["Fury".into()],
                ..Default::default()
            }],
            Source::Store(0),
        );
        assert_eq!(one.domain_names(), vec!["Fury"]);
    }

    #[test]
    fn a_store_smaller_than_the_pool_never_replaces_it() {
        let two = Catalog::from_cards(
            vec![
                CatalogCard {
                    name: "Lonely Poro".into(),
                    riftbound_id: "sfd-036-221".into(),
                    kind: CardKind::Unit,
                    domain: vec!["Calm".into()],
                    ..Default::default()
                },
                CatalogCard {
                    name: "Padded".into(),
                    riftbound_id: "ogn-999-298".into(),
                    ..Default::default()
                },
            ],
            Source::Store(0),
        );
        assert_eq!(two.listed(), 1, "a padded record is not listed");
        assert!(!outranks_pool(&two));
        assert!(outranks_pool(&dump_catalog()));
        let pool = pool_catalog();
        assert!(
            pool.listed() < pool::cards().len(),
            "the pool lists a token print the catalog hides"
        );
        assert!(outranks_pool(&pool));
    }
}
