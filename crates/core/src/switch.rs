//! Debounced switch with long-press detection (port of the C++ `IOSwitch`).

use crate::config::{SwitchMode, SwitchType};

/// Level must be stable this long before it counts.
pub const DEBOUNCE_MS: u64 = 20;
/// A momentary switch held this long is a long press.
pub const LONG_PRESS_MS: u64 = 500;

/// One physical switch. Feed it the raw GPIO level every control tick.
#[derive(Debug, Clone)]
pub struct Switch {
    kind: SwitchType,
    mode: SwitchMode,
    last_raw: bool,
    raw_since_ms: u64,
    pressed: bool,
    pressed_since_ms: u64,
}

impl Switch {
    pub fn new(kind: SwitchType, mode: SwitchMode) -> Self {
        // Start from the resting level so a normally-closed switch does not read as a
        // press at boot.
        let rest = matches!(mode, SwitchMode::NormallyClosed);
        Self { kind, mode, last_raw: rest, raw_since_ms: 0, pressed: false, pressed_since_ms: 0 }
    }

    pub fn kind(&self) -> SwitchType {
        self.kind
    }

    /// Updates from the raw pin level (`true` = high) and returns the debounced state.
    pub fn update(&mut self, raw_high: bool, now_ms: u64) -> bool {
        if raw_high != self.last_raw {
            self.last_raw = raw_high;
            self.raw_since_ms = now_ms;
        }
        if now_ms.saturating_sub(self.raw_since_ms) > DEBOUNCE_MS {
            let active = raw_high ^ matches!(self.mode, SwitchMode::NormallyClosed);
            if active != self.pressed {
                self.pressed = active;
                if active {
                    self.pressed_since_ms = now_ms;
                }
            }
        }
        self.pressed
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    /// Held for [`LONG_PRESS_MS`]. Toggle switches never long-press.
    pub fn is_long_press(&self, now_ms: u64) -> bool {
        self.kind == SwitchType::Momentary
            && self.pressed
            && now_ms.saturating_sub(self.pressed_since_ms) >= LONG_PRESS_MS
    }

    /// How long the current press lasts so far.
    pub fn held_ms(&self, now_ms: u64) -> u64 {
        if self.pressed { now_ms.saturating_sub(self.pressed_since_ms) } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(s: &mut Switch, level: bool, from: u64, to: u64) -> bool {
        let mut r = false;
        for t in (from..=to).step_by(10) {
            r = s.update(level, t);
        }
        r
    }

    #[test]
    fn ignores_bounces_shorter_than_debounce() {
        let mut s = Switch::new(SwitchType::Momentary, SwitchMode::NormallyOpen);
        assert!(!s.update(true, 1000));
        assert!(!s.update(false, 1010));
        assert!(!s.update(true, 1015));
        assert!(!s.update(true, 1030));
        assert!(s.update(true, 1040));
    }

    #[test]
    fn normally_closed_inverts() {
        let mut s = Switch::new(SwitchType::Toggle, SwitchMode::NormallyClosed);
        assert!(!feed(&mut s, true, 0, 100));
        assert!(feed(&mut s, false, 110, 200));
    }

    #[test]
    fn long_press_only_for_momentary() {
        let mut m = Switch::new(SwitchType::Momentary, SwitchMode::NormallyOpen);
        feed(&mut m, true, 0, 600);
        assert!(m.is_long_press(600));
        let mut t = Switch::new(SwitchType::Toggle, SwitchMode::NormallyOpen);
        feed(&mut t, true, 0, 600);
        assert!(!t.is_long_press(600));
    }

    #[test]
    fn short_press_is_not_long() {
        let mut m = Switch::new(SwitchType::Momentary, SwitchMode::NormallyOpen);
        feed(&mut m, true, 0, 300);
        assert!(!m.is_long_press(300));
        assert_eq!(m.held_ms(300), 270);
    }
}
