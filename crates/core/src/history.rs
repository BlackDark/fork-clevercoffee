//! Temperature history for the web UI chart: one point every few seconds, about
//! half an hour in total.

use heapless::HistoryBuf;

/// Points kept.
pub const HISTORY_LEN: usize = 600;
/// Every n-th sample is kept (samples arrive once a second).
pub const HISTORY_EVERY: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub temperature: f32,
    pub setpoint: f32,
    pub heater_power: f32,
}

#[derive(Debug)]
pub struct History {
    points: HistoryBuf<Point, HISTORY_LEN>,
    skip: u32,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub const fn new() -> Self {
        Self {
            points: HistoryBuf::new(),
            skip: 0,
        }
    }

    /// Offers a sample; keeps every [`HISTORY_EVERY`]-th.
    pub fn offer(&mut self, p: Point) {
        self.skip += 1;
        if self.skip >= HISTORY_EVERY {
            self.skip = 0;
            self.points.write(p);
        }
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &Point> {
        self.points.oldest_ordered()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_every_third_and_wraps() {
        let mut h = History::new();
        for i in 0..(HISTORY_LEN as u32 * 3 + 30) {
            h.offer(Point {
                temperature: i as f32,
                setpoint: 0.0,
                heater_power: 0.0,
            });
        }
        assert_eq!(h.len(), HISTORY_LEN);
        let first = h.iter().next().unwrap().temperature;
        let last = h.iter().last().unwrap().temperature;
        assert_eq!(last - first, (HISTORY_LEN as f32 - 1.0) * 3.0);
    }
}
