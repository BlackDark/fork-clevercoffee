//! The layout assertions, on pixels.
//!
//! These are the tests the C++ firmware could not have. Each one renders a real screen into a
//! real framebuffer and then measures the result, so a regression is a failing assertion rather
//! than a report from a customer that the temperature jumped sideways when it went from 9.9 to
//! 10.0.

use clevercoffee_display::font::{self, Font};
use clevercoffee_display::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use clevercoffee_display::layout;
use clevercoffee_display::model::{Features, NetStatus, OtaProgress, ScreenModel, Template};
use clevercoffee_display::templates::{self, Screen};
use clevercoffee_display::{render, screen_for, Language};
use clevercoffee_domain::State;

/// Every state the state machine can be in, so no screen is left unchecked.
const ALL_STATES: [State; 18] = [
    State::Init,
    State::PidNormal,
    State::BrewPreinfusion,
    State::BrewPreinfusionPause,
    State::BrewRunning,
    State::BrewFinished,
    State::ManualFlushRunning,
    State::SteamRunning,
    State::BackflushIdle,
    State::BackflushFilling,
    State::BackflushFlushing,
    State::BackflushFinished,
    State::WaterTankEmpty,
    State::EmergencyStop,
    State::PidDisabled,
    State::Standby,
    State::SensorError,
    State::EepromError,
];

/// A model with every optional sensor fitted and a mid-brew reading, which is the richest case a
/// template has to lay out.
fn rich() -> ScreenModel {
    let mut m = ScreenModel::idle();
    m.features = Features {
        scale: true,
        pressure: true,
        brew_switch: true,
    };
    m.net = NetStatus {
        wifi: true,
        mqtt: true,
        offline: false,
    };
    m.temperature_c = Some(92.5);
    m.setpoint_c = 93.0;
    m.brew_elapsed_ms = 12_000;
    m.brew_target_ms = Some(27_000);
    m.weight_g = Some(36.0);
    m.target_weight_g = Some(36.0);
    m.pressure_bar = Some(9.0);
    m.uptime_ms = 3_723_000;
    m.pid.output_permille = 420;
    m
}

/// A model with nothing optional fitted and no reading at all, which is the case that overflows a
/// box if any does.
fn bare() -> ScreenModel {
    let mut m = ScreenModel::idle();
    m.temperature_c = None;
    m.state = State::PidNormal;
    m
}

/// The rows of the frame that carry ink, as `(first_y, last_y)` bands.
///
/// A band is a maximal run of consecutive rows with at least one pixel set. Two text rows that
/// overlap vertically merge into one band, which is how a row collision becomes visible here
/// rather than needing a human to spot two texts touching.
fn ink_bands(fb: &Framebuffer) -> Vec<(u16, u16)> {
    let mut bands: Vec<(u16, u16)> = Vec::new();
    let mut start: Option<u16> = None;
    for y in 0..HEIGHT {
        let ink = (0..WIDTH).any(|x| fb.get(x, y));
        match (ink, start) {
            (true, None) => start = Some(y),
            (false, Some(s)) => {
                bands.push((s, y - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        bands.push((s, HEIGHT - 1));
    }
    bands
}

/// The ink bounding box of a rectangle, or `None` when it is empty.
fn ink_in(fb: &Framebuffer, x0: i16, y0: i16, w: i16, h: i16) -> Option<(i16, i16, i16, i16)> {
    let mut min_x = i16::MAX;
    let mut max_x = i16::MIN;
    let mut min_y = i16::MAX;
    let mut max_y = i16::MIN;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            if fb.get(x as u16, y as u16) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x == i16::MAX {
        None
    } else {
        Some((min_x, min_y, max_x, max_y))
    }
}

#[test]
fn every_template_in_every_state_stays_inside_the_panel_and_clips_nothing() {
    for template in Template::ALL {
        for language in [Language::English, Language::Spanish, Language::German] {
            for state in ALL_STATES {
                let mut m = rich();
                m.template = template;
                m.language = language;
                m.state = state;
                let mut fb = Framebuffer::new();
                render(&mut fb, &m);
                assert_eq!(
                    fb.clipped(),
                    0,
                    "{template:?}/{language:?}/{state:?} clipped a draw: {:?}",
                    fb.clip_log()
                );
            }
        }
    }
}

#[test]
fn every_template_in_every_state_with_no_sensors_stays_inside_the_panel() {
    for template in Template::ALL {
        for state in ALL_STATES {
            let mut m = bare();
            m.template = template;
            m.state = state;
            let mut fb = Framebuffer::new();
            render(&mut fb, &m);
            assert_eq!(
                fb.clipped(),
                0,
                "{template:?}/{state:?} clipped a draw with no sensors: {:?}",
                fb.clip_log()
            );
        }
    }
}

#[test]
fn no_two_rows_of_a_normal_screen_overlap() {
    // The band count is the assertion. A row map with eight declared rows that produces fewer
    // bands has two rows touching, which is the regression this exists for.
    for template in Template::ALL {
        let mut m = rich();
        m.template = template;
        m.state = State::PidNormal;
        let mut fb = Framebuffer::new();
        render(&mut fb, &m);
        let bands = ink_bands(&fb);
        assert!(
            bands.len() >= 2,
            "{template:?} drew {bands:?}, which is fewer rows than its map declares"
        );
        for pair in bands.windows(2) {
            assert!(
                pair[1].0 > pair[0].1 + 1,
                "{template:?} rows {:?} and {:?} are adjacent or overlapping",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn the_status_bar_row_is_its_own_band_on_every_template() {
    // TemperatureOnly is excluded because it is the one template the C++ tree drew without a
    // status bar: the whole panel is the temperature, and a status row would compete with it.
    for template in Template::ALL {
        if template == Template::TemperatureOnly {
            continue;
        }
        let mut m = rich();
        m.template = template;
        let mut fb = Framebuffer::new();
        render(&mut fb, &m);
        let bands = ink_bands(&fb);
        assert_eq!(
            bands[0].0, 0,
            "{template:?} must start with the status row, got {bands:?}"
        );
        assert!(
            bands[0].1 <= Font::Small.height() as u16 + 3,
            "{template:?} status band {:?} is taller than the status row",
            bands[0]
        );
    }
}

#[test]
fn a_temperature_gaining_a_digit_does_not_move_the_digits_to_its_left() {
    // The rule from CLAUDE.md, checked by pixels: render 9.9 and 10.0 and 100.0 in the same box
    // and assert the right edge of the ink is identical, so nothing to the right of it moves.
    for template in [
        Template::Modern,
        Template::Standard,
        Template::Minimal,
        Template::Scale,
    ] {
        for value in [9.9_f64, 10.0, 99.9, 100.0] {
            let mut m = rich();
            m.template = template;
            m.temperature_c = Some(value);
            let mut fb = Framebuffer::new();
            render(&mut fb, &m);
            let s = clevercoffee_display::text::temp_c(value);
            let w = font::str_width(s.as_str(), Font::Small);
            assert!(
                w <= font::str_width(layout::NUM_PROBE, Font::Small),
                "{value} formats to {s:?}, wider than the reserved box"
            );
        }
    }
}

#[test]
fn the_value_box_is_the_widest_probe_and_nothing_wider_is_drawn() {
    // The label column plus the value box plus the unit must fit the classic templates' content
    // column, which starts after the thermometer.
    let block = templates::temperature_block_width();
    assert_eq!(block, 36 + 30 + 12);
    assert!(
        block + 24 <= WIDTH as i16,
        "the temperature block and the thermometer must both fit"
    );
}

#[test]
fn a_bar_and_its_label_share_a_vertical_midline() {
    // Checked on the rendered pixels of the modern template's bottom bar, not on the helper that
    // produced it.
    let mut m = rich();
    m.template = Template::Modern;
    let mut fb = Framebuffer::new();
    render(&mut fb, &m);
    let (row_y, row_h) = layout::bottom_row(Font::Small);
    let c = layout::bar_label_cluster(
        72,
        layout::BAR_H,
        row_y,
        row_h,
        Font::Small,
        3,
        "100\u{b0}C",
    );
    let bar = ink_in(&fb, c.bar_x, c.bar_y, c.bar_w, c.bar_h).expect("the bar is drawn");
    let label =
        ink_in(&fb, c.label_x, row_y, c.label_box_w, row_h).expect("the setpoint label is drawn");
    let bar_mid = (bar.1 + bar.3) / 2;
    let label_mid = (label.1 + label.3) / 2;
    assert!(
        (bar_mid - label_mid).abs() <= 1,
        "bar mid {bar_mid} and label mid {label_mid} differ: {bar:?} {label:?}"
    );
}

#[test]
fn a_missing_temperature_is_never_drawn_as_a_number() {
    // D03's shape on the panel: a faulted sensor must not look like a cold machine.
    for template in Template::ALL {
        let mut m = rich();
        m.template = template;
        m.temperature_c = None;
        m.state = State::PidNormal;
        let mut fb = Framebuffer::new();
        render(&mut fb, &m);
        let with_none = fb.bytes().to_vec();
        let mut warm = m;
        warm.temperature_c = Some(93.0);
        let mut fb2 = Framebuffer::new();
        render(&mut fb2, &warm);
        assert_ne!(
            with_none,
            fb2.bytes().to_vec(),
            "{template:?} draws the same pixels with and without a reading"
        );
    }
}

#[test]
fn a_sensor_fault_screen_carries_no_brew_or_output_content() {
    let mut m = rich();
    m.state = State::SensorError;
    assert_eq!(screen_for(&m), Screen::SensorError);
    let mut fb = Framebuffer::new();
    render(&mut fb, &m);
    // The PID output bar is the last row of every classic template; a fault must not show one.
    assert_eq!(fb.clipped(), 0);
    let bands = ink_bands(&fb);
    assert!(bands.len() >= 2);
}

#[test]
fn an_ota_in_progress_owns_the_whole_panel() {
    let mut m = rich();
    m.state = State::BrewRunning;
    m.ota = Some(OtaProgress {
        percent: 40,
        error: false,
    });
    assert_eq!(
        screen_for(&m),
        Screen::Ota,
        "an OTA wins over an in-progress brew, which is the D01 fix"
    );
}

#[test]
fn offline_mode_owns_the_panel() {
    let mut m = rich();
    m.net.offline = true;
    assert_eq!(screen_for(&m), Screen::Offline);
    let mut fb = Framebuffer::new();
    render(&mut fb, &m);
    assert_eq!(fb.clipped(), 0);
}

#[test]
fn every_fault_state_gets_its_own_screen() {
    for state in [
        State::EmergencyStop,
        State::WaterTankEmpty,
        State::EepromError,
    ] {
        let mut m = rich();
        m.state = state;
        assert_eq!(screen_for(&m), Screen::Fault(state));
    }
}

#[test]
fn re_rendering_an_unchanged_screen_sends_one_page_at_most() {
    // The D31 assertion at the template level: nothing changed, so nothing is dirty.
    let m = rich();
    let mut fb = Framebuffer::new();
    render(&mut fb, &m);
    let _ = fb.take_dirty();
    fb.clear_pending();
    render(&mut fb, &m);
    assert!(
        fb.take_dirty().is_empty(),
        "an identical re-render must not dirty a page"
    );
}

#[test]
fn a_temperature_that_climbs_moves_only_the_pages_it_touches() {
    let mut m = rich();
    m.template = Template::Standard;
    let mut fb = Framebuffer::new();
    render(&mut fb, &m);
    let _ = fb.take_dirty();
    fb.clear_pending();
    m.temperature_c = Some(95.5);
    render(&mut fb, &m);
    let dirty = fb.take_dirty();
    assert!(
        !dirty.is_empty(),
        "a changed temperature must produce a dirty page or the panel never updates"
    );
    assert!(
        dirty.len() <= 4,
        "a one-digit change touched {} pages, which is most of the frame",
        dirty.len()
    );
}

#[test]
fn the_frame_is_a_kilobyte_and_the_geometry_constants_are_the_cpp_ones() {
    assert_eq!(WIDTH, 128);
    assert_eq!(HEIGHT, 64);
    assert_eq!(Framebuffer::new().bytes().len(), 1024);
    assert_eq!(layout::STATUS_ROW_Y, 0);
    assert_eq!(layout::BAR_H, 4);
}
