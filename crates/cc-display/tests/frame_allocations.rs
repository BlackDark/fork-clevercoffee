//! Rendering a frame must not touch the heap.
//!
//! This is the *gate*. `benches/layout.rs` reports the same number for a human
//! to watch over time; this fails the build. Both include `benches/alloc.rs`, so
//! the two cannot measure different things.
//!
//! Why it is an invariant and not a micro-optimisation: `cc-display` is
//! `no_std` with **no `alloc` in the device build**, so a nonzero count here
//! does not mean a slow frame — it means the firmware would not link. Number
//! formatting goes through `fixed_str::String<24>` (`cc-display/src/fmt.rs`),
//! hand-matched to C's `printf` rounding and checked against an 80,000-line
//! oracle generated from the real U8G2. That is why the count is zero: a
//! display cannot afford a heap.
//!
//! The same review noted that the display bench target this recipe named
//! (`layout`) did not exist at all until now, so nothing was checking this.

#[path = "../benches/alloc.rs"]
mod alloc;

use cc_display::display::Display;
use cc_display::model::{BrewTimerState, Config, DisplayInput};
use cc_display::templates::{self, TemplateId};

use alloc::Counting;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// All six templates, the way the golden-image suite enumerates them.
const TEMPLATES: [TemplateId; 6] = [
    TemplateId::Standard,
    TemplateId::Minimal,
    TemplateId::TemperatureOnly,
    TemplateId::Scale,
    TemplateId::Upright,
    TemplateId::Modern,
];

/// Six templates times ten rounds. Enough to be impossible to miss an
/// allocation in, cheap enough to run in milliseconds.
const FRAMES: u64 = 60;

/// The frame an operator actually sees: heating, with a scale reading on the
/// panel. Every field that can affect layout is at a value that *uses* pixels --
/// a template that short-circuits on a zero would otherwise skip the drawing
/// this is checking.
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
        brew_timer: BrewTimerState::Idle,
        brew_active: false,
        now_ms: 987_654,
        ..DisplayInput::default()
    }
}

#[test]
fn a_frame_does_not_allocate() {
    let config = Config::default();
    let input = input();
    let mut display = Display::new();
    // Warm up: the first call initialises lazily-initialised statics that are not
    // a frame's cost.
    std::hint::black_box(templates::render(
        TemplateId::Standard,
        &mut display,
        &input,
        &config,
    ));

    let mark = alloc::counters();
    #[allow(
        clippy::cast_possible_truncation,
        reason = "TEMPLATES.len() is 6, a literal, so the truncation is exact"
    )]
    let rounds = FRAMES / TEMPLATES.len() as u64;
    for _ in 0..rounds {
        for template in TEMPLATES {
            std::hint::black_box(templates::render(template, &mut display, &input, &config));
        }
    }
    let (allocs, bytes) = alloc::delta_since(mark);

    assert_eq!(
        allocs, 0,
        "rendering {FRAMES} frames allocated {allocs} times ({bytes} bytes). \
         `cc-display` is `no_std` with no `alloc` in the device build, so a \
         nonzero count here means the firmware would not link."
    );
}
