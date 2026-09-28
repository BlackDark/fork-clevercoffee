//! Golden images for the six templates and every shared stage.
//!
//! # What a golden catches that the parity oracle cannot
//!
//! `tests/parity.rs` proves that every draw call in [`cc_display::display`]
//! produces the U8g2 pixel. It does *not* prove the templates issue the right
//! calls: a template that draws its temperature row at y=41 instead of y=14
//! passes engine parity perfectly and shows a wrong screen on the panel.
//!
//! So the templates are driven here directly, with no scenario in between, and
//! the frame is compared against a committed P4 image. That is the check that
//! covers template *logic*.
//!
//! # Why a scenario transcription is not enough either
//!
//! Hand-transcribing each template's draw calls into a scenario would make the
//! two sides agree by construction -- the transcription encodes my reading of
//! the C++, and a misreading would be invisible. A golden of the *actual* Rust
//! template has no such problem: it records what the code does, and a change to
//! the code changes the image.
//!
//! It also means the goldens do not prove the templates match the C++ --
//! `tests/parity.rs` does that for the shared stages, through the
//! `shared_*.txt` transcriptions. The six *normal* layouts have no oracle
//! counterpart, because the C++ template classes are methods on
//! `UICoordinator` and pull their values through `SystemContext`; running them
//! off-device would mean reimplementing `SystemContext`, at which point the
//! oracle would no longer be measuring U8g2. `tests/parity.rs` says the same
//! thing about its own limits.
//!
//! # Each case carries the stage it expects
//!
//! A golden proves the pixels; the expected [`Stage`] proves *which* code path
//! produced them. The two fail differently. A wrong pixel is a layout bug. A
//! wrong stage is a policy bug in the ADR-0001 order -- Modern falling through
//! to the offline splash, the OTA screen ceasing to win -- and it can render a
//! perfectly plausible image of the wrong screen. So both are checked, from one
//! list, so a case cannot assert an image for a stage it did not reach.
//!
//! # Regenerating
//!
//! ```text
//! just snapshot-display
//! ```
//!
//! **Read the diff before committing a regenerated golden.** Every pixel is
//! supposed to stay put; a change is either a deliberate layout fix or a bug,
//! and the image is the only way to tell.
//!
//! Compiled out without the `scenarios` feature, for the reason given in
//! `tests/parity.rs`.
#![cfg(feature = "scenarios")]

mod support;

use std::path::PathBuf;

use cc_display::display::{Display, Framebuffer};
use cc_display::model::{
    BrewMode, BrewTimerState, Config, DisplayInput, Language, OtaInput, OtaKind, OtaStatus,
    ScaleType,
};
use cc_display::templates::{self, Stage, SystemScreenId, TemplateId};

/// One golden: a name, the input that produces it, the stage it must reach, and
/// the frame it must produce.
struct Snapshot {
    name: String,
    template: TemplateId,
    input: DisplayInput,
    /// The case's own config. Stored because two of the system screens are
    /// gated on `pid_off_logo`, so a case that reaches them needs a config the
    /// default does not have. Keeping it here is what lets the stage assertion
    /// re-render independently instead of reusing the golden's own render.
    config: Config,
    /// Human-readable, for the failure message. `Stage` is not `Display` and
    /// adding a `Display` impl to the library for a test message is not worth
    /// it.
    stage: &'static str,
    stage_value: Stage,
    frame: Framebuffer,
}

/// Where the committed images live.
fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// An input with everything filled in, so a case only states what it is about.
///
/// A `DisplayInput` built field by field is unreadable at forty fields, and a
/// `default` input would render six near-empty screens differing only in which
/// numbers happen to be zero. This is a plausible mid-brew machine; each case
/// overrides only the two or three fields it is about.
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
        pump_on_time_ms: 4_000.0,
        pressure: 9.0,
        brew_weight: 12.5,
        weight: 250.0,
        state: cc_domain::state::MachineState::PidNormal,
        isr_counter: 0,
        wifi_connected: true,
        wifi_signal: 3,
        mqtt_connected: true,
        ..DisplayInput::default()
    }
}

fn config() -> Config {
    Config {
        language: Language::English,
        ..Config::default()
    }
}

/// Render one case and record the stage that produced it.
fn snapshot(
    name: &str,
    stage: &'static str,
    template: TemplateId,
    input: DisplayInput,
    config: &Config,
) -> Snapshot {
    let mut d = Display::new();
    d.set_display_rotation(config.rotation());
    let rendered = templates::render(template, &mut d, &input, config);
    Snapshot {
        name: name.to_string(),
        template,
        input,
        config: *config,
        stage,
        stage_value: rendered.stage.clone(),
        frame: rendered.display.into_framebuffer(),
    }
}

/// Every case, in one list, so the generator and the comparison cannot disagree
/// about which images should exist.
///
/// A case added here without a committed golden fails; a golden with no case
/// behind it is reported as stale.
fn cases() -> Vec<Snapshot> {
    let cfg = config();
    let mut out = Vec::new();

    // ---- The six normal layouts, in one plausible mid-brew state.
    for t in TemplateId::ALL {
        out.push(snapshot(template_name(t), "Normal", t, input(), &cfg));
    }

    // ---- The same six while heating, which is where the templates differ most:
    // Modern's temperature bar fills, Standard's PID row goes to zero, and
    // Upright is the only one with a portrait progress bar.
    // 61 °C against a 93 °C setpoint, which is far enough out to need the bar
    // and close enough *not* to trip `is_heating_logo_condition_met` -- that
    // would hand the frame to the heating system screen and the case would no
    // longer be about the template.
    let heating = DisplayInput {
        temperature: 61.0,
        setpoint: 93.0,
        pid_output: 1000.0,
        brew_timer: BrewTimerState::Idle,
        brew_active: false,
        ..input()
    };
    for t in TemplateId::ALL {
        out.push(snapshot(
            &format!("{}_heating", template_name(t)),
            "Normal",
            t,
            heating.clone(),
            &cfg,
        ));
    }

    // ---- Offline, which every template shares.
    for t in TemplateId::ALL {
        let offline = DisplayInput {
            offline: true,
            ..input()
        };
        out.push(snapshot(
            &format!("{}_offline", template_name(t)),
            "Offline",
            t,
            offline,
            &cfg,
        ));
    }

    // ---- The three fullscreen timers, on Modern.
    //
    // Per ADR-0001 the timers are shared stages and only Modern opts into all
    // three, so Modern is the one frame that exercises every timer.
    let timers: [(&str, DisplayInput, &str); 3] = [
        (
            "fullscreen_brew",
            DisplayInput {
                brew_timer: BrewTimerState::Running,
                brew_active: true,
                ..input()
            },
            "FullscreenBrew",
        ),
        (
            "fullscreen_manual_flush",
            DisplayInput {
                state: cc_domain::state::MachineState::ManualFlushRunning,
                brew_timer: BrewTimerState::Idle,
                ..input()
            },
            "FullscreenManualFlush",
        ),
        (
            "fullscreen_hot_water",
            DisplayInput {
                pump_on_time_ms: 4_000.0,
                brew_timer: BrewTimerState::Idle,
                ..input()
            },
            "FullscreenHotWater",
        ),
    ];
    for (name, i, stage) in timers {
        out.push(snapshot(name, stage, TemplateId::Modern, i, &cfg));
    }

    // ---- Every shared system screen.
    //
    // The trigger for each is a different field, and the dispatch is the C++'s
    // ordered `if` chain, so one case per screen is the only way to cover all
    // nine.
    //
    // The triggers are the C++'s, not guesses. Two of them are gated on a
    // *config* flag (`pid_off_logo`), so those cases need a config too, which is
    // why this loop carries one.
    let pid_off = Config {
        pid_off_logo: 1,
        ..cfg
    };
    let screens: [(&str, DisplayInput, SystemScreenId, Config); 9] = [
        (
            "screen_heating",
            DisplayInput {
                temperature: 45.0,
                ..input()
            },
            SystemScreenId::Heating,
            cfg,
        ),
        (
            "screen_pid_disabled",
            DisplayInput {
                state: cc_domain::state::MachineState::PidDisabled,
                ..input()
            },
            SystemScreenId::PidDisabled,
            pid_off,
        ),
        (
            "screen_standby",
            DisplayInput {
                state: cc_domain::state::MachineState::Standby,
                ..input()
            },
            SystemScreenId::Standby,
            pid_off,
        ),
        (
            "screen_steam",
            DisplayInput {
                state: cc_domain::state::MachineState::SteamRunning,
                ..input()
            },
            SystemScreenId::Steam,
            cfg,
        ),
        (
            "screen_water_tank_empty",
            DisplayInput {
                state: cc_domain::state::MachineState::WaterTankEmpty,
                ..input()
            },
            SystemScreenId::WaterTankEmpty,
            cfg,
        ),
        (
            "screen_backflush",
            DisplayInput {
                state: cc_domain::state::MachineState::BackflushIdle,
                ..input()
            },
            SystemScreenId::Backflush,
            cfg,
        ),
        (
            "screen_emergency_stop",
            DisplayInput {
                state: cc_domain::state::MachineState::EmergencyStop,
                ..input()
            },
            SystemScreenId::EmergencyStop,
            cfg,
        ),
        (
            "screen_sensor_error",
            DisplayInput {
                state: cc_domain::state::MachineState::SensorError,
                ..input()
            },
            SystemScreenId::SensorError,
            cfg,
        ),
        (
            "screen_eeprom_error",
            DisplayInput {
                state: cc_domain::state::MachineState::EepromError,
                ..input()
            },
            SystemScreenId::EepromError,
            cfg,
        ),
    ];
    for (name, i, id, c) in screens {
        out.push(snapshot(name, stage_name(id), TemplateId::Modern, i, &c));
    }

    // The backflush screen has three bodies, not one: the two idle-ish states
    // show a prompt and everything else shows the cycle counter. One case would
    // cover only the first.
    for (name, state) in [
        (
            "screen_backflush_finished",
            cc_domain::state::MachineState::BackflushFinished,
        ),
        (
            "screen_backflush_filling",
            cc_domain::state::MachineState::BackflushFilling,
        ),
        (
            "screen_backflush_flushing",
            cc_domain::state::MachineState::BackflushFlushing,
        ),
    ] {
        out.push(snapshot(
            name,
            stage_name(SystemScreenId::Backflush),
            TemplateId::Modern,
            DisplayInput {
                state,
                backflush_cycle_count: 2,
                ..input()
            },
            &cfg,
        ));
    }

    // ---- OTA, in the states that render differently.
    //
    // `OtaInput::show` is what gates the screen, and `kind` is what the title
    // says ("Firmware" vs "Filesystem"), so both are varied.
    for (name, status, kind) in [
        ("ota_firmware", OtaStatus::Uploading, OtaKind::Firmware),
        ("ota_filesystem", OtaStatus::Uploading, OtaKind::Filesystem),
        ("ota_processing", OtaStatus::Processing, OtaKind::Firmware),
        ("ota_complete", OtaStatus::Complete, OtaKind::Firmware),
        ("ota_failed", OtaStatus::Error, OtaKind::Firmware),
    ] {
        let i = DisplayInput {
            ota: OtaInput {
                show: true,
                status,
                kind,
                progress: 50,
                ..OtaInput::default()
            },
            ..input()
        };
        out.push(snapshot(name, "Ota", TemplateId::Modern, i, &cfg));
    }

    // ---- Brew by weight and by time, which change Modern's target field.
    let by_weight = DisplayInput {
        brew_weight: 18.0,
        ..input()
    };
    out.push(snapshot(
        "modern_brew_by_weight",
        "Normal",
        TemplateId::Modern,
        by_weight,
        &{
            let mut c = cfg;
            c.brew_mode = BrewMode::Automatic;
            c.brew_by_weight_enabled = true;
            c.brew_by_weight_target = 36.0;
            c
        },
    ));

    let by_time = DisplayInput {
        brew_timer: BrewTimerState::Running,
        brew_active: true,
        ..input()
    };
    out.push(snapshot(
        "modern_brew_by_time",
        "Normal",
        TemplateId::Modern,
        by_time,
        &{
            let mut c = cfg;
            c.brew_mode = BrewMode::Automatic;
            c.brew_by_weight_enabled = false;
            c
        },
    ));

    // ---- Post-brew, the one state the timer FSM reaches on its own.
    out.push(snapshot(
        "modern_post_brew",
        "Normal",
        TemplateId::Modern,
        DisplayInput {
            brew_timer: BrewTimerState::PostBrew,
            brew_active: false,
            brew_time_ms: 30_000.0,
            now_ms: 3_000,
            ..input()
        },
        &cfg,
    ));

    // ---- A scale fault, which replaces the weight with a word.
    out.push(snapshot(
        "scale_fault",
        "Normal",
        TemplateId::Scale,
        DisplayInput {
            scale_fault: true,
            ..input()
        },
        &{
            let mut c = cfg;
            c.scale_enabled = true;
            c.scale_type = ScaleType::Hx711;
            c
        },
    ));

    // ---- Every language, on the template with the most translated text.
    for (i, lang) in [Language::English, Language::German, Language::Spanish]
        .into_iter()
        .enumerate()
    {
        out.push(snapshot(
            &format!("modern_language_{i}"),
            "Normal",
            TemplateId::Modern,
            input(),
            &{
                let mut c = cfg;
                c.language = lang;
                c
            },
        ));
    }

    // ---- The other three rotations, on the same template.
    //
    // A rotation is a *display* property, not a template one: `Config::rotation`
    // is `inverted * 2 + upright_template`, and it is the config that is sent
    // to the device, not the `TemplateId`. So the same template has to come out
    // right in all four, and these three cases plus `standard` cover them.
    for (i, (inverted, upright)) in [(true, false), (false, true), (true, true)]
        .into_iter()
        .enumerate()
    {
        out.push(snapshot(
            &format!("standard_rotation_{i}"),
            "Normal",
            TemplateId::Standard,
            input(),
            &{
                let mut c = cfg;
                c.inverted = inverted;
                c.upright_template = upright;
                c
            },
        ));
    }

    out
}

fn template_name(t: TemplateId) -> &'static str {
    match t {
        TemplateId::Standard => "standard",
        TemplateId::Scale => "scale",
        TemplateId::Minimal => "minimal",
        TemplateId::TemperatureOnly => "temperature_only",
        TemplateId::Upright => "upright",
        TemplateId::Modern => "modern",
    }
}

fn stage_name(id: SystemScreenId) -> &'static str {
    match id {
        SystemScreenId::Heating => "SystemScreen(Heating)",
        SystemScreenId::PidDisabled => "SystemScreen(PidDisabled)",
        SystemScreenId::Standby => "SystemScreen(Standby)",
        SystemScreenId::Steam => "SystemScreen(Steam)",
        SystemScreenId::WaterTankEmpty => "SystemScreen(WaterTankEmpty)",
        SystemScreenId::Backflush => "SystemScreen(Backflush)",
        SystemScreenId::EmergencyStop => "SystemScreen(EmergencyStop)",
        SystemScreenId::SensorError => "SystemScreen(SensorError)",
        SystemScreenId::EepromError => "SystemScreen(EepromError)",
    }
}

#[test]
#[ignore = "regenerates the committed images; run via `just snapshot-display`"]
fn render_goldens() {
    let dir = golden_dir();
    std::fs::create_dir_all(&dir).expect("can create the golden directory");
    let cases = cases();
    for c in &cases {
        std::fs::write(
            dir.join(format!("{}.ppm", c.name)),
            support::to_ppm(&c.frame),
        )
        .expect("can write the golden");
    }
    println!("wrote {} goldens to {}", cases.len(), dir.display());
}

#[test]
fn every_case_reaches_the_stage_its_image_shows() {
    let mut failures = Vec::new();
    for c in cases() {
        // Re-render from the stored input and config rather than reusing the
        // frame, so the stage assertion is independent of the render that
        // produced the image.
        let mut d = Display::new();
        d.set_display_rotation(c.config.rotation());
        let rendered = templates::render(c.template, &mut d, &c.input, &c.config);
        if rendered.stage != c.stage_value {
            failures.push(format!(
                "  {}: expected {}, got {:?}",
                c.name, c.stage, rendered.stage
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "stage mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_golden_matches_its_template() {
    let dir = golden_dir();
    let cases = cases();
    assert!(!cases.is_empty(), "the golden corpus is empty");

    let mut missing = Vec::new();
    let mut failures = Vec::new();

    for c in &cases {
        let path = dir.join(format!("{}.ppm", c.name));
        if !path.is_file() {
            missing.push(c.name.clone());
            continue;
        }
        let d = support::diff(&support::read_golden(&path), &c.frame);
        if d.differing() != 0 {
            failures.push(format!("  {}: {}", c.name, d.summary()));
        }
    }

    // A golden with no case behind it is dead weight that will silently rot.
    let mut stale = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.filter_map(std::result::Result::ok) {
            let stem = e
                .path()
                .file_stem()
                .map_or(String::new(), |s| s.to_string_lossy().into_owned());
            if e.path().extension().is_some_and(|x| x == "ppm")
                && !cases.iter().any(|c| c.name == stem)
            {
                stale.push(stem);
            }
        }
    }
    stale.sort();

    assert!(
        missing.is_empty() && failures.is_empty() && stale.is_empty(),
        "goldens do not match the templates.\n  missing: {}\n  differing:\n{}\n  stale (no case behind them): {}\n\
         Regenerate with `just snapshot-display`, then read the diff before committing.",
        missing.join(", "),
        failures.join("\n"),
        stale.join(", ")
    );
}

#[test]
fn the_corpus_actually_draws_something() {
    // Guards the guard: if every case rendered an empty frame, the comparison
    // above would be satisfied by empty goldens.
    let cases = cases();
    let ink: usize = cases.iter().map(|c| c.frame.lit_count()).sum();
    assert!(
        ink > 20_000,
        "the golden corpus only drew {ink} lit pixels; is it empty?"
    );
    for c in &cases {
        assert!(
            c.frame.lit_count() > 0,
            "{} rendered an empty frame",
            c.name
        );
    }
}
