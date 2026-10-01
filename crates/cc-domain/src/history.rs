//! The temperature-history ring.
//!
//! Owner: **R3-09** (the timeseries ring behind `GET /api/history`).
//!
//! # What it replaces
//!
//! `TemperatureHistory` in `src/network/WebServerManager.cpp:107-156`: a
//! fixed 600-point ring of `{currentTemp, targetTemp, heaterPower}` triples,
//! written from `sendTempEvent` (`:1134`) and rendered oldest-first by
//! `generateJson` (`:134-153`).
//!
//! # Why it is here and not in the HTTP layer
//!
//! The C++'s ring is a file-static next to the web server, which is possible
//! there because `Config::getInstance()` is a singleton the whole firmware
//! reads. Here the configuration and the machine live in the control task, and
//! no HTTP handler may reach into either (04 §3.2). So the ring is a **value**
//! with no dependencies at all — which also means the arithmetic that decides
//! what a client sees is host-testable, where a ring living in the httpd task
//! would only be testable on hardware.
//!
//! # The two numbers that are not arbitrary
//!
//! * [`CAPACITY`] = 600, the C++'s `HISTORY_SIZE`. At the C++'s cadence that is
//!   30 minutes of history.
//! * [`SKIP_INTERVAL`] = 2, the C++'s. `addPoint` drops two samples and keeps
//!   the third, so one point lands every three `sendTempEvent` calls. The UI
//!   assumes exactly that spacing — it reconstructs timestamps as
//!   `now - 3 * i` seconds (`CleverCoffeeContext.tsx:240-243`) — so the skip is
//!   part of the wire contract, not an optimisation.

/// How many points the ring keeps. `HISTORY_SIZE` (`WebServerManager.cpp:109`).
pub const CAPACITY: usize = 600;

/// How many samples are dropped between two kept points. `SKIP_INTERVAL`
/// (`WebServerManager.cpp:110`).
///
/// The C++ writes it as `if (++skipCounter <= SKIP_INTERVAL) return;`, so a
/// counter starting at zero keeps every third call. Reproduced exactly,
/// including the counter's reset to zero on the sample it keeps — an
/// off-by-one here changes the chart's time axis.
pub const SKIP_INTERVAL: u32 = 2;

/// One sample: the reading, the target, and the heater's output in per cent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// The measured temperature, °C.
    pub current_temp: f32,
    /// The setpoint, °C.
    pub target_temp: f32,
    /// The PID output, per cent.
    pub heater_power: f32,
}

impl Point {
    /// A point from three numbers.
    #[must_use]
    pub const fn new(current_temp: f32, target_temp: f32, heater_power: f32) -> Self {
        Self {
            current_temp,
            target_temp,
            heater_power,
        }
    }
}

/// The 600-point ring, oldest point first when read out.
///
/// The C++'s `currentIndex`/`valueCount` pair, kept as a write cursor and a
/// count rather than as a growable buffer: the ring is full-size from the first
/// instant, so adding a point never reallocates and a client reading it while
/// the control task writes it cannot see a half-built vector.
#[derive(Clone, Debug)]
pub struct History {
    points: [Point; CAPACITY],
    /// `currentIndex` — where the next point goes.
    next: usize,
    /// `valueCount` — how many points are actually in the ring.
    len: usize,
    /// `skipCounter` — samples seen since the last one kept.
    skip: u32,
}

impl History {
    /// An empty ring.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            points: [Point::new(0.0, 0.0, 0.0); CAPACITY],
            next: 0,
            len: 0,
            skip: 0,
        }
    }

    /// Offer a sample. Returns whether it was kept.
    ///
    /// The return value is not in the C++ — it has no caller that cares — but
    /// here it is what lets the caller log the ring filling up, and what lets a
    /// test assert the skip arithmetic without reading the buffer.
    pub fn push(&mut self, current_temp: f32, target_temp: f32, heater_power: f32) -> bool {
        // `if (++skipCounter <= SKIP_INTERVAL) return;` — note the increment
        // happens on the dropped samples too, which is the whole arithmetic.
        self.skip += 1;
        if self.skip <= SKIP_INTERVAL {
            return false;
        }
        self.skip = 0;

        self.points[self.next] = Point::new(current_temp, target_temp, heater_power);
        self.next = (self.next + 1) % CAPACITY;
        if self.len < CAPACITY {
            self.len += 1;
        }
        true
    }

    /// How many points are in the ring.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the ring has no points yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The oldest point's index.
    ///
    /// The C++ computes it inline in `generateJson` (`:144-146`) from the two
    /// cursors; it is the same arithmetic, named once so the reader and the
    /// writer cannot compute it differently.
    #[must_use]
    const fn oldest(&self) -> usize {
        if self.next >= self.len {
            self.next - self.len
        } else {
            CAPACITY - (self.len - self.next)
        }
    }

    /// The point `index` places from the oldest, or `None` past the end.
    ///
    /// Indexed access rather than a `Vec` because this crate is `no_std` with no
    /// `alloc` (04 §1), and because the only caller walks the ring three times
    /// to write three JSON columns — handing back a 7 KB copy to do that would
    /// be the larger cost.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Point> {
        if index >= self.len {
            return None;
        }
        let start = self.oldest();
        // Cannot overflow: `index < len <= CAPACITY` and `start < CAPACITY`.
        Some(self.points[(start + index) % CAPACITY])
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // The sample numbers here run to a few thousand, and `f32` represents every
    // integer below 2^24 exactly, so the casts lose nothing. The lint cannot
    // know that, and a per-cast `expect` would be noise on five lines.
    #![allow(
        clippy::cast_precision_loss,
        reason = "test sample values are far below f32's exact-integer range"
    )]

    use super::{History, CAPACITY, SKIP_INTERVAL};

    #[test]
    fn the_first_two_samples_are_dropped_and_the_third_is_kept() {
        // The C++'s counter: `++skipCounter <= 2` drops two, then resets.
        let mut h = History::new();
        assert!(!h.push(1.0, 0.0, 0.0));
        assert!(!h.push(2.0, 0.0, 0.0));
        assert!(h.push(3.0, 0.0, 0.0));
        assert_eq!(h.len(), 1);
        assert_eq!(h.get(0).map(|p| p.current_temp), Some(3.0));
    }

    #[test]
    fn one_point_lands_every_third_call_forever() {
        // The UI reconstructs timestamps as `now - 3 * i` seconds, so the
        // spacing has to be exactly three or the chart's x axis lies.
        let mut h = History::new();
        for i in 0..30 {
            h.push(i as f32, 0.0, 0.0);
        }
        assert_eq!(h.len(), 10);
        assert_eq!(h.get(0).map(|p| p.current_temp), Some(2.0));
        assert_eq!(h.get(9).map(|p| p.current_temp), Some(29.0));
        assert!(h.get(10).is_none());
    }

    #[test]
    fn the_ring_wraps_and_keeps_the_newest_six_hundred() {
        let mut h = History::new();
        // 4 x CAPACITY samples keeps 4/3 x CAPACITY points, so the ring wraps
        // and has to evict.
        for i in 0..(CAPACITY * 4) {
            h.push(i as f32, 0.0, 0.0);
        }
        assert_eq!(h.len(), CAPACITY);
        // The kept samples are 2, 5, 8, ... — the third of every three — so the
        // oldest surviving point is the (n - CAPACITY + 1)-th of them.
        let kept = (CAPACITY * 4) / (SKIP_INTERVAL as usize + 1);
        let nth = |k: usize| (3 * k - 1) as f32;
        assert_eq!(
            h.get(0).map(|p| p.current_temp),
            Some(nth(kept - CAPACITY + 1))
        );
        assert_eq!(h.get(CAPACITY - 1).map(|p| p.current_temp), Some(nth(kept)));
        assert!(h.get(CAPACITY).is_none());
    }

    #[test]
    fn a_half_full_ring_reads_oldest_first() {
        let mut h = History::new();
        for i in 0..9 {
            h.push(i as f32, 0.0, 0.0);
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.get(0).map(|p| p.current_temp), Some(2.0));
        assert_eq!(h.get(1).map(|p| p.current_temp), Some(5.0));
        assert_eq!(h.get(2).map(|p| p.current_temp), Some(8.0));
    }

    #[test]
    fn an_empty_ring_reports_empty_rather_than_zeroes() {
        // The reason the C++'s `valueCount` exists: a chart that draws 600
        // zeroes is a chart of nothing, and it looks like a flatlined boiler.
        let h = History::new();
        assert!(h.is_empty());
        assert!(h.get(0).is_none());
    }
}
