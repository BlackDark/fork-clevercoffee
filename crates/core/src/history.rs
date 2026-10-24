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

/// A point as stored: hundredths of a degree and of a percent, 6 bytes instead of 12.
/// The API reports two decimals, so nothing it shows is lost.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Packed {
    temperature: i16,
    setpoint: i16,
    heater_power: u16,
}

fn centi(v: f32) -> f32 {
    libm::roundf(v * 100.0)
}

impl Packed {
    fn pack(p: Point) -> Self {
        // `as` saturates: an implausible reading (a broken sensor) clips at ±327 °C.
        Self {
            temperature: centi(p.temperature) as i16,
            setpoint: centi(p.setpoint) as i16,
            heater_power: centi(p.heater_power) as u16,
        }
    }

    fn unpack(self) -> Point {
        Point {
            temperature: f32::from(self.temperature) / 100.0,
            setpoint: f32::from(self.setpoint) / 100.0,
            heater_power: f32::from(self.heater_power) / 100.0,
        }
    }
}

/// 3.6 KB (600 points of 6 bytes).
#[derive(Debug, Clone)]
pub struct History {
    points: HistoryBuf<Packed, HISTORY_LEN>,
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
            self.points.write(Packed::pack(p));
        }
    }

    /// Oldest first, rounded to hundredths.
    pub fn iter(&self) -> impl Iterator<Item = Point> + '_ {
        self.points.oldest_ordered().map(|p| p.unpack())
    }

    /// Copies `other` into `self` point by point. Unlike `clone` it needs no 3.6 KB
    /// temporary on the stack, so a heap copy fits the ESP32's small main stack.
    pub fn copy_from(&mut self, other: &History) {
        self.points.clear();
        for p in other.points.oldest_ordered() {
            self.points.write(*p);
        }
        self.skip = other.skip;
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
                temperature: i as f32 / 10.0,
                setpoint: 0.0,
                heater_power: 0.0,
            });
        }
        assert_eq!(h.len(), HISTORY_LEN);
        let first = h.iter().next().unwrap().temperature;
        let last = h.iter().last().unwrap().temperature;
        assert!((last - first - (HISTORY_LEN as f32 - 1.0) * 0.3).abs() < 0.01);
    }

    #[test]
    fn copy_from_keeps_order_and_phase() {
        let mut src = History::new();
        for i in 0..(HISTORY_LEN as u32 * 3 + 31) {
            src.offer(Point {
                temperature: i as f32 / 10.0,
                setpoint: 1.0,
                heater_power: 2.0,
            });
        }
        let mut dst = History::new();
        dst.offer(Point {
            temperature: -1.0,
            setpoint: 0.0,
            heater_power: 0.0,
        });
        dst.copy_from(&src);
        assert!(dst.iter().eq(src.iter()));
        assert_eq!(dst.skip, src.skip);
    }

    #[test]
    fn stores_hundredths_and_clips_implausible_readings() {
        let mut h = History::new();
        for _ in 0..HISTORY_EVERY {
            h.offer(Point {
                temperature: 93.456,
                setpoint: 94.0,
                heater_power: 37.125,
            });
        }
        for _ in 0..HISTORY_EVERY {
            h.offer(Point {
                temperature: 900.0,
                setpoint: -5.0,
                heater_power: 100.0,
            });
        }
        let p: heapless::Vec<Point, 2> = h.iter().collect();
        assert!((p[0].temperature - 93.46).abs() < 1e-4);
        assert!((p[0].heater_power - 37.13).abs() < 1e-4);
        assert_eq!(p[0].setpoint, 94.0);
        assert!((p[1].temperature - 327.67).abs() < 1e-3, "saturates");
        assert_eq!(p[1].setpoint, -5.0);
        assert_eq!(p[1].heater_power, 100.0);
        assert_eq!(core::mem::size_of::<Packed>(), 6);
    }
}
