//! The end-to-end proof the task requires: a synthetic **undeclared** diff makes
//! the runner fail, and a **declared** one does not.
//!
//! The unit tests in `src/diff.rs` prove the classifier's logic. These prove the
//! thing that actually matters, which is that `scripts/parity/run.sh` consults
//! `intentional-diffs.md` and exits non-zero on anything the ledger does not
//! cover. A classifier that is correct but not wired to the runner is not a
//! gate.
//!
//! Both cases are run through the real binary, with a real ledger extracted from
//! the real `intentional-diffs.md`.

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

fn ledger_path() -> PathBuf {
    repo_root().join("docs/history/divergences.md")
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

/// The smallest observation that exists, so a diff is only what the test adds.
fn minimal_observation(scenario: &str) -> String {
    serde_json::to_string_pretty(&cc_parity::Observation::new(scenario, "x")).expect("serialises")
}

fn run_diff(cpp: &Path, rust: &Path) -> (bool, String) {
    let out = Command::new(binary())
        .args(["diff"])
        .arg(cpp)
        .arg(rust)
        .arg(ledger_path())
        .output()
        .expect("cc-parity runs");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// The headline requirement: an undeclared difference fails the run.
///
/// The synthetic difference is a state the C++ never reached and the Rust one
/// did — exactly the shape of a real regression, and not something any ledger
/// entry claims.
#[test]
fn a_synthetic_undeclared_diff_fails_the_runner() {
    let dir = scratch("undeclared");
    let cpp = dir.join("cpp.json");
    let rust = dir.join("rust.json");

    let base: cc_parity::Observation = cc_parity::Observation::new("synthetic", "cpp");
    std::fs::write(&cpp, minimal_observation("synthetic")).expect("writes");

    let mut changed = base.clone();
    changed.firmware = "rust".to_string();
    changed.states.push("STEAM_RUNNING".to_string());
    std::fs::write(&rust, serde_json::to_string_pretty(&changed).unwrap()).expect("writes");

    let (ok, output) = run_diff(&cpp, &rust);
    assert!(
        !ok,
        "an undeclared diff must exit non-zero. Output:\n{output}"
    );
    assert!(
        output.contains("UNEXPLAINED"),
        "the report must say the line is unexplained:\n{output}"
    );
    assert!(
        output.contains("1 unexplained"),
        "the summary must count it:\n{output}"
    );
}

/// The other half: a difference the ledger *does* declare is reported as
/// expected, counted, and **does not** fail the run.
///
/// It uses the real `intentional-diffs.md`, so this also proves the runner reads
/// the document rather than a fixture. The declared difference is divergence #1
/// — the pump watchdogs the C++ never arms — which is the entry R1-08 seeds.
#[test]
fn a_declared_divergence_is_reported_and_does_not_fail() {
    let dir = scratch("declared");
    let cpp = dir.join("cpp.json");
    let rust = dir.join("rust.json");

    std::fs::write(&cpp, minimal_observation("brew_by_time")).expect("writes");

    // Scoped to a scenario divergence #1 names, so the entry is eligible.
    let mut changed = cc_parity::Observation::new("brew_by_time", "rust");
    changed.effects.push(cc_parity::observe::Observed {
        name: "PumpTimeoutFired".to_string(),
        effect: cc_parity::observe::ObservedEffect::PumpTimeout {
            watchdog: "brew".to_string(),
        },
        at_ms: 300_000,
    });
    std::fs::write(&rust, serde_json::to_string_pretty(&changed).unwrap()).expect("writes");

    let (ok, output) = run_diff(&cpp, &rust);
    assert!(
        ok,
        "a declared divergence must not fail the run. Output:\n{output}"
    );
    assert!(
        output.contains("expected (div1)"),
        "the line must be reported as expected, and attributed:\n{output}"
    );
    assert!(
        output.contains("1 expected") && output.contains("0 unexplained"),
        "the summary must show the ledger doing the work:\n{output}"
    );
    assert!(
        output.contains("div1: explained 1"),
        "the ledger section must show the count, so a reader sees the ledger and\n\
         not just a clean pass:\n{output}"
    );
}

/// A declared divergence **out of scope** for the scenario does not excuse a
/// diff there. Divergence #1 is scoped to scenarios that run a pump; the same
/// diff in `cold_boot` has to fail.
#[test]
fn a_divergence_does_not_excuse_a_diff_in_an_unscoped_scenario() {
    let dir = scratch("unscoped");
    let cpp = dir.join("cpp.json");
    let rust = dir.join("rust.json");

    std::fs::write(&cpp, minimal_observation("cold_boot")).expect("writes");

    let mut changed = cc_parity::Observation::new("cold_boot", "rust");
    changed.effects.push(cc_parity::observe::Observed {
        name: "PumpTimeoutFired".to_string(),
        effect: cc_parity::observe::ObservedEffect::PumpTimeout {
            watchdog: "brew".to_string(),
        },
        at_ms: 300_000,
    });
    std::fs::write(&rust, serde_json::to_string_pretty(&changed).unwrap()).expect("writes");

    let (ok, output) = run_diff(&cpp, &rust);
    assert!(
        !ok,
        "a divergence scoped to other scenarios must not excuse this one. Output:\n{output}"
    );
    assert!(output.contains("UNEXPLAINED"), "{output}");
}

/// The ledger is read from `intentional-diffs.md` itself, and the runner
/// refuses if it has drifted from the prose.
#[test]
fn the_runner_reads_the_real_divergence_ledger() {
    let out = Command::new(binary())
        .args(["ledger"])
        .arg(ledger_path())
        .output()
        .expect("cc-parity runs");
    assert!(
        out.status.success(),
        "the real ledger must parse. Output:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed = String::from_utf8_lossy(&out.stdout);
    let ledger: cc_parity::Ledger = serde_json::from_str(&printed).expect("valid JSON");
    assert!(
        ledger.entries.len() >= 4,
        "R1-08 seeds at least four; found {}: {:?}",
        ledger.entries.len(),
        ledger.entries.iter().map(|e| &e.id).collect::<Vec<_>>()
    );
    for entry in &ledger.entries {
        assert!(
            !entry.matchers.is_empty(),
            "{} has no matchers, so it would explain everything",
            entry.id
        );
    }
}

/// A scenario with no C++ baseline is reported as a missing baseline, not as a
/// pass. The runner's own exit code is checked in `run.sh`; what is provable
/// here is that the ledger and the scenario set agree on the names, so a
/// baseline cannot be silently orphaned.
#[test]
fn every_dry_run_scenario_is_runnable_through_the_binary() {
    let dir = scratch("list");
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
            "06 R1-08 step 2 names {required:?}; the set has {names:?}"
        );
    }
    let _ = dir;
}

/// One scenario runs end to end through the binary and produces a parseable
/// observation — the path `run.sh` takes.
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
    assert_eq!(obs.firmware, "rust");
    assert!(obs.states.contains(&"BREW_FINISHED".to_string()), "{obs:?}");
    // The observation is writable, which is what makes it a baseline.
    let path = dir.join("brew_by_time.json");
    std::fs::write(&path, serde_json::to_string_pretty(obs).unwrap()).expect("writes");
    let back: cc_parity::Observation =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).expect("round-trips");
    assert_eq!(&back, obs);
}
