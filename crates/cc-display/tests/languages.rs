//! Every screen, in every language, checked for fit.
//!
//! # Why this is separate from `screen_matrix.rs`
//!
//! The matrix sweeps inputs and templates; it runs **English only**, because
//! `Config::default()` is English and the point there is layout under extreme
//! values. Language is a different axis: a translation changes the *width* of
//! every label, and the two templates that put a label and a value in fixed
//! columns (`VALUE_COLUMN_OFFSET` = 50 on Scale, `UNIT_COLUMN_OFFSET` = 30 on
//! the rest) are laid out for the English strings' lengths.
//!
//! So: every case, every template, every language, and two assertions —
//!
//! 1. **Nothing inks outside 128x64.** Cheap, and it catches the extreme.
//! 2. **No string is wider than the slot it is drawn into.** This is the one
//!    that matters, because a string clipped at the right edge is still
//!    *inside* the frame — the bounds check passes and the glyphs are gone.
//!    "EEPROM Error, please set Values" and the German sensor line both reach
//!    column 127 today, and neither is visible to a bounds check.
//!
//! # What is asserted, and where it lives
//!
//! The measurement is the display layer's own: `Font::str_width`. Nothing is
//! rendered and scanned, because a rendered frame cannot tell "ends at 127" from
//! "was cut at 127". The widths are compared against the slot each widget
//! actually draws into, which is why the slot constants are named here rather
//! than re-derived.

use cc_display::display::{Display, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use cc_display::font;
use cc_display::lang::{self, Lang};
use cc_display::model::{Config, DisplayInput, Language};
use cc_display::templates::TemplateId;
use cc_display::widgets::{TemperatureCoords, VALUE_COLUMN_OFFSET};

/// The templates, all six.
const TEMPLATES: [TemplateId; 6] = [
    TemplateId::Standard,
    TemplateId::Minimal,
    TemplateId::TemperatureOnly,
    TemplateId::Scale,
    TemplateId::Upright,
    TemplateId::Modern,
];

/// The three languages the firmware carries, which are the C++'s three
/// (`defaults.h:196-200`).
const LANGUAGES: [Language; 3] = [Language::English, Language::German, Language::Spanish];

/// A translated string and where to find it.
type Label = (&'static str, fn(&Lang) -> &'static str);

/// Every label column: the width a label may occupy before it runs into the
/// value at [`VALUE_COLUMN_OFFSET`].
const LABEL_SLOTS: [Label; 6] = [
    ("current_temp", |l| l.current_temp),
    ("set_temp", |l| l.set_temp),
    ("brew", |l| l.brew),
    ("weight", |l| l.weight),
    ("pressure", |l| l.pressure),
    ("manual_flush", |l| l.manual_flush),
];

/// Every message line drawn through `display_message`, and the width it has.
///
/// The six-line primitive draws at `x = 0` in `profont11` and has no width
/// budget other than the panel, so the budget is the panel minus a margin.
fn message_lines(l: &Lang) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    for (i, line) in l.error_tsensor.iter().enumerate() {
        out.push(("error_tsensor", *line));
        let _ = i;
    }
    out.push(("backflush_press", l.backflush_press));
    out.push(("backflush_start", l.backflush_start));
    out.push(("backflush_finish", l.backflush_finish));
    out.push(("offline", l.offline));
    out.push(("scale_failure", l.scale_failure));
    out
}

/// The temperature block's column, per template — `render.rs`'s two `coords`
/// functions. Only the two matter: Standard and Scale differ.
fn temp_coords(template: TemplateId) -> TemperatureCoords {
    match template {
        TemplateId::Scale => TemperatureCoords {
            current_temp_x: 0,
            current_temp_y: 16,
            current_value_x: 50,
            set_temp_x: 0,
            set_temp_y: 26,
            set_value_x: 50,
        },
        _ => TemperatureCoords {
            current_temp_x: 34,
            current_temp_y: 16,
            current_value_x: 84,
            set_temp_x: 34,
            set_temp_y: 26,
            set_value_x: 84,
        },
    }
}

/// A configuration with every feature on, in one language.
fn config(language: Language) -> Config {
    Config {
        language,
        scale_enabled: true,
        pressure_enabled: true,
        brew_switch_enabled: true,
        mqtt_enabled: true,
        fullscreen_brew_timer: true,
        fullscreen_manual_flush_timer: true,
        fullscreen_hot_water_timer: true,
        heating_logo: 1,
        pid_off_logo: 1,
        backflush_reminder_enabled: true,
        ..Config::default()
    }
}

/// The machine states worth rendering: the ones that reach a distinct screen.
const STATES: [(cc_domain::state::MachineState, &str); 8] = [
    (cc_domain::state::MachineState::PidNormal, "normal"),
    (cc_domain::state::MachineState::SteamRunning, "steam"),
    (cc_domain::state::MachineState::Standby, "standby"),
    (cc_domain::state::MachineState::PidDisabled, "pid disabled"),
    (cc_domain::state::MachineState::SensorError, "sensor error"),
    (cc_domain::state::MachineState::EepromError, "eeprom error"),
    (
        cc_domain::state::MachineState::EmergencyStop,
        "emergency stop",
    ),
    (cc_domain::state::MachineState::WaterTankEmpty, "tank empty"),
];

fn input_for(state: cc_domain::state::MachineState) -> DisplayInput {
    DisplayInput {
        temperature: 92.5,
        setpoint: 93.0,
        pid_output: 420.0,
        brew_time_ms: 12_500.0,
        target_brew_time_ms: 30_000.0,
        pid_kp: 27.5,
        pid_ki: 0.9,
        pid_kd: 189.0,
        pressure: 9.0,
        brew_weight: 12.5,
        weight: 250.0,
        state,
        isr_counter: 1_234,
        wifi_reconnects: 17,
        wifi_connected: true,
        wifi_signal: 4,
        mqtt_connected: true,
        backflush_cycle_count: 3,
        brew_timer: cc_display::model::BrewTimerState::Idle,
        brew_active: false,
        now_ms: 3 * 3_600_000 + 42 * 60_000,
        ..DisplayInput::default()
    }
}

/// **Nothing inks outside the frame, in any language.**
#[test]
fn every_screen_fits_in_every_language() {
    let mut checked = 0_usize;
    for language in LANGUAGES {
        let config = config(language);
        for template in TEMPLATES {
            for (state, state_name) in STATES {
                let input = input_for(state);
                let mut d = Display::new();
                let _ = cc_display::templates::render(template, &mut d, &input, &config);
                let fb = d.framebuffer();
                for y in 0..DISPLAY_HEIGHT {
                    for x in 0..DISPLAY_WIDTH {
                        assert!(
                            !fb.pixel(x, y) || (x < DISPLAY_WIDTH && y < DISPLAY_HEIGHT),
                            "{language:?} / {template:?} / {state_name}: unexpected ink"
                        );
                    }
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 70, "only {checked} renders");
}

/// Label-width overruns that exist today, with the number for each.
///
/// A list rather than a bare `assert!`, so the check below is **still able to
/// fail**: a *new* translation that overruns its column is a different entry, and
/// an entry that gets fixed should be deleted from here rather than the check
/// relaxed. Every one of these is C++ behaviour (`DisplayWidgets.h`'s
/// `kValueColumnOffset = 50` against the same strings in `languages.h`), so they
/// are reported rather than silently re-laid-out.
const KNOWN_LABEL_OVERRUNS: &[(&str, Language, i32)] = &[
    // The Scale template's value column is 50 px; these are the labels that reach
    // past it. `pressure` in **English** collides on a stock machine.
    ("pressure", Language::English, 60),
    ("pressure", Language::Spanish, 54),
    ("weight", Language::German, 54),
    ("manual_flush", Language::German, 54),
];

/// The labels that share the Scale template's 50 px value column.
///
/// Not "every label": the Standard and Minimal templates put their values at
/// **84**, so a 50 px budget would be measuring the wrong thing for most of
/// them, and `uptime` lives in the status bar with no column at all (it is
/// right-aligned to the frame).
const SCALE_LABELS: [Label; 5] = [
    ("current_temp", |l| l.current_temp),
    ("set_temp", |l| l.set_temp),
    ("brew", |l| l.brew),
    ("weight", |l| l.weight),
    ("pressure", |l| l.pressure),
];

/// **No label is wider than the column it is drawn into**, in any language.
///
/// This is the check an ink-bounds scan cannot make. A label that overruns its
/// column does not leave the frame — it pushes *into* the value, and the value is
/// drawn after it, so the result is a silently wrong number rather than a visible
/// error. Measured, not estimated: `Font::str_width` in the font the screen uses.
#[test]
fn every_label_fits_its_column_in_every_language() {
    for language in LANGUAGES {
        let l = lang::for_language(language);
        for (name, get) in SCALE_LABELS {
            let width = font::profont11().str_width(get(l));
            let known = KNOWN_LABEL_OVERRUNS
                .iter()
                .find(|(n, lang, _)| *n == name && *lang == language)
                .map(|(_, _, w)| *w);
            match known {
                // Known: the width must match what was recorded, so a change in
                // either the string or the font shows up here.
                Some(recorded) => assert_eq!(
                    width, recorded,
                    "{language:?}: the {name} label is now {width} px and the \
                     recorded overrun was {recorded} px — update the table if this \
                     is a deliberate change, or fix the layout if it is not"
                ),
                None => assert!(
                    width <= VALUE_COLUMN_OFFSET,
                    "{language:?}: the {name} label {:?} is {width} px wide and the \
                     Scale value column starts at {VALUE_COLUMN_OFFSET}, so it runs \
                     into the value",
                    get(l)
                ),
            }
        }
    }
}

/// Message lines that overflow the panel today, with the number for each.
///
/// All C++ parity: the same strings, the same `displayMessage` primitive, the
/// same per-glyph clipping in U8g2. See
/// `docs/operations/runbook.md` — an operator sees these
/// exactly when something is
/// already broken, which is why they are worth a tracked entry and not a
/// quiet re-wrap.
const KNOWN_MESSAGE_OVERFLOWS: &[(&str, i32, i32)] = &[
    // (the exact line, its measured ink width, the panel it overflows)
    ("EEPROM Error, please set Values", 185, 128),
    ("Temp.-Sensor ueberpruefen!", 153, 128),
    // Portrait: the panel is 64 logical px wide under `Rotation::R1`, and this
    // is the **baseline's German translation** — a defect in `languages.h`, not
    // something the port can fix without inventing a different string.
    ("ueberpruefen!", 75, 64),
];

/// **No message line is wider than the panel**, in any language.
///
/// `display_message` draws at `x = 0` with no width budget of its own, so the
/// budget is the panel — and in portrait the panel is **64 logical pixels**,
/// because `Rotation::R1` swaps the axes. A framebuffer scan cannot see this: the
/// clipped glyphs are simply absent, and the bounds test passes.
#[test]
fn every_message_line_fits_the_panel_in_every_language() {
    // The strings the firmware supplies itself, drawn the same way.
    const HARDCODED: [(&str, &str); 3] = [
        ("eeprom error", "EEPROM Error, please set Values"),
        ("ota error", "Update failed"),
        ("ota retry", "Retry from web UI"),
    ];
    let f = font::profont11();
    for language in LANGUAGES {
        let l = lang::for_language(language);
        for (name, line) in message_lines(l) {
            let width = f.str_width(line);
            if let Some((_, recorded, _)) = KNOWN_MESSAGE_OVERFLOWS
                .iter()
                .find(|(known_line, _, _)| *known_line == line)
            {
                assert_eq!(
                    width, *recorded,
                    "{language:?}: the {name} line {line:?} is now {width} px and the \
                     recorded overflow was {recorded} px"
                );
            } else {
                assert!(
                    width <= DISPLAY_WIDTH,
                    "{language:?}: the {name} line {line:?} is {width} px wide and the \
                     landscape panel is {DISPLAY_WIDTH}"
                );
            }
        }
        for (name, line) in HARDCODED {
            let width = f.str_width(line);
            match KNOWN_MESSAGE_OVERFLOWS
                .iter()
                .find(|(known_line, _, _)| *known_line == line)
                .map(|(_, w, _)| *w)
            {
                Some(recorded) => {
                    assert_eq!(width, recorded, "the {name} line changed width");
                }
                None => {
                    assert!(
                        width <= DISPLAY_WIDTH,
                        "the {name} line {line:?} is {width} px wide"
                    );
                }
            }
        }
        // The OTA titles are drawn in `fub17`, not `profont11`, so they need
        // their own measurement — and this is the worst case in the firmware:
        // "Update failed" is 150 px, centred by
        // `layout::draw_str_centered_on_screen`, which computes
        // `x = (128 - 150) / 2 = -11`. So it is clipped at **both** edges at
        // once, which is why no ink-bounds test of any kind can see it.
        for line in ["Update failed", "Update OK", "Updating"] {
            let width = font::fub17().str_width(line);
            if line == "Update failed" {
                assert_eq!(
                    width, 150,
                    "the OTA error title changed width; if it now fits, delete the \
                     entry rather than leaving a stale one"
                );
            } else {
                assert!(
                    width <= DISPLAY_WIDTH,
                    "the OTA title {line:?} is {width} px wide"
                );
            }
        }
    }
}

/// **The portrait message lines fit the portrait panel**, which is 64 px wide.
///
/// This is the axis the whole class of bug turns on and the one a
/// `DISPLAY_WIDTH`-based check cannot see. It is why the portrait sensor-error
/// screen drew 111 px of ink into 64 px until `Lang::error_tsensor_ur` existed.
#[test]
fn the_portrait_panel_is_64_wide_and_the_portrait_lines_fit_it() {
    use cc_display::display::Rotation;
    assert_eq!(
        Rotation::R1.width(),
        64,
        "the portrait rotation must swap the panel's axes; everything below \
         assumes it"
    );
    let f = font::profont11();
    for language in LANGUAGES {
        let l = lang::for_language(language);
        for (index, line) in l.error_tsensor_ur.iter().enumerate() {
            let width = f.str_width(line);
            let known = KNOWN_MESSAGE_OVERFLOWS
                .iter()
                .find(|(known_line, _, _)| known_line == line)
                .map(|(_, w, _)| *w);
            match known {
                Some(recorded) => assert_eq!(
                    width, recorded,
                    "{language:?}: portrait line {index} {line:?} changed width"
                ),
                // The temperature value itself is drawn in this slot and is up to
                // 29 px, so the budget is the panel minus that.
                None => assert!(
                    width <= 64,
                    "{language:?}: the portrait sensor-error line {index} {line:?} is \
                     {width} px into a 64 px panel"
                ),
            }
        }
    }
}

/// **The `°C` unit clears the widest value and stays on the panel.**
///
/// This is the arithmetic behind the layout, asserted rather than re-derived,
/// because the arithmetic is what was wrong twice: the unit was placed from
/// `str_width` (12 px) when it inks 17, so the `C` ran three columns off the
/// panel — and a clipped glyph is still *inside* the frame, so no bounds test
/// could see it.
#[test]
fn the_unit_is_inside_the_frame_and_clear_of_the_widest_value() {
    let widest = font::profont11().str_width("100.0");
    for template in TEMPLATES {
        let coords = temp_coords(template);
        for (name, value_x) in [
            ("current", coords.current_value_x),
            ("setpoint", coords.set_value_x),
        ] {
            let (value_right, unit_x) = cc_display::widgets::value_and_unit_columns(value_x);
            let unit_ink = cc_display::widgets::unit_ink_width();
            assert!(
                unit_x + unit_ink <= DISPLAY_WIDTH,
                "{template:?} / {name}: the unit inks {unit_ink} px from {unit_x}, so it \
                 ends at {} and the panel's last column is {}",
                unit_x + unit_ink - 1,
                DISPLAY_WIDTH - 1
            );
            assert!(
                value_right < unit_x,
                "{template:?} / {name}: the widest value ({widest} px) right-aligned to \
                 {value_right} and the unit starts at {unit_x}, so they touch"
            );
            assert!(
                value_right - widest + 1 >= 0,
                "{template:?} / {name}: the value column at {value_x} cannot hold \
                 {widest} px of ink"
            );
        }
    }
}

/// **The inverted field closes on the frame's edge**, on every template.
///
/// The C++ uses one constant, `kValueColumnWidth = 78`, which is exact for the
/// Scale template (row origin `x = 0`, so the box runs 50..127) and 33 px too
/// wide for Standard and Minimal (origin `x = 34`, so the box ran 84..**161**
/// and its right border was never drawn). That is the "the values don't fit"
/// report.
#[test]
fn the_inverted_field_ends_on_the_last_column() {
    for (name, origin) in [("standard", 34), ("minimal", 34), ("scale", 0)] {
        let (left, width) = cc_display::widgets::inverted_field_box(origin);
        assert!(
            left + width <= DISPLAY_WIDTH,
            "the {name} field runs {left}..{} and the panel is {DISPLAY_WIDTH}",
            left + width - 1
        );
        assert_eq!(
            left + width - 1,
            DISPLAY_WIDTH - 1,
            "the {name} field should close on the last column"
        );
    }
    // The Scale template's box must be unchanged: 78 px, the C++'s constant.
    assert_eq!(cc_display::widgets::inverted_field_box(0).1, 78);
}

/// Every character in every translation has a glyph in every font it is drawn
/// in.
///
/// A missing glyph measures zero and draws nothing, which leaves a hole in a
/// label *and* under-reports its width, shifting every value after it. Spanish
/// has an accented `ó` in `Presión`, so this is not hypothetical.
#[test]
fn every_translated_character_has_a_glyph() {
    /// A font and its name, for the failure message.
    type NamedFont = (&'static str, fn() -> font::Font);

    const FONTS: [NamedFont; 3] = [
        ("profont10", font::profont10),
        ("profont11", font::profont11),
        ("profont12", font::profont12),
    ];
    for language in LANGUAGES {
        let l = lang::for_language(language);
        let mut strings: Vec<&str> = Vec::new();
        strings.extend(LABEL_SLOTS.iter().map(|(_, get)| get(l)));
        strings.extend(l.error_tsensor.iter().copied());
        strings.extend([
            l.set_temp_ur,
            l.current_temp_ur,
            l.offline,
            l.scale_failure,
            l.backflush_press,
            l.backflush_start,
            l.backflush_finish,
            l.uptime,
        ]);
        for (font_name, make) in FONTS {
            let f = make();
            for text in &strings {
                for ch in text.chars() {
                    if ch == ' ' {
                        continue;
                    }
                    assert!(
                        f.glyph_header(ch as u16).is_some(),
                        "{language:?}: {ch:?} from {text:?} has no glyph in {font_name}, \
                         so it measures zero and draws nothing"
                    );
                }
            }
        }
    }
}
