//! `bench HOST [--hang] [--ota FILE]`: smoke tests against a device on the bench
//! (see docs/bench.md). Plain HTTP/1.1 over `std::net`, one check per line, exit
//! code 1 if any check fails.
//!
//! `--hang` needs a `just image bench` build: it stalls the control loop and
//! expects the watchdog to reset the device. `--ota` uploads a firmware image and
//! waits for the device to come back.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> Result<Value, String> {
        serde_json::from_slice(&self.body).map_err(|e| format!("invalid JSON: {e}"))
    }
}

fn request(host: &str, method: &str, path: &str, ct: &str, body: &[u8]) -> Result<Response, String> {
    let addr = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    let mut s = TcpStream::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(|e| e.to_string())?;
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Type: {ct}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())
        .and_then(|()| s.write_all(body))
        .map_err(|e| e.to_string())?;
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or(format!("bad status line {line:?}"))?;
    let mut headers = Vec::new();
    loop {
        line.clear();
        r.read_line(&mut line).map_err(|e| e.to_string())?;
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    // By length when given: after `/api/debug/hang` the device answers but never
    // closes the connection (the hung control loop starves the network stack).
    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
        .and_then(|(_, v)| v.parse::<usize>().ok());
    let mut body = Vec::new();
    match length {
        Some(n) => {
            body.resize(n, 0);
            r.read_exact(&mut body).map_err(|e| e.to_string())?;
        }
        None => {
            r.read_to_end(&mut body).map_err(|e| e.to_string())?;
        }
    }
    Ok(Response {
        status,
        headers,
        body,
    })
}

fn get(host: &str, path: &str) -> Result<Response, String> {
    request(host, "GET", path, "text/plain", b"")
}

fn post_form(host: &str, path: &str, form: &str) -> Result<Response, String> {
    request(
        host,
        "POST",
        path,
        "application/x-www-form-urlencoded",
        form.as_bytes(),
    )
}

fn ensure(cond: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if cond { Ok(()) } else { Err(what()) }
}

fn ok_json(r: &Response) -> Result<Value, String> {
    ensure(r.status == 200, || format!("HTTP {}", r.status))?;
    r.json()
}

fn uptime(host: &str) -> Option<u64> {
    get(host, "/api/status").ok()?.json().ok()?["uptime"].as_u64()
}

/// Waits until the device answers with an uptime below the one seen before.
fn wait_for_reboot(host: &str, before: u64, limit: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < limit {
        thread::sleep(Duration::from_secs(2));
        if let Some(u) = uptime(host)
            && u < before
        {
            return Ok(());
        }
    }
    Err(format!("no reboot within {limit:?}"))
}

fn param_value(params: &Value, key: &str) -> Option<Value> {
    params
        .as_array()?
        .iter()
        .find(|p| p["name"] == key)
        .map(|p| p["value"].clone())
}

type Check = (&'static str, fn(&str) -> Result<(), String>);

/// Lowest acceptable heap low-water mark (`minFreeHeap`), bytes. The `full` image on
/// the bench bottoms out near 16 KB (docs/firmware.md); a failed allocation is a reboot.
const HEAP_FLOOR: u64 = 12_000;

const CHECKS: &[Check] = &[
    ("status", |h| {
        let v = ok_json(&get(h, "/api/status")?)?;
        for k in [
            "temperature",
            "setpoint",
            "machineState",
            "uptime",
            "freeHeap",
            "minFreeHeap",
        ] {
            ensure(!v[k].is_null(), || format!("missing {k}"))?;
        }
        ensure(v["minFreeHeap"].as_u64().unwrap_or(0) >= HEAP_FLOOR, || {
            format!("heap low-water mark {} below {HEAP_FLOOR}", v["minFreeHeap"])
        })
    }),
    ("all parameters (largest response)", |h| {
        let v = ok_json(&get(h, "/api/parameters?filter=all")?)?;
        let n = v.as_array().map_or(0, Vec::len);
        ensure(n >= 90, || format!("only {n} parameters"))
    }),
    ("parallel parameter downloads", |h| {
        let workers: Vec<_> = (0..3)
            .map(|_| {
                let h = h.to_string();
                thread::spawn(move || {
                    get(&h, "/api/parameters?filter=all")
                        .and_then(|r| ok_json(&r))
                        .map(drop)
                })
            })
            .collect();
        workers
            .into_iter()
            .try_for_each(|w| w.join().map_err(|_| "thread panicked".to_string())?)
    }),
    ("parameter write persists", |h| {
        let key = "display.blinking.delta";
        let before =
            param_value(&ok_json(&get(h, "/api/parameters?filter=all")?)?, key).ok_or("no such key")?;
        let target = if before.as_f64() == Some(0.7) {
            "0.6"
        } else {
            "0.7"
        };
        ok_json(&post_form(h, "/api/parameters", &format!("{key}={target}"))?)?;
        let after = param_value(&ok_json(&get(h, "/api/parameters?filter=all")?)?, key);
        let restore = post_form(h, "/api/parameters", &format!("{key}={before}"))?;
        ensure(restore.status == 200, || "restore failed".into())?;
        ensure(after.and_then(|v| v.as_f64()) == target.parse().ok(), || {
            "value not applied".into()
        })
    }),
    ("history", |h| ok_json(&get(h, "/api/history")?).map(drop)),
    ("config download", |h| {
        let v = ok_json(&get(h, "/api/config/download")?)?;
        ensure(v.is_object(), || "not an object".into())
    }),
    ("unknown API route is a JSON 404", |h| {
        let r = get(h, "/api/nope")?;
        ensure(r.status == 404 && r.json().is_ok(), || {
            format!("HTTP {}", r.status)
        })
    }),
    ("web UI is embedded and gzipped", |h| {
        let r = get(h, "/ui/")?;
        ensure(r.status == 200, || format!("HTTP {}", r.status))?;
        ensure(r.header("Content-Encoding") == Some("gzip"), || {
            "UI not gzipped (built without `just ui`?)".into()
        })
    }),
    ("server-sent events", |h| {
        let addr = if h.contains(':') {
            h.to_string()
        } else {
            format!("{h}:80")
        };
        let mut s = TcpStream::connect(&addr).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        s.write_all(format!("GET /events HTTP/1.1\r\nHost: {h}\r\n\r\n").as_bytes())
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 4096];
        let mut got = String::new();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) && !got.contains("data:") {
            let n = s.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            got.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        // Open UI tabs hold the stream slots (`STA_STREAMS`): the device turns a further
        // stream away rather than block a worker. Close the tabs to test the stream.
        ensure(!got.starts_with("HTTP/1.1 503"), || {
            "all event-stream slots busy (UI tabs open?)".into()
        })?;
        ensure(got.contains("data:"), || "no event within 5 s".into())
    }),
    ("more connections than HTTP workers", |h| {
        // Without a listen backlog the device used to refuse (RST) a connection while
        // every worker was busy; now it drops the SYN and the client's retry gets in.
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let h = h.to_string();
                thread::spawn(move || get(&h, "/api/status").and_then(|r| ok_json(&r)).map(drop))
            })
            .collect();
        workers
            .into_iter()
            .try_for_each(|w| w.join().map_err(|_| "thread panicked".to_string())?)
    }),
];

fn run(host: &str, hang: bool, ota: Option<&str>) -> Result<(), String> {
    let mut failed = 0;
    for (name, check) in CHECKS {
        let t = Instant::now();
        match check(host) {
            Ok(()) => println!("PASS {name} ({} ms)", t.elapsed().as_millis()),
            Err(e) => {
                println!("FAIL {name}: {e}");
                failed += 1;
            }
        }
    }
    ensure(failed == 0, || {
        format!("{failed} of {} checks failed", CHECKS.len())
    })?;
    if let Some(path) = ota {
        let image = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let before = uptime(host).ok_or("no status before update")?;
        let boundary = "ccbench";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"firmware\"; filename=\"fw.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(&image);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let t = Instant::now();
        let r = request(
            host,
            "POST",
            "/api/ota/firmware",
            &format!("multipart/form-data; boundary={boundary}"),
            &body,
        )?;
        ok_json(&r).map_err(|e| format!("FAIL firmware upload: {e}"))?;
        wait_for_reboot(host, before, Duration::from_secs(90))
            .map_err(|e| format!("FAIL firmware upload: {e}"))?;
        println!("PASS firmware upload and reboot ({} s)", t.elapsed().as_secs());
    }
    if hang {
        let before = uptime(host).ok_or("no status before hang")?;
        let r = post_form(host, "/api/debug/hang", "")?;
        ensure(r.status == 200, || {
            format!("FAIL watchdog: hang route HTTP {} (not a bench build?)", r.status)
        })?;
        wait_for_reboot(host, before, Duration::from_secs(60)).map_err(|e| format!("FAIL watchdog: {e}"))?;
        println!("PASS watchdog reset after a hung control loop");
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(host) = args.first().filter(|a| !a.starts_with('-')) else {
        eprintln!("usage: bench HOST [--hang] [--ota FILE]");
        return ExitCode::FAILURE;
    };
    let hang = args.iter().any(|a| a == "--hang");
    let ota = args
        .iter()
        .position(|a| a == "--ota")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);
    match run(host, hang, ota) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
