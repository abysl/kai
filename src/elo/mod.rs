mod storage;
mod ui;

use serde::{Deserialize, Serialize};

pub use ui::Panel;

const INITIAL: i32 = 1200;
const K: f64 = 32.0;
const HISTORY_LIMIT: usize = 50;
const INPUT_ERROR: &str = "Enter a whole-number Elo from -10000 to 10000.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Outcome {
    Win,
    Loss,
    Draw,
}

impl Outcome {
    fn score(self) -> f64 {
        match self {
            Self::Win => 1.0,
            Self::Loss => 0.0,
            Self::Draw => 0.5,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Win => "win",
            Self::Loss => "loss",
            Self::Draw => "draw",
        }
    }
}

fn opponent_rating(text: &str) -> Result<i32, String> {
    text.trim()
        .parse::<i32>()
        .ok()
        .filter(|rating| (-10000..=10000).contains(rating))
        .ok_or_else(|| INPUT_ERROR.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    before: i32,
    opponent: i32,
    outcome: Outcome,
    after: i32,
}

impl Entry {
    fn new(before: i32, opponent: i32, outcome: Outcome) -> Result<Self, String> {
        opponent_rating(&opponent.to_string())?;
        let expected = 1.0 / (1.0 + 10_f64.powf((f64::from(opponent) - f64::from(before)) / 400.0));
        let delta = (K * (outcome.score() - expected)).round() as i32;
        let after = before.checked_add(delta).ok_or("Elo is out of range.")?;
        Ok(Self {
            before,
            opponent,
            outcome,
            after,
        })
    }

    fn delta(self) -> i32 {
        self.after - self.before
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct History {
    version: u8,
    baseline: i32,
    entries: Vec<Entry>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            version: 1,
            baseline: INITIAL,
            entries: Vec::new(),
        }
    }
}

impl History {
    fn rating(&self) -> i32 {
        self.entries
            .last()
            .map_or(self.baseline, |entry| entry.after)
    }

    fn record(&mut self, opponent: i32, outcome: Outcome) -> Result<Entry, String> {
        let entry = Entry::new(self.rating(), opponent, outcome)?;
        self.entries.push(entry);
        if self.entries.len() > HISTORY_LIMIT {
            self.baseline = self.entries.remove(0).after;
        }
        Ok(entry)
    }

    fn undo(&mut self) -> Option<Entry> {
        self.entries.pop()
    }

    fn decode(text: &str) -> Result<Self, String> {
        let history: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if history.version != 1 || history.entries.len() > HISTORY_LIMIT {
            return Err("Unsupported personal Elo history.".into());
        }
        let mut before = history.baseline;
        for entry in &history.entries {
            if entry.before != before
                || *entry != Entry::new(before, entry.opponent, entry.outcome)?
            {
                return Err("Personal Elo history is inconsistent.".into());
            }
            before = entry.after;
        }
        Ok(history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_elo_outcomes_use_the_pre_game_rating() {
        for (opponent, outcomes) in [
            (1200, [1216, 1184, 1200]),
            (1600, [1229, 1197, 1213]),
            (800, [1203, 1171, 1187]),
        ] {
            for (outcome, after) in [Outcome::Win, Outcome::Loss, Outcome::Draw]
                .into_iter()
                .zip(outcomes)
            {
                assert_eq!(Entry::new(1200, opponent, outcome).unwrap().after, after);
            }
        }
        assert_eq!(Entry::new(1216, 1200, Outcome::Win).unwrap().after, 1231);
        assert_eq!(Entry::new(0, 0, Outcome::Loss).unwrap().after, -16);
    }

    #[test]
    fn rounding_extremes_and_invalid_inputs() {
        assert_eq!(Entry::new(1200, 10000, Outcome::Win).unwrap().delta(), 32);
        assert_eq!(
            Entry::new(1200, -10000, Outcome::Loss).unwrap().delta(),
            -32
        );
        assert_eq!(Entry::new(1200, -10000, Outcome::Win).unwrap().delta(), 0);
        assert_eq!(opponent_rating(" 1200 ").unwrap(), 1200);
        assert_eq!(opponent_rating("-10000").unwrap(), -10000);
        assert_eq!(opponent_rating("10000").unwrap(), 10000);
        for text in [
            "",
            " ",
            "1200.5",
            "NaN",
            "inf",
            "1e3",
            "no",
            "10001",
            "-10001",
            "9999999999999",
        ] {
            assert!(opponent_rating(text).is_err(), "{text}");
        }
        let mut history = History::default();
        assert!(history.record(10001, Outcome::Win).is_err());
        assert_eq!(history, History::default());
    }

    #[test]
    fn undo_then_correct_recalculates_from_the_original_rating() {
        let mut history = History::default();
        assert_eq!(history.undo(), None);
        let first = history.record(1600, Outcome::Win).unwrap();
        history.record(1000, Outcome::Loss).unwrap();
        history.undo().unwrap();
        assert_eq!(history.rating(), first.after);
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.rating(), 1200);
        history.record(800, Outcome::Draw).unwrap();
        assert_eq!(history.rating(), 1187);
        assert_eq!(history.entries.len(), 1);
    }

    #[test]
    fn bounded_history_preserves_the_rating_and_undo_checkpoint() {
        let mut history = History::default();
        let checkpoint = history.record(1200, Outcome::Win).unwrap().after;
        for _ in 0..HISTORY_LIMIT {
            history.record(1600, Outcome::Draw).unwrap();
        }
        assert_eq!(history.entries.len(), HISTORY_LIMIT);
        let decoded = History::decode(&serde_json::to_string(&history).unwrap()).unwrap();
        assert_eq!(decoded, history);
        for _ in 0..HISTORY_LIMIT {
            history.undo().unwrap();
        }
        assert_eq!(history.rating(), checkpoint);
        assert_eq!(history.undo(), None);
    }

    #[test]
    fn unreadable_or_inconsistent_history_is_rejected() {
        assert!(History::decode("broken").is_err());
        let mut history = History::default();
        history.record(1200, Outcome::Win).unwrap();
        history.entries[0].after += 1;
        assert!(History::decode(&serde_json::to_string(&history).unwrap()).is_err());
        history.entries.clear();
        history.version = 2;
        assert!(History::decode(&serde_json::to_string(&history).unwrap()).is_err());
    }
}
