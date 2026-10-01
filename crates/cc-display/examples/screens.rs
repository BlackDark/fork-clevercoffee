//! Every screen, in one PNG, so a human can look at all of them at once.
//!
//! ```text
//! cargo run -p cc-display --features scenarios --example screens -- out.png
//! ```
//!
//! # Why this exists
//!
//! The golden images answer "did this screen change?". They do not answer "does
//! this screen look right", and three bugs on 2026-10-01 were only ever going to
//! be found by a person looking at a panel: a `°C` that was off the edge, an
//! uptime whose `m` was cut in half, a brew timer that never appeared. None of
//! them failed an assertion.
//!
//! So this renders **every reachable screen on every template** into one
//! contact sheet, labelled, in dependency-free PNG. The alternative — a PPM per
//! screen plus an external converter — produces twenty files and needs
//! `sips`/`ffmpeg`; one sheet with labels is the thing a person can actually
//! use in a code review.
//!
//! # The PNG writer
//!
//! A hand-rolled one, ~90 lines, no crate: PNG's IDAT is a zlib stream, and
//! zlib's *stored* (uncompressed) deflate blocks are legal. So the encoder is
//! chunk framing + CRC32 + Adler-32, and the cost is that the file is larger
//! than a compressed one. For a 128x64 monochrome screen that is irrelevant —
//! and a screendump is not something to compress for its own sake.
//!
//! # What is on the sheet
//!
//! One tile per (template, case), which is the cross product a person actually
//! wants to see: the same machine state on all six layouts. The labels say which
//! is which, because a sheet of 80 identical-looking rectangles is useless.

use std::io::Write as _;

use cc_display::display::{Display, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use cc_display::model::{
    BrewTimerState, Config, DisplayInput, Language, OtaInput, OtaKind, OtaStatus,
};
use cc_display::templates::TemplateId;

/// A tile: 128x64 with a caption strip under it.
const TILE_W: usize = DISPLAY_WIDTH as usize;
const TILE_H: usize = DISPLAY_HEIGHT as usize;
/// The caption band. Proportional text is out of scope; the index is drawn as a
/// tally of ticks, which is enough to name a tile against the printed list.
const CAPTION_H: usize = 8;
const GAP: usize = 8;
const COLUMNS: usize = 4;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "screens.png".into());
    let cases = cases();
    let templates = [
        TemplateId::Standard,
        TemplateId::Minimal,
        TemplateId::TemperatureOnly,
        TemplateId::Scale,
        TemplateId::Upright,
        TemplateId::Modern,
    ];

    // The two boot screens, which `templates::render` cannot produce: they are
    // `boot::draw`, called by the firmware's display task before the first frame
    // exists. They are the tiles the human reported as "the startup screen is
    // missing", so the sheet has to be able to show them.
    let boot_cases: Vec<(&str, &str, TemplateId)> = vec![
        ("boot: version", "0.1.0", TemplateId::Standard),
        ("boot: wifi address", "192.168.71.23", TemplateId::Standard),
        ("boot: version (upright)", "0.1.0", TemplateId::Upright),
        ("boot: no wifi", "Check settings", TemplateId::Standard),
    ];
    let total = templates.len() * cases.len() + boot_cases.len();
    let rows = total.div_ceil(COLUMNS);
    let sheet_w = COLUMNS * (TILE_W + GAP) + GAP;
    let sheet_h = rows * (TILE_H + CAPTION_H + GAP) + GAP;
    let mut sheet = Sheet::new(sheet_w, sheet_h);

    let mut index = 0_usize;

    for (name, line2, template) in &boot_cases {
        let (line1, _) = match *name {
            "boot: wifi address" => cc_display::boot::text::wifi_connected(line2),
            "boot: no wifi" => cc_display::boot::text::no_wifi(),
            _ => cc_display::boot::text::version(line2),
        };
        let col = index % COLUMNS;
        let row = index / COLUMNS;
        render_boot(
            &mut sheet,
            GAP + col * (TILE_W + GAP),
            GAP + row * (TILE_H + CAPTION_H + GAP),
            line1,
            line2,
            *template,
            index + 1,
        );
        println!("{index:3}  {template:<16?} {name}");
        index += 1;
    }
    for &template in &templates {
        for (name, input, config) in &cases {
            let mut d = Display::new();
            // Rotation comes from the **config**, not the template id, so the
            // Upright column has to be told it is upright or every portrait
            // coordinate is clipped away through an R0 window. The firmware does
            // this (`display_config` sets `upright_template` from
            // `display.template`); without it the sheet's Upright column was 25
            // copies of a clipped landscape screen.
            let config = if template == TemplateId::Upright && !input_is_upright(config) {
                let mut upright = *config;
                upright.upright_template = true;
                upright
            } else {
                *config
            };
            let stage = cc_display::templates::render(template, &mut d, input, &config).stage;
            let col = index % COLUMNS;
            let row = index / COLUMNS;
            let x = GAP + col * (TILE_W + GAP);
            let y = GAP + row * (TILE_H + CAPTION_H + GAP);
            sheet.blit(d.framebuffer(), x, y);
            // The caption: a tally of `index + 1` ticks, plus a marker for the
            // stage the template actually reached. A sheet you cannot key back
            // to a list is a screenshot, not a tool.
            sheet.tally(x, y + TILE_H + 1, index + 1);
            if matches!(stage, cc_display::templates::Stage::SystemScreen(_)) {
                sheet.mark(x + TILE_W - 6, y + TILE_H + 1);
            }
            println!("{index:3}  {template:<16?} {name:<34} {stage:?}");
            index += 1;
        }
    }

    sheet.write_png(&out).expect("can write the PNG");
    println!("\n{index} screens written to {out} ({sheet_w}x{sheet_h})");
}

/// One boot-screen tile. `boot::draw` is not reachable through
/// `templates::render`, so the sheet draws these itself.
fn render_boot(
    sheet: &mut Sheet,
    x: usize,
    y: usize,
    line1: &str,
    line2: &str,
    template: TemplateId,
    tally: usize,
) {
    let mut d = Display::new();
    if template == TemplateId::Upright {
        d.set_display_rotation(cc_display::display::Rotation::R1);
    }
    cc_display::boot::draw(&mut d, line1, line2, template);
    sheet.blit(d.framebuffer(), x, y);
    sheet.tally(x, y + TILE_H + 1, tally);
}

/// The cases, in the order they are printed.
#[allow(
    clippy::too_many_lines,
    reason = "this IS the table of cases; splitting it hides which case is which, and the case names are the failure messages"
)]
fn cases() -> Vec<(&'static str, DisplayInput, Config)> {
    let base = input();
    let cfg = config();
    let mut out: Vec<(&'static str, DisplayInput, Config)> = Vec::new();

    let mut push = |name: &'static str, input: DisplayInput, config: Config| {
        out.push((name, input, config));
    };

    push("idle", base, cfg);
    push(
        "heating",
        DisplayInput {
            temperature: 20.0,
            setpoint: 95.0,
            ..base
        },
        cfg,
    );
    push("brewing", base, cfg);
    push(
        "post-brew",
        DisplayInput {
            brew_timer: BrewTimerState::PostBrew,
            brew_active: false,
            ..base
        },
        cfg,
    );
    push(
        "standby",
        DisplayInput {
            state: cc_domain::state::MachineState::Standby,
            ..base
        },
        cfg,
    );
    push(
        "pid disabled",
        DisplayInput {
            state: cc_domain::state::MachineState::PidDisabled,
            ..base
        },
        cfg,
    );
    push(
        "emergency stop",
        DisplayInput {
            state: cc_domain::state::MachineState::EmergencyStop,
            temperature: 148.0,
            ..base
        },
        cfg,
    );
    push(
        "sensor error",
        DisplayInput {
            state: cc_domain::state::MachineState::SensorError,
            ..base
        },
        cfg,
    );
    push(
        "eeprom error",
        DisplayInput {
            state: cc_domain::state::MachineState::EepromError,
            ..base
        },
        cfg,
    );
    push(
        "water tank empty",
        DisplayInput {
            state: cc_domain::state::MachineState::WaterTankEmpty,
            ..base
        },
        cfg,
    );
    push(
        "steam",
        DisplayInput {
            state: cc_domain::state::MachineState::SteamRunning,
            ..base
        },
        cfg,
    );
    push(
        "manual flush",
        DisplayInput {
            state: cc_domain::state::MachineState::ManualFlushRunning,
            ..base
        },
        cfg,
    );
    push(
        "backflush filling",
        DisplayInput {
            state: cc_domain::state::MachineState::BackflushFilling,
            ..base
        },
        cfg,
    );
    push(
        "hot water",
        DisplayInput {
            state: cc_domain::state::MachineState::PidNormal,
            pump_on_time_ms: 9_000.0,
            ..base
        },
        cfg,
    );
    push(
        "backflush reminder due",
        DisplayInput {
            backflush_reminder_due: true,
            ..base
        },
        cfg,
    );
    push(
        "offline mode",
        DisplayInput {
            display_offline: 1,
            ..base
        },
        cfg,
    );
    push(
        "ota uploading",
        DisplayInput {
            ota: OtaInput {
                show: true,
                status: OtaStatus::Uploading,
                kind: OtaKind::Firmware,
                progress: 42,
                error_message: "",
            },
            ..base
        },
        cfg,
    );
    push(
        "ota failed",
        DisplayInput {
            ota: OtaInput {
                show: true,
                status: OtaStatus::Error,
                kind: OtaKind::Firmware,
                progress: 0,
                error_message: "connection reset",
            },
            ..base
        },
        cfg,
    );
    push(
        "uptime past 100 hours",
        DisplayInput {
            now_ms: 377 * 3_600_000 + 25 * 60_000,
            ..base
        },
        cfg,
    );
    push(
        "a three-digit temperature",
        DisplayInput {
            temperature: 103.5,
            ..base
        },
        cfg,
    );
    push(
        "no wifi, no mqtt",
        DisplayInput {
            wifi_connected: false,
            mqtt_connected: false,
            wifi_reconnects: 4,
            ..base
        },
        cfg,
    );
    push(
        "a scale fault",
        DisplayInput {
            scale_fault: true,
            ..base
        },
        cfg,
    );
    push("no scale, no pressure", base, Config::default());
    push(
        "german",
        base,
        Config {
            language: Language::German,
            ..cfg
        },
    );
    push(
        "spanish",
        base,
        Config {
            language: Language::Spanish,
            ..cfg
        },
    );
    push(
        "inverted",
        base,
        Config {
            inverted: true,
            ..cfg
        },
    );
    push(
        "every feature off",
        base,
        Config {
            language: Language::English,
            ..Config::default()
        },
    );
    push("the boot screen", DisplayInput::default(), cfg);
    out
}

/// The plausible mid-brew machine the tests use, kept in step deliberately.
/// Whether a config already asks for the portrait rotation.
fn input_is_upright(config: &Config) -> bool {
    config.upright_template || config.inverted
}

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
        // Idle, not Running: with the brew timer running *and*
        // `fullscreen_brew_timer` set, the fullscreen stage beats every system
        // screen, so 25 of 28 tiles rendered the same cup-and-timer frame and
        // the sheet looked like a rendering bug rather than a masking one.
        brew_timer: BrewTimerState::Idle,
        brew_active: false,
        now_ms: 3 * 3_600_000 + 42 * 60_000,
        ..DisplayInput::default()
    }
}

/// Every feature on, so the gated branches are taken.
fn config() -> Config {
    Config {
        language: Language::English,
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

/// A monochrome sheet, 1 byte per pixel, 0 = white.
struct Sheet {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Sheet {
    fn new(w: usize, h: usize) -> Self {
        // 1 = white background, 0 = ink. Inverted because a sheet that is mostly
        // black reads as a black rectangle in a browser.
        Self {
            w,
            h,
            px: vec![1; w * h],
        }
    }

    fn set(&mut self, x: usize, y: usize, ink: u8) {
        if x < self.w && y < self.h {
            self.px[y * self.w + x] = ink;
        }
    }

    /// Copy a framebuffer in, flipping it (the panel is 0 = lit).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "the loops are bounded by TILE_W/TILE_H, which are 128 and 64"
    )]
    fn blit(&mut self, fb: &cc_display::display::Framebuffer, x: usize, y: usize) {
        for row in 0..TILE_H {
            for col in 0..TILE_W {
                self.set(
                    x + col,
                    y + row,
                    u8::from(!fb.pixel(col as i32, row as i32)),
                );
            }
        }
        // A one-pixel frame around the tile, so a screen that inks the whole
        // panel still reads as a tile.
        for col in 0..=TILE_W {
            self.set(x + col, y, 0);
            self.set(x + col, y + TILE_H, 0);
        }
        for row in 0..=TILE_H {
            self.set(x, y + row, 0);
            self.set(x + TILE_W, y + row, 0);
        }
    }

    /// `n` ticks in the caption band: a 3x5 digit tally.
    fn tally(&mut self, x: usize, y: usize, n: usize) {
        for (i, digit) in n.to_string().chars().enumerate() {
            let d = digit.to_digit(10).unwrap_or(0) as usize;
            let ox = x + i * 5;
            for (dx, dy) in DIGIT_PIXELS[d] {
                self.set(ox + dx, y + dy, 0);
            }
        }
    }

    /// A filled square: "this tile reached a system screen".
    fn mark(&mut self, x: usize, y: usize) {
        for dy in 0..5 {
            for dx in 0..5 {
                self.set(x + dx, y + dy, 0);
            }
        }
    }

    /// Write an 8-bit greyscale PNG.
    ///
    /// Greyscale rather than 1-bit indexed: the framebuffer is a byte per pixel
    /// and a `P3`/palette PNG would need a palette; greyscale keeps the encoder
    /// to chunk framing and two checksums.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the sheet is a few hundred pixels across; PNG's fields are u32"
    )]
    fn write_png(&self, path: &str) -> std::io::Result<()> {
        let mut raw = Vec::with_capacity(self.h * (self.w + 1));
        for y in 0..self.h {
            raw.push(0); // filter type 0 (None) for every scanline
            for x in 0..self.w {
                raw.push(if self.px[y * self.w + x] == 0 { 0 } else { 255 });
            }
        }

        let mut png = Vec::new();
        // PNG's IHDR fields are u32; the sheet is a few hundred pixels across.
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(self.w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 0, 0, 0, 0]); // 8-bit greyscale
        chunk(&mut png, *b"IHDR", &ihdr);
        chunk(&mut png, *b"IDAT", &zlib_stored(&raw));
        chunk(&mut png, *b"IEND", &[]);

        std::fs::File::create(path)?.write_all(&png)
    }
}

/// One PNG chunk: length, type, data, CRC over type+data.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a PNG chunk is u32-length by definition; the data is a framebuffer \
              row, so it is kilobytes at most"
)]
fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(&kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// A zlib stream of stored (uncompressed) deflate blocks.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the chunks are 0xffff bytes by construction, so the u16 is exact"
)]
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // CMF/FLG: deflate, 32K window, no dictionary
    let mut chunks = data.chunks(0xffff).peekable();
    if chunks.peek().is_none() {
        // An empty final block, because a zlib stream must end with one.
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]);
    }
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(u8::from(last));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// The standard PNG CRC-32.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

/// The zlib Adler-32.
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// A 3x5 digit font, one bit per pixel, for the caption tally.
const DIGIT_PIXELS: [[(usize, usize); 9]; 10] = [
    [
        (0, 0),
        (0, 1),
        (0, 2),
        (0, 3),
        (1, 1),
        (2, 1),
        (1, 3),
        (2, 3),
        (1, 4),
    ],
    [
        (1, 0),
        (2, 0),
        (2, 1),
        (1, 2),
        (2, 2),
        (2, 3),
        (1, 4),
        (2, 4),
        (1, 4),
    ],
    [
        (0, 0),
        (1, 0),
        (2, 0),
        (2, 1),
        (1, 2),
        (0, 2),
        (0, 3),
        (0, 4),
        (1, 4),
    ],
    [
        (1, 0),
        (2, 0),
        (1, 1),
        (2, 1),
        (1, 2),
        (2, 2),
        (1, 3),
        (2, 3),
        (1, 4),
    ],
    [
        (0, 0),
        (0, 1),
        (1, 1),
        (0, 2),
        (2, 2),
        (0, 3),
        (1, 3),
        (2, 3),
        (0, 4),
    ],
    [
        (0, 0),
        (1, 0),
        (2, 0),
        (0, 1),
        (0, 2),
        (1, 2),
        (2, 2),
        (0, 3),
        (0, 4),
    ],
    [
        (0, 0),
        (1, 0),
        (2, 0),
        (0, 1),
        (0, 2),
        (1, 2),
        (2, 2),
        (0, 3),
        (0, 4),
    ],
    [
        (0, 0),
        (1, 0),
        (2, 0),
        (2, 1),
        (2, 2),
        (1, 3),
        (2, 3),
        (1, 4),
        (2, 4),
    ],
    [
        (1, 0),
        (2, 0),
        (0, 1),
        (1, 1),
        (2, 1),
        (0, 2),
        (1, 2),
        (0, 3),
        (0, 4),
    ],
    [
        (0, 0),
        (1, 0),
        (2, 0),
        (0, 1),
        (1, 1),
        (2, 1),
        (1, 2),
        (0, 3),
        (0, 4),
    ],
];

/// Unused import guard: `Display` is only needed for `framebuffer()`, which the
/// caller uses through the trait.
#[allow(dead_code, reason = "the type is named in the blit signature")]
type _Display = Display;
