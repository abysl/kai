#[derive(Default)]
pub struct Batch {
    count: u32,
    revision: u64,
    last_press: f64,
}

pub fn shortcut(
    shift: bool,
    control: bool,
    other_modifier: bool,
    backspace: bool,
    z: bool,
    typing: bool,
) -> bool {
    !typing && !other_modifier && ((shift && !control && backspace) || (control && !shift && z))
}

impl Batch {
    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn invalidate(&mut self, revision: u64) {
        if self.revision != revision {
            self.count = 0;
        }
    }

    pub fn press(&mut self, now: f64, revision: u64, available: u32) {
        self.invalidate(revision);
        self.revision = revision;
        self.count = self.count.saturating_add(1).min(available);
        self.last_press = now;
    }

    pub fn take_due(&mut self, now: f64) -> Option<(u32, u64)> {
        if self.count == 0 || now - self.last_press < 1.0 {
            return None;
        }
        Some((std::mem::take(&mut self.count), self.revision))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_exclude_typing_plain_keys_and_other_modifier_combinations() {
        assert!(shortcut(true, false, false, true, false, false));
        assert!(shortcut(false, true, false, false, true, false));
        assert!(!shortcut(false, false, false, true, true, false));
        assert!(!shortcut(true, false, false, true, false, true));
        assert!(!shortcut(false, true, false, false, true, true));
        assert!(!shortcut(true, true, false, false, true, false));
        assert!(!shortcut(false, true, true, false, true, false));
    }

    #[test]
    fn repeated_presses_restart_the_full_second_and_send_once() {
        let mut batch = Batch::default();
        batch.press(0.0, 9, 10);
        batch.press(0.5, 9, 10);
        batch.press(1.0, 9, 10);
        assert_eq!(batch.take_due(1.999), None);
        assert_eq!(batch.take_due(2.0), Some((3, 9)));
        assert_eq!(batch.take_due(3.0), None);
    }

    #[test]
    fn history_changes_cancel_a_batch_and_counts_are_bounded() {
        let mut batch = Batch::default();
        batch.press(0.0, 9, 1);
        batch.press(0.1, 9, 1);
        assert_eq!(batch.count(), 1);
        batch.invalidate(10);
        assert_eq!(batch.take_due(2.0), None);
        batch.press(3.0, 10, 0);
        assert_eq!(batch.take_due(4.0), None);
    }
}
