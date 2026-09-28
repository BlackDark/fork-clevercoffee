//! Host side of the CleverCoffee USB provisioning channel.
//!
//! Speaks the line protocol described in `docs/rust-migration/architecture.md` section 6 over a
//! serial port. Credentials come from `.env` and go straight to the port: they are never
//! printed, never logged, and never written to disk. The only output is a status code plus the
//! device's non-secret reply.
//!
//! Subcommands: `wifi`, `config`, `factory-reset`, `status`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use serialport::{SerialPort, SerialPortType, TTYPort};

const PROMPT: &str = "provision> ";
const TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_BYTES: usize = 512;

fn fail(code: &str, detail: &str) -> ExitCode {
    println!("FAIL {code} {detail}");
    ExitCode::FAILURE
}

fn ok(code: &str, detail: &str) -> ExitCode {
    println!("OK {code} {detail}");
    ExitCode::SUCCESS
}

/// Reads `KEY=VALUE` pairs from a dotenv file. No interpolation, no shell expansion.
fn read_env(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            Some((key.trim().to_string(), value.to_string()))
        })
        .collect()
}

struct Console {
    port: TTYPort,
    reader: BufReader<Box<dyn SerialPort>>,
}

impl Console {
    fn open(name: &str) -> Result<Self, String> {
        let mut port: TTYPort = TTYPort::open(&serialport::new(name, 115_200))
            .map_err(|e| format!("cannot open {name}: {e}"))?;
        port.set_timeout(Duration::from_millis(200))
            .map_err(|e| e.to_string())?;
        let reader = BufReader::new(
            port.try_clone()
                .map_err(|e| format!("cannot clone {name}: {e}"))?,
        );
        Ok(Self { port, reader })
    }

    fn send(&mut self, line: &str) -> Result<(), String> {
        self.port
            .write_all(line.as_bytes())
            .and_then(|()| self.port.write_all(b"\r\n"))
            .and_then(|()| self.port.flush())
            .map_err(|e| e.to_string())
    }

    fn read_line(&mut self) -> Result<String, String> {
        let deadline = Instant::now() + TIMEOUT;
        let mut line = String::new();
        while Instant::now() < deadline {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => continue,
                Ok(_) => return Ok(line.trim_end().to_string()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("no reply from the device".into())
    }

    /// Drains until the device's prompt, discarding any banner it printed.
    fn expect_prompt(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + TIMEOUT;
        let mut scratch = [0u8; 256];
        while Instant::now() < deadline {
            match self.reader.get_mut().read(&mut scratch) {
                Ok(0) => continue,
                Ok(n) => {
                    let tail = String::from_utf8_lossy(&scratch[..n]);
                    if tail.trim_end().ends_with(PROMPT.trim_end()) {
                        return Ok(());
                    }
                }
                Err(_) => continue,
            }
        }
        Err("device did not present a prompt".into())
    }
}

fn list_ports() {
    match serialport::available_ports() {
        Ok(ports) => {
            for p in ports {
                let kind = match p.port_type {
                    SerialPortType::UsbPort(_) => "usb",
                    SerialPortType::BluetoothPort => "bluetooth",
                    SerialPortType::PciPort => "pci",
                    SerialPortType::Unknown => "unknown",
                };
                println!("{kind} {}", p.port_name);
            }
        }
        Err(e) => println!("FAIL ENUMERATE {e}"),
    }
}

fn do_wifi(console: &mut Console, ssid: &str, password: &str) -> ExitCode {
    match console.expect_prompt() {
        Ok(()) => {}
        Err(e) => return fail("PROMPT", &e),
    }
    console.send("PING").ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => {}
        other => return fail("PING", &format!("{other:?}")),
    }
    console.send(&format!("WIFI SSID {ssid}")).ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => {}
        other => return fail("WIFI_SSID", &format!("{other:?}")),
    }
    console.send(&format!("WIFI PASS {password}")).ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => {}
        other => return fail("WIFI_PASS", &format!("{other:?}")),
    }
    console.send("WIFI COMMIT").ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => {}
        other => return fail("WIFI_COMMIT", &format!("{other:?}")),
    }
    ok(
        "WIFI_SET",
        "credentials written; the device connects on its next boot",
    )
}

fn do_config(console: &mut Console, payload: &[u8]) -> ExitCode {
    let crc = crc32fast::hash(payload);
    console.expect_prompt().ok();
    console
        .send(&format!("CONFIG BEGIN {} {crc:08x}", payload.len()))
        .ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => {}
        other => return fail("CONFIG_BEGIN", &format!("{other:?}")),
    }
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    for block in payload.chunks(CHUNK_BYTES) {
        console
            .send(&format!("CONFIG {}", engine.encode(block)))
            .ok();
        match console.read_line() {
            Ok(l) if l.starts_with("OK") => {}
            other => return fail("CONFIG_CHUNK", &format!("{other:?}")),
        }
    }
    console.send("CONFIG END").ok();
    match console.read_line() {
        // The device replies with counts only. It never echoes a value, secret or not.
        Ok(l) if l.starts_with("OK") => ok("CONFIG_APPLIED", l[3..].trim()),
        other => fail("CONFIG_REJECTED", &format!("{other:?}")),
    }
}

fn do_factory_reset(console: &mut Console) -> ExitCode {
    console.expect_prompt().ok();
    console.send("FACTORY RESET").ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => ok(
            "FACTORY_RESET",
            "config region cleared; the device will reboot",
        ),
        other => fail("FACTORY_RESET", &format!("{other:?}")),
    }
}

fn do_status(console: &mut Console) -> ExitCode {
    console.expect_prompt().ok();
    console.send("STATUS").ok();
    match console.read_line() {
        Ok(l) if l.starts_with("OK") => ok("STATUS", l[3..].trim()),
        other => fail("STATUS", &format!("{other:?}")),
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let action = args.next().unwrap_or_default();
    let mut port = String::new();
    let mut file = String::new();
    let mut env_path = ".env".to_string();
    while let Some(arg) = args.next() {
        let value = args.next().unwrap_or_default();
        match arg.as_str() {
            "--port" => port = value,
            "--file" => file = value,
            "--env" => env_path = value,
            _ => {}
        }
    }

    if action == "list-ports" {
        list_ports();
        return ExitCode::SUCCESS;
    }
    if !matches!(
        action.as_str(),
        "wifi" | "config" | "factory-reset" | "status"
    ) {
        println!("usage: provision <wifi|config|factory-reset|status|list-ports> --port <port> [--file <f>] [--env <f>]");
        return ExitCode::FAILURE;
    }
    if port.is_empty() {
        return fail("USAGE", "--port is required");
    }
    if action == "config" {
        if file.is_empty() {
            return fail("USAGE", "config requires --file");
        }
        if !Path::new(&file).exists() {
            return fail("NOFILE", &file);
        }
    }

    let env = read_env(Path::new(&env_path));
    if action == "wifi" {
        let ssid = env
            .iter()
            .find(|(k, _)| k == "WIFI_SSID")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        if ssid.is_empty() {
            return fail("NO_SSID", &format!("{env_path} has no WIFI_SSID"));
        }
        let password = env
            .iter()
            .find(|(k, _)| k == "WIFI_PASS")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let mut console = match Console::open(&port) {
            Ok(c) => c,
            Err(e) => return fail("OPEN_PORT", &e),
        };
        return do_wifi(&mut console, &ssid, &password);
    }

    let mut console = match Console::open(&port) {
        Ok(c) => c,
        Err(e) => return fail("OPEN_PORT", &e),
    };
    match action.as_str() {
        "config" => {
            let payload = std::fs::read(&file).unwrap_or_default();
            do_config(&mut console, &payload)
        }
        "factory-reset" => do_factory_reset(&mut console),
        _ => do_status(&mut console),
    }
}
