//! Standby countdown and the brew-timer display state.

use crate::config::Config;

/// The display goes dark this long after the machine entered standby.
pub const DISPLAY_OFF_AFTER_STANDBY_MS: u64 = 10 * 60 * 1000;

/// Counts down to standby. Every user action calls [`Standby::reset`].
#[derive(Debug, Clone, Default)]
pub struct Standby {
    started_ms: Option<u64>,
    remaining_ms: u64,
    display_remaining_ms: u64,
}

impl Standby {
    fn timeout_ms(cfg: &Config) -> u64 {
        (cfg.standby_time * 60_000.0) as u64
    }

    pub fn reset(&mut self, cfg: &Config, now_ms: u64) {
        if !cfg.standby_enabled {
            return;
        }
        self.started_ms = Some(now_ms);
        self.remaining_ms = Self::timeout_ms(cfg);
        self.display_remaining_ms = DISPLAY_OFF_AFTER_STANDBY_MS;
    }

    /// Starts counting if nothing started it yet.
    pub fn arm(&mut self, cfg: &Config, now_ms: u64) {
        if self.started_ms.is_none() {
            self.reset(cfg, now_ms);
        }
    }

    /// Forces standby on the next check (power switch off).
    pub fn expire(&mut self) {
        self.remaining_ms = 0;
    }

    pub fn update(&mut self, cfg: &Config, now_ms: u64) {
        let Some(start) = self.started_ms else { return };
        if !cfg.standby_enabled {
            return;
        }
        let elapsed = now_ms.saturating_sub(start);
        let timeout = Self::timeout_ms(cfg);
        if self.remaining_ms != 0 {
            self.remaining_ms = timeout.saturating_sub(elapsed);
        } else {
            self.display_remaining_ms = (timeout + DISPLAY_OFF_AFTER_STANDBY_MS).saturating_sub(elapsed);
        }
    }

    pub fn should_enter(&self, cfg: &Config) -> bool {
        cfg.standby_enabled && self.started_ms.is_some() && self.remaining_ms == 0
    }

    pub fn display_off(&self) -> bool {
        self.started_ms.is_some() && self.remaining_ms == 0 && self.display_remaining_ms == 0
    }

    pub fn remaining_ms(&self) -> u64 {
        self.remaining_ms
    }
}

/// Whether the brew time is on screen: while brewing and for a while after.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrewTimer {
    #[default]
    Idle,
    Running,
    PostBrew {
        since_ms: u64,
    },
}

impl BrewTimer {
    pub fn update(&mut self, brew_active: bool, cfg: &Config, now_ms: u64) {
        *self = match *self {
            BrewTimer::Idle if brew_active => BrewTimer::Running,
            BrewTimer::Running if !brew_active => BrewTimer::PostBrew { since_ms: now_ms },
            BrewTimer::PostBrew { since_ms }
                if now_ms.saturating_sub(since_ms) > (cfg.display_post_brew_timer_duration * 1000.0) as u64 =>
            {
                BrewTimer::Idle
            }
            BrewTimer::PostBrew { .. } if brew_active => BrewTimer::Running,
            s => s,
        };
    }

    pub fn visible(self) -> bool {
        self != BrewTimer::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config { standby_enabled: true, standby_time: 1.0, ..Config::default() }
    }

    #[test]
    fn standby_counts_down_and_triggers() {
        let c = cfg();
        let mut s = Standby::default();
        s.arm(&c, 0);
        s.update(&c, 30_000);
        assert_eq!(s.remaining_ms(), 30_000);
        assert!(!s.should_enter(&c));
        s.update(&c, 60_000);
        assert!(s.should_enter(&c));
        assert!(!s.display_off());
        s.update(&c, 60_000 + DISPLAY_OFF_AFTER_STANDBY_MS);
        assert!(s.display_off());
    }

    #[test]
    fn reset_restarts_the_countdown() {
        let c = cfg();
        let mut s = Standby::default();
        s.arm(&c, 0);
        s.update(&c, 50_000);
        s.reset(&c, 50_000);
        s.update(&c, 100_000);
        assert!(!s.should_enter(&c));
    }

    #[test]
    fn disabled_standby_never_triggers() {
        let c = Config::default();
        let mut s = Standby::default();
        s.arm(&c, 0);
        s.update(&c, u64::MAX / 2);
        assert!(!s.should_enter(&c));
    }

    #[test]
    fn brew_timer_lingers_after_the_shot() {
        let c = Config::default(); // 3 s post-brew
        let mut t = BrewTimer::Idle;
        t.update(true, &c, 0);
        assert_eq!(t, BrewTimer::Running);
        t.update(false, &c, 25_000);
        assert!(t.visible());
        t.update(false, &c, 27_000);
        assert!(t.visible());
        t.update(false, &c, 28_001);
        assert!(!t.visible());
    }
}
