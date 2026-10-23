//! Scale: raw load-cell counts to grams, tare, calibration and brew-weight tracking.
//!
//! The C++ firmware exposed tare and calibration on the API and MQTT but never ran
//! them (the flags were set and nobody read them). Both work here.

use heapless::Deque;

use crate::config::{Config, ScaleType};

/// Wait this long after taring before asking for the known weight.
pub const CALIBRATION_TARE_MS: u64 = 3_000;
/// Time to place the known weight.
pub const CALIBRATION_PLACE_MS: u64 = 10_000;
/// Brew-weight auto-tare gives up after this long.
pub const AUTO_TARE_TIMEOUT_MS: u64 = 3_000;
/// Below this the scale counts as empty.
pub const AUTO_TARE_THRESHOLD_G: f32 = 0.2;

const MAX_SAMPLES: usize = 20;

/// One reading from the scale hardware.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleReading {
    /// HX711 counts; the second cell is `None` on single-cell scales.
    Raw { cell1: i32, cell2: Option<i32> },
    /// A Bluetooth scale reports grams itself.
    Grams(f32),
}

/// What the calibration is doing, for the display and the API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Calibration {
    Idle,
    /// Taring the empty scale.
    Taring { until_ms: u64 },
    /// Waiting for the known weight to be placed.
    Placing { until_ms: u64 },
    /// Finished: the new factor, already written to the config.
    Done { factor: f64, at_ms: u64 },
    /// No usable reading change: the factor was left as it was.
    Failed { at_ms: u64 },
}

/// Something the scale needs the hardware to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleAction {
    /// A Bluetooth scale tares itself.
    BluetoothTare,
}

#[derive(Debug, Clone)]
struct Average {
    values: Deque<f64, MAX_SAMPLES>,
}

impl Average {
    fn new() -> Self {
        Self { values: Deque::new() }
    }

    fn push(&mut self, v: f64, window: usize) -> f64 {
        while self.values.len() >= window.clamp(1, MAX_SAMPLES) {
            self.values.pop_front();
        }
        let _ = self.values.push_back(v);
        self.mean()
    }

    fn mean(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.values.iter().sum::<f64>() / self.values.len() as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct Scale {
    avg1: Average,
    avg2: Average,
    tare1: f64,
    tare2: f64,
    raw1: f64,
    raw2: f64,
    grams: Option<f32>,
    last_reading_ms: Option<u64>,
    tare_pending: bool,
    calibration: Calibration,
    brew: BrewWeight,
}

/// Brew weight = current weight minus the weight when the shot started.
#[derive(Debug, Clone, Copy, Default)]
struct BrewWeight {
    active: bool,
    pre_brew: Option<f32>,
    auto_tare_since_ms: Option<u64>,
    value: f32,
}

/// No reading for this long means the scale is gone.
pub const SCALE_TIMEOUT_MS: u64 = 2_000;

impl Default for Scale {
    fn default() -> Self {
        Self::new()
    }
}

impl Scale {
    pub fn new() -> Self {
        Self {
            avg1: Average::new(),
            avg2: Average::new(),
            tare1: 0.0,
            tare2: 0.0,
            raw1: 0.0,
            raw2: 0.0,
            grams: None,
            last_reading_ms: None,
            tare_pending: true, // HX711 tares on boot, as the C++ did
            calibration: Calibration::Idle,
            brew: BrewWeight::default(),
        }
    }

    /// Current weight in grams, if the scale delivers.
    pub fn weight(&self) -> Option<f32> {
        self.grams
    }

    pub fn brew_weight(&self) -> f32 {
        self.brew.value
    }

    pub fn calibration(&self) -> Calibration {
        self.calibration
    }

    /// The scale stopped reporting.
    pub fn is_faulty(&self, now_ms: u64) -> bool {
        self.last_reading_ms.is_none_or(|t| now_ms.saturating_sub(t) > SCALE_TIMEOUT_MS)
    }

    /// Asks for a tare; HX711 scales tare on their next reading.
    pub fn tare(&mut self, cfg: &Config) -> Option<ScaleAction> {
        if cfg.hardware_sensors_scale_type == ScaleType::Bluetooth {
            Some(ScaleAction::BluetoothTare)
        } else {
            self.tare_pending = true;
            None
        }
    }

    /// Starts calibration (HX711 only; Bluetooth scales calibrate themselves).
    pub fn start_calibration(&mut self, cfg: &Config, now_ms: u64) -> bool {
        if cfg.hardware_sensors_scale_type == ScaleType::Bluetooth {
            return false;
        }
        self.tare_pending = true;
        self.calibration = Calibration::Taring { until_ms: now_ms + CALIBRATION_TARE_MS };
        true
    }

    pub fn cancel_calibration(&mut self) {
        self.calibration = Calibration::Idle;
    }

    /// Feeds one reading. Returns `true` when calibration wrote a new factor to `cfg`.
    pub fn on_reading(&mut self, reading: ScaleReading, cfg: &mut Config, now_ms: u64) -> bool {
        self.last_reading_ms = Some(now_ms);
        let window = cfg.hardware_sensors_scale_samples as usize;
        match reading {
            ScaleReading::Grams(g) => {
                self.grams = Some(g);
                false
            }
            ScaleReading::Raw { cell1, cell2 } => {
                self.raw1 = self.avg1.push(f64::from(cell1), window);
                self.raw2 = cell2.map_or(0.0, |c| self.avg2.push(f64::from(c), window));
                if self.tare_pending {
                    self.tare1 = self.raw1;
                    self.tare2 = self.raw2;
                    self.tare_pending = false;
                }
                let calibrated = self.advance_calibration(cfg, now_ms);
                let f1 = nonzero(cfg.hardware_sensors_scale_calibration);
                let mut grams = (self.raw1 - self.tare1) / f1;
                if cell2.is_some() {
                    grams += (self.raw2 - self.tare2) / nonzero(cfg.hardware_sensors_scale_calibration2);
                }
                self.grams = Some(grams as f32);
                calibrated
            }
        }
    }

    fn advance_calibration(&mut self, cfg: &mut Config, now_ms: u64) -> bool {
        match self.calibration {
            Calibration::Taring { until_ms } if now_ms >= until_ms => {
                self.calibration = Calibration::Placing { until_ms: now_ms + CALIBRATION_PLACE_MS };
                false
            }
            Calibration::Placing { until_ms } if now_ms >= until_ms => {
                let delta = (self.raw1 - self.tare1) + (self.raw2 - self.tare2);
                let factor = delta / cfg.hardware_sensors_scale_known_weight;
                if factor.is_finite() && factor.abs() >= 1e-6 && factor.abs() <= 999_999.0 {
                    cfg.hardware_sensors_scale_calibration = factor;
                    cfg.hardware_sensors_scale_calibration2 = factor;
                    self.calibration = Calibration::Done { factor, at_ms: now_ms };
                    true
                } else {
                    self.calibration = Calibration::Failed { at_ms: now_ms };
                    false
                }
            }
            _ => false,
        }
    }

    /// Tracks the brew weight; call every tick with whether a shot is running.
    pub fn track_brew(&mut self, brew_active: bool, cfg: &Config, now_ms: u64) -> Option<ScaleAction> {
        let weight = self.grams.unwrap_or(0.0);
        if brew_active && !self.brew.active {
            self.brew = BrewWeight { active: true, ..BrewWeight::default() };
            let auto_tare = cfg.hardware_sensors_scale_type == ScaleType::Bluetooth
                && cfg.brew_by_weight_enabled
                && cfg.brew_by_weight_auto_tare
                && weight.abs() > AUTO_TARE_THRESHOLD_G;
            if auto_tare {
                self.brew.auto_tare_since_ms = Some(now_ms);
                return Some(ScaleAction::BluetoothTare);
            }
            self.brew.pre_brew = Some(weight);
        } else if !brew_active && self.brew.active {
            // Keep the final weight on screen until the next shot starts.
            self.brew.active = false;
            return None;
        }
        if !self.brew.active {
            return None;
        }
        if self.brew.pre_brew.is_none() {
            let since = self.brew.auto_tare_since_ms.unwrap_or(now_ms);
            let timed_out = now_ms.saturating_sub(since) >= AUTO_TARE_TIMEOUT_MS;
            if timed_out || weight.abs() <= AUTO_TARE_THRESHOLD_G {
                self.brew.pre_brew = Some(weight);
            }
        }
        if let Some(pre) = self.brew.pre_brew {
            self.brew.value = weight - pre;
        }
        None
    }
}

fn nonzero(v: f64) -> f64 {
    if v == 0.0 { 1.0 } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hx_cfg() -> Config {
        Config {
            hardware_sensors_scale_enabled: true,
            hardware_sensors_scale_type: ScaleType::Hx711Single,
            hardware_sensors_scale_samples: 1,
            hardware_sensors_scale_calibration: 100.0,
            ..Config::default()
        }
    }

    fn raw(c: i32) -> ScaleReading {
        ScaleReading::Raw { cell1: c, cell2: None }
    }

    #[test]
    fn first_reading_tares_and_factor_scales() {
        let mut cfg = hx_cfg();
        let mut s = Scale::new();
        s.on_reading(raw(5000), &mut cfg, 0);
        assert_eq!(s.weight(), Some(0.0));
        s.on_reading(raw(8600), &mut cfg, 100);
        assert_eq!(s.weight(), Some(36.0));
    }

    #[test]
    fn dual_cells_add_up() {
        let mut cfg = Config { hardware_sensors_scale_type: ScaleType::Hx711Dual, ..hx_cfg() };
        cfg.hardware_sensors_scale_calibration2 = 50.0;
        let mut s = Scale::new();
        s.on_reading(ScaleReading::Raw { cell1: 0, cell2: Some(0) }, &mut cfg, 0);
        s.on_reading(ScaleReading::Raw { cell1: 1000, cell2: Some(500) }, &mut cfg, 100);
        assert_eq!(s.weight(), Some(20.0));
    }

    #[test]
    fn calibration_computes_the_factor() {
        let mut cfg = Config { hardware_sensors_scale_known_weight: 200.0, ..hx_cfg() };
        let mut s = Scale::new();
        s.on_reading(raw(1000), &mut cfg, 0);
        assert!(s.start_calibration(&cfg, 0));
        s.on_reading(raw(1000), &mut cfg, 100);
        s.on_reading(raw(1000), &mut cfg, CALIBRATION_TARE_MS);
        assert!(matches!(s.calibration(), Calibration::Placing { .. }));
        let done_at = CALIBRATION_TARE_MS + CALIBRATION_PLACE_MS;
        assert!(s.on_reading(raw(41_000), &mut cfg, done_at));
        assert_eq!(cfg.hardware_sensors_scale_calibration, 200.0);
        assert!((s.weight().unwrap() - 200.0).abs() < 1e-3);
    }

    #[test]
    fn calibration_without_weight_fails_and_keeps_factor() {
        let mut cfg = hx_cfg();
        let mut s = Scale::new();
        s.start_calibration(&cfg, 0);
        s.on_reading(raw(10), &mut cfg, 0);
        s.on_reading(raw(10), &mut cfg, CALIBRATION_TARE_MS);
        s.on_reading(raw(10), &mut cfg, CALIBRATION_TARE_MS + CALIBRATION_PLACE_MS);
        assert!(matches!(s.calibration(), Calibration::Failed { .. }));
        assert_eq!(cfg.hardware_sensors_scale_calibration, 100.0);
    }

    #[test]
    fn brew_weight_is_relative_to_shot_start() {
        let mut cfg = hx_cfg();
        let mut s = Scale::new();
        s.on_reading(raw(0), &mut cfg, 0);
        s.on_reading(raw(30_000), &mut cfg, 10); // cup: 300 g
        assert_eq!(s.track_brew(true, &cfg, 20), None);
        s.on_reading(raw(33_600), &mut cfg, 30);
        s.track_brew(true, &cfg, 30);
        assert!((s.brew_weight() - 36.0).abs() < 1e-3);
        s.track_brew(false, &cfg, 40);
        s.on_reading(raw(30_000), &mut cfg, 50); // cup removed and put back
        s.track_brew(false, &cfg, 50);
        assert!((s.brew_weight() - 36.0).abs() < 1e-3, "final weight stays visible");
    }

    #[test]
    fn bluetooth_auto_tare_waits_for_zero() {
        let cfg = Config {
            hardware_sensors_scale_type: ScaleType::Bluetooth,
            brew_by_weight_enabled: true,
            brew_by_weight_auto_tare: true,
            ..hx_cfg()
        };
        let mut c2 = cfg.clone();
        let mut s = Scale::new();
        s.on_reading(ScaleReading::Grams(310.0), &mut c2, 0);
        assert_eq!(s.track_brew(true, &cfg, 0), Some(ScaleAction::BluetoothTare));
        s.track_brew(true, &cfg, 100);
        assert_eq!(s.brew_weight(), 0.0);
        s.on_reading(ScaleReading::Grams(0.1), &mut c2, 200);
        s.track_brew(true, &cfg, 200);
        s.on_reading(ScaleReading::Grams(18.1), &mut c2, 300);
        s.track_brew(true, &cfg, 300);
        assert!((s.brew_weight() - 18.0).abs() < 1e-3);
    }

    #[test]
    fn missing_readings_mean_fault() {
        let mut cfg = hx_cfg();
        let mut s = Scale::new();
        assert!(s.is_faulty(0));
        s.on_reading(raw(0), &mut cfg, 0);
        assert!(!s.is_faulty(SCALE_TIMEOUT_MS));
        assert!(s.is_faulty(SCALE_TIMEOUT_MS + 1));
    }
}
