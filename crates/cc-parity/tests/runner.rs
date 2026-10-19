//! The binary's contract: `list` sees the scenario set, and `run` produces a
//! parseable observation.
//!
//! Both go through the real `cc-parity` binary rather than the library, because
//! the binary is what a human and a script call; a library function that works
//! while the CLI misparses its arguments is not the thing anyone uses.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<repo>/crates/cc-parity`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cc-parity"))
}

/// A scratch directory under `target/`, cleaned by `cargo clean`.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/parity-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// `list` sees the set: every scenario named here is one the harness is expected
/// to keep, and a scenario file that stops parsing takes the listing down.
#[test]
fn every_dry_run_scenario_is_runnable_through_the_binary() {
    let _dir = scratch("list");
    let out = Command::new(binary())
        .args(["list"])
        .arg(repo_root().join("docs/history/scenarios"))
        .output()
        .expect("cc-parity runs");
    assert!(out.status.success(), "list must succeed");
    let listing = String::from_utf8_lossy(&out.stdout);
    let names: Vec<&str> = listing
        .lines()
        .filter_map(|l| l.split('\t').next())
        .collect();
    assert!(
        names.len() >= 12,
        "the task list names twelve scenarios; found {names:?}"
    );
    for required in [
        "cold_boot",
        "brew_by_time",
        "brew_aborted_mid_preinfusion",
        "brew_aborted_mid_flow",
        "overtemp_trip",
        "overtemp_recovery",
        "water_tank_empty_mid_brew",
        "backflush_full_cycle",
        "steam_on_off",
        "standby_wake",
        "ota_start_from_idle",
        "ota_start_during_brew",
    ] {
        assert!(
            names.contains(&required),
            "the scenario set lost {required:?}; it has {names:?}"
        );
    }
}

/// One scenario runs end to end through the binary and produces a parseable
/// observation that survives a JSON round trip.
#[test]
fn one_scenario_runs_end_to_end_through_the_binary() {
    let dir = scratch("run");
    let out = Command::new(binary())
        .args(["run"])
        .arg(repo_root().join("docs/history/scenarios/brew_by_time.yaml"))
        .output()
        .expect("cc-parity runs");
    assert!(
        out.status.success(),
        "brew_by_time must run. Output:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observations: Vec<cc_parity::Observation> =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
    assert_eq!(observations.len(), 1);
    let obs = &observations[0];
    assert_eq!(obs.scenario, "brew_by_time");
    assert!(obs.states.contains(&"BREW_FINISHED".to_string()), "{obs:?}");
    // The JSON is the binary's output format, so it has to survive a round trip.
    let path = dir.join("brew_by_time.json");
    std::fs::write(&path, serde_json::to_string_pretty(obs).unwrap()).expect("writes");
    let back: cc_parity::Observation =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).expect("round-trips");
    assert_eq!(&back, obs);
}
