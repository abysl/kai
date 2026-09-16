use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardText {
    pub name: String,
    pub kind: String,
    pub domain: Vec<String>,
    pub energy: Option<i64>,
    pub power: Option<i64>,
    pub might: Option<i64>,
    pub text: String,
}

impl CardText {
    pub fn line(&self) -> String {
        let mut cost = Vec::new();
        if let Some(energy) = self.energy {
            cost.push(format!("{energy} energy"));
        }
        if let Some(power) = self.power {
            cost.push(format!("{power} power ({})", self.domain.join("/")));
        }
        if let Some(might) = self.might {
            cost.push(format!("{might} might"));
        }
        let text = self.text.replace('\n', " ");
        format!(
            "{} [{}{}]: {}",
            self.name,
            self.kind,
            if cost.is_empty() {
                String::new()
            } else {
                format!(", {}", cost.join(", "))
            },
            if text.is_empty() { "(no text)" } else { &text }
        )
    }
}

#[derive(Debug, Default)]
pub struct CardTexts {
    by_name: HashMap<String, CardText>,
}

impl CardTexts {
    pub fn load(store_dir: &Path) -> Self {
        let mut texts = Self::default();
        if let Ok(Some(manifest)) = agni_importers::riftbound::ingest::load_manifest(store_dir) {
            for card in &manifest.cards {
                texts.insert(CardText {
                    name: card.name.clone(),
                    kind: card.card_type.clone(),
                    domain: card.domain.clone(),
                    energy: card.energy,
                    power: card.power,
                    might: card.might,
                    text: card.text.clone(),
                });
            }
        }
        texts
    }

    pub fn from_pool(markdown: &str) -> Self {
        let mut texts = Self::default();
        for line in markdown.lines() {
            if let Some(card) = pool_line(line) {
                texts.insert(card);
            }
        }
        texts
    }

    pub fn pool() -> Self {
        let mut texts = Self::default();
        for (_, text) in crate::deck::pool::FILES {
            texts.absorb(Self::from_pool(text));
        }
        texts
    }

    pub fn absorb(&mut self, other: Self) {
        for card in other.by_name.into_values() {
            self.insert(card);
        }
    }

    pub fn insert(&mut self, card: CardText) {
        self.by_name
            .entry(card.name.to_ascii_lowercase())
            .or_insert(card);
    }

    pub fn get(&self, name: &str) -> Option<&CardText> {
        self.by_name.get(&name.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    pub fn reference(&self, names: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut lines = Vec::new();
        for name in names {
            if name.is_empty() || !seen.insert(name.to_ascii_lowercase()) {
                continue;
            }
            match self.get(&name) {
                Some(card) => lines.push(card.line()),
                None => lines.push(format!("{name}: (text unknown)")),
            }
        }
        lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coaching {
    pub slug: String,
    pub label: String,
    pub legend: String,
    pub lines: Vec<(String, String)>,
}

pub const COACHING_HEADING: &str = "## Coaching";

fn coaching_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("- **")?;
    let (name, rest) = rest.split_once("**")?;
    let advice = rest.trim_start_matches([' ', ':', '—', '-']).trim();
    (!name.is_empty() && !advice.is_empty()).then(|| (name.to_string(), advice.to_string()))
}

pub fn coaching_of(text: &str) -> Vec<(String, String)> {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            inside = line.trim() == COACHING_HEADING;
            continue;
        }
        if inside {
            lines.extend(coaching_line(line));
        }
    }
    lines
}

pub fn coaching() -> Vec<Coaching> {
    crate::deck::pool::decks()
        .into_iter()
        .filter_map(|deck| {
            let text = crate::deck::pool::FILES
                .iter()
                .find(|(slug, _)| *slug == deck.slug)
                .map(|(_, text)| *text)?;
            Some(Coaching {
                slug: deck.slug,
                label: deck.label,
                legend: deck.legend,
                lines: coaching_of(text),
            })
        })
        .filter(|coaching| !coaching.lines.is_empty())
        .collect()
}

pub fn coaching_for(own_label: Option<&str>, legends: &[String]) -> Vec<Coaching> {
    let all = coaching();
    let own = own_label.and_then(|label| all.iter().find(|deck| deck.label == label).cloned());
    let mut chosen: Vec<Coaching> = own.into_iter().collect();
    for legend in legends {
        let taken = chosen
            .iter()
            .any(|deck| deck.legend.eq_ignore_ascii_case(legend));
        if taken {
            continue;
        }
        if let Some(deck) = all
            .iter()
            .find(|deck| deck.legend.eq_ignore_ascii_case(legend))
        {
            chosen.push(deck.clone());
        }
    }
    chosen
}

pub fn coaching_text(decks: &[Coaching]) -> String {
    let mut text = String::new();
    for deck in decks {
        text.push_str(&format!("## Coaching for {}\n", deck.label));
        for (card, advice) in &deck.lines {
            text.push_str(&format!("- {card}: {advice}\n"));
        }
    }
    text
}

fn pool_line(line: &str) -> Option<CardText> {
    let rest = line.strip_prefix("- **")?;
    let (name, rest) = rest.split_once("** (")?;
    let (head, text) = rest.split_once("): ")?;
    let mut parts = head.split(';').map(str::trim);
    let _id = parts.next()?;
    let kind = parts.next()?.split('/').next()?.trim().to_string();
    let domain: Vec<String> = parts
        .next()?
        .split('/')
        .map(|domain| domain.trim().to_string())
        .filter(|domain| !domain.is_empty() && domain != "Colorless")
        .collect();
    let mut card = CardText {
        name: name.to_string(),
        kind,
        domain,
        energy: None,
        power: None,
        might: None,
        text: plain_icons(text.trim()),
    };
    for token in parts.next().unwrap_or("").split_whitespace() {
        let (digits, suffix) = token.split_at(token.len().saturating_sub(1));
        let value = digits.parse::<i64>().ok();
        match suffix {
            "E" => card.energy = value,
            "P" => card.power = value,
            "M" => card.might = value,
            _ => {}
        }
    }
    Some(card)
}

fn plain_icons(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(":rb_") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(':') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let icon = &after[..end];
        out.push_str(&icon_word(icon));
        rest = &after[end + 1..];
        if rest.starts_with(":rb_") {
            out.push(' ');
        }
    }
    out.push_str(rest);
    out.replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace(".)", ".) ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn icon_word(icon: &str) -> String {
    let body = icon.strip_prefix("rb_").unwrap_or(icon);
    if let Some(n) = body.strip_prefix("energy_") {
        return format!("{n} energy");
    }
    if let Some(domain) = body.strip_prefix("rune_") {
        return match domain {
            "rainbow" => "any rune".to_string(),
            other => format!("{other} rune"),
        };
    }
    match body {
        "might" => "might".to_string(),
        "exhaust" => "exhaust".to_string(),
        other => other.replace('_', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_line_carries_kind_cost_and_text() {
        let mut texts = CardTexts::default();
        texts.insert(CardText {
            name: "Vi - Destructive".into(),
            kind: "Unit".into(),
            domain: vec!["Fury".into()],
            energy: Some(2),
            power: Some(1),
            might: Some(3),
            text: "[Ganking]\nRecycle 1 from your trash: +1 might.".into(),
        });
        let lines = texts.reference([
            "Vi - Destructive".to_string(),
            "vi - destructive".into(),
            "Nobody".into(),
        ]);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with(
            "Vi - Destructive [Unit, 2 energy, 1 power (Fury), 3 might]: [Ganking] Recycle"
        ));
        assert_eq!(lines[1], "Nobody: (text unknown)");
    }

    #[test]
    fn the_pool_file_yields_card_texts_with_the_icons_spelled_out() {
        let texts = CardTexts::pool();
        assert!(texts.len() >= 40, "{} cards", texts.len());
        let lillia = texts.get("Lillia - Bashful Bloom").unwrap();
        assert_eq!(lillia.kind, "Legend");
        assert_eq!(lillia.domain, ["Calm", "Mind"]);
        assert!(lillia
            .text
            .starts_with("4 energy, exhaust: Play a ready 3 might Sprite"));
        assert!(!lillia.text.contains(":rb_"));
        let fawn = texts.get("lillia - fae fawn").unwrap();
        assert_eq!(
            (fawn.energy, fawn.power, fawn.might),
            (Some(3), None, Some(3))
        );
        assert!(
            fawn.text.contains("pay 1 energy mind rune as"),
            "{}",
            fawn.text
        );
        let defy = texts.get("Defy").unwrap();
        assert_eq!((defy.energy, defy.power), (Some(1), Some(1)));
        assert!(defy
            .text
            .contains("no more than 4 energy and no more than any rune"));
        let rockfall = texts.get("Rockfall Path").unwrap();
        assert!(rockfall.domain.is_empty());
        assert_eq!(rockfall.text, "Units can't be played here.");
        let mut merged = CardTexts::default();
        merged.insert(CardText {
            name: "Defy".into(),
            text: "from the store".into(),
            ..CardText::default()
        });
        merged.absorb(texts);
        assert_eq!(merged.get("defy").unwrap().text, "from the store");
        assert!(merged.get("Charm").is_some());
    }

    #[test]
    fn the_pool_union_covers_every_face_and_spells_the_m9_keywords_as_words() {
        let texts = CardTexts::pool();
        let faces = crate::deck::pool::cards();
        assert!(faces.len() >= 100, "{} faces", faces.len());
        for face in &faces {
            let card = texts
                .get(&face.name)
                .unwrap_or_else(|| panic!("{} has a reference line", face.name));
            assert!(!card.text.contains(":rb_"), "{}: {}", face.name, card.text);
            assert!(!card.text.contains("&gt;") && !card.text.contains("&quot;"));
        }
        assert_eq!(texts.len(), faces.len(), "first file wins, nothing doubled");
        let yi = texts.get("Master Yi - Tempered").unwrap();
        assert!(yi.text.contains("[Hunt 2]") && yi.text.contains("[Level 6][>]"));
        let rengar = texts.get("Rengar - Trophy Hunter").unwrap();
        assert!(rengar.text.starts_with("[Ambush]"));
        assert_eq!(rengar.domain, ["Body"]);
        let onslaught = texts.get("Onslaught").unwrap();
        assert!(onslaught.text.contains("[Flow] 4 energy"));
        let breath = texts.get("Bellows Breath").unwrap();
        assert!(breath.text.contains("[Repeat] 1 energy mind rune"));
        let disciple = texts.get("Shadow Order Disciple").unwrap();
        assert!(disciple.text.contains("[Burn 1]"));
        let nasus = texts.get("Nasus, Ascended").unwrap();
        assert!(nasus.text.contains("[Empower] 8 energy") && nasus.text.contains("[Empowered][>]"));
        let akshan = texts.get("Akshan - Mischievous").unwrap();
        assert!(akshan.text.starts_with("[Weaponmaster]"));
        assert!(akshan
            .text
            .contains("pay body rune body rune as an additional cost"));
        let zed = texts.get("Zed, Without a Sound").unwrap();
        assert!(zed.text.contains("(It has \"When I attack"), "{}", zed.text);
        let kha = texts.get("Kha'Zix - Voidreaver").unwrap();
        assert!(kha.text.contains("Spend 1 XP, exhaust: [Buff] a unit."));
    }

    #[test]
    fn every_pool_deck_carries_coaching_for_its_own_cards_in_lines_no_parser_mistakes_for_cards() {
        let decks = coaching();
        let all = crate::deck::pool::decks();
        assert_eq!(decks.len(), all.len(), "every pool file coaches");
        for deck in &decks {
            let (_, text) = crate::deck::pool::FILES
                .iter()
                .find(|(slug, _)| *slug == deck.slug)
                .unwrap();
            let names: Vec<String> = text
                .lines()
                .filter_map(pool_line)
                .map(|card| card.name)
                .collect();
            assert!(!deck.lines.is_empty(), "{} coaches something", deck.slug);
            for (card, advice) in &deck.lines {
                assert!(
                    names.contains(card),
                    "{}: {card} is a card of the file",
                    deck.slug
                );
                assert!(
                    !advice.contains("): ") && !advice.contains("** ("),
                    "{advice}"
                );
                assert!(!advice.is_empty() && advice.len() < 400);
            }
            let coaching_start = text.find(COACHING_HEADING).unwrap();
            assert!(
                text[coaching_start..]
                    .lines()
                    .all(|line| pool_line(line).is_none()),
                "{}: no coaching line reads as a card line",
                deck.slug
            );
            assert!(
                text[coaching_start + COACHING_HEADING.len()..]
                    .find("\n## ")
                    .is_none(),
                "{}: the coaching section is the last",
                deck.slug
            );
        }
        let lillia = decks
            .iter()
            .filter(|deck| deck.legend == "Lillia - Bashful Bloom")
            .count();
        assert_eq!(lillia, 2, "two Lillia decks share a legend");
        let mine = coaching_for(
            Some("Lillia (Jonnynick)"),
            &[
                "Lillia - Bashful Bloom".into(),
                "Kha'Zix - Voidreaver".into(),
            ],
        );
        let labels: Vec<&str> = mine.iter().map(|deck| deck.label.as_str()).collect();
        assert_eq!(labels, ["Lillia (Jonnynick)", "Kha'Zix (Hotkee)"]);
        let theirs = coaching_for(None, &["Lillia - Bashful Bloom".into()]);
        assert_eq!(theirs.len(), 1);
        assert_eq!(
            theirs[0].label, "Lillia (house)",
            "no label: the first Lillia file"
        );
        assert!(coaching_for(Some("Jinx"), &["Jinx".into()]).is_empty());
        let text = coaching_text(&mine);
        assert!(text.starts_with("## Coaching for Lillia (Jonnynick)\n- "));
        assert!(text.contains("## Coaching for Kha'Zix (Hotkee)\n- Kha'Zix - Voidreaver: "));
    }
}
