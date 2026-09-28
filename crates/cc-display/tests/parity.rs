//! Display parity, against the real U8g2 the firmware links.
//!
//! Entirely compiled out without the `scenarios` feature, because the scenario
//! interpreter needs `alloc` and the crate must not pull a heap in for the
//! device. With the feature off this file is an empty test binary rather than a
//! compile error, so `cargo clippy --all-targets` works on the default feature
//! set -- which is the set the device build uses.
#![cfg(feature = "scenarios")]
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use cc_display::display::Framebuffer;
use cc_display::scenario;

/// The oracle's build script, and the name it puts the binary at.
const RUN_SH: &str = "tools/oracle/run.sh";

/// This crate's root, from `CARGO_MANIFEST_DIR`.
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every scenario file, sorted so a failure names the same one every time.
fn scenario_files() -> Vec<PathBuf> {
    let dir = crate_root().join("tests/scenarios");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.expect("readable directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "the scenario corpus is empty");
    files
}

/// The U8g2 tree the firmware links, or `None` when it has not been fetched.
fn u8g2_dir() -> Option<PathBuf> {
    let libdeps = crate_root().join("../../.pio/libdeps");
    let direct = libdeps.join("esp32_usb/U8g2");
    if direct.join("src/clib").is_dir() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(&libdeps).ok()?;
    entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .map(|p| p.join("U8g2"))
        .find(|p| p.join("src/clib").is_dir())
}

fn work_dir() -> PathBuf {
    let dir = crate_root().join("../../target/parity");
    std::fs::create_dir_all(&dir).expect("can create the parity work dir");
    dir
}

/// Run one scenario through the oracle and return the framebuffer it wrote.
fn run_oracle(scenario_path: &Path, out: &Path) -> Framebuffer {
    let script = crate_root().join(RUN_SH);
    assert!(
        script.is_file(),
        "the oracle is missing: {} -- it is committed, so this is a broken checkout",
        script.display()
    );

    let status = Command::new("bash")
        .arg(&script)
        .arg(scenario_path)
        .arg(out)
        .status()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", script.display()));
    assert!(
        status.success(),
        "the oracle failed on {}",
        scenario_path.display()
    );

    support::read_ppm(out)
}

#[test]
#[ignore = "needs the U8g2 tree from .pio/libdeps; run with --ignored"]
fn every_scenario_renders_identically_to_u8g2() {
    let Some(u8g2) = u8g2_dir() else {
        panic!(
            "U8g2 is not under .pio/libdeps. Run `pio run -e esp32_usb` (or \
             `pio pkg install -e esp32_usb`) first: the parity claim must be \
             against the tree the firmware links."
        );
    };
    // Fail early and clearly if the build script cannot find it either.
    assert!(
        u8g2.join("src/clib").is_dir(),
        "{} is not a U8g2 checkout",
        u8g2.display()
    );

    let dir = work_dir();
    let mut failures = Vec::new();
    let mut total_ink = 0usize;
    let files = scenario_files();

    for path in &files {
        let name = path
            .file_stem()
            .expect("scenario file has a stem")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(path).expect("scenario is readable");

        let ours = scenario::run(&text)
            .unwrap_or_else(|e| panic!("scenario {} is malformed: {e}", path.display()));
        let out = dir.join(format!("{name}.oracle.ppm"));
        let theirs = run_oracle(path, &out);

        total_ink += ours.lit_count();
        let d = support::diff(&theirs, &ours);
        if d.differing() != 0 {
            failures.push(format!("  {name}: {}", d.summary()));
            // Keep the actual render next to the oracle's so the difference can
            // be looked at rather than only counted.
            std::fs::write(dir.join(format!("{name}.rust.ppm")), support::to_ppm(&ours))
                .expect("can write the actual render");
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} scenario(s) differ from U8g2:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
    // A corpus that renders nothing would pass the parity check while testing
    // nothing, so the ink total is asserted too.
    assert!(
        total_ink > 10_000,
        "the corpus only drew {total_ink} lit pixels; is it empty?"
    );
}

#[test]
#[ignore = "needs the U8g2 tree from .pio/libdeps; run with --ignored"]
fn the_oracle_binary_builds() {
    // The parity test above depends on `run.sh`; if the build breaks, that test
    // fails with "the oracle failed", which reads like a pixel difference. This
    // one fails with the compiler's message instead.
    u8g2_dir().expect("U8g2 is not under .pio/libdeps; run `pio run -e esp32_usb`");
    let script = crate_root().join(RUN_SH);
    let out = Command::new("bash")
        .arg(&script)
        .arg("--all")
        .output()
        .expect("can run run.sh");
    assert!(
        out.status.success(),
        "run.sh --all failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
