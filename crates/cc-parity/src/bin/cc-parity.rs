//! `cc-parity` — the scenario runner.
//!
//! ```text
//! cc-parity list
//! cc-parity run <scenario.yaml>...          # dry_run, to stdout as JSON
//! ```
//!
//! A `hardware` scenario is refused rather than skipped: there is no device here,
//! and the driver has no path to one.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
cc-parity — the scenario runner

  list                              list the scenarios in a directory
  run <file.yaml>...                run dry_run scenarios, print observations as JSON

Exit codes: 0 clean, 1 a failed assertion, 2 usage/IO."
    );
}

fn cmd_list() -> Result<u8, String> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios").to_string()),
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
