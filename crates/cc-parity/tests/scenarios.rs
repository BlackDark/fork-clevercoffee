//! Every `dry_run` scenario in `scenarios/` runs in process and every
//! assertion it declares holds.
//!
//! The driver wires the real reducer, the real safety monitor and the real applier, and
//! hands them a recorder where the GPIO would be, so one scenario is an end-to-end check
//! of the control loop with nothing energised. The file format is
//! [`docs/scenarios.md`](../../docs/scenarios.md).
//!
//! `hardware` scenarios are refused by the driver, and nothing in this repository can
//! drive a device, so they are skipped and counted rather than asserted on.

use std::path::{Path, PathBuf};

use cc_parity::{run, RunError, Scenario};

/// The floor the set has to keep for this test to be worth running. A `read_dir` that
/// silently finds nothing, or a set that quietly shrank, must not read as a pass.
const LEAST_SCENARIOS: usize = 12;

fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios")
}

#[test]
fn every_dry_run_scenario_holds_its_assertions() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(scenarios_dir())
        .expect("the scenario directory exists")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    paths.sort();

    let mut ran: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for path in paths {
        let scenario = Scenario::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = scenario.name.clone();
        match run(scenario) {
            Ok(_) => ran.push(name),
            Err(RunError::NotDryRun { .. }) => skipped.push(name),
            Err(e) => panic!("{}: {e}", path.display()),
        }
    }

    assert!(
        ran.len() >= LEAST_SCENARIOS,
        "{} dry_run scenario(s) ran ({ran:?}); {} hardware scenario(s) skipped ({skipped:?})",
        ran.len(),
        skipped.len()
    );
}
