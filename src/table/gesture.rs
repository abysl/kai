use crate::viewport::InputKind;
use bevy::prelude::*;

pub const TOUCH_SLOP: f32 = 8.0;
pub const MOUSE_SLOP: f32 = 4.0;
pub const LONG_PRESS_MS: u32 = 500;
pub const DOUBLE_TAP_MS: u32 = 300;
pub const DOUBLE_TAP_DIST: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Tap,
    DoubleTap,
    LongPress,
    DragStart,
    Drag,
    Drop,
    Cancel,
}

pub fn slop(kind: InputKind) -> f32 {
    match kind {
        InputKind::Touch => TOUCH_SLOP,
        InputKind::Pointer => MOUSE_SLOP,
    }
}

pub fn classify(press: Vec2, now: Vec2, held_ms: u32, moved_max: f32, kind: InputKind) -> Gesture {
    let moved = moved_max.max(now.distance(press));
    if moved > slop(kind) {
        Gesture::DragStart
    } else if held_ms >= LONG_PRESS_MS {
        Gesture::LongPress
    } else {
        Gesture::Tap
    }
}

pub fn is_double(prior: Option<(Vec2, u32)>, at: Vec2) -> bool {
    prior.is_some_and(|(there, ago_ms)| {
        ago_ms <= DOUBLE_TAP_MS && there.distance(at) <= DOUBLE_TAP_DIST
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Press {
    pub target: Entity,
    pub at: Vec2,
    pub since: f64,
    pub moved_max: f32,
    pub lifted: bool,
    pub long_fired: bool,
}

impl Press {
    pub fn held_ms(&self, now: f64) -> u32 {
        ((now - self.since).max(0.0) * 1000.0) as u32
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Tracker {
    pub press: Option<Press>,
    pub last_tap: Option<(Vec2, f64)>,
}

impl Tracker {
    pub fn press(&mut self, target: Entity, at: Vec2, now: f64) {
        self.press = Some(Press {
            target,
            at,
            since: now,
            moved_max: 0.0,
            lifted: false,
            long_fired: false,
        });
    }

    pub fn pressed(&self, target: Entity) -> Option<&Press> {
        self.press.as_ref().filter(|press| press.target == target)
    }

    pub fn moved(
        &mut self,
        target: Entity,
        at: Vec2,
        now: f64,
        kind: InputKind,
    ) -> Option<Gesture> {
        let press = self.press.as_mut().filter(|press| press.target == target)?;
        press.moved_max = press.moved_max.max(at.distance(press.at));
        if press.lifted {
            return Some(Gesture::Drag);
        }
        if press.long_fired {
            return None;
        }
        match classify(press.at, at, press.held_ms(now), press.moved_max, kind) {
            Gesture::DragStart => {
                press.lifted = true;
                Some(Gesture::DragStart)
            }
            _ => None,
        }
    }

    pub fn tick(&mut self, now: f64, kind: InputKind) -> Option<(Entity, Gesture)> {
        let press = self.press.as_mut()?;
        if press.lifted || press.long_fired {
            return None;
        }
        let held = press.held_ms(now);
        if classify(press.at, press.at, held, press.moved_max, kind) == Gesture::LongPress {
            press.long_fired = true;
            return Some((press.target, Gesture::LongPress));
        }
        None
    }

    pub fn release(&mut self, target: Entity, at: Vec2, now: f64) -> Option<Gesture> {
        self.pressed(target)?;
        let press = self.press.take()?;
        if press.lifted {
            return Some(Gesture::Drop);
        }
        if press.long_fired {
            return Some(Gesture::Cancel);
        }
        let prior = self
            .last_tap
            .map(|(there, then)| (there, ((now - then).max(0.0) * 1000.0) as u32));
        if is_double(prior, at) {
            self.last_tap = None;
            Some(Gesture::DoubleTap)
        } else {
            self.last_tap = Some((at, now));
            Some(Gesture::Tap)
        }
    }

    pub fn settle(&mut self, at: Vec2, now: f64) -> Option<Gesture> {
        let press = self.press.as_ref()?;
        if press.lifted {
            return None;
        }
        let target = press.target;
        self.release(target, at, now)
    }

    pub fn cancel(&mut self) -> Option<Gesture> {
        let press = self.press.take()?;
        press.lifted.then_some(Gesture::Cancel)
    }

    pub fn lifted(&self, target: Entity) -> bool {
        self.pressed(target).is_some_and(|press| press.lifted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Vec2 = Vec2::new(100.0, 100.0);

    fn card(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    #[test]
    fn the_classifier_table() {
        let table = [
            (7.0, 0, InputKind::Touch, Gesture::Tap),
            (9.0, 0, InputKind::Touch, Gesture::DragStart),
            (0.0, 450, InputKind::Touch, Gesture::Tap),
            (0.0, 500, InputKind::Touch, Gesture::LongPress),
            (7.0, 600, InputKind::Touch, Gesture::LongPress),
            (9.0, 600, InputKind::Touch, Gesture::DragStart),
            (3.0, 0, InputKind::Pointer, Gesture::Tap),
            (5.0, 0, InputKind::Pointer, Gesture::DragStart),
            (3.0, 500, InputKind::Pointer, Gesture::LongPress),
        ];
        for (moved, held, kind, want) in table {
            let now = ORIGIN + Vec2::new(moved, 0.0);
            assert_eq!(
                classify(ORIGIN, now, held, 0.0, kind),
                want,
                "{moved} pt after {held} ms on {kind:?}"
            );
            assert_eq!(
                classify(ORIGIN, ORIGIN, held, moved, kind),
                want,
                "the farthest point of the press counts, not where it rests"
            );
        }
        assert_eq!(slop(InputKind::Touch), 8.0);
        assert_eq!(slop(InputKind::Pointer), 4.0);
    }

    #[test]
    fn a_second_tap_within_the_window_and_the_distance_is_a_double_tap() {
        assert!(is_double(
            Some((ORIGIN, 299)),
            ORIGIN + Vec2::new(24.0, 0.0)
        ));
        assert!(!is_double(Some((ORIGIN, 301)), ORIGIN));
        assert!(!is_double(
            Some((ORIGIN, 100)),
            ORIGIN + Vec2::new(25.0, 0.0)
        ));
        assert!(!is_double(None, ORIGIN));
    }

    #[test]
    fn a_jittery_finger_stays_a_tap_and_a_real_pull_lifts_once() {
        let mut tracker = Tracker::default();
        tracker.press(card(1), ORIGIN, 0.0);
        assert_eq!(
            tracker.moved(
                card(1),
                ORIGIN + Vec2::new(5.0, 3.0),
                0.05,
                InputKind::Touch
            ),
            None,
            "5 pt of jitter is under the touch slop"
        );
        assert_eq!(
            tracker.release(card(1), ORIGIN + Vec2::new(4.0, 2.0), 0.1),
            Some(Gesture::Tap)
        );
        tracker.press(card(1), ORIGIN, 0.2);
        assert_eq!(
            tracker.release(card(1), ORIGIN, 0.25),
            Some(Gesture::DoubleTap),
            "the second tap within 300 ms fires the default action"
        );
        tracker.press(card(2), ORIGIN, 1.0);
        assert_eq!(
            tracker.moved(card(2), ORIGIN + Vec2::new(9.0, 0.0), 1.1, InputKind::Touch),
            Some(Gesture::DragStart)
        );
        assert_eq!(
            tracker.moved(
                card(2),
                ORIGIN + Vec2::new(40.0, 0.0),
                1.2,
                InputKind::Touch
            ),
            Some(Gesture::Drag)
        );
        assert!(tracker.lifted(card(2)));
        assert_eq!(
            tracker.tick(5.0, InputKind::Touch),
            None,
            "a lifted card never turns into a long press"
        );
        assert_eq!(
            tracker.release(card(2), ORIGIN + Vec2::new(40.0, 0.0), 1.3),
            Some(Gesture::Drop)
        );
        assert!(tracker.press.is_none());
    }

    #[test]
    fn a_still_press_becomes_a_long_press_once_and_its_release_is_swallowed() {
        let mut tracker = Tracker::default();
        tracker.press(card(3), ORIGIN, 0.0);
        assert_eq!(tracker.tick(0.45, InputKind::Touch), None);
        assert_eq!(
            tracker.tick(0.5, InputKind::Touch),
            Some((card(3), Gesture::LongPress))
        );
        assert_eq!(tracker.tick(0.6, InputKind::Touch), None, "fires once");
        assert_eq!(
            tracker.moved(
                card(3),
                ORIGIN + Vec2::new(30.0, 0.0),
                0.7,
                InputKind::Touch
            ),
            None,
            "after the long press a pull does not start a drag"
        );
        assert_eq!(
            tracker.release(card(3), ORIGIN, 0.8),
            Some(Gesture::Cancel),
            "the release after a long press is not a tap"
        );
        tracker.press(card(4), ORIGIN, 1.0);
        assert_eq!(
            tracker.moved(
                card(4),
                ORIGIN + Vec2::new(5.0, 0.0),
                1.1,
                InputKind::Pointer
            ),
            Some(Gesture::DragStart),
            "a mouse lifts past 4 pt"
        );
        assert_eq!(
            tracker.cancel(),
            Some(Gesture::Cancel),
            "escape mid-drag cancels the drop"
        );
        assert!(tracker.release(card(4), ORIGIN, 1.2).is_none());
    }

    #[test]
    fn the_pointer_going_up_settles_a_still_press_so_it_never_becomes_a_long_press() {
        let mut tracker = Tracker::default();
        tracker.press(card(7), ORIGIN, 0.0);
        assert_eq!(tracker.settle(ORIGIN, 0.1), Some(Gesture::Tap));
        assert!(tracker.press.is_none());
        assert_eq!(tracker.tick(1.0, InputKind::Touch), None);
        tracker.press(card(8), ORIGIN, 2.0);
        assert_eq!(
            tracker.moved(
                card(8),
                ORIGIN + Vec2::new(20.0, 0.0),
                2.1,
                InputKind::Touch
            ),
            Some(Gesture::DragStart)
        );
        assert_eq!(
            tracker.settle(ORIGIN + Vec2::new(20.0, 0.0), 2.2),
            None,
            "a lifted card is closed by its drag end, not by the pointer going up over another entity"
        );
        assert!(tracker.lifted(card(8)));
    }

    #[test]
    fn a_release_on_another_entity_does_not_close_the_press() {
        let mut tracker = Tracker::default();
        tracker.press(card(5), ORIGIN, 0.0);
        assert!(tracker.release(card(6), ORIGIN, 0.1).is_none());
        assert!(tracker.pressed(card(5)).is_some());
        assert!(tracker.pressed(card(6)).is_none());
        assert_eq!(tracker.release(card(5), ORIGIN, 0.2), Some(Gesture::Tap));
    }
}
