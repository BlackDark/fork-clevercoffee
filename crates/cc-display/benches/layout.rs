//! Rendering a frame must not touch the heap.
//!
//! # Why this exists
//!
//! A 2026-10 review found `just bench` named two bench targets — `reducers` and
//! `layout` — and **neither existed**, so the recipe failed with
//! `no bench target named 'reducers'`. A documented-but-nonexistent gate is
//! worse than none, because everyone reading the recipe assumes it ran. This is
//! the `layout` one.
//!
//! What it measures is the thing that matters on a device with ~320 KB of RAM:
//! **heap allocations per frame.** `cc-display` is `no_std` with no `alloc` in
//! the device build, so a nonzero count here means a device build would not link
//! at all — which makes this a real invariant, not a micro-optimisation.
//!
//! The timing figure is reported for scale only. At 100 ms per refresh
//! (`display_task::REFRESH_MS`) a frame has an enormous budget on the LX6; the
//! cost that would actually hurt is the allocation. The four per-tick
//! allocations in `cc-machine` were found by exactly this method.
//!
//! Number formatting goes through `fixed_str::String<24>` (`cc-display`'s
//! `fmt.rs`), hand-matched to C's `printf` rounding and checked against an
//! 80,000-line oracle generated from the real U8G2. That is why the count is
//! zero: the display crate formats into stack buffers because a display cannot
//! afford a heap.
//!
//! # Running it
//!
//! ```sh
//! just bench
//! ```

#[path = "alloc.rs"]
mod alloc;

use std::time::Instant;

use cc_display::display::Display;
use cc_display::model::{BrewTimerState, Config, DisplayInput};
use cc_display::templates::{self, TemplateId};

use alloc::{measured_per, nanos_per, per, Counting};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Frames for the reported figure. Module scope because
/// `clippy::items_after_statements` is right that an item declared mid-function
/// reads as if it only existed from that point.
const REPORTED_FRAMES: u64 = 120_000;

/// All six templates, the way the golden-image suite enumerates them.
const TEMPLATES: [TemplateId; 6] = [
    TemplateId::Standard,
    TemplateId::Minimal,
    TemplateId::TemperatureOnly,
    TemplateId::Scale,
    TemplateId::Upright,
    TemplateId::Modern,
];

/// The frame an operator actually sees: heating, with a scale reading on the
/// panel. Every field that can affect layout is at a value that *uses* pixels --
/// a template that short-circuits on a zero would otherwise skip the drawing
/// this bench exists to time.
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

/// Render one frame and consume the result, so nothing can be optimised away
/// and the bench measures a frame that was actually produced.
fn render_once(display: &mut Display, input: &DisplayInput, config: &Config, template: TemplateId) {
    let rendered = templates::render(template, display, input, config);
    // Touch the output so nothing can be optimised away, and so the bench
    // measures a frame that was actually produced rather than discarded.
    std::hint::black_box(rendered);
}

fn main() {
    let config = Config::default();
    let input = input();
    let mut display = Display::new();
    render_once(&mut display, &input, &config, TemplateId::Standard);

    let mark = alloc::counters();
    let started = Instant::now();
    #[allow(
        clippy::cast_possible_truncation,
        reason = "TEMPLATES.len() is 6, a literal, so the truncation is exact"
    )]
    let rounds = REPORTED_FRAMES / TEMPLATES.len() as u64;
    for _ in 0..rounds {
        for template in TEMPLATES {
            render_once(&mut display, &input, &config, template);
        }
    }
    let elapsed = started.elapsed();
    let frames = rounds * TEMPLATES.len() as u64;
    let (allocs, bytes) = alloc::delta_since(mark);

    println!(
        "display frame: {frames} frames across {} templates",
        TEMPLATES.len()
    );
    println!(
        "  {:>9.0} ns/frame (host x86_64; the LX6 is several times slower)",
        nanos_per(elapsed, frames)
    );
    println!(
        "  {:>9.4} ms of the 100 ms panel refresh",
        measured_per(elapsed.as_secs_f64() * 1_000.0, frames)
    );
    println!(
        "  {allocs:>10} allocations total ({:.4} per frame)",
        per(allocs, frames)
    );
    println!(
        "  {bytes:>10} bytes total ({:.1} per frame)",
        per(bytes, frames)
    );
    assert_eq!(allocs, 0, "rendering allocated {allocs} times");
}
