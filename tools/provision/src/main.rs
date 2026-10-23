//! `provision [--ssid S] [--password P] [KEY=VALUE ...] -o FILE`
//!
//! Writes a provisioning blob (see `cc_api::provision`) padded to the 4 KB
//! `cc_prov` partition. Every key is validated against the parameter registry, so
//! a typo fails here, not silently on the device.

use std::process::ExitCode;

/// Size of the `cc_prov` partition in `firmware/partitions.csv`.
const PARTITION_SIZE: usize = 0x1000;

const USAGE: &str = "usage: provision [--ssid SSID] [--password PASS] [KEY=VALUE ...] -o FILE
  keys as in /api/config, e.g. system.hostname=bench mqtt.broker=10.0.0.2";

fn run(args: &[String]) -> Result<(), String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--ssid" => pairs.push(("system.wifi.ssid".into(), value("--ssid")?)),
            "--password" => pairs.push(("system.wifi.password".into(), value("--password")?)),
            "-o" | "--output" => out = Some(value("-o")?),
            "-h" | "--help" => return Err(USAGE.into()),
            kv => {
                let (k, v) = kv
                    .split_once('=')
                    .ok_or(format!("not KEY=VALUE: {kv}\n{USAGE}"))?;
                pairs.push((k.into(), v.into()));
            }
        }
    }
    let out = out.ok_or(USAGE)?;
    let mut blob = cc_api::provision::build(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))?;
    if blob.len() > PARTITION_SIZE {
        return Err(format!(
            "{} bytes do not fit the {PARTITION_SIZE}-byte partition",
            blob.len()
        ));
    }
    blob.resize(PARTITION_SIZE, 0xFF);
    std::fs::write(&out, &blob).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("{out}: {} settings", pairs.len());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
