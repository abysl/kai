use crate::menu::browser::BrowserState;
use crate::menu::{Menu, Screen};
use agni_importers::riftbound::resolve::canonical_name;
use agni_riftbound::legality::{self, Grade, Mode, Report, Rule, COPY_LIMIT};
use agni_riftbound::{
    DeckEntry, ResolvedCard, ResolvedDeck, BATTLEFIELD_COUNT, KIND_BATTLEFIELD, KIND_LEGEND,
    KIND_RUNE, KIND_UNIT, RUNE_DECK_SIZE,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, VecDeque};

pub const UNDO_DEPTH: usize = 32;
pub const PERSIST_AFTER_SECS: f32 = 1.0;
pub const NEW_LABEL: &str = "new deck";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    New,
    Pool(String),
    Saved(spirit_sdk::CiHash),
    Import(String),
    Seated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    List,
    Cards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Legend,
    Champion,
    Main,
    Runes,
    Battlefields,
    Sideboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    SetLegend(ResolvedCard),
    ClearLegend,
    SetChampion(ResolvedCard),
    ClearChampion,
    Add(ResolvedCard),
    AddTo {
        zone: Zone,
        card: ResolvedCard,
    },
    SetCount {
        zone: Zone,
        id: String,
        count: u32,
    },
    ToSideboard {
        id: String,
    },
    ToMain {
        id: String,
    },
    ChangePrint {
        zone: Zone,
        id: String,
        card: ResolvedCard,
    },
    FillRunes(Vec<(ResolvedCard, u32)>),
    Replace(Box<ResolvedDeck>),
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placed {
    Legend,
    Champion,
    Main,
    Runes,
    Battlefields,
    Sideboard,
}

impl Placed {
    pub fn label(self) -> &'static str {
        match self {
            Self::Legend => "legend",
            Self::Champion => "champion",
            Self::Main => "main deck",
            Self::Runes => "runes",
            Self::Battlefields => "battlefields",
            Self::Sideboard => "sideboard",
        }
    }
}

pub struct Draft {
    pub deck: ResolvedDeck,
    pub label: String,
    pub origin: Origin,
    pub dirty: bool,
    pub report: Report,
    pub unresolved: Vec<String>,
    pub undo: VecDeque<ResolvedDeck>,
    pub pane: Pane,
    pub browser: BrowserState,
    pub scroll_to: Option<String>,
    pub edits: u64,
}

fn folded(mut card: ResolvedCard) -> ResolvedCard {
    card.name = canonical_name(&card.riftbound_id, &card.name);
    card
}

fn same_name(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn push_by_id(zone: &mut Vec<DeckEntry>, card: ResolvedCard, count: u32) {
    match zone
        .iter_mut()
        .find(|entry| entry.card.riftbound_id == card.riftbound_id)
    {
        Some(entry) => entry.count += count,
        None => zone.push(DeckEntry { card, count }),
    }
}

fn take_one(zone: &mut Vec<DeckEntry>, id: &str) -> Option<ResolvedCard> {
    let index = zone
        .iter()
        .position(|entry| entry.card.riftbound_id == id)?;
    let card = zone[index].card.clone();
    zone[index].count -= 1;
    if zone[index].count == 0 {
        zone.remove(index);
    }
    Some(card)
}

fn name_count(entries: &[DeckEntry], name: &str) -> u32 {
    entries
        .iter()
        .filter(|entry| same_name(&entry.card.name, name))
        .map(|entry| entry.count)
        .sum()
}

fn take_one_named(zone: &mut Vec<DeckEntry>, name: &str, prefer_id: &str) -> Option<ResolvedCard> {
    let index = zone
        .iter()
        .position(|entry| entry.card.riftbound_id == prefer_id)
        .or_else(|| {
            zone.iter()
                .position(|entry| same_name(&entry.card.name, name))
        })?;
    let id = zone[index].card.riftbound_id.clone();
    take_one(zone, &id)
}

pub fn copy_limit_reason(name: &str) -> String {
    format!("already {COPY_LIMIT} copies of {name} — rule 103.2.b")
}

pub fn unit_or_unknown_kind(kind: &str) -> bool {
    kind == KIND_UNIT || kind == "Other" || kind.is_empty()
}

pub fn looks_like_champion(card: &ResolvedCard) -> bool {
    let stem = legality::name_stem(&card.name);
    if stem == card.name.trim() {
        return false;
    }
    card.tags.is_empty() || card.tags.iter().any(|tag| tag.eq_ignore_ascii_case(stem))
}

impl Draft {
    pub fn new(label: &str, origin: Origin) -> Self {
        Self::from_deck(ResolvedDeck::default(), label, origin)
    }

    pub fn from_deck(deck: ResolvedDeck, label: &str, origin: Origin) -> Self {
        let report = legality::check(&deck, Mode::Standard);
        Self {
            deck,
            label: label.to_string(),
            origin,
            dirty: false,
            report,
            unresolved: Vec::new(),
            undo: VecDeque::new(),
            pane: Pane::List,
            browser: BrowserState::default(),
            scroll_to: None,
            edits: 0,
        }
    }

    pub fn apply(&mut self, edit: Edit) -> Result<Placed, String> {
        let before = self.deck.clone();
        let placed = self.mutate(edit);
        match placed {
            Ok(_) if self.deck != before => {
                self.undo.push_back(before);
                while self.undo.len() > UNDO_DEPTH {
                    self.undo.pop_front();
                }
                self.touch();
            }
            Ok(_) => {}
            Err(_) => self.deck = before,
        }
        placed
    }

    fn touch(&mut self) {
        self.report = legality::check(&self.deck, Mode::Standard);
        self.dirty = true;
        self.edits += 1;
    }

    pub fn load(
        &mut self,
        deck: ResolvedDeck,
        label: &str,
        origin: Origin,
        unresolved: Vec<String>,
    ) {
        let fresh = self.label == NEW_LABEL || self.label.trim().is_empty();
        if self.apply(Edit::Replace(Box::new(deck))).is_ok() {
            self.origin = origin;
            self.unresolved = unresolved;
            if fresh {
                self.rename(label);
            }
            self.dirty = true;
            self.pane = Pane::List;
        }
    }

    pub fn rename(&mut self, label: &str) {
        let label = label.trim();
        if label.is_empty() || label == self.label {
            return;
        }
        self.label = label.to_string();
        self.dirty = true;
        self.edits += 1;
    }

    fn mutate(&mut self, edit: Edit) -> Result<Placed, String> {
        match edit {
            Edit::SetLegend(card) => self.set_legend(folded(card)),
            Edit::ClearLegend => {
                self.deck.legend = None;
                Ok(Placed::Legend)
            }
            Edit::SetChampion(card) => self.set_champion(folded(card)),
            Edit::ClearChampion => {
                self.deck.chosen_champion = None;
                Ok(Placed::Champion)
            }
            Edit::Add(card) => {
                let card = folded(card);
                let zone = self.route(&card);
                self.add_to(zone, card)
            }
            Edit::AddTo { zone, card } => self.add_to(zone, folded(card)),
            Edit::SetCount { zone, id, count } => self.set_count(zone, &id, count),
            Edit::ToSideboard { id } => {
                let card = take_one(&mut self.deck.main_deck, &id)
                    .ok_or_else(|| format!("{id} is not in the main deck"))?;
                if name_count(&self.deck.sideboard, &card.name) >= COPY_LIMIT {
                    return Err(format!(
                        "already {COPY_LIMIT} {} in the sideboard",
                        card.name
                    ));
                }
                push_by_id(&mut self.deck.sideboard, card, 1);
                Ok(Placed::Sideboard)
            }
            Edit::ToMain { id } => {
                let card = take_one(&mut self.deck.sideboard, &id)
                    .ok_or_else(|| format!("{id} is not in the sideboard"))?;
                if self.main_copies(&card.name) >= COPY_LIMIT {
                    return Err(copy_limit_reason(&card.name));
                }
                push_by_id(&mut self.deck.main_deck, card, 1);
                Ok(Placed::Main)
            }
            Edit::ChangePrint { zone, id, card } => self.change_print(zone, &id, folded(card)),
            Edit::FillRunes(runes) => {
                for (card, count) in runes {
                    push_by_id(&mut self.deck.runes, folded(card), count);
                }
                Ok(Placed::Runes)
            }
            Edit::Replace(deck) => {
                self.deck = *deck;
                Ok(Placed::Main)
            }
            Edit::Clear => {
                self.deck = ResolvedDeck::default();
                Ok(Placed::Main)
            }
        }
    }

    fn set_legend(&mut self, card: ResolvedCard) -> Result<Placed, String> {
        if let Some(kind) = card.kind.as_deref().filter(|kind| *kind != KIND_LEGEND) {
            return Err(format!("{} is a {kind}, not a legend", card.name));
        }
        self.deck.legend = Some(card);
        Ok(Placed::Legend)
    }

    fn set_champion(&mut self, card: ResolvedCard) -> Result<Placed, String> {
        if let Some(kind) = card
            .kind
            .as_deref()
            .filter(|kind| !unit_or_unknown_kind(kind))
        {
            return Err(format!("{} is a {kind}, not a champion unit", card.name));
        }
        if name_count(&self.deck.main_deck, &card.name) >= COPY_LIMIT {
            take_one_named(&mut self.deck.main_deck, &card.name, &card.riftbound_id);
        }
        self.deck.chosen_champion = Some(card);
        Ok(Placed::Champion)
    }

    fn route(&self, card: &ResolvedCard) -> Zone {
        match card.kind.as_deref() {
            Some(KIND_LEGEND) => Zone::Legend,
            Some(KIND_RUNE) => Zone::Runes,
            Some(KIND_BATTLEFIELD) => Zone::Battlefields,
            _ if self.deck.chosen_champion.is_none() && self.champion_fits(card) => Zone::Champion,
            _ => Zone::Main,
        }
    }

    pub fn champion_fits(&self, card: &ResolvedCard) -> bool {
        if card.signature || !card.kind.as_deref().is_none_or(unit_or_unknown_kind) {
            return false;
        }
        let tags = self.champion_tags();
        if tags.is_empty() {
            return looks_like_champion(card);
        }
        let probe = ResolvedCard {
            kind: Some(KIND_UNIT.to_string()),
            ..card.clone()
        };
        legality::fits_champion(&probe, &tags)
    }

    fn add_to(&mut self, zone: Zone, card: ResolvedCard) -> Result<Placed, String> {
        match zone {
            Zone::Legend => self.set_legend(card),
            Zone::Champion => self.set_champion(card),
            Zone::Main => {
                if self.main_copies(&card.name) >= COPY_LIMIT {
                    return Err(copy_limit_reason(&card.name));
                }
                push_by_id(&mut self.deck.main_deck, card, 1);
                Ok(Placed::Main)
            }
            Zone::Runes => {
                push_by_id(&mut self.deck.runes, card, 1);
                Ok(Placed::Runes)
            }
            Zone::Battlefields => {
                if agni_deck::total(&self.deck.battlefields) >= BATTLEFIELD_COUNT as u32 {
                    return Err(format!("already {BATTLEFIELD_COUNT} battlefields"));
                }
                if name_count(&self.deck.battlefields, &card.name) > 0 {
                    return Err(format!("a second {}", card.name));
                }
                push_by_id(&mut self.deck.battlefields, card, 1);
                Ok(Placed::Battlefields)
            }
            Zone::Sideboard => {
                if name_count(&self.deck.sideboard, &card.name) >= COPY_LIMIT {
                    return Err(format!(
                        "already {COPY_LIMIT} {} in the sideboard",
                        card.name
                    ));
                }
                push_by_id(&mut self.deck.sideboard, card, 1);
                Ok(Placed::Sideboard)
            }
        }
    }

    fn set_count(&mut self, zone: Zone, id: &str, count: u32) -> Result<Placed, String> {
        let (entries, placed) = match zone {
            Zone::Legend => {
                if count == 0 {
                    self.deck.legend = None;
                }
                return Ok(Placed::Legend);
            }
            Zone::Champion => {
                if count == 0 {
                    self.deck.chosen_champion = None;
                }
                return Ok(Placed::Champion);
            }
            Zone::Main => (&mut self.deck.main_deck, Placed::Main),
            Zone::Runes => (&mut self.deck.runes, Placed::Runes),
            Zone::Battlefields => (&mut self.deck.battlefields, Placed::Battlefields),
            Zone::Sideboard => (&mut self.deck.sideboard, Placed::Sideboard),
        };
        let index = entries
            .iter()
            .position(|entry| entry.card.riftbound_id == id)
            .ok_or_else(|| format!("{id} is not in the {}", placed.label()))?;
        if count == 0 {
            entries.remove(index);
            return Ok(placed);
        }
        let name = entries[index].card.name.clone();
        let others: u32 = entries
            .iter()
            .enumerate()
            .filter(|(at, entry)| *at != index && same_name(&entry.card.name, &name))
            .map(|(_, entry)| entry.count)
            .sum();
        match zone {
            Zone::Main => {
                let champion = self
                    .deck
                    .chosen_champion
                    .as_ref()
                    .is_some_and(|card| same_name(&card.name, &name))
                    as u32;
                if others + champion + count > COPY_LIMIT {
                    return Err(copy_limit_reason(&name));
                }
            }
            Zone::Sideboard if others + count > COPY_LIMIT => {
                return Err(format!("already {COPY_LIMIT} {name} in the sideboard"));
            }
            Zone::Battlefields => {
                if others + count > 1 {
                    return Err(format!("a second {name}"));
                }
                let total: u32 = self
                    .deck
                    .battlefields
                    .iter()
                    .enumerate()
                    .map(|(at, entry)| if at == index { count } else { entry.count })
                    .sum();
                if total > BATTLEFIELD_COUNT as u32 {
                    return Err(format!("already {BATTLEFIELD_COUNT} battlefields"));
                }
            }
            _ => {}
        }
        let entries = match zone {
            Zone::Main => &mut self.deck.main_deck,
            Zone::Runes => &mut self.deck.runes,
            Zone::Battlefields => &mut self.deck.battlefields,
            _ => &mut self.deck.sideboard,
        };
        entries[index].count = count;
        Ok(placed)
    }

    fn change_print(&mut self, zone: Zone, id: &str, card: ResolvedCard) -> Result<Placed, String> {
        let slot = match zone {
            Zone::Legend => return self.set_legend(card),
            Zone::Champion => return self.set_champion(card),
            Zone::Main => (&mut self.deck.main_deck, Placed::Main),
            Zone::Runes => (&mut self.deck.runes, Placed::Runes),
            Zone::Battlefields => (&mut self.deck.battlefields, Placed::Battlefields),
            Zone::Sideboard => (&mut self.deck.sideboard, Placed::Sideboard),
        };
        let (entries, placed) = slot;
        let index = entries
            .iter()
            .position(|entry| entry.card.riftbound_id == id)
            .ok_or_else(|| format!("{id} is not in the {}", placed.label()))?;
        let count = entries[index].count;
        entries.remove(index);
        push_by_id(entries, card, count);
        Ok(placed)
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.deck = previous;
        self.touch();
        true
    }

    fn main_copies(&self, name: &str) -> u32 {
        name_count(&self.deck.main_deck, name)
            + self
                .deck
                .chosen_champion
                .as_ref()
                .is_some_and(|card| same_name(&card.name, name)) as u32
    }

    pub fn copies(&self) -> BTreeMap<String, u32> {
        let mut copies: BTreeMap<String, u32> = BTreeMap::new();
        if let Some(champion) = &self.deck.chosen_champion {
            *copies.entry(champion.name.clone()).or_default() += 1;
        }
        for entry in self
            .deck
            .main_deck
            .iter()
            .chain(&self.deck.runes)
            .chain(&self.deck.battlefields)
        {
            *copies.entry(entry.card.name.clone()).or_default() += entry.count;
        }
        copies
    }

    pub fn identity(&self) -> Vec<String> {
        self.deck
            .legend
            .as_ref()
            .map(legality::identity)
            .unwrap_or_default()
    }

    pub fn champion_tags(&self) -> Vec<String> {
        self.deck
            .legend
            .as_ref()
            .map(legality::champion_tags)
            .unwrap_or_default()
    }

    pub fn curve(&self) -> [u32; 8] {
        let mut curve = [0u32; 8];
        let champion = self
            .deck
            .chosen_champion
            .iter()
            .map(|card| (card.energy, 1));
        for (energy, count) in champion.chain(
            self.deck
                .main_deck
                .iter()
                .map(|entry| (entry.card.energy, entry.count)),
        ) {
            if let Some(energy) = energy {
                curve[(energy as usize).min(7)] += count;
            }
        }
        curve
    }

    pub fn locate(&self, name: &str) -> Option<(Zone, String)> {
        if let Some(card) = self
            .deck
            .legend
            .as_ref()
            .filter(|card| same_name(&card.name, name))
        {
            return Some((Zone::Legend, card.riftbound_id.clone()));
        }
        if let Some(card) = self
            .deck
            .chosen_champion
            .as_ref()
            .filter(|card| same_name(&card.name, name))
        {
            return Some((Zone::Champion, card.riftbound_id.clone()));
        }
        let zones = [
            (Zone::Main, &self.deck.main_deck),
            (Zone::Sideboard, &self.deck.sideboard),
            (Zone::Runes, &self.deck.runes),
            (Zone::Battlefields, &self.deck.battlefields),
        ];
        zones.into_iter().find_map(|(zone, entries)| {
            entries
                .iter()
                .find(|entry| same_name(&entry.card.name, name))
                .map(|entry| (zone, entry.card.riftbound_id.clone()))
        })
    }

    pub fn rune_fill(
        &self,
        basic: impl Fn(&str) -> Option<ResolvedCard>,
    ) -> Result<Vec<(ResolvedCard, u32)>, String> {
        let have = agni_deck::total(&self.deck.runes);
        let need = RUNE_DECK_SIZE as u32;
        if have >= need {
            return Err(format!("the rune deck already holds {have}"));
        }
        let domains: Vec<String> = self
            .identity()
            .into_iter()
            .filter(|domain| domain != legality::COLORLESS)
            .collect();
        if domains.is_empty() {
            return Err("choose a legend first".into());
        }
        let split = legality::rune_split(domains.len(), need - have);
        domains
            .iter()
            .zip(split)
            .filter(|(_, count)| *count > 0)
            .map(|(domain, count)| {
                basic(domain)
                    .map(|card| (card, count))
                    .ok_or_else(|| format!("the catalog has no basic {domain} rune"))
            })
            .collect()
    }

    pub fn back(&mut self) -> bool {
        if self.browser.detail.is_some() {
            self.browser.close_detail();
            return true;
        }
        if self.pane == Pane::Cards {
            self.pane = Pane::List;
            return true;
        }
        false
    }

    pub fn flagged(&self, riftbound_id: &str) -> Option<Grade> {
        self.report.flagged(riftbound_id)
    }
}

pub const MAIN_GROUPS: [&str; 4] = ["Units", "Spells", "Gear", "Other"];

fn group_index(card: &ResolvedCard) -> usize {
    match card.kind.as_deref() {
        Some(KIND_UNIT) => 0,
        Some("Spell") => 1,
        Some("Gear") => 2,
        _ => 3,
    }
}

pub fn main_groups(entries: &[DeckEntry]) -> Vec<(&'static str, Vec<&DeckEntry>)> {
    let mut groups: Vec<Vec<&DeckEntry>> = vec![Vec::new(); MAIN_GROUPS.len()];
    for entry in entries {
        groups[group_index(&entry.card)].push(entry);
    }
    for group in groups.iter_mut() {
        group.sort_by(|left, right| {
            left.card.energy.cmp(&right.card.energy).then_with(|| {
                left.card
                    .name
                    .to_lowercase()
                    .cmp(&right.card.name.to_lowercase())
            })
        });
    }
    MAIN_GROUPS
        .into_iter()
        .zip(groups)
        .filter(|(_, group)| !group.is_empty())
        .collect()
}

pub fn sorted_by_name(entries: &[DeckEntry]) -> Vec<&DeckEntry> {
    let mut rows: Vec<&DeckEntry> = entries.iter().collect();
    rows.sort_by_key(|entry| entry.card.name.to_lowercase());
    rows
}

pub fn shortfall_filter(rule: &Rule) -> Option<crate::deck::catalog::Filter> {
    use agni_importers::riftbound::catalog::CardKind;
    let mut filter = crate::deck::catalog::Filter::default();
    let kinds: &[CardKind] = match rule {
        Rule::LegendMissing => &[CardKind::Legend],
        Rule::ChampionMissing => {
            filter.champion_only = true;
            &[CardKind::Unit]
        }
        Rule::MainSize { .. } => &[CardKind::Unit, CardKind::Spell, CardKind::Gear],
        Rule::RuneCount { .. } => &[CardKind::Rune],
        Rule::BattlefieldCount { .. } => &[CardKind::Battlefield],
        _ => return None,
    };
    filter.kinds = kinds.iter().copied().collect();
    Some(filter)
}

#[derive(Resource, Default)]
pub struct DeckEditor {
    pub draft: Option<Draft>,
    pub note: Option<String>,
    pub opens: u64,
}

pub fn open(editor: &mut DeckEditor, menu: &mut Menu, draft: Draft) {
    editor.draft = Some(draft);
    reopen(editor, menu);
}

pub fn reopen(editor: &mut DeckEditor, menu: &mut Menu) {
    editor.opens = editor.opens.wrapping_add(1);
    menu.open_editor();
}

pub fn close(editor: &mut DeckEditor, menu: &mut Menu) {
    let _ = editor;
    if menu.screen == Screen::DeckEditor {
        menu.screen = Screen::Decks;
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<DeckEditor>()
        .init_resource::<crate::menu::editor::EditorSheet>()
        .init_resource::<crate::settings::BrowserThumbs>()
        .add_systems(Startup, restore_draft)
        .add_systems(Update, persist_draft);
}

fn restore_draft(mut editor: ResMut<DeckEditor>) {
    if editor.draft.is_some() {
        return;
    }
    if let Some(draft) = crate::os::drafts::load() {
        editor.draft = Some(draft);
    }
}

#[derive(Default)]
pub struct PersistState {
    pub written: Option<(u64, String)>,
    pub since_change: f32,
    pub seen: Option<(u64, String)>,
}

pub fn persist_step(state: &mut PersistState, draft: Option<&Draft>, dt: f32) -> Option<bool> {
    let Some(draft) = draft.filter(|draft| draft.dirty) else {
        let written = state.written.take().is_some();
        let seen = state.seen.take().is_some();
        return (written || seen).then_some(false);
    };
    let key = (draft.edits, draft.label.clone());
    if state.seen.as_ref() != Some(&key) {
        state.seen = Some(key);
        state.since_change = 0.0;
        return None;
    }
    state.since_change += dt;
    if state.since_change < PERSIST_AFTER_SECS || state.written == state.seen {
        return None;
    }
    state.written = state.seen.clone();
    Some(true)
}

fn persist_draft(
    time: Res<Time>,
    editor: Res<DeckEditor>,
    mut state: Local<PersistState>,
    mut cleared: Local<bool>,
) {
    match persist_step(&mut state, editor.draft.as_ref(), time.delta_secs()) {
        Some(true) => {
            if let Some(draft) = editor.draft.as_ref() {
                if let Err(error) = crate::os::drafts::store(draft) {
                    warn!("the deck draft was not kept: {error}");
                }
                *cleared = false;
            }
        }
        Some(false) if !*cleared => {
            crate::os::drafts::clear();
            *cleared = true;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_riftbound::legality::Verdict;

    fn card(name: &str, id: &str, kind: &str) -> ResolvedCard {
        ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            kind: Some(kind.into()),
            domain: vec!["Calm".into()],
            ..Default::default()
        }
    }

    fn legend() -> ResolvedCard {
        ResolvedCard {
            domain: vec!["Calm".into(), "Mind".into()],
            tags: vec!["Lillia".into()],
            ..card("Lillia - Bashful Bloom", "ogn-189-298", KIND_LEGEND)
        }
    }

    fn champion() -> ResolvedCard {
        ResolvedCard {
            tags: vec!["Fae".into(), "Lillia".into(), "Ionia".into()],
            energy: Some(3),
            ..card("Lillia - Fae Fawn", "ogn-082-298", KIND_UNIT)
        }
    }

    fn poro() -> ResolvedCard {
        ResolvedCard {
            energy: Some(1),
            ..card("Lonely Poro", "sfd-036-298", KIND_UNIT)
        }
    }

    fn rune(name: &str, id: &str, domain: &str) -> ResolvedCard {
        ResolvedCard {
            domain: vec![domain.into()],
            ..card(name, id, KIND_RUNE)
        }
    }

    fn field(name: &str, id: &str) -> ResolvedCard {
        ResolvedCard {
            domain: Vec::new(),
            ..card(name, id, KIND_BATTLEFIELD)
        }
    }

    #[test]
    fn add_routes_by_kind_and_a_matching_champion_unit_fills_the_empty_slot() {
        let mut draft = Draft::new("t", Origin::New);
        assert_eq!(draft.apply(Edit::Add(legend())), Ok(Placed::Legend));
        assert_eq!(draft.apply(Edit::Add(champion())), Ok(Placed::Champion));
        assert_eq!(
            draft.apply(Edit::Add(champion())),
            Ok(Placed::Main),
            "with the slot filled a second copy joins the main deck"
        );
        assert_eq!(
            draft.apply(Edit::Add(rune("Calm Rune", "ogn-042-298", "Calm"))),
            Ok(Placed::Runes)
        );
        assert_eq!(
            draft.apply(Edit::Add(field("Seat of Power", "ogn-260-298"))),
            Ok(Placed::Battlefields)
        );
        assert_eq!(draft.apply(Edit::Add(poro())), Ok(Placed::Main));
        assert_eq!(draft.copies().get("Lillia - Fae Fawn"), Some(&2));
        assert_eq!(draft.copies().get("Lonely Poro"), Some(&1));
        assert!(draft.dirty);
        assert_eq!(draft.undo.len(), 6);
        assert_eq!(draft.edits, 6);
    }

    #[test]
    fn a_catalog_without_tags_still_routes_the_named_champion_by_its_name_stem() {
        let untagged = |card: ResolvedCard| ResolvedCard {
            tags: Vec::new(),
            ..card
        };
        let mut draft = Draft::new("t", Origin::New);
        assert_eq!(
            draft.apply(Edit::Add(untagged(legend()))),
            Ok(Placed::Legend)
        );
        assert_eq!(draft.champion_tags(), vec!["Lillia"]);
        assert_eq!(
            draft.apply(Edit::Add(untagged(champion()))),
            Ok(Placed::Champion)
        );
        assert_eq!(draft.apply(Edit::Add(untagged(poro()))), Ok(Placed::Main));
        let sprite = untagged(ResolvedCard {
            energy: Some(4),
            ..card("Sprite Mother", "ogn-106-298", KIND_UNIT)
        });
        assert_eq!(draft.apply(Edit::Add(sprite)), Ok(Placed::Main));
    }

    #[test]
    fn a_fourth_copy_is_refused_with_the_rule_and_nothing_changes() {
        let mut draft = Draft::new("t", Origin::New);
        for _ in 0..3 {
            draft.apply(Edit::Add(poro())).unwrap();
        }
        let before = draft.deck.clone();
        let edits = draft.edits;
        assert_eq!(
            draft.apply(Edit::Add(poro())),
            Err("already 3 copies of Lonely Poro — rule 103.2.b".into())
        );
        assert_eq!(draft.deck, before);
        assert_eq!(draft.edits, edits, "a refusal is not an edit");
        assert_eq!(draft.undo.len(), 3);
        assert_eq!(
            draft.apply(Edit::SetCount {
                zone: Zone::Main,
                id: "sfd-036-298".into(),
                count: 4,
            }),
            Err(copy_limit_reason("Lonely Poro"))
        );
        assert_eq!(
            draft.apply(Edit::SetChampion(poro())),
            Ok(Placed::Champion),
            "an explicit champion pick promotes one of the three copies"
        );
        assert_eq!(draft.deck.main_deck[0].count, 2);
        assert_eq!(draft.copies().get("Lonely Poro"), Some(&3));
    }

    #[test]
    fn loading_an_import_replaces_the_list_keeps_a_named_label_and_undoes() {
        let mut draft = Draft::new(NEW_LABEL, Origin::New);
        draft.apply(Edit::Add(poro())).unwrap();
        let imported = crate::deck::pool::deck("lillia-house").unwrap();
        draft.load(
            imported.clone(),
            "Lillia by Jonnynick",
            Origin::Import("clipboard".into()),
            vec!["Nobody".into()],
        );
        assert_eq!(draft.deck, imported);
        assert_eq!(
            draft.label, "Lillia by Jonnynick",
            "a fresh draft takes the import's name"
        );
        assert_eq!(draft.origin, Origin::Import("clipboard".into()));
        assert_eq!(draft.unresolved, ["Nobody"]);
        assert!(draft.dirty);
        assert!(draft.undo());
        assert_eq!(
            draft.copies().get("Lonely Poro"),
            Some(&1),
            "undo restores the old list"
        );
        let mut named = Draft::new("My Lillia", Origin::New);
        named.load(
            imported,
            "Lillia by Jonnynick",
            Origin::Import("clipboard".into()),
            vec![],
        );
        assert_eq!(named.label, "My Lillia", "a named draft keeps its name");
        assert!(named.report.meter.champion);
    }

    #[test]
    fn a_champion_unit_added_before_the_legend_takes_the_empty_slot() {
        let mut draft = Draft::new("t", Origin::New);
        assert!(draft.champion_tags().is_empty());
        assert_eq!(draft.apply(Edit::Add(champion())), Ok(Placed::Champion));
        assert_eq!(draft.apply(Edit::Add(champion())), Ok(Placed::Main));
        assert_eq!(draft.apply(Edit::Add(poro())), Ok(Placed::Main));
        let poppy = ResolvedCard {
            tags: vec!["Poppy".into(), "Yordle".into()],
            ..card("Poppy - Paragon", "unl-116-219", KIND_UNIT)
        };
        assert_eq!(
            draft.apply(Edit::Add(poppy.clone())),
            Ok(Placed::Main),
            "the slot is taken, so a second champion unit is a main card"
        );
        assert_eq!(draft.apply(Edit::Add(legend())), Ok(Placed::Legend));
        assert_eq!(
            draft.deck.chosen_champion.as_ref().unwrap().name,
            "Lillia - Fae Fawn"
        );
        let mut fresh = Draft::new("t", Origin::New);
        assert_eq!(fresh.apply(Edit::Add(legend())), Ok(Placed::Legend));
        assert_eq!(
            fresh.apply(Edit::Add(poppy)),
            Ok(Placed::Main),
            "with a legend chosen only its champion fills the slot"
        );
    }

    #[test]
    fn setting_the_champion_from_a_full_main_deck_promotes_a_copy() {
        let mut draft = Draft::new("t", Origin::New);
        draft.apply(Edit::Add(legend())).unwrap();
        for _ in 0..3 {
            draft
                .apply(Edit::AddTo {
                    zone: Zone::Main,
                    card: champion(),
                })
                .unwrap();
        }
        assert!(draft.deck.chosen_champion.is_none());
        assert_eq!(draft.copies().get("Lillia - Fae Fawn"), Some(&3));
        assert_eq!(
            draft.apply(Edit::SetChampion(champion())),
            Ok(Placed::Champion)
        );
        assert_eq!(draft.deck.main_deck[0].count, 2);
        assert_eq!(draft.copies().get("Lillia - Fae Fawn"), Some(&3));
        assert!(draft.report.meter.champion);
    }

    #[test]
    fn a_record_without_a_kind_still_routes_to_the_champion_slot_by_its_name() {
        let padded = ResolvedCard {
            kind: None,
            tags: Vec::new(),
            ..champion()
        };
        let mut draft = Draft::new("t", Origin::New);
        draft
            .apply(Edit::Add(ResolvedCard {
                tags: Vec::new(),
                ..legend()
            }))
            .unwrap();
        assert_eq!(draft.apply(Edit::Add(padded.clone())), Ok(Placed::Champion));
        let other = ResolvedCard {
            kind: Some("Other".into()),
            ..card("Sprite Mother", "ogn-106-298", KIND_UNIT)
        };
        assert_eq!(draft.apply(Edit::Add(other)), Ok(Placed::Main));
        let spell = ResolvedCard {
            kind: Some("Spell".into()),
            ..card("Lillia - Dream Dance", "x-001", KIND_UNIT)
        };
        assert_eq!(
            draft.apply(Edit::SetChampion(spell)),
            Err("Lillia - Dream Dance is a Spell, not a champion unit".into())
        );
        assert_eq!(draft.apply(Edit::SetChampion(padded)), Ok(Placed::Champion));
    }

    #[test]
    fn a_second_print_of_one_name_counts_toward_the_same_limit() {
        let mut draft = Draft::new("t", Origin::New);
        draft.apply(Edit::Add(poro())).unwrap();
        draft.apply(Edit::Add(poro())).unwrap();
        let alternate = ResolvedCard {
            name: "Lonely Poro (Alternate Art)".into(),
            riftbound_id: "sfd-036a-298".into(),
            ..poro()
        };
        assert_eq!(draft.apply(Edit::Add(alternate.clone())), Ok(Placed::Main));
        assert_eq!(
            draft.deck.main_deck[1].card.name, "Lonely Poro",
            "the name is folded at add"
        );
        assert_eq!(draft.copies().get("Lonely Poro"), Some(&3));
        assert!(draft.apply(Edit::Add(alternate)).is_err());
    }

    #[test]
    fn battlefields_cap_at_three_distinct_names() {
        let mut draft = Draft::new("t", Origin::New);
        draft
            .apply(Edit::Add(field("Seat of Power", "ogn-260-298")))
            .unwrap();
        assert_eq!(
            draft.apply(Edit::Add(field("Seat of Power", "ogn-260-298"))),
            Err("a second Seat of Power".into())
        );
        draft
            .apply(Edit::Add(field("Dusk Rose Lab", "sfd-215-298")))
            .unwrap();
        draft
            .apply(Edit::Add(field("Ravenbloom Conservatory", "sfd-217-298")))
            .unwrap();
        assert_eq!(
            draft.apply(Edit::Add(field("Rockfall Path", "ogn-261-298"))),
            Err("already 3 battlefields".into())
        );
        assert_eq!(draft.report.meter.battlefields, (3, 3));
    }

    #[test]
    fn set_count_zero_removes_and_the_sideboard_swaps_keep_the_pool() {
        let mut draft = Draft::new("t", Origin::New);
        draft.apply(Edit::Add(poro())).unwrap();
        draft.apply(Edit::Add(poro())).unwrap();
        assert_eq!(
            draft.apply(Edit::ToSideboard {
                id: "sfd-036-298".into()
            }),
            Ok(Placed::Sideboard)
        );
        assert_eq!(draft.deck.main_deck[0].count, 1);
        assert_eq!(draft.deck.sideboard[0].count, 1);
        assert_eq!(
            draft.apply(Edit::ToMain {
                id: "sfd-036-298".into()
            }),
            Ok(Placed::Main)
        );
        assert!(draft.deck.sideboard.is_empty());
        assert_eq!(
            draft.apply(Edit::SetCount {
                zone: Zone::Main,
                id: "sfd-036-298".into(),
                count: 0,
            }),
            Ok(Placed::Main)
        );
        assert!(draft.deck.main_deck.is_empty());
        assert!(draft
            .apply(Edit::ToMain {
                id: "sfd-036-298".into()
            })
            .is_err());
    }

    #[test]
    fn to_main_respects_the_copy_limit_including_the_champion() {
        let mut draft = Draft::new("t", Origin::New);
        draft.apply(Edit::SetChampion(champion())).unwrap();
        draft.apply(Edit::Add(champion())).unwrap();
        draft.apply(Edit::Add(champion())).unwrap();
        draft
            .apply(Edit::AddTo {
                zone: Zone::Sideboard,
                card: champion(),
            })
            .unwrap();
        assert_eq!(
            draft.apply(Edit::ToMain {
                id: "ogn-082-298".into()
            }),
            Err(copy_limit_reason("Lillia - Fae Fawn"))
        );
        assert_eq!(draft.deck.sideboard[0].count, 1, "the refusal restores");
    }

    #[test]
    fn change_print_keeps_the_count_and_the_canonical_name() {
        let mut draft = Draft::new("t", Origin::New);
        draft.apply(Edit::Add(poro())).unwrap();
        draft.apply(Edit::Add(poro())).unwrap();
        let alternate = ResolvedCard {
            name: "Lonely Poro (Alternate Art)".into(),
            riftbound_id: "sfd-036a-298".into(),
            ..poro()
        };
        assert_eq!(
            draft.apply(Edit::ChangePrint {
                zone: Zone::Main,
                id: "sfd-036-298".into(),
                card: alternate,
            }),
            Ok(Placed::Main)
        );
        assert_eq!(draft.deck.main_deck.len(), 1);
        assert_eq!(draft.deck.main_deck[0].count, 2);
        assert_eq!(draft.deck.main_deck[0].card.riftbound_id, "sfd-036a-298");
        assert_eq!(draft.deck.main_deck[0].card.name, "Lonely Poro");
        assert_eq!(
            draft.locate("lonely poro"),
            Some((Zone::Main, "sfd-036a-298".into()))
        );
    }

    #[test]
    fn undo_walks_back_and_the_ring_holds_thirty_two() {
        let mut draft = Draft::new("t", Origin::New);
        assert!(!draft.undo(), "nothing to undo on a fresh draft");
        for i in 0..40u32 {
            draft
                .apply(Edit::Add(rune("Calm Rune", &format!("r-{i}"), "Calm")))
                .unwrap();
        }
        assert_eq!(draft.undo.len(), UNDO_DEPTH);
        assert_eq!(draft.deck.runes.len(), 40);
        assert!(draft.undo());
        assert_eq!(draft.deck.runes.len(), 39);
        for _ in 0..31 {
            assert!(draft.undo());
        }
        assert!(!draft.undo());
        assert_eq!(draft.deck.runes.len(), 8);
        assert!(draft.dirty);
    }

    #[test]
    fn the_report_follows_every_edit() {
        let mut draft = Draft::new("t", Origin::New);
        assert_eq!(draft.report.verdict, Verdict::Broken(5));
        draft.apply(Edit::SetLegend(legend())).unwrap();
        assert!(draft.report.meter.legend);
        assert_eq!(draft.report.verdict, Verdict::Broken(4));
        assert_eq!(draft.identity(), vec!["Calm", "Mind"]);
        assert_eq!(draft.champion_tags(), vec!["Lillia"]);
        draft.apply(Edit::ClearLegend).unwrap();
        assert_eq!(draft.report.verdict, Verdict::Broken(5));
        assert!(draft.identity().is_empty());
    }

    #[test]
    fn a_no_op_edit_is_not_an_edit() {
        let mut draft = Draft::new("t", Origin::New);
        assert_eq!(draft.apply(Edit::ClearLegend), Ok(Placed::Legend));
        assert!(!draft.dirty);
        assert!(draft.undo.is_empty());
        draft.rename("  ");
        assert_eq!(draft.label, "t");
        assert!(!draft.dirty);
        draft.rename(" Lillia Aggro ");
        assert_eq!(draft.label, "Lillia Aggro");
        assert!(draft.dirty);
        assert_eq!(draft.edits, 1);
    }

    #[test]
    fn the_curve_counts_main_and_champion_by_energy_with_seven_plus_folded() {
        let mut draft = Draft::new("t", Origin::New);
        draft.apply(Edit::SetChampion(champion())).unwrap();
        draft.apply(Edit::Add(poro())).unwrap();
        draft.apply(Edit::Add(poro())).unwrap();
        draft
            .apply(Edit::Add(ResolvedCard {
                energy: Some(9),
                ..card("Singularity", "ogn-105-298", "Spell")
            }))
            .unwrap();
        draft
            .apply(Edit::Add(ResolvedCard {
                energy: None,
                ..card("Mystery", "x-1", "Gear")
            }))
            .unwrap();
        assert_eq!(draft.curve(), [0, 2, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn fill_runes_splits_the_shortfall_over_the_identity() {
        let mut draft = Draft::new("t", Origin::New);
        assert_eq!(
            draft.rune_fill(|_| None),
            Err("choose a legend first".into())
        );
        draft.apply(Edit::SetLegend(legend())).unwrap();
        draft
            .apply(Edit::Add(rune("Calm Rune", "ogn-042-298", "Calm")))
            .unwrap();
        let basic = |domain: &str| match domain {
            "Calm" => Some(rune("Calm Rune", "ogn-042-298", "Calm")),
            "Mind" => Some(rune("Mind Rune", "ogn-089-298", "Mind")),
            _ => None,
        };
        let fill = draft.rune_fill(basic).unwrap();
        assert_eq!(fill.len(), 2);
        assert_eq!((fill[0].0.name.as_str(), fill[0].1), ("Calm Rune", 6));
        assert_eq!((fill[1].0.name.as_str(), fill[1].1), ("Mind Rune", 5));
        assert_eq!(draft.apply(Edit::FillRunes(fill)), Ok(Placed::Runes));
        assert_eq!(draft.report.meter.runes, (12, 12));
        assert_eq!(draft.deck.runes.len(), 2);
        assert_eq!(draft.deck.runes[0].count, 7);
        assert!(draft.rune_fill(basic).is_err());
        assert_eq!(
            Draft::from_deck(draft.deck.clone(), "x", Origin::New)
                .rune_fill(|_| None)
                .unwrap_err(),
            "the rune deck already holds 12"
        );
        let mut short = Draft::new("t", Origin::New);
        short.apply(Edit::SetLegend(legend())).unwrap();
        assert_eq!(
            short.rune_fill(|domain| (domain == "Calm").then(|| rune("Calm Rune", "r", "Calm"))),
            Err("the catalog has no basic Mind rune".into())
        );
    }

    #[test]
    fn main_groups_split_by_kind_and_sort_by_energy_then_name() {
        let entries = vec![
            DeckEntry {
                card: ResolvedCard {
                    energy: Some(3),
                    ..card("Zephyr", "z", KIND_UNIT)
                },
                count: 1,
            },
            DeckEntry {
                card: ResolvedCard {
                    energy: Some(1),
                    ..card("aardvark", "a", KIND_UNIT)
                },
                count: 1,
            },
            DeckEntry {
                card: ResolvedCard {
                    energy: Some(1),
                    ..card("Beacon", "b", KIND_UNIT)
                },
                count: 1,
            },
            DeckEntry {
                card: card("Charm", "c", "Spell"),
                count: 2,
            },
        ];
        let groups = main_groups(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "Units");
        let names: Vec<&str> = groups[0]
            .1
            .iter()
            .map(|entry| entry.card.name.as_str())
            .collect();
        assert_eq!(names, ["aardvark", "Beacon", "Zephyr"]);
        assert_eq!(groups[1].0, "Spells");
        assert!(main_groups(&[]).is_empty());
    }

    #[test]
    fn the_back_rung_leaves_the_cards_pane_then_a_detail_then_the_sheet() {
        let mut draft = Draft::new("t", Origin::New);
        draft.pane = Pane::Cards;
        draft.browser.detail = Some(3);
        assert!(draft.back());
        assert_eq!(draft.browser.detail, None);
        assert_eq!(draft.pane, Pane::Cards);
        assert!(draft.back());
        assert_eq!(draft.pane, Pane::List);
        assert!(!draft.back());
    }

    #[test]
    fn shortfalls_map_to_a_browser_filter_and_card_findings_do_not() {
        use agni_importers::riftbound::catalog::CardKind;
        let legend = shortfall_filter(&Rule::LegendMissing).unwrap();
        assert!(legend.kinds.contains(&CardKind::Legend));
        let champion = shortfall_filter(&Rule::ChampionMissing).unwrap();
        assert!(champion.champion_only);
        assert!(shortfall_filter(&Rule::RuneCount { have: 3 })
            .unwrap()
            .kinds
            .contains(&CardKind::Rune));
        assert_eq!(
            shortfall_filter(&Rule::BattlefieldCount { have: 1 })
                .unwrap()
                .kinds
                .len(),
            1
        );
        assert_eq!(
            shortfall_filter(&Rule::MainSize { have: 12 })
                .unwrap()
                .kinds
                .len(),
            3
        );
        assert!(shortfall_filter(&Rule::CopyLimit {
            name: "x".into(),
            have: 4
        })
        .is_none());
    }

    #[test]
    fn a_draft_persists_one_second_after_the_last_change_and_clears_when_clean() {
        let mut state = PersistState::default();
        let mut draft = Draft::new("t", Origin::New);
        assert_eq!(persist_step(&mut state, Some(&draft), 0.5), None);
        draft.apply(Edit::Add(poro())).unwrap();
        assert_eq!(persist_step(&mut state, Some(&draft), 0.5), None);
        assert_eq!(persist_step(&mut state, Some(&draft), 0.5), None);
        assert_eq!(persist_step(&mut state, Some(&draft), 0.6), Some(true));
        assert_eq!(persist_step(&mut state, Some(&draft), 5.0), None);
        draft.apply(Edit::Add(poro())).unwrap();
        assert_eq!(persist_step(&mut state, Some(&draft), 5.0), None);
        assert_eq!(persist_step(&mut state, Some(&draft), 1.0), Some(true));
        draft.dirty = false;
        assert_eq!(persist_step(&mut state, Some(&draft), 0.1), Some(false));
        assert_eq!(persist_step(&mut state, None, 0.1), None);
    }

    #[test]
    fn open_puts_the_draft_in_the_editor_and_shows_the_editor_screen() {
        let mut editor = DeckEditor::default();
        let mut menu = Menu::default();
        open(
            &mut editor,
            &mut menu,
            Draft::new("Lillia", Origin::Pool("lillia-house".into())),
        );
        assert_eq!(menu.screen, Screen::DeckEditor);
        assert_eq!(menu.decks_from, Screen::Games);
        assert_eq!(
            editor.draft.as_ref().map(|draft| draft.label.as_str()),
            Some("Lillia")
        );
        assert_eq!(
            editor.draft.as_ref().map(|draft| draft.origin.clone()),
            Some(Origin::Pool("lillia-house".into()))
        );
        assert_eq!(editor.opens, 1);
        close(&mut editor, &mut menu);
        assert_eq!(menu.screen, Screen::Decks, "closing lands in the library");
        assert!(editor.draft.is_some(), "closing keeps the draft");
        menu.open_lobby(crate::net::TableGame::Riftbound);
        open(&mut editor, &mut menu, Draft::new("x", Origin::New));
        assert_eq!(editor.opens, 2);
        assert_eq!(
            menu.decks_from,
            Screen::Lobby(crate::net::TableGame::Riftbound)
        );
        reopen(&mut editor, &mut menu);
        assert_eq!(editor.opens, 3);
        assert_eq!(
            editor.draft.as_ref().map(|draft| draft.label.as_str()),
            Some("x"),
            "reopen keeps the draft"
        );
        close(&mut editor, &mut menu);
        assert_eq!(menu.screen, Screen::Decks);
        close(&mut editor, &mut menu);
        assert_eq!(menu.screen, Screen::Decks, "closing twice is harmless");
    }
}
