//! The OLED screens, rendered into a [`canvas::Frame`] from the machine status.
//!
//! Ported from the C++ templates with the same coordinates (u8g2 `setFontPosTop`
//! convention: text `y` is its top). Selection order per refresh, as before:
//! firmware update → fullscreen timers → scale calibration → system screens (heating,
//! PID off, standby, steam, tank, backflush, emergency, sensor error) → the template.
//!
//! Fonts are the u8g2 fonts the C++ used, each in the smallest glyph set its texts
//! need (`_tn` digits, `_tr` ASCII, `_tf` Latin-1); `tests::every_screen_renders`
//! fails if a text needs a missing glyph or draws outside the panel.

#![no_std]

pub mod bitmaps;
pub mod canvas;
pub mod lang;

use core::fmt::Write;

use cc_core::config::{BrewMode, Config, DisplayTemplate, ScaleType};
use cc_core::scale::Calibration;
use cc_core::{State, Status};
use heapless::String;
use u8g2_fonts::{FontRenderer, fonts};

use bitmaps as bm;
use canvas::{Canvas, Frame, Rotation};
use lang::Texts;

static PROFONT10: FontRenderer = FontRenderer::new::<fonts::u8g2_font_profont10_tf>();
static PROFONT11: FontRenderer = FontRenderer::new::<fonts::u8g2_font_profont11_tf>();
static PROFONT12: FontRenderer = FontRenderer::new::<fonts::u8g2_font_profont12_tr>();
static PROFONT15: FontRenderer = FontRenderer::new::<fonts::u8g2_font_profont15_tf>();
static PROFONT17: FontRenderer = FontRenderer::new::<fonts::u8g2_font_profont17_tf>();
static PROFONT22: FontRenderer = FontRenderer::new::<fonts::u8g2_font_profont22_tr>();
static FUB17: FontRenderer = FontRenderer::new::<fonts::u8g2_font_fub17_tr>();
static FUB20: FontRenderer = FontRenderer::new::<fonts::u8g2_font_fub20_tr>();
static FUB25: FontRenderer = FontRenderer::new::<fonts::u8g2_font_fub25_tn>();
static FUB30: FontRenderer = FontRenderer::new::<fonts::u8g2_font_fub30_tn>();

pub const STATUS_BAR_Y: i32 = 12;

/// Network state for the status bar.
#[derive(Debug, Clone, Copy, Default)]
pub struct Net {
    pub offline: bool,
    pub wifi_connected: bool,
    /// 0..=4
    pub signal: u8,
    pub wifi_reconnects: u32,
    pub mqtt_connected: bool,
    pub ble_scale_connected: bool,
}

/// Firmware update progress, shown instead of everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ota<'a> {
    Running { percent: u8 },
    Done,
    Failed(&'a str),
}

/// Full-screen boot and connection messages (the C++ `displayLogo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Banner<'a> {
    pub line1: &'a str,
    pub line2: &'a str,
}

/// Everything a refresh needs.
#[derive(Debug, Clone, Copy)]
pub struct View<'a> {
    pub status: &'a Status,
    pub cfg: &'a Config,
    pub net: Net,
    pub uptime_ms: u64,
    pub ota: Option<Ota<'a>>,
    pub banner: Option<Banner<'a>>,
}

/// What a refresh produced, for the caller and for layout tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rendered {
    /// The panel should be switched off (long standby).
    pub panel_off: bool,
    /// Something was drawn outside the panel.
    pub clipped: bool,
    /// A text needed a glyph the chosen font subset lacks.
    pub missing_glyph: bool,
    /// First off-panel pixel (logical coordinates).
    pub first_clip: Option<(i32, i32)>,
}

struct Policy {
    heating_logo: bool,
    fullscreen_brew: bool,
}

fn policy(t: DisplayTemplate) -> Policy {
    match t {
        DisplayTemplate::Minimal | DisplayTemplate::TemperatureOnly => Policy {
            heating_logo: false,
            fullscreen_brew: true,
        },
        DisplayTemplate::Modern => Policy {
            heating_logo: false,
            fullscreen_brew: false,
        },
        _ => Policy {
            heating_logo: true,
            fullscreen_brew: true,
        },
    }
}

type Buf = String<64>;

fn fmt(args: core::fmt::Arguments<'_>) -> Buf {
    let mut s = Buf::new();
    let _ = s.write_fmt(args);
    s
}

/// Arduino `print(value, 0)`: rounds half away from zero.
fn round0(v: f64) -> f64 {
    libm::round(v)
}

/// Arduino `map()` with integer truncation.
fn map(x: i64, in_min: i64, in_max: i64, out_min: i64, out_max: i64) -> i32 {
    ((x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min) as i32
}

/// Renders one refresh into `frame`.
pub fn render(frame: &mut Frame, v: &View<'_>) -> Rendered {
    let upright = v.cfg.display_template == DisplayTemplate::Upright;
    let mut c = Canvas::new(frame, Rotation::for_display(upright, v.cfg.display_inverted));
    let panel_off = v.status.display_off;
    if !panel_off {
        Screens {
            v,
            t: lang::texts(v.cfg.display_language),
            upright,
        }
        .draw(&mut c);
    }
    Rendered {
        panel_off,
        clipped: c.clipped,
        missing_glyph: c.missing_glyph,
        first_clip: c.first_clip,
    }
}

struct Screens<'v, 'a> {
    v: &'v View<'a>,
    t: &'static Texts,
    upright: bool,
}

impl Screens<'_, '_> {
    fn st(&self) -> &Status {
        self.v.status
    }

    fn cfg(&self) -> &Config {
        self.v.cfg
    }

    fn draw(&self, c: &mut Canvas<'_>) {
        if let Some(b) = self.v.banner {
            return self.banner(c, b);
        }
        if let Some(o) = self.v.ota {
            return if self.upright {
                ota_upright(c, o);
            } else {
                ota(c, o);
            };
        }
        let p = policy(self.cfg().display_template);
        if (p.fullscreen_brew && self.fullscreen_brew(c))
            || self.fullscreen_flush(c)
            || self.fullscreen_hot_water(c)
        {
            return;
        }
        if self.calibration(c) || self.system_screen(c, p.heating_logo) {
            return;
        }
        match self.cfg().display_template {
            DisplayTemplate::Standard => self.standard(c),
            DisplayTemplate::Minimal => self.minimal(c),
            DisplayTemplate::TemperatureOnly => self.temperature_only(c),
            DisplayTemplate::Scale => self.scale(c),
            DisplayTemplate::Upright => self.upright(c),
            DisplayTemplate::Modern => self.modern(c),
        }
    }

    // --- shared widgets ---------------------------------------------------------

    fn statusbar(&self, c: &mut Canvas<'_>) {
        c.hline(0, STATUS_BAR_Y, c.width());
        if self.v.net.offline {
            c.text(&PROFONT11, 40, 0, self.t.offline);
        } else {
            self.wifi(c, 4, 1);
            self.mqtt(c, 40, 0);
        }
        if self.cfg().hardware_sensors_scale_enabled
            && self.cfg().hardware_sensors_scale_type == ScaleType::Bluetooth
            && self.v.net.ble_scale_connected
        {
            c.xbm(24, 1, &bm::BLUETOOTH);
        }
        if !self.maintenance_flag(c, 78, 0) {
            let mins = self.v.uptime_ms / 60_000;
            c.text(
                &PROFONT11,
                84,
                0,
                &fmt(format_args!("{:02}h {:02}m", mins / 60, mins % 60)),
            );
        }
    }

    fn wifi(&self, c: &mut Canvas<'_>, x: i32, y: i32) {
        if self.v.net.wifi_connected {
            c.xbm(x, y, &bm::ANTENNA_OK);
            for b in 0..=i32::from(self.v.net.signal.min(4)) {
                c.vline(x + 5 + b * 2, y + 8 - b * 2, b * 2);
            }
        } else {
            c.xbm(x, y, &bm::ANTENNA_NOK);
            let rx = if self.upright { x + 12 } else { x + 36 };
            c.text(
                &PROFONT11,
                rx,
                y - 1,
                &fmt(format_args!("RC: {}", self.v.net.wifi_reconnects)),
            );
        }
    }

    fn mqtt(&self, c: &mut Canvas<'_>, x: i32, y: i32) {
        if self.cfg().mqtt_enabled && self.v.net.mqtt_connected {
            let w = c.text(&PROFONT11, x, y, "MQTT");
            if self.v.net.signal <= 1 {
                c.text(&PROFONT11, x + w, y, "!");
            }
        }
    }

    fn maintenance_flag(&self, c: &mut Canvas<'_>, x: i32, y: i32) -> bool {
        if !self.st().backflush_reminder_due(self.cfg()) {
            return false;
        }
        c.text(&PROFONT11, x, y, "CLEAN");
        true
    }

    fn thermometer(&self, c: &mut Canvas<'_>, x: i32, y: i32) {
        c.line(x + 3, y - 9, x + 3, y - 42);
        c.line(x + 9, y - 9, x + 9, y - 42);
        c.pixel(x + 4, y - 43);
        c.pixel(x + 8, y - 43);
        c.line(x + 5, y - 44, x + 7, y - 44);
        c.disc(x + 6, y - 5, 6);
        let h = map(
            self.st().setpoint as i64,
            0,
            100,
            i64::from(y - 9),
            i64::from(y - 39),
        );
        c.line(x + 11, h, x + 16, h);
    }

    fn temperature_bar(&self, c: &mut Canvas<'_>, x: i32, range: i32) {
        let t = self.st().temperature;
        let h = map(t as i64, 0, 100, 0, i64::from(range)).clamp(0, range + 12);
        for i in x..x + 5 {
            c.vline(i, 52 - h, h);
        }
        if t > 100.0 {
            c.line(x, range - 11, x + 3, range - 11);
            c.line(x, range - 10, x + 4, range - 10);
            c.line(x, range - 9, x + 4, range - 9);
        }
    }

    fn big_temperature(&self, c: &mut Canvas<'_>, x: i32, y: i32) {
        let t = self.st().temperature;
        let s = fmt(format_args!("{:.0}", round0(t)));
        c.text(&FUB30, if t < 99.499 { x + 20 } else { x }, y, &s);
        c.circle(x + 72, y + 4, 3);
    }

    fn progress(c: &mut Canvas<'_>, value: f64, x: i32, y: i32, w: i32) {
        c.frame_rect(x, y, w, 4);
        let out = map(value.clamp(0.0, 100.0) as i64, 0, 100, 0, i64::from(w));
        if out - 2 > 0 {
            c.line(x + 1, y + 1, x + out - 1, y + 1);
            c.line(x + 1, y + 2, x + out - 1, y + 2);
        }
    }

    fn temperature_info(&self, c: &mut Canvas<'_>, x: i32, y: i32, value_x: i32, labels: (&str, &str)) {
        let st = self.st();
        for (row, label, value) in [(0, labels.0, st.temperature), (10, labels.1, st.setpoint)] {
            c.text(&PROFONT11, x, y + row, label);
            c.text(&PROFONT11, value_x, y + row, &fmt(format_args!("{value:.1}")));
            c.text(&PROFONT11, value_x + 31, y + row, "°C");
        }
    }

    fn pid_info(&self, c: &mut Canvas<'_>, x: i32, y: i32, out_x: i32, out_y: i32, sep: &str) {
        let tn = self.st().tunings;
        let tn_s = if tn.ki == 0.0 { 0.0 } else { tn.kp / tn.ki };
        let tv_s = if tn.kp == 0.0 { 0.0 } else { tn.kd / tn.kp };
        let s = fmt(format_args!(
            "{:.0}{sep}{:.0}{sep}{:.0}",
            round0(tn.kp),
            round0(tn_s),
            round0(tv_s)
        ));
        c.text(&PROFONT11, x, y, &s);
        let p = self.st().heater_power;
        let s = if p * 10.0 < 99.0 {
            fmt(format_args!("{p:.1}%"))
        } else {
            fmt(format_args!("{:.0}%", round0(p)))
        };
        c.text(&PROFONT11, out_x, out_y, &s);
    }

    fn hot_water_visible(&self) -> bool {
        self.st().hot_water_ms > 0 && matches!(self.st().state, State::PidNormal | State::Steam)
    }

    fn brew_info(&self, c: &mut Canvas<'_>, x: i32, y: i32, labels: (&str, &str, &str)) {
        if !self.cfg().hardware_switches_brew_enabled {
            return;
        }
        let st = self.st();
        if st.state == State::ManualFlush {
            self.brew_time(c, x, y, labels.1, st.brew_time_ms, None);
        } else if self.hot_water_visible() {
            self.brew_time(c, x, y, labels.2, st.hot_water_ms as f64, None);
        } else if st.brew_timer_visible {
            let target = (self.cfg().brew_mode == BrewMode::Automatic && self.cfg().brew_by_time_enabled)
                .then_some(st.brew_target_ms);
            self.brew_time(c, x, y, labels.0, st.brew_time_ms, target);
        }
    }

    fn brew_time(&self, c: &mut Canvas<'_>, x: i32, y: i32, label: &str, ms: f64, target: Option<f64>) {
        let mut s = fmt(format_args!("{:.0}", round0(ms / 1000.0)));
        if let Some(t) = target.filter(|t| *t > 0.0) {
            let _ = write!(s, "/{:.0}", round0(t / 1000.0));
        }
        let _ = s.push_str(" s");
        self.label_value(c, x, y, label, &s);
    }

    fn label_value(&self, c: &mut Canvas<'_>, x: i32, y: i32, label: &str, value: &str) {
        c.color = false;
        // Clear the value column up to the right edge (the C++ cleared 78 px from
        // x + 50 and relied on u8g2 clipping the overhang).
        if self.upright {
            c.fill_rect(x, y + 1, c.width() - x, 10);
        } else {
            c.fill_rect(x + 50, y + 1, c.width() - x - 50, 10);
        }
        c.color = true;
        if self.upright {
            let w = c.text(&PROFONT11, x, y, label);
            c.text(&PROFONT11, x + w, y, value);
        } else {
            c.text(&PROFONT11, x, y, label);
            c.text(&PROFONT11, x + 50, y, value);
        }
    }

    fn weight(&self, c: &mut Canvas<'_>, x: i32, y: i32, grams: f32, target: Option<f64>) {
        let label = if self.upright {
            self.t.weight_ur
        } else {
            self.t.weight
        };
        if self.st().scale_fault {
            return self.label_value(c, x, y, label, self.t.scale_failure);
        }
        let mut s = fmt(format_args!("{:.0}", round0(f64::from(grams))));
        if let Some(t) = target.filter(|t| *t > 0.0) {
            let _ = write!(s, "/{:.0}", round0(t));
        }
        let _ = s.push_str(" g");
        self.label_value(c, x, y, label, &s);
    }

    fn weight_target(&self) -> Option<f64> {
        (self.cfg().brew_mode == BrewMode::Automatic && self.cfg().brew_by_weight_enabled)
            .then_some(self.cfg().brew_by_weight_target_weight)
    }

    // --- full-screen modes ---------------------------------------------------------

    fn timer_fs(&self, c: &mut Canvas<'_>, x: i32, y: i32, ms: f64) {
        let s = fmt(format_args!("{:.1}", ms / 1000.0));
        if self.upright {
            c.text(&FUB20, if ms < 9_950.0 { x + 15 } else { x }, y, &s);
            c.text(&PROFONT11, x + 56, y + 12, "s");
        } else {
            c.text(&FUB25, if ms < 9_950.0 { x + 16 } else { x }, y, &s);
            c.text(
                &PROFONT15,
                if ms < 9_950.0 { x + 67 } else { x + 69 },
                y + 14,
                "s",
            );
        }
    }

    fn fullscreen_brew(&self, c: &mut Canvas<'_>) -> bool {
        let st = self.st();
        if !self.cfg().display_fullscreen_brew_timer || !st.brew_timer_visible {
            return false;
        }
        let scale = self.cfg().hardware_sensors_scale_enabled;
        if self.upright {
            c.xbm(12, 12, &bm::BREW_CUP_LOGO);
            if scale {
                c.text(
                    &PROFONT22,
                    5,
                    60,
                    &fmt(format_args!("{:.1}s", st.brew_time_ms / 1000.0)),
                );
                c.text(&PROFONT22, 5, 90, &fmt(format_args!("{:.1}g", st.brew_weight)));
            } else {
                self.timer_fs(c, 1, 80, st.brew_time_ms);
            }
        } else {
            c.xbm(0, 11, &bm::BREW_CUP_LOGO);
            if scale {
                c.text(
                    &PROFONT22,
                    48,
                    14,
                    &fmt(format_args!("{}s", (st.brew_time_ms / 1000.0) as i32)),
                );
                c.text(&PROFONT22, 48, 38, &fmt(format_args!("{:.1}g", st.brew_weight)));
            } else {
                self.timer_fs(c, 48, 25, st.brew_time_ms);
            }
        }
        true
    }

    fn fullscreen_flush(&self, c: &mut Canvas<'_>) -> bool {
        if !self.cfg().display_fullscreen_manual_flush_timer || self.st().state != State::ManualFlush {
            return false;
        }
        self.fullscreen_logo_timer(c, &bm::MANUAL_FLUSH_LOGO, self.st().brew_time_ms);
        true
    }

    fn fullscreen_hot_water(&self, c: &mut Canvas<'_>) -> bool {
        if !self.cfg().display_fullscreen_hot_water_timer || !self.hot_water_visible() {
            return false;
        }
        self.fullscreen_logo_timer(c, &bm::HOT_WATER_LOGO, self.st().hot_water_ms as f64);
        true
    }

    fn fullscreen_logo_timer(&self, c: &mut Canvas<'_>, logo: &bm::Xbm, ms: f64) {
        if self.upright {
            c.xbm(12, 12, logo);
            self.timer_fs(c, 1, 80, ms);
        } else {
            c.xbm(0, 12, logo);
            self.timer_fs(c, 48, 25, ms);
        }
    }

    // --- scale calibration ------------------------------------------------------------

    fn calibration(&self, c: &mut Canvas<'_>) -> bool {
        let now = self.st().now_ms;
        let recent = |at: u64| now.saturating_sub(at) < 5_000;
        let mut buf: String<160> = String::new();
        let text: &str = match self.st().scale_calibration {
            Calibration::Taring { .. } => self.t.calibrate_start,
            Calibration::Placing { .. } => self.t.calibrate_in_progress,
            Calibration::Done { factor, at_ms } if recent(at_ms) => {
                let _ = write!(buf, "{} {factor:.2}", self.t.calibrate_complete);
                &buf
            }
            Calibration::Failed { at_ms } if recent(at_ms) => self.t.calibrate_failed,
            _ => return false,
        };
        wrapped(c, if self.upright { &PROFONT10 } else { &PROFONT11 }, text);
        true
    }

    // --- system screens -----------------------------------------------------------------

    fn system_screen(&self, c: &mut Canvas<'_>, heating_logo: bool) -> bool {
        let st = self.st();
        let cfg = self.cfg();
        if heating_logo && st.heating_logo_due(cfg) && self.upright {
            c.xbm(12, 16, &bm::HEATING_LOGO);
            c.text(&FUB20, 2, 66, &fmt(format_args!("{:.1}", st.temperature)));
            return true;
        }
        if heating_logo && st.heating_logo_due(cfg) {
            self.statusbar(c);
            c.xbm(0, 20, &bm::HEATING_LOGO);
            c.text(&FUB25, 50, 30, &fmt(format_args!("{:.1}", st.temperature)));
            c.circle(122, 32, 3);
            return true;
        }
        match st.state {
            State::PidDisabled if cfg.display_pid_off_logo => {
                if self.upright {
                    c.xbm(6, 24, &bm::OFF_LOGO);
                    wrapped_at(c, &PROFONT10, 0, 84, self.t.pid_disabled);
                } else {
                    c.xbm(38, 0, &bm::OFF_LOGO);
                    c.text_centered(&PROFONT10, 54, self.t.pid_disabled);
                }
            }
            State::Standby if cfg.display_pid_off_logo => {
                if self.upright {
                    c.xbm(6, 50, &bm::OFF_LOGO);
                    c.text(&PROFONT10, 1, 110, self.t.standby);
                } else {
                    c.xbm(38, 0, &bm::OFF_LOGO);
                    c.text_centered(&PROFONT10, 54, self.t.standby);
                }
            }
            State::Steam => {
                if self.upright {
                    c.xbm(12, 10, &bm::STEAM_LOGO);
                    let s = fmt(format_args!("{:.0}", round0(st.temperature)));
                    c.text(&FUB20, 2, 60, &s);
                    c.circle(56, 64, 3);
                } else {
                    c.xbm(0, 12, &bm::STEAM_LOGO);
                    self.big_temperature(c, 48, 16);
                }
            }
            State::WaterTankEmpty => {
                if self.upright {
                    c.xbm(8, 50, &bm::WATER_TANK_EMPTY_LOGO);
                } else {
                    c.xbm(45, 0, &bm::WATER_TANK_EMPTY_LOGO);
                }
            }
            s if s.is_backflush() => self.backflush(c, s),
            State::EmergencyStop => self.emergency(c),
            State::SensorError => {
                let temp = fmt(format_args!("{:.1}", st.temperature));
                if self.upright {
                    let e = &self.t.error_tsensor_ur;
                    message(c, &[e[0], e[1], &temp]);
                    let mut rest: String<64> = String::new();
                    let _ = write!(rest, "{} {} {}", e[2], e[3], e[4]);
                    wrapped_at(c, &PROFONT11, 0, 32, &rest);
                } else {
                    let e = &self.t.error_tsensor;
                    c.text(&PROFONT11, 0, 0, &fmt(format_args!("{}{temp}", e[0])));
                    wrapped_at(c, &PROFONT11, 0, 12, e[1]);
                }
            }
            _ => return false,
        }
        true
    }

    fn backflush(&self, c: &mut Canvas<'_>, s: State) {
        let w = c.width();
        let font = if self.upright { &PROFONT10 } else { &PROFONT12 };
        if self.upright {
            c.text(&PROFONT15, 2, 10, "Backflush");
        } else {
            c.text(&FUB17, 2, 4, "Backflush");
        }
        match s {
            State::BackflushIdle | State::BackflushFinished => {
                let second = if s == State::BackflushIdle {
                    self.t.backflush_start
                } else {
                    self.t.backflush_finish
                };
                if self.upright {
                    wrapped_at(c, font, 0, 40, self.t.backflush_press);
                    wrapped_at(c, font, 0, 80, second);
                } else {
                    c.text(font, 4, 37, self.t.backflush_press);
                    c.text(font, 4, 50, second);
                }
            }
            _ => {
                let txt = fmt(format_args!(
                    "{}/{}",
                    self.st().backflush_cycle,
                    self.cfg().backflush_cycles
                ));
                if self.upright {
                    c.text(&FUB20, 4, 60, &txt);
                } else {
                    let tw = c.text_width(&FUB17, &txt);
                    c.text(&FUB17, (w - tw) / 2, 34, &txt);
                }
            }
        }
    }

    fn emergency(&self, c: &mut Canvas<'_>) {
        let st = self.st();
        let (x, y) = if self.upright { (2, 70) } else { (32, 24) };
        let (lt, ls) = if self.upright {
            (self.t.current_temp_ur, self.t.set_temp_ur)
        } else {
            (self.t.current_temp, self.t.set_temp)
        };
        c.text(
            &PROFONT11,
            x,
            y,
            &fmt(format_args!("{lt}{:.1} °C", st.temperature)),
        );
        c.text(
            &PROFONT11,
            x,
            y + 10,
            &fmt(format_args!("{ls}{:.1} °C", st.setpoint)),
        );
        if !self.upright {
            self.thermometer(c, 4, 58);
        }
        if st.blink_on() {
            if !self.upright {
                self.temperature_bar(c, 8, 30);
            }
            if self.upright {
                c.text(&PROFONT11, 2, 20, "PID");
                c.text(&PROFONT11, 2, 30, "STOPPED");
            } else {
                c.text(&PROFONT11, 32, 4, "PID STOPPED");
            }
        }
    }

    fn banner(&self, c: &mut Canvas<'_>, b: Banner<'_>) {
        if self.upright {
            c.xbm(11, 4, &bm::CLEVERCOFFEE_LOGO);
            let mut y = 47;
            for word in b
                .line1
                .split(' ')
                .chain(b.line2.split(' '))
                .filter(|w| !w.is_empty())
            {
                c.text(&PROFONT11, 0, y, word);
                y += 10;
            }
        } else {
            c.xbm(44, 0, &bm::CLEVERCOFFEE_LOGO);
            c.text_centered(&PROFONT11, 42, b.line1);
            c.text_centered(&PROFONT11, 53, b.line2);
        }
    }

    // --- templates ---------------------------------------------------------------------

    fn labels(&self) -> (&'static str, &'static str) {
        (self.t.current_temp, self.t.set_temp)
    }

    fn brew_labels(&self) -> (&'static str, &'static str, &'static str) {
        (self.t.brew, self.t.manual_flush, self.t.hot_water)
    }

    fn standard(&self, c: &mut Canvas<'_>) {
        self.statusbar(c);
        self.temperature_info(c, 34, 16, 84, self.labels());
        self.thermometer(c, 4, 62);
        if !self.st().near_setpoint(self.cfg().display_blinking_delta) || self.st().blink_on() {
            self.temperature_bar(c, 8, 30);
        }
        self.brew_info(c, 34, 36, self.brew_labels());
        self.pid_info(c, 38, 47, 96, 47, "|");
        Self::progress(c, self.st().heater_power, 30, 60, 98);
    }

    fn minimal(&self, c: &mut Canvas<'_>) {
        self.statusbar(c);
        self.temperature_info(c, 34, 16, 84, self.labels());
        self.brew_info(c, 34, 36, self.brew_labels());
        Self::progress(c, self.st().heater_power, 0, 58, 128);
    }

    fn temperature_only(&self, c: &mut Canvas<'_>) {
        self.big_temperature(c, 0, 8);
        let s = fmt(format_args!("Target  {:.1} °C", self.st().setpoint));
        c.text_centered(&PROFONT11, 46, &s);
        Self::progress(c, self.st().heater_power, 0, 58, 128);
    }

    fn scale(&self, c: &mut Canvas<'_>) {
        self.statusbar(c);
        self.temperature_info(c, 0, 16, 50, self.labels());
        self.brew_info(c, 0, 26, self.brew_labels());
        let cfg = self.cfg();
        let st = self.st();
        if cfg.hardware_sensors_scale_enabled {
            if st.brew_timer_visible {
                self.weight(c, 0, 36, st.brew_weight, self.weight_target());
            } else {
                self.weight(c, 0, 36, st.weight.unwrap_or(0.0), None);
            }
        }
        if cfg.hardware_sensors_pressure_enabled {
            let s = fmt(format_args!(
                "{}{:.1} bar",
                self.t.pressure,
                st.pressure.unwrap_or(0.0)
            ));
            c.text(&PROFONT11, 0, 46, &s);
        }
        Self::progress(c, st.heater_power, 0, 60, 128);
    }

    fn upright(&self, c: &mut Canvas<'_>) {
        let st = self.st();
        let cfg = self.cfg();
        self.temperature_info(c, 1, 14, 21, (self.t.current_temp_ur, self.t.set_temp_ur));
        // Heat bar at the bottom.
        c.frame_rect(0, 124, 64, 4);
        let w = (st.heater_power * 10.0 / 16.13) as i32 + 1;
        c.line(1, 125, w, 125);
        c.line(1, 126, w, 126);
        // Main status word.
        let scale = cfg.hardware_sensors_scale_enabled;
        let pressure = cfg.hardware_sensors_pressure_enabled;
        let y = match (scale, pressure) {
            (true, true) => 65,
            (true, false) | (false, true) => 60,
            (false, false) => 55,
        };
        let word = if st.state == State::ManualFlush {
            "FLUSH"
        } else if st.brew_timer_visible {
            "BREW"
        } else if st.backflush_reminder_due(cfg) {
            "CLEAN"
        } else if st.near_setpoint(cfg.display_blinking_delta) && st.blink_on() {
            "OK"
        } else {
            "WAIT"
        };
        // ProFont22: in FUB20 (the C++ choice) "WAIT" and "CLEAN" are wider than 64 px.
        c.text(&PROFONT22, 1, y, word);
        self.brew_info(
            c,
            1,
            34,
            (self.t.brew_ur, self.t.manual_flush_ur, self.t.hot_water_ur),
        );
        if scale {
            if st.brew_timer_visible {
                self.weight(c, 1, 44, st.brew_weight, self.weight_target());
            } else {
                self.weight(c, 1, 44, st.weight.unwrap_or(0.0), None);
            }
        }
        if pressure {
            // No space before the unit: "Pr: 9.1 bar" is 66 px in Spanish.
            let s = fmt(format_args!(
                "{}{:.1}bar",
                self.t.pressure_ur,
                st.pressure.unwrap_or(0.0)
            ));
            c.text(&PROFONT11, 1, if scale { 54 } else { 44 }, &s);
        }
        self.pid_info(c, 1, 100, 1, 110, " ");
        // Status bar, half width.
        c.hline(0, STATUS_BAR_Y, 64);
        if self.v.net.offline {
            c.text(&PROFONT11, 4, 1, self.t.offline);
        } else {
            self.wifi(c, 4, 2);
            self.mqtt(c, 21, 0);
        }
        if !self.maintenance_flag(c, 34, 0)
            && cfg.hardware_sensors_scale_enabled
            && cfg.hardware_sensors_scale_type == ScaleType::Bluetooth
            && self.v.net.ble_scale_connected
        {
            c.xbm(54, 1, &bm::BLUETOOTH);
        }
    }

    // --- modern -----------------------------------------------------------------------

    fn modern(&self, c: &mut Canvas<'_>) {
        let st = self.st();
        if st.state == State::ManualFlush {
            self.modern_brew(c, true);
        } else if st.post_brew {
            self.modern_post_brew(c);
        } else if st.brew_timer_visible {
            self.modern_brew(c, false);
        } else {
            self.modern_idle(c);
        }
    }

    fn modern_idle(&self, c: &mut Canvas<'_>) {
        const ROW_GAP: i32 = 2;
        const TEMP_Y: i32 = STATUS_BAR_Y + ROW_GAP;
        const BOTTOM_ROW_H: i32 = 10;
        const BOTTOM_ROW_Y: i32 = 64 - BOTTOM_ROW_H;
        const ICON: i32 = 16;
        const STATUS_ROW_Y: i32 = BOTTOM_ROW_Y - 1 - ICON + 1;
        let st = self.st();
        self.statusbar(c);
        // Large temperature, right-aligned in a fixed box so digits do not shift.
        let digits_w = c.text_width(&FUB20, "100.0");
        let unit_w = c.text_width(&PROFONT17, "°C");
        let start_x = (128 - (digits_w + ROW_GAP + unit_w)) / 2;
        c.text_right(
            &FUB20,
            start_x,
            digits_w,
            TEMP_Y,
            &fmt(format_args!("{:.1}", st.temperature)),
        );
        c.text(
            &PROFONT17,
            start_x + digits_w + ROW_GAP,
            TEMP_Y + (23 - 15) / 2,
            "°C",
        );
        // Status row.
        let ready = st.temperature >= st.setpoint - cc_core::machine::HEATING_LOGO_DELTA_C;
        let at = st.near_setpoint(self.cfg().display_blinking_delta);
        if !(ready && at && !st.blink_on()) {
            let label = if ready { "READY" } else { "HEATING" };
            let tw = c.text_width(&PROFONT11, label);
            let x = (128 - (ICON + 4 + tw)) / 2;
            if ready {
                c.frame_rect(x + 3, STATUS_ROW_Y + 2, 9, 8);
                c.line(x + 12, STATUS_ROW_Y + 3, x + 15, STATUS_ROW_Y + 3);
                c.line(x + 12, STATUS_ROW_Y + 7, x + 15, STATUS_ROW_Y + 7);
                c.line(x + 4, STATUS_ROW_Y + 14, x + 11, STATUS_ROW_Y + 14);
            } else {
                c.triangle(
                    (x + 8, STATUS_ROW_Y),
                    (x + 3, STATUS_ROW_Y + 12),
                    (x + 13, STATUS_ROW_Y + 12),
                );
                c.fill_rect(x + 5, STATUS_ROW_Y + 12, 6, 3);
                c.pixel(x + 8, STATUS_ROW_Y + 4);
            }
            c.text(&PROFONT11, x + ICON + 4, STATUS_ROW_Y + (ICON - 11) / 2, label);
        }
        // Temperature-to-setpoint bar with label.
        if st.setpoint > 20.0 {
            const BAR_W: i32 = 72;
            let label_w = c.text_width(&PROFONT10, "100°C");
            let x = (128 - (BAR_W + 3 + label_w)) / 2;
            let y = BOTTOM_ROW_Y + (BOTTOM_ROW_H - 4) / 2;
            c.frame_rect(x, y, BAR_W, 4);
            let inner = BAR_W - 2;
            let fill = |t: f64| map(t as i64, 20, st.setpoint as i64, 0, i64::from(inner)).clamp(0, inner);
            let f = fill(st.temperature);
            if f > 0 {
                c.fill_rect(x + 1, y + 1, f, 2);
            }
            let ready_t = st.setpoint - cc_core::machine::HEATING_LOGO_DELTA_C;
            if ready_t > 20.0 {
                c.vline(x + 1 + fill(ready_t), y - 1, 6);
            }
            let s = fmt(format_args!("{:.0}°C", round0(st.setpoint)));
            c.text_right(&PROFONT10, x + BAR_W + 3, label_w, BOTTOM_ROW_Y, &s);
        }
    }

    fn modern_brew(&self, c: &mut Canvas<'_>, flushing: bool) {
        let st = self.st();
        let cfg = self.cfg();
        // Phase indicator.
        if flushing {
            c.text(&PROFONT10, 2, 1, "FLUSHING");
        } else {
            let active = match st.state {
                State::BrewPreinfusion | State::BrewPreinfusionPause => 0,
                State::BrewFinished => 2,
                _ => 1,
            };
            for (i, (label, x)) in [("PRE-INF", 2), ("BREW", 46), ("DONE", 86)]
                .into_iter()
                .enumerate()
            {
                if i > 0 {
                    c.text(&PROFONT10, x - 8, 1, ">");
                }
                if i == active {
                    let w = c.text_width(&PROFONT10, label);
                    c.fill_rect(x - 1, 0, w + 2, 10);
                    c.color = false;
                    c.text(&PROFONT10, x, 1, label);
                    c.color = true;
                } else {
                    c.text(&PROFONT10, x, 1, label);
                }
            }
        }
        c.hline(0, STATUS_BAR_Y, 128);
        // "25 s - 93.4°C" in ProFont15: ProFont17 (the C++ choice) is 135 px wide.
        let time_w = c.text_width(&PROFONT15, "999 s");
        let sep_w = c.text_width(&PROFONT15, " - ");
        let temp_w = c.text_width(&PROFONT15, "100.0°C");
        let x = (128 - (time_w + sep_w + temp_w)) / 2;
        let y = STATUS_BAR_Y + 2;
        c.text_right(
            &PROFONT15,
            x,
            time_w,
            y,
            &fmt(format_args!("{} s", (st.brew_time_ms / 1000.0) as i32)),
        );
        c.text(&PROFONT15, x + time_w, y, " - ");
        c.text_right(
            &PROFONT15,
            x + time_w + sep_w,
            temp_w,
            y,
            &fmt(format_args!("{:.1}°C", st.temperature)),
        );
        // Progress towards the target.
        if !flushing {
            let auto = cfg.brew_mode == BrewMode::Automatic;
            let by_weight = auto && cfg.brew_by_weight_enabled && cfg.hardware_sensors_scale_enabled;
            let target = if by_weight && cfg.brew_by_weight_target_weight > 0.0 {
                Some((
                    f64::from(st.brew_weight) / cfg.brew_by_weight_target_weight,
                    cfg.brew_by_weight_target_weight,
                    "g",
                ))
            } else if auto && cfg.brew_by_time_enabled && st.brew_target_ms > 0.0 {
                Some((
                    st.brew_time_ms / st.brew_target_ms,
                    st.brew_target_ms / 1000.0,
                    "s",
                ))
            } else {
                None
            };
            const BAR_W: i32 = 88;
            let row_y = 54;
            let bar_y = row_y + 3;
            match target {
                Some((ratio, t, unit)) => {
                    let label_w = c.text_width(&PROFONT10, "/ 999s");
                    let bx = (128 - (BAR_W + 3 + label_w)) / 2;
                    c.frame_rect(bx, bar_y, BAR_W, 4);
                    let fill = ((ratio.clamp(0.0, 1.0)) * f64::from(BAR_W - 2)) as i32;
                    if fill > 0 {
                        c.fill_rect(bx + 1, bar_y + 1, fill, 2);
                    }
                    let s = fmt(format_args!("/ {:.0}{unit}", round0(t)));
                    c.text_right(&PROFONT10, bx + BAR_W + 3, label_w, row_y, &s);
                }
                None => c.frame_rect((128 - BAR_W) / 2, bar_y, BAR_W, 4),
            }
        }
        // Footer: weight and pressure.
        let mut x = 4;
        if cfg.hardware_sensors_scale_enabled {
            let s = match self.weight_target() {
                Some(t) if t > 0.0 => fmt(format_args!("{:.1}g/{:.0}g", st.brew_weight, round0(t))),
                _ => fmt(format_args!("{:.1} g", st.brew_weight)),
            };
            x += c.text(&PROFONT10, x, 43, &s) + 10;
        }
        if cfg.hardware_sensors_pressure_enabled {
            c.text(
                &PROFONT10,
                x,
                43,
                &fmt(format_args!("{:.1} bar", st.pressure.unwrap_or(0.0))),
            );
        }
    }

    fn modern_post_brew(&self, c: &mut Canvas<'_>) {
        let cup_x = (128 - bm::BREW_CUP_LOGO.width as i32) / 2;
        c.xbm(cup_x, 2, &bm::BREW_CUP_LOGO);
        let ms = self.st().brew_time_ms as i64;
        let s = fmt(format_args!("{}.{}", ms / 1000, (ms % 1000) / 100));
        let digits_w = c.text_width(&PROFONT17, "999.9");
        let unit_w = c.text_width(&PROFONT17, " s");
        let x = (128 - (digits_w + unit_w)) / 2;
        let y = 2 + bm::BREW_CUP_LOGO.height as i32 + 4;
        c.text_right(&PROFONT17, x, digits_w, y, &s);
        c.text(&PROFONT17, x + digits_w, y, " s");
    }
}

fn ota(c: &mut Canvas<'_>, o: Ota<'_>) {
    match o {
        Ota::Failed(msg) => {
            // FUB17 "Update failed" is wider than the panel (C++ overflowed).
            c.text_centered(&PROFONT15, 4, "Update failed");
            let msg = if msg.is_empty() { "Unknown error" } else { msg };
            let mut s: String<64> = String::new();
            for ch in msg.chars() {
                let _ = s.push(ch);
                if c.text_width(&PROFONT10, &s) > 124 {
                    s.pop();
                    break;
                }
            }
            c.text_centered(&PROFONT10, 32, &s);
            c.text_centered(&PROFONT10, 48, "Retry from web UI");
        }
        Ota::Done => {
            c.text_centered(&FUB17, 8, "Update OK");
            c.text_centered(&PROFONT11, 36, "Restarting...");
        }
        Ota::Running { percent } => {
            c.text_centered(&FUB17, 2, "Updating");
            c.text_centered(&PROFONT10, 24, "Firmware");
            Screens::progress(c, f64::from(percent), 14, 38, 100);
            c.text_centered(&PROFONT11, 48, &fmt(format_args!("{percent}%")));
        }
    }
}

fn ota_upright(c: &mut Canvas<'_>, o: Ota<'_>) {
    match o {
        Ota::Failed(msg) => {
            c.text(&PROFONT11, 0, 4, "Update");
            c.text(&PROFONT11, 0, 14, "failed");
            wrapped_at(
                c,
                &PROFONT10,
                0,
                34,
                if msg.is_empty() { "Unknown error" } else { msg },
            );
        }
        Ota::Done => {
            c.text(&PROFONT11, 0, 40, "Update OK");
            c.text(&PROFONT10, 0, 56, "Restarting...");
        }
        Ota::Running { percent } => {
            c.text(&PROFONT11, 0, 40, "Updating");
            Screens::progress(c, f64::from(percent), 2, 60, 60);
            c.text_centered(&PROFONT11, 70, &fmt(format_args!("{percent}%")));
        }
    }
}

/// Up to six lines at y = 0, 10, ... (the C++ `displayMessage`).
fn message(c: &mut Canvas<'_>, lines: &[&str]) {
    for (i, l) in lines.iter().enumerate() {
        c.text(&PROFONT11, 0, i as i32 * 10, l);
    }
}

fn wrapped(c: &mut Canvas<'_>, font: &FontRenderer, text: &str) {
    wrapped_at(c, font, 0, 0, text);
}

/// Word-wrapped text; `\n` forces a break. A word wider than a whole line is split.
fn wrapped_at(c: &mut Canvas<'_>, font: &FontRenderer, x: i32, mut y: i32, text: &str) {
    const LINE_H: i32 = 12;
    let max_w = c.width() - x;
    for para in text.split('\n') {
        let mut line: String<96> = String::new();
        for word in para.split(' ').filter(|w| !w.is_empty()) {
            let mut candidate = line.clone();
            if !candidate.is_empty() {
                let _ = candidate.push(' ');
            }
            let _ = candidate.push_str(word);
            if c.text_width(font, &candidate) <= max_w {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                c.text(font, x, y, &line);
                y += LINE_H;
                line.clear();
            }
            for ch in word.chars() {
                let mut next = line.clone();
                let _ = next.push(ch);
                if !line.is_empty() && c.text_width(font, &next) > max_w {
                    c.text(font, x, y, &line);
                    y += LINE_H;
                    line.clear();
                    let _ = line.push(ch);
                } else {
                    line = next;
                }
            }
        }
        if !line.is_empty() {
            c.text(font, x, y, &line);
        }
        y += LINE_H;
    }
}

#[cfg(any(test, feature = "samples"))]
pub mod samples;
#[cfg(test)]
mod tests;
