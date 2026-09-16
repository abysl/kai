use agni_riftbound::ResolvedDeck;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_TURN_CAP: u32 = 40;
pub const DEFAULT_GAMES: u32 = 20;
pub const MAX_REFUSALS: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail")]
pub enum Ending {
    Winner,
    EngineFault(String),
    Stuck(String),
    TurnCap,
}

impl Ending {
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::EngineFault(_) | Self::Stuck(_))
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Winner => "winner",
            Self::EngineFault(_) => "engine fault",
            Self::Stuck(_) => "stuck",
            Self::TurnCap => "turn cap",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::EngineFault(text) | Self::Stuck(text) => text,
            _ => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRecord {
    pub game: u32,
    pub seed: u64,
    pub game_seed: u64,
    pub turn_cap: u32,
    pub engine: String,
    pub plugin: String,
    pub decks: [String; 2],
    pub brains: [String; 2],
    pub first: u8,
    pub winner: Option<u8>,
    pub points: [i32; 2],
    pub turns: u32,
    pub entries: u64,
    pub millis: u64,
    pub ending: Ending,
    #[serde(default)]
    pub quiet: u32,
    #[serde(default)]
    pub brain_calls: u32,
}

impl GameRecord {
    pub fn replay_flags(&self) -> String {
        format!(
            "--seed {} --turn-cap {} --engine {}",
            self.seed, self.turn_cap, self.engine
        )
    }

    pub fn first_deck(&self) -> &str {
        &self.decks[usize::from(self.first)]
    }

    pub fn winner_deck(&self) -> Option<&str> {
        self.winner
            .map(|seat| self.decks[usize::from(seat)].as_str())
    }

    pub fn loser_points(&self) -> Option<i32> {
        self.winner.map(|seat| self.points[usize::from(1 - seat)])
    }

    pub fn line(&self) -> String {
        let outcome = match self.winner_deck() {
            Some(deck) => format!(
                "{deck} wins {}-{}",
                self.points[usize::from(self.winner.unwrap_or(0))],
                self.loser_points().unwrap_or(0)
            ),
            None => format!(
                "{} ({}-{})",
                self.ending.label(),
                self.points[0],
                self.points[1]
            ),
        };
        format!(
            "game {}: {} (seat 0) vs {} (seat 1), {} first → {} in {} turns, {} entries, {:.1}s",
            self.game,
            self.decks[0],
            self.decks[1],
            self.first_deck(),
            outcome,
            self.turns,
            self.entries,
            self.millis as f64 / 1000.0
        )
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeckTally {
    pub games: u32,
    pub wins: u32,
    pub wins_first: u32,
    pub wins_second: u32,
    pub games_first: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub games: u32,
    pub decided: u32,
    pub first_wins: u32,
    pub second_wins: u32,
    pub decks: BTreeMap<String, DeckTally>,
    pub turns: u64,
    pub entries: u64,
    pub millis: u64,
    pub failures: Vec<(u32, Ending)>,
    pub capped: Vec<u32>,
    pub unplayable: Vec<(u32, String)>,
    pub replay: Option<String>,
}

impl Summary {
    pub fn of(records: &[GameRecord]) -> Self {
        Self::of_outcomes(records, &[])
    }

    pub fn of_outcomes(records: &[GameRecord], unplayable: &[(u32, String)]) -> Self {
        let mut summary = Self {
            unplayable: unplayable.to_vec(),
            replay: records.first().map(GameRecord::replay_flags),
            ..Self::default()
        };
        for record in records {
            summary.games += 1;
            summary.turns += u64::from(record.turns);
            summary.entries += record.entries;
            summary.millis += record.millis;
            for (seat, deck) in record.decks.iter().enumerate() {
                let tally = summary.decks.entry(deck.clone()).or_default();
                tally.games += 1;
                if seat == usize::from(record.first) {
                    tally.games_first += 1;
                }
            }
            if let Some(winner) = record.winner {
                summary.decided += 1;
                let tally = summary
                    .decks
                    .entry(record.decks[usize::from(winner)].clone())
                    .or_default();
                tally.wins += 1;
                if winner == record.first {
                    tally.wins_first += 1;
                    summary.first_wins += 1;
                } else {
                    tally.wins_second += 1;
                    summary.second_wins += 1;
                }
            }
            match &record.ending {
                Ending::Winner => {}
                Ending::TurnCap => summary.capped.push(record.game),
                failure => summary.failures.push((record.game, failure.clone())),
            }
        }
        summary
    }

    pub fn failed(&self) -> bool {
        !self.failures.is_empty() || !self.unplayable.is_empty()
    }

    fn replay_hint(&self, game: u32) -> String {
        match &self.replay {
            Some(flags) => format!("replay with {flags} --start {game} --games 1"),
            None => format!("replay with --start {game} --games 1"),
        }
    }

    fn average(total: u64, count: u32) -> f64 {
        if count == 0 {
            0.0
        } else {
            total as f64 / f64::from(count)
        }
    }

    pub fn render(&self) -> String {
        let mut lines = vec![format!(
            "soak: {} games, {} decided, {} won going first, {} going second",
            self.games, self.decided, self.first_wins, self.second_wins
        )];
        for (deck, tally) in &self.decks {
            lines.push(format!(
                "  {deck}: {} wins in {} games ({} going first of {}, {} going second of {})",
                tally.wins,
                tally.games,
                tally.wins_first,
                tally.games_first,
                tally.wins_second,
                tally.games - tally.games_first
            ));
        }
        lines.push(format!(
            "  average {:.1} turns, {:.0} entries, {:.1}s per game",
            Self::average(self.turns, self.games),
            Self::average(self.entries, self.games),
            Self::average(self.millis, self.games) / 1000.0
        ));
        for game in &self.capped {
            lines.push(format!(
                "  game {game}: turn cap ({})",
                self.replay_hint(*game)
            ));
        }
        for (game, ending) in &self.failures {
            lines.push(format!(
                "  FAILURE game {game}: {} — {} ({})",
                ending.label(),
                ending.detail(),
                self.replay_hint(*game)
            ));
        }
        for (game, reason) in &self.unplayable {
            lines.push(format!(
                "  FAILURE game {game}: could not be set up — {reason}"
            ));
        }
        lines.push(if self.failed() {
            let mut counts = Vec::new();
            if !self.failures.is_empty() {
                counts.push(format!("{} game(s) faulted or stuck", self.failures.len()));
            }
            if !self.unplayable.is_empty() {
                counts.push(format!(
                    "{} game(s) could not be set up",
                    self.unplayable.len()
                ));
            }
            format!("soak FAILED: {}", counts.join(", "))
        } else {
            "soak passed: every game ended by a winner or the turn cap".to_string()
        });
        lines.join("\n")
    }
}

pub fn pool_deck_names() -> Vec<String> {
    crate::deck::pool::decks()
        .into_iter()
        .map(|deck| deck.legend)
        .collect()
}

pub fn in_domain_identity(card: &[String], legend: &[String]) -> bool {
    crate::deck::pool::in_domain_identity(card, legend)
}

pub fn pool_deck(which: &str) -> Option<ResolvedDeck> {
    crate::deck::pool::deck(which).ok()
}

pub fn turn_of(status: &[String]) -> Option<u32> {
    let first = status.first()?;
    let rest = first.strip_prefix("turn ")?;
    rest.split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        game: u32,
        decks: [&str; 2],
        first: u8,
        winner: Option<u8>,
        ending: Ending,
    ) -> GameRecord {
        GameRecord {
            game,
            seed: 1,
            game_seed: 1000 + u64::from(game),
            turn_cap: 40,
            engine: "native".into(),
            plugin: "bundled".into(),
            decks: [decks[0].into(), decks[1].into()],
            brains: ["random".into(), "random".into()],
            first,
            winner,
            points: match winner {
                Some(0) => [8, 3],
                Some(_) => [2, 8],
                None => [4, 4],
            },
            turns: 10 + game,
            entries: 100,
            millis: 500,
            ending,
            quiet: 0,
            brain_calls: 0,
        }
    }

    #[test]
    fn the_summary_counts_wins_per_deck_and_by_turn_order() {
        let records = vec![
            record(1, ["Lillia", "Irelia"], 0, Some(0), Ending::Winner),
            record(2, ["Irelia", "Lillia"], 0, Some(1), Ending::Winner),
            record(3, ["Lillia", "Irelia"], 1, Some(1), Ending::Winner),
            record(4, ["Irelia", "Lillia"], 1, None, Ending::TurnCap),
        ];
        let summary = Summary::of(&records);
        assert_eq!(summary.games, 4);
        assert_eq!(summary.decided, 3);
        assert_eq!((summary.first_wins, summary.second_wins), (2, 1));
        let lillia = &summary.decks["Lillia"];
        assert_eq!((lillia.games, lillia.wins), (4, 2));
        assert_eq!((lillia.wins_first, lillia.wins_second), (1, 1));
        assert_eq!(lillia.games_first, 2);
        let irelia = &summary.decks["Irelia"];
        assert_eq!((irelia.games, irelia.wins), (4, 1));
        assert_eq!((irelia.wins_first, irelia.wins_second), (1, 0));
        assert_eq!(summary.capped, vec![4]);
        assert!(!summary.failed());
        let text = summary.render();
        assert!(
            text.contains("Lillia: 2 wins in 4 games (1 going first of 2, 1 going second of 2)")
        );
        assert!(text.contains("average 12.5 turns, 100 entries, 0.5s per game"));
        assert!(text.contains(
            "game 4: turn cap (replay with --seed 1 --turn-cap 40 --engine native --start 4 --games 1)"
        ));
        assert!(text.ends_with("soak passed: every game ended by a winner or the turn cap"));
        assert_eq!(
            records[0].replay_flags(),
            "--seed 1 --turn-cap 40 --engine native"
        );
        assert_eq!(records[0].loser_points(), Some(3));
        assert_eq!(records[1].winner_deck(), Some("Lillia"));
        assert_eq!(records[3].first_deck(), "Lillia");
        assert!(records[0].line().starts_with("game 1: Lillia (seat 0) vs Irelia (seat 1), Lillia first → Lillia wins 8-3 in 11 turns"));
    }

    #[test]
    fn a_fault_or_a_stuck_prompt_fails_the_soak() {
        let records = vec![
            record(1, ["Lillia", "Irelia"], 0, Some(0), Ending::Winner),
            record(
                2,
                ["Irelia", "Lillia"],
                1,
                None,
                Ending::Stuck("nobody may act".into()),
            ),
            record(
                3,
                ["Lillia", "Irelia"],
                0,
                None,
                Ending::EngineFault("gas exhausted".into()),
            ),
        ];
        let summary = Summary::of(&records);
        assert!(summary.failed());
        assert_eq!(summary.failures.len(), 2);
        let text = summary.render();
        assert!(text.contains(
            "FAILURE game 2: stuck — nobody may act (replay with --seed 1 --turn-cap 40 --engine native --start 2 --games 1)"
        ));
        assert!(text.contains("FAILURE game 3: engine fault — gas exhausted"));
        assert!(text.ends_with("soak FAILED: 2 game(s) faulted or stuck"));
        let json = serde_json::to_string(&records[1]).unwrap();
        assert!(json.contains("\"ending\":{\"kind\":\"Stuck\",\"detail\":\"nobody may act\"}"));
        assert!(json.contains("\"game_seed\":1002"));
        assert!(json.contains("\"turn_cap\":40"));
        assert!(json.contains("\"engine\":\"native\""));
        assert!(json.contains("\"plugin\":\"bundled\""));
        let back: GameRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, records[1]);
    }

    #[test]
    fn a_game_that_could_not_be_set_up_fails_the_soak_and_says_so() {
        let played = vec![record(2, ["Lillia", "Irelia"], 0, Some(0), Ending::Winner)];
        let broken = vec![(1, "the roll for first player never settled".to_string())];
        let summary = Summary::of_outcomes(&played, &broken);
        assert!(summary.failed());
        assert!(summary.failures.is_empty());
        assert_eq!(summary.games, 1);
        let text = summary.render();
        assert!(text.contains(
            "FAILURE game 1: could not be set up — the roll for first player never settled"
        ));
        assert!(text.ends_with("soak FAILED: 1 game(s) could not be set up"));
        assert!(!text.contains("soak passed"));
        let nothing = Summary::of_outcomes(&[], &broken);
        assert!(nothing.failed());
        assert!(nothing
            .render()
            .ends_with("soak FAILED: 1 game(s) could not be set up"));
        let both = Summary::of_outcomes(
            &[record(
                3,
                ["Lillia", "Irelia"],
                0,
                None,
                Ending::Stuck("nobody may act".into()),
            )],
            &broken,
        );
        assert!(both
            .render()
            .ends_with("soak FAILED: 1 game(s) faulted or stuck, 1 game(s) could not be set up"));
    }

    #[test]
    fn the_pool_file_builds_both_decks() {
        assert_eq!(
            pool_deck_names()[..2],
            ["Lillia - Bashful Bloom", "Irelia - Blade Dancer"]
        );
        assert_eq!(
            pool_deck_names().len(),
            crate::deck::pool::FILES
                .iter()
                .filter(|(_, text)| crate::deck::pool::is_deck_file(text))
                .count()
        );
        let lillia = pool_deck("lillia").expect("the Lillia deck");
        assert_eq!(
            lillia
                .chosen_champion
                .as_ref()
                .map(|card| card.name.as_str()),
            Some("Lillia - Fae Fawn")
        );
        assert_eq!(lillia.battlefields.len(), 3);
        assert_eq!(
            lillia.runes.iter().map(|entry| entry.count).sum::<u32>(),
            12
        );
        assert!(lillia.main_deck.len() >= 20);
        assert!(lillia
            .main_deck
            .iter()
            .all(|entry| (1..=3).contains(&entry.count) && entry.card.kind.is_some()));
        let fawn = lillia
            .main_deck
            .iter()
            .find(|entry| entry.card.name == "Lillia - Fae Fawn")
            .unwrap();
        assert_eq!((fawn.card.energy, fawn.card.might), (Some(3), Some(3)));
        assert_eq!(fawn.card.domain, ["Mind"]);
        let irelia = pool_deck("Irelia").expect("the Irelia deck");
        assert_eq!(
            irelia
                .chosen_champion
                .as_ref()
                .map(|card| card.name.as_str()),
            Some("Irelia - Fervent")
        );
        let rune_domains: Vec<&str> = irelia
            .runes
            .iter()
            .map(|entry| entry.card.domain[0].as_str())
            .collect();
        assert_eq!(rune_domains, ["Calm", "Chaos"]);
        assert!(irelia
            .main_deck
            .iter()
            .any(|entry| entry.card.name == "Pyke - Returned" && entry.card.power.is_none()));
        let hero = irelia
            .main_deck
            .iter()
            .find(|entry| entry.card.name == "Akali, Silent")
            .is_none();
        assert!(hero, "Akali belongs to the Lillia section");
        assert!(pool_deck("jinx").is_none());
    }

    fn legal_constructed(deck: &ResolvedDeck) {
        let legend = deck.legend.as_ref().expect("a legend");
        let champion = deck.chosen_champion.as_ref().expect("a champion");
        let main: u32 = deck.main_deck.iter().map(|entry| entry.count).sum();
        assert_eq!(
            main as usize + 1,
            agni_riftbound::MAIN_DECK_SIZE,
            "{}: thirty-nine in the main deck, the champion in its zone",
            legend.name
        );
        let runes: u32 = deck.runes.iter().map(|entry| entry.count).sum();
        assert_eq!(
            runes as usize,
            agni_riftbound::RUNE_DECK_SIZE,
            "{}",
            legend.name
        );
        let battlefields: u32 = deck.battlefields.iter().map(|entry| entry.count).sum();
        assert_eq!(
            battlefields as usize,
            agni_riftbound::BATTLEFIELD_COUNT,
            "{}",
            legend.name
        );
        assert!(legend.kind.as_deref() == Some("Legend"));
        assert!(champion.kind.as_deref() == Some("Unit"));
        assert!(in_domain_identity(&champion.domain, &legend.domain));
        for entry in &deck.main_deck {
            assert!(
                in_domain_identity(&entry.card.domain, &legend.domain),
                "{} is outside {}'s domain identity",
                entry.card.name,
                legend.name
            );
            let with_champion = entry.count + u32::from(entry.card.name == champion.name);
            assert!(
                with_champion <= 3,
                "{} copies of {} in {}",
                with_champion,
                entry.card.name,
                legend.name
            );
        }
        for entry in &deck.runes {
            assert!(in_domain_identity(&entry.card.domain, &legend.domain));
        }
        let mut names: Vec<&str> = deck
            .main_deck
            .iter()
            .map(|entry| entry.card.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), deck.main_deck.len(), "no card is listed twice");
    }

    #[test]
    fn both_pool_decks_are_legal_constructed_decks() {
        let lillia = pool_deck("Lillia").unwrap();
        legal_constructed(&lillia);
        let irelia = pool_deck("Irelia").unwrap();
        legal_constructed(&irelia);
        assert!(
            !irelia
                .main_deck
                .iter()
                .any(|entry| entry.card.name == "Unsung Hero"),
            "an Order card stays out of a Calm/Chaos deck"
        );
        let count = |deck: &ResolvedDeck, name: &str| {
            deck.main_deck
                .iter()
                .find(|entry| entry.card.name == name)
                .map(|entry| entry.count)
        };
        assert_eq!(count(&lillia, "Unchecked Power"), Some(1));
        assert_eq!(count(&lillia, "Lillia - Fae Fawn"), Some(1));
        assert_eq!(count(&irelia, "Irelia - Fervent"), Some(1));
        assert_eq!(count(&irelia, "Treasure Hunter"), Some(3));
        assert_eq!(count(&irelia, "Rebuke"), Some(2));
        for held in crate::deck::pool::pinnable() {
            legal_constructed(&pool_deck(&held.slug).unwrap());
        }
        assert_eq!(
            pool_deck("nasus").unwrap().legend.unwrap().name,
            "Nasus - Curator of the Sands"
        );
    }
}
