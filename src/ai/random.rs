use agni_sim::wire::{AffordanceKind, LegalKind, PluginView};

pub const PANIC_LABELS: [&str; 2] = ["free table", "confirm free table"];

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    pub fn secret(&mut self) -> [u8; 8] {
        self.next_u64().to_le_bytes()
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

pub fn derive_seed(base: u64, game: u32) -> u64 {
    let mut rng = Rng::seeded(base ^ (u64::from(game).wrapping_mul(0xD6E8_FEB8_6659_FD93)));
    rng.next_u64()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Action { index: usize, label: String },
    Move { card: u32, zone: u16, hidden: bool },
}

impl Choice {
    pub fn describe(&self, zone_name: &dyn Fn(u16) -> String) -> String {
        match self {
            Self::Action { index, label } => format!("do {} ({label})", index + 1),
            Self::Move {
                card,
                zone,
                hidden: false,
            } => format!("move {card} {}", zone_name(*zone)),
            Self::Move {
                card,
                zone,
                hidden: true,
            } => format!("hide {card} {}", zone_name(*zone)),
        }
    }
}

fn pressable(kind: AffordanceKind) -> bool {
    matches!(kind, AffordanceKind::Plain | AffordanceKind::Commit { .. })
}

pub fn actions(view: &PluginView) -> Vec<Choice> {
    view.shown()
        .filter(|(_, affordance)| {
            affordance.enabled
                && pressable(affordance.kind)
                && !PANIC_LABELS.contains(&affordance.label.as_str())
        })
        .map(|(index, affordance)| Choice::Action {
            index,
            label: affordance.label.clone(),
        })
        .collect()
}

pub fn moves(view: &PluginView) -> Vec<Choice> {
    let mut choices = Vec::new();
    for row in &view.legal {
        let moves = row.kinds.iter().any(|kind| {
            matches!(
                kind,
                LegalKind::Play { .. } | LegalKind::March | LegalKind::React | LegalKind::Answer
            )
        });
        if moves {
            for &zone in &row.zones {
                choices.push(Choice::Move {
                    card: row.card,
                    zone,
                    hidden: false,
                });
            }
        }
        if row.kinds.contains(&LegalKind::Hide) {
            for &zone in &row.hidden {
                choices.push(Choice::Move {
                    card: row.card,
                    zone,
                    hidden: true,
                });
            }
        }
    }
    choices
}

pub fn options(view: &PluginView, me: u8) -> Vec<Choice> {
    let mut choices = actions(view);
    let asked = view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == me);
    if asked {
        let gestures = moves(view)
            .into_iter()
            .filter(|choice| matches!(choice, Choice::Move { hidden: false, .. }));
        let answers: Vec<u32> = view
            .legal
            .iter()
            .filter(|row| row.kinds.contains(&LegalKind::Answer))
            .map(|row| row.card)
            .collect();
        choices.extend(gestures.filter(|choice| match choice {
            Choice::Move { card, .. } => answers.contains(card),
            Choice::Action { .. } => false,
        }));
    } else {
        choices.extend(moves(view));
    }
    choices
}

pub fn pick(rng: &mut Rng, options: &[Choice]) -> Option<Choice> {
    if options.is_empty() {
        return None;
    }
    Some(options[rng.below(options.len())].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Affordance, Legal, PromptSummary};
    use serde_bytes::ByteBuf;

    fn affordance(label: &str, enabled: bool, kind: AffordanceKind) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: None,
            enabled,
            kind,
            data: ByteBuf::from(vec![1]),
            card: None,
        }
    }

    fn strip() -> PluginView {
        PluginView {
            affordances: vec![
                affordance("end turn", true, AffordanceKind::Plain),
                affordance("free table", true, AffordanceKind::Plain),
                affordance("Lillia: play a Sprite", false, AffordanceKind::Plain),
                affordance("reveal", true, AffordanceKind::Reveal { roll: 1 }),
                affordance("roll", true, AffordanceKind::Commit { roll: 2 }),
                affordance("concede", true, AffordanceKind::Plain),
            ],
            hidden: vec![5],
            legal: vec![
                Legal {
                    card: 10,
                    kinds: vec![LegalKind::Play { accelerate: false }],
                    zones: vec![1, 2],
                    hidden: Vec::new(),
                },
                Legal {
                    card: 11,
                    kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
                    zones: vec![1],
                    hidden: vec![3],
                },
                Legal {
                    card: 12,
                    kinds: vec![LegalKind::March],
                    zones: vec![3, 4],
                    hidden: Vec::new(),
                },
                Legal {
                    card: 13,
                    kinds: vec![LegalKind::Activate { ability: 0 }],
                    zones: vec![],
                    hidden: Vec::new(),
                },
            ],
            ..PluginView::default()
        }
    }

    #[test]
    fn every_option_is_legal_and_every_legal_option_is_reached() {
        let view = strip();
        let options = options(&view, 0);
        let expected = vec![
            Choice::Action {
                index: 0,
                label: "end turn".into(),
            },
            Choice::Action {
                index: 4,
                label: "roll".into(),
            },
            Choice::Move {
                card: 10,
                zone: 1,
                hidden: false,
            },
            Choice::Move {
                card: 10,
                zone: 2,
                hidden: false,
            },
            Choice::Move {
                card: 11,
                zone: 1,
                hidden: false,
            },
            Choice::Move {
                card: 11,
                zone: 3,
                hidden: true,
            },
            Choice::Move {
                card: 12,
                zone: 3,
                hidden: false,
            },
            Choice::Move {
                card: 12,
                zone: 4,
                hidden: false,
            },
        ];
        assert_eq!(options, expected);
        let mut rng = Rng::seeded(7);
        let mut seen = vec![0usize; options.len()];
        for _ in 0..2000 {
            let choice = pick(&mut rng, &options).expect("options exist");
            let index = options
                .iter()
                .position(|option| *option == choice)
                .expect("a pick is one of the options");
            seen[index] += 1;
        }
        assert!(
            seen.iter().all(|&count| count > 150),
            "uniform enough: {seen:?}"
        );
        assert!(pick(&mut rng, &[]).is_none());
    }

    #[test]
    fn an_open_prompt_narrows_the_choice_to_its_answers() {
        let mut view = strip();
        view.prompt = Some(PromptSummary {
            seat: 0,
            why: "mulligan".into(),
            min: 0,
            max: 4,
            picked: 0,
            optional: false,
        });
        view.legal.push(Legal {
            card: 20,
            kinds: vec![LegalKind::Answer],
            zones: vec![1],
            hidden: Vec::new(),
        });
        let mine = options(&view, 0);
        assert_eq!(
            mine,
            vec![
                Choice::Action {
                    index: 0,
                    label: "end turn".into()
                },
                Choice::Action {
                    index: 4,
                    label: "roll".into()
                },
                Choice::Move {
                    card: 20,
                    zone: 1,
                    hidden: false
                },
            ]
        );
        let theirs = options(&view, 1);
        assert_eq!(theirs.len(), 9, "the other seat sees every row as a move");
    }

    #[test]
    fn every_hide_destination_the_presenter_names_is_tried() {
        let mut view = strip();
        view.legal[1].hidden = vec![3, 1, 2];
        let hides: Vec<u16> = moves(&view)
            .into_iter()
            .filter_map(|choice| match choice {
                Choice::Move {
                    card: 11,
                    zone,
                    hidden: true,
                } => Some(zone),
                _ => None,
            })
            .collect();
        assert_eq!(
            hides,
            vec![3, 1, 2],
            "the presenter is the authority on where a hide goes; a refusal would be counted, never hidden"
        );
    }

    #[test]
    fn the_seed_replays_and_games_differ() {
        let mut a = Rng::seeded(derive_seed(42, 3));
        let mut b = Rng::seeded(derive_seed(42, 3));
        let mut c = Rng::seeded(derive_seed(42, 4));
        let first: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let again: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        let other: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
        assert_eq!(first, again);
        assert_ne!(first, other);
        let mut items: Vec<u32> = (0..20).collect();
        a.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<u32>>());
        assert_ne!(items, sorted);
        assert_eq!(a.secret().len(), 8);
    }
}
