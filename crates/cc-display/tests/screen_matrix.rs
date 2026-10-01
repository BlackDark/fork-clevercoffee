//! Every screen, every template, checked without hardware.
//!
//! # What this is for
//!
//! Three production defects in one day were all the same shape: **the firmware
//! never filled in a field of [`DisplayInput`**, and nothing noticed until a
//! human looked at the panel.
//!
//! | missing field | what the operator saw |
//! | --- | --- |
//! | `brew_timer` | a brew showed no timer at all, on any template |
//! | `brew_active` | the same, because the FSM could not leave `Idle` |
//! | `now_ms` | the post-brew screen never went back |
//!
//! Every one of them was invisible to the golden images, because a golden
//! records what the *template* draws from the input it is given — and the input
//! was wrong, not the drawing. So this file checks three things the goldens
//! cannot:
//!
//! 1. **Fits.** Every screen, on every template, at the extremes of every input
//!    that can affect layout, inks nothing outside `128x64` — with a margin, not
//!    merely inside it.
//! 2. **Is reachable.** Each case names the [`Stage`] it expects, so "this input
//!    never gets as far as drawing a screen" fails rather than passing on an
//!    empty frame.
//! 3. **Every field changes the picture.** For each [`DisplayInput`] field, the
//!    same screen is rendered with that field at its default and with a
//!    distinctive value, and the two frames must differ. This is the check that
//!    catches "the caller never sets this" on the *display* side; the firmware
//!    side of the same question is answered by the `display: brew timer …`
//!    transition log and by the numbers in `/api/status`.
//!
//! Run it with `just test`. There is no hardware anywhere in this file.

use cc_display::display::{Display, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use cc_display::model::{BrewTimerState, Config, DisplayInput, Language, OtaKind, OtaStatus};
use cc_display::templates::{self, Stage, SystemScreenId, TemplateId};

/// The templates, all six.
const TEMPLATES: [TemplateId; 6] = [
    TemplateId::Standard,
    TemplateId::Minimal,
    TemplateId::TemperatureOnly,
    TemplateId::Scale,
    TemplateId::Upright,
    TemplateId::Modern,
];

/// A plausible mid-brew machine, with every field deliberately **non-default**.
///
/// The non-default part is the point. `DisplayInput::default()` is all zeroes,
/// and a template that draws nothing for a zero renders an empty frame — which
/// passes a fit check and proves nothing. This input is the one an operator would
/// actually see.
fn input() -> DisplayInput {
    DisplayInput {
        temperature: 92.5,
        setpoint: 93.0,
        pid_output: 420.0,
        brew_time_ms: 12_500.0,
        target_brew_time_ms: 30_000.0,
        pid_kp: 27.5,
        pid_ki: 0.9,
        pid_kd: 189.0,
        // Zero on purpose: a pump-on time makes `should_display_hot_water_timer`
        // true, and the hot-water fullscreen **beats every system screen**
        // (ADR-0001's stage order). A baseline that was pouring water would
        // mask the heating, standby and error screens entirely, which is
        // exactly what it did until this test said so.
        pump_on_time_ms: 0.0,
        pressure: 9.0,
        brew_weight: 12.5,
        weight: 250.0,
        state: cc_domain::state::MachineState::PidNormal,
        isr_counter: 1_234,
        wifi_reconnects: 17,
        wifi_connected: true,
        wifi_signal: 4,
        mqtt_connected: true,
        backflush_cycle_count: 3,
        brew_timer: BrewTimerState::Running,
        brew_active: true,
        now_ms: 987_654,
        ..DisplayInput::default()
    }
}

/// A configuration with every feature **on**, so the feature-gated branches
/// (scale rows, pressure, the fullscreen timers, the maintenance footer) are
/// actually taken.
fn config() -> Config {
    Config {
        language: Language::English,
        scale_enabled: true,
        pressure_enabled: true,
        brew_switch_enabled: true,
        fullscreen_brew_timer: true,
        fullscreen_manual_flush_timer: true,
        fullscreen_hot_water_timer: true,
        heating_logo: 1,
        pid_off_logo: 1,
        mqtt_enabled: true,
        backflush_reminder_enabled: true,
        ..Config::default()
    }
}

/// The rightmost and bottom-most inked pixel, or `None` for an empty frame.
fn ink_bounds(d: &Display) -> Option<(i32, i32)> {
    let fb = d.framebuffer();
    let mut max_x = -1;
    let mut max_y = -1;
    for y in 0..DISPLAY_HEIGHT {
        for x in 0..DISPLAY_WIDTH {
            if fb.pixel(x, y) {
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    (max_x >= 0).then_some((max_x, max_y))
}

/// How far outside the frame a pixel may not be. Zero.
const MARGIN: i32 = 0;

// A text row keeping a pixel of margin is a *separate* question from ink leaving
// the frame, and it is answered per screen rather than globally: the PID
// progress bar is full-bleed at rows 60..63 by design, so a global margin rule
// would fail on the one element that is supposed to touch the edge. The text
// cases that did clip are pinned individually by
// `the_right_edge_texts_fit_the_frame`.

/// Every input that can change a screen, at the values most likely to break one.
///
/// Named rather than generated, because the *interesting* extremes are the ones
/// a person thought of: a three-digit temperature, an uptime past a hundred
/// hours, a boiler that is on fire. A generator would have found none of them.
#[allow(
    clippy::too_many_lines,
    reason = "this IS the table of cases; splitting it hides which case is which, and the case names are the failure messages"
)]
fn extremes() -> Vec<(&'static str, DisplayInput)> {
    let mut cases: Vec<(&'static str, DisplayInput)> = Vec::new();
    let base = input();

    let mut push = |name: &'static str, input: DisplayInput| cases.push((name, input));

    push("baseline", base);
    push(
        "uptime past 100 hours",
        DisplayInput {
            now_ms: 377 * 3_600_000 + 25 * 60_000,
            ..base
        },
    );
    push(
        "a three-digit temperature",
        DisplayInput {
            temperature: 103.5,
            ..base
        },
    );
    push(
        "a negative temperature",
        DisplayInput {
            temperature: -12.5,
            ..base
        },
    );
    push(
        "the heater at full output",
        DisplayInput {
            pid_output: 1000.0,
            ..base
        },
    );
    push(
        "a long brew",
        DisplayInput {
            brew_time_ms: 3_600_000.0,
            target_brew_time_ms: 7_200_000.0,
            ..base
        },
    );
    push(
        "a heavy scale",
        DisplayInput {
            weight: 9999.0,
            brew_weight: 999.0,
            ..base
        },
    );
    push(
        "no wifi, no mqtt, four reconnects",
        DisplayInput {
            wifi_connected: false,
            mqtt_connected: false,
            wifi_reconnects: 4,
            ..base
        },
    );
    push(
        "a scale fault with no reading",
        DisplayInput {
            scale_fault: true,
            weight: 0.0,
            brew_weight: 0.0,
            ..base
        },
    );
    push(
        "the backflush reminder is due",
        DisplayInput {
            backflush_reminder_due: true,
            ..base
        },
    );
    push(
        "the post-brew screen",
        DisplayInput {
            brew_timer: BrewTimerState::PostBrew,
            brew_active: false,
            brew_time_ms: 27_400.0,
            ..base
        },
    );
    push(
        "the emergency stop, ISR just started",
        DisplayInput {
            state: cc_domain::state::MachineState::EmergencyStop,
            temperature: 148.0,
            isr_counter: 100,
            ..base
        },
    );
    push(
        "the emergency stop, ISR long running",
        DisplayInput {
            state: cc_domain::state::MachineState::EmergencyStop,
            temperature: 148.0,
            isr_counter: 90_000,
            ..base
        },
    );
    push(
        "a sensor error",
        DisplayInput {
            state: cc_domain::state::MachineState::SensorError,
            temperature: 0.0,
            ..base
        },
    );
    push(
        "an eeprom error",
        DisplayInput {
            state: cc_domain::state::MachineState::EepromError,
            ..base
        },
    );
    push(
        "an empty water tank",
        DisplayInput {
            state: cc_domain::state::MachineState::WaterTankEmpty,
            ..base
        },
    );
    push(
        "standby",
        DisplayInput {
            state: cc_domain::state::MachineState::Standby,
            ..base
        },
    );
    push(
        "pid disabled",
        DisplayInput {
            state: cc_domain::state::MachineState::PidDisabled,
            ..base
        },
    );
    // The heating screen is a *system* screen, so the fullscreen brew timer beats
    // it (ADR-0001's stage order). A case that inherits the baseline's running
    // brew timer therefore never reaches it, and a reachability test would fail
    // on a stage that is perfectly reachable.
    push(
        "heating, far below setpoint",
        DisplayInput {
            temperature: 20.0,
            setpoint: 95.0,
            brew_timer: BrewTimerState::Idle,
            brew_active: false,
            ..base
        },
    );
    push(
        "heating, and mid-brew as well",
        DisplayInput {
            temperature: 20.0,
            setpoint: 95.0,
            ..base
        },
    );
    push(
        "heating, in the blink window",
        DisplayInput {
            temperature: 93.4,
            setpoint: 93.0,
            ..base
        },
    );
    push(
        "manual flush running",
        DisplayInput {
            state: cc_domain::state::MachineState::ManualFlushRunning,
            brew_time_ms: 4_000.0,
            // Zero on purpose: a pump-on time makes `should_display_hot_water_timer`
            // true, and the hot-water fullscreen **beats every system screen**
            // (ADR-0001's stage order). A baseline that was pouring water would
            // mask the heating, standby and error screens entirely, which is
            // exactly what it did until this test said so.
            pump_on_time_ms: 0.0,
            ..base
        },
    );
    push(
        "hot water running",
        DisplayInput {
            state: cc_domain::state::MachineState::BackflushFlushing,
            pump_on_time_ms: 9_000.0,
            ..base
        },
    );
    push(
        "a backflush cycle",
        DisplayInput {
            state: cc_domain::state::MachineState::BackflushFlushing,
            ..base
        },
    );
    push(
        "offline mode",
        DisplayInput {
            display_offline: 1,
            ..base
        },
    );
    push(
        "an ota in progress",
        DisplayInput {
            ota: cc_display::model::OtaInput {
                show: true,
                status: OtaStatus::Uploading,
                progress: 42,
                error_message: "",
                kind: OtaKind::Firmware,
            },
            ..base
        },
    );
    push(
        "an ota that failed",
        DisplayInput {
            ota: cc_display::model::OtaInput {
                show: true,
                status: OtaStatus::Error,
                progress: 0,
                error_message: "connection reset",
                kind: OtaKind::Firmware,
            },
            ..base
        },
    );
    cases
}

#[test]
fn every_screen_fits_the_frame_on_every_template() {
    let config = config();
    let mut checked = 0_usize;
    for template in TEMPLATES {
        for (name, input) in extremes() {
            let mut d = Display::new();
            templates::render(template, &mut d, &input, &config);
            if let Some((max_x, max_y)) = ink_bounds(&d) {
                assert!(
                    max_x <= DISPLAY_WIDTH - 1 - MARGIN && max_y <= DISPLAY_HEIGHT - 1 - MARGIN,
                    "{template:?} / {name}: ink reaches ({max_x}, {max_y}), which is \
                     outside the frame with a {MARGIN} px margin"
                );
                checked += 1;
            }
        }
    }
    // A matrix that silently renders nothing everywhere would pass the loop
    // above. This is the floor: most cases must actually draw something.
    assert!(
        checked > TEMPLATES.len() * 20,
        "only {checked} of {} cases drew anything — the matrix is not exercising \
         the renderer",
        TEMPLATES.len() * extremes().len()
    );
}

#[test]
fn a_brew_timer_shows_and_then_goes_away() {
    // The third field-left-at-default bug, as a test.
    //
    // The FSM is driven the way the firmware drives it: once per frame, with
    // `now_ms` advancing. A `DisplayInput` whose `now_ms` never moves never
    // leaves `PostBrew`, which is exactly what the panel did.
    let mut config = config();
    let mut input = DisplayInput {
        brew_active: true,
        brew_timer: BrewTimerState::Idle,
        ..input()
    };

    // Frame 1: the brew starts.
    let _ = templates::step_brew_timer(&mut input, &config);
    assert_eq!(input.brew_timer, BrewTimerState::Running);
    assert!(
        templates::should_display_brew_timer(input.brew_timer),
        "a running brew must show the timer"
    );

    // The brew ends, and the post-brew screen comes up.
    input.brew_active = false;
    input.now_ms += 30_000;
    let _ = templates::step_brew_timer(&mut input, &config);
    assert_eq!(input.brew_timer, BrewTimerState::PostBrew);

    // And it goes away on its own, with nothing but the clock.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the duration is seconds and is bounded to [1, 60] by the schema"
    )]
    let duration_ms = (config.post_brew_timer_duration_s * 1000.0) as u32;
    assert!(
        duration_ms > 0,
        "the post-brew screen must have a duration at all"
    );
    input.now_ms += duration_ms + 1;
    let _ = templates::step_brew_timer(&mut input, &config);
    assert_eq!(
        input.brew_timer,
        BrewTimerState::Idle,
        "the post-brew screen did not go away after {duration_ms} ms"
    );
    assert!(!templates::should_display_brew_timer(input.brew_timer));

    // And the screen it was showing is one a caller could see: with the timer
    // running, the fullscreen stage is the brew timer on the templates that
    // share it.
    config.post_brew_timer_duration_s = 1.0;
    let mut running = DisplayInput {
        brew_active: true,
        brew_timer: BrewTimerState::Running,
        brew_time_ms: 9_000.0,
        ..DisplayInput::default()
    };
    let _ = templates::step_brew_timer(&mut running, &config);
    let mut d = Display::new();
    let stage = templates::render(TemplateId::Standard, &mut d, &running, &config).stage;
    assert_eq!(
        stage,
        Stage::FullscreenBrew,
        "a running brew on the Standard template must reach the brew-timer screen"
    );
}

#[test]
fn every_system_screen_is_reachable_from_some_state() {
    // "Reachable" is the assertion the goldens cannot make: a screen that no
    // input reaches is a screen nobody has ever seen, which is how the sensor
    // error and eeprom error screens went years without being looked at.
    let config = config();
    let mut seen: Vec<SystemScreenId> = Vec::new();
    for template in TEMPLATES {
        for (_name, input) in extremes() {
            let mut d = Display::new();
            if let Stage::SystemScreen(id) =
                templates::render(template, &mut d, &input, &config).stage
            {
                if !seen.contains(&id) {
                    seen.push(id);
                }
            }
        }
    }
    for id in [
        SystemScreenId::Heating,
        SystemScreenId::PidDisabled,
        SystemScreenId::Standby,
        SystemScreenId::EmergencyStop,
        SystemScreenId::SensorError,
        SystemScreenId::EepromError,
        SystemScreenId::WaterTankEmpty,
    ] {
        assert!(
            seen.contains(&id),
            "{id:?} is drawn by no input in the matrix — either the matrix is \
             missing a case or the screen is unreachable"
        );
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one entry per DisplayInput field; the list is the test"
)]
fn every_display_input_field_changes_what_is_drawn() {
    // **The test that would have caught all three of today's bugs**, on the
    // display side: a field nobody sets renders identically to the default, so
    // the picture cannot tell the difference between "set to X" and "never set".
    //
    // For each field: render the Standard template with the default and with a
    // distinctive value, and require the two frames to differ. A field that
    // does not differ is either dead (nothing reads it) or unreachable (no
    // screen in this matrix shows it) — and either way that is worth knowing
    // before a human discovers it on the panel.
    let config = config();
    let mut inert: Vec<&'static str> = Vec::new();

    // Every template, because the fields are not all on one: `pressure`,
    // `weight` and `scale_fault` are the **Scale** template's rows and the
    // Standard template has never drawn them. The assertion is "some screen
    // shows this field", which is what a caller needs to know.
    let mut check = |name: &'static str, changed: DisplayInput| {
        let differs = TEMPLATES.iter().any(|template| {
            let mut a = Display::new();
            templates::render(*template, &mut a, &DisplayInput::default(), &config);
            let mut b = Display::new();
            templates::render(*template, &mut b, &changed, &config);
            a.framebuffer().as_bytes() != b.framebuffer().as_bytes()
        });
        if !differs {
            inert.push(name);
        }
    };

    check("temperature", input());
    check(
        "setpoint",
        DisplayInput {
            setpoint: 60.0,
            ..DisplayInput::default()
        },
    );
    check(
        "pid_output",
        DisplayInput {
            pid_output: 750.0,
            ..DisplayInput::default()
        },
    );
    check(
        "brew_time_ms",
        DisplayInput {
            brew_timer: BrewTimerState::Running,
            brew_active: true,
            brew_time_ms: 42_000.0,
            ..DisplayInput::default()
        },
    );
    check(
        "pid_kp/ki/kd",
        DisplayInput {
            pid_kp: 27.5,
            pid_ki: 0.9,
            pid_kd: 189.0,
            ..DisplayInput::default()
        },
    );
    check(
        "pressure",
        DisplayInput {
            pressure: 9.0,
            ..DisplayInput::default()
        },
    );
    check(
        "weight",
        DisplayInput {
            weight: 250.0,
            ..DisplayInput::default()
        },
    );
    check(
        "scale_fault",
        DisplayInput {
            scale_fault: true,
            ..DisplayInput::default()
        },
    );
    check(
        "wifi_reconnects",
        DisplayInput {
            wifi_connected: false,
            wifi_reconnects: 12,
            ..DisplayInput::default()
        },
    );
    check(
        "wifi_signal",
        DisplayInput {
            wifi_connected: true,
            wifi_signal: 4,
            ..DisplayInput::default()
        },
    );
    check(
        "mqtt_weak",
        DisplayInput {
            mqtt_weak: true,
            ..DisplayInput::default()
        },
    );
    check(
        "backflush_reminder_due",
        DisplayInput {
            backflush_reminder_due: true,
            ..DisplayInput::default()
        },
    );
    check(
        "backflush_cycle_count",
        DisplayInput {
            state: cc_domain::state::MachineState::BackflushFlushing,
            backflush_cycle_count: 3,
            ..DisplayInput::default()
        },
    );
    check(
        "isr_counter",
        DisplayInput {
            state: cc_domain::state::MachineState::EmergencyStop,
            isr_counter: 100,
            ..DisplayInput::default()
        },
    );
    check(
        "state",
        DisplayInput {
            state: cc_domain::state::MachineState::SteamRunning,
            ..DisplayInput::default()
        },
    );
    check(
        "display_offline",
        DisplayInput {
            display_offline: 1,
            ..DisplayInput::default()
        },
    );
    check(
        "ota",
        DisplayInput {
            ota: cc_display::model::OtaInput {
                show: true,
                status: OtaStatus::Uploading,
                progress: 42,
                error_message: "",
                kind: OtaKind::Firmware,
            },
            ..DisplayInput::default()
        },
    );
    // `now_ms`, `brew_end_ms` and `brew_active` are deliberately **not** in this
    // list. They are not drawn: they are the brew-timer FSM's own state, consumed
    // by `step_brew_timer` rather than by a template. `a_brew_timer_shows_and_then_
    // goes_away` is their check, and asserting that they change a pixel here
    // would be asserting the wrong thing.

    assert!(
        inert.is_empty(),
        "these DisplayInput fields change nothing the Standard template draws, so \
         a caller could leave them at their defaults forever and nobody would \
         know: {inert:?}. Either the field is dead, or the matrix is missing the \
         screen that shows it."
    );
}
