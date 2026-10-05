//! `cc-parity` — the scenario runner behind `scripts/parity/run.sh`.
//!
//! ```text
//! cc-parity list
//! cc-parity run <scenario.yaml>...          # dry_run, to stdout as JSON
//! cc-parity ledger <intentional-diffs.md>
//! cc-parity diff <cpp.json> <rust.json> <ledger.md> [--scenario NAME]
//! ```
//!
//! The shell script owns the device-facing half (flashing, HTTP, the serial
//! log); this binary owns the half that has to be Rust because it is the
//! reducer. Splitting it there is what keeps `run.sh` small enough to read and
//! keeps the safety-critical part — the thing that could energise an actuator —
//! in a crate that has no GPIO.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cc_parity::diff::{self, Ledger};
use cc_parity::scenario::Scenario;
use cc_parity::{run, Observation};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return ExitCode::from(2);
    };
    let rest = &args[1..];

    let result = match command {
        "list" => cmd_list(),
        "run" => cmd_run(rest),
        "ledger" => cmd_ledger(rest),
        "diff" => cmd_diff(rest),
        "-h" | "--help" | "help" => {
            usage();
            Ok(0)
        }
        other => Err(format!("unknown command {other:?}; try --help")),
    };

    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("cc-parity: {message}");
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!(
        "\
cc-parity — the parity scenario runner (06 R1-08)

  list                              list the scenarios in a directory
  run <file.yaml>...                run dry_run scenarios, print observations as JSON
  ledger <intentional-diffs.md>     print the divergence ledger the diff consults
  diff <cpp.json> <rust.json> <ledger.md> [--scenario NAME]
                                    diff two observations under the ledger

Exit codes: 0 clean, 1 an unexplained diff or a failed assertion, 2 usage/IO."
    );
}

fn cmd_list() -> Result<u8, String> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| "docs/rust-migration/scenarios".to_string()),
    );
    let mut names: Vec<(String, String, Vec<String>)> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "yaml") {
            let s = Scenario::load(&path).map_err(|e| e.to_string())?;
            names.push((s.name.clone(), format!("{:?}", s.mode), s.covers.clone()));
        }
    }
    names.sort();
    for (name, mode, covers) in &names {
        println!("{name}\t{mode}\t{}", covers.join(","));
    }
    Ok(0)
}

fn cmd_run(args: &[String]) -> Result<u8, String> {
    let mut observations: Vec<Observation> = Vec::new();
    let mut failed = false;
    for path in args {
        let scenario = Scenario::load(Path::new(path)).map_err(|e| e.to_string())?;
        match run(scenario) {
            Ok(obs) => observations.push(obs),
            Err(e) => {
                eprintln!("cc-parity: {e}");
                failed = true;
            }
        }
    }
    let text = serde_json::to_string_pretty(&observations).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(u8::from(failed))
}

fn cmd_ledger(args: &[String]) -> Result<u8, String> {
    let path = args
        .first()
        .ok_or("ledger: expected the path to intentional-diffs.md")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let ledger = diff::ledger_from_markdown(&text)?;
    let out = serde_json::to_string_pretty(&ledger).map_err(|e| e.to_string())?;
    println!("{out}");
    Ok(0)
}

fn cmd_diff(args: &[String]) -> Result<u8, String> {
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if positional.len() < 3 {
        return Err("diff: expected <cpp.json> <rust.json> <ledger.md>".to_string());
    }
    let cpp: Observation =
        serde_json::from_str(&std::fs::read_to_string(positional[0]).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", positional[0]))?;
    let rust: Observation =
        serde_json::from_str(&std::fs::read_to_string(positional[1]).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", positional[1]))?;
    let text =
        std::fs::read_to_string(positional[2]).map_err(|e| format!("{}: {e}", positional[2]))?;
    let ledger: Ledger = diff::ledger_from_markdown(&text)?;

    let verdict = diff::classify(&cpp, &rust, &ledger);

    // Every diff line, expected or not. A clean pass prints nothing here, and
    // the ledger section below is what makes a clean pass legible rather than
    // silent.
    for line in &verdict.lines {
        let tag = if line.explained_by.is_empty() {
            "UNEXPLAINED".to_string()
        } else {
            format!("expected ({})", line.explained_by.join(", "))
        };
        println!("{} [{tag}] {}", line.side, line.text);
    }

    println!();
    println!(
        "{}: {} diff line(s), {} expected, {} unexplained",
        rust.scenario,
        verdict.lines.len(),
        verdict.expected,
        verdict.unexplained
    );
    if !verdict.ledger_hits.is_empty() {
        println!("ledger:");
        for (id, count) in &verdict.ledger_hits {
            println!("  {id}: explained {count} diff line(s)");
        }
    }
    if !verdict.ledger_unused.is_empty() {
        println!(
            "ledger entries that explained nothing (a divergence that stopped diverging — \
             remove them from the ledger): {}",
            verdict.ledger_unused.join(", ")
        );
    }

    Ok(u8::from(!verdict.is_clean()))
}
