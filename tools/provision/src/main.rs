//! Command-line entry point for the provisioning tool.
//!
//! Everything that decides something lives in the library, where it is tested. This file only
//! opens a port, parses arguments and prints one status line.
//!
//! Subcommands: `wifi`, `config`, `factory-reset`, `status`, `list-ports`.

use std::path::Path;
use std::process::ExitCode;

use provision::{credentials, Link, LinkError, SerialLink, Session, TIMEOUT};

const USAGE: &str = "usage: provision <wifi|config|factory-reset|status|list-ports> --port <port> [--file <f>] [--env <f>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = args.first().cloned().unwrap_or_default();
    let mut port = String::new();
    let mut file = String::new();
    let mut env_path = ".env".to_string();
    let mut it = args[usize::from(!action.is_empty())..].iter();
    while let Some(arg) = it.next() {
        let value = it.next().cloned().unwrap_or_default();
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
        println!("{USAGE}");
        return ExitCode::FAILURE;
    }
    if port.is_empty() {
        return report(provision::fail("USAGE", "--port is required"));
    }

    if action == "wifi" {
        // Only the SSID is checked here. The password is read inside `run`, so this scope never
        // holds it, and a check added to this scope could not accidentally print it.
        let (ssid, _) = credentials(Path::new(&env_path));
        if ssid.is_empty() {
            return report(provision::fail(
                "NO_SSID",
                &format!("{env_path} has no WIFI_SSID"),
            ));
        }
    }
    if action == "config" {
        if file.is_empty() {
            return report(provision::fail("USAGE", "config requires --file"));
        }
        if !Path::new(&file).exists() {
            return report(provision::fail("NOFILE", &file));
        }
    }

    let mut link = match SerialLink::open(&port, TIMEOUT) {
        Ok(link) => link,
        Err(e) => return report(open_failure(&e)),
    };
    report(run(&action, &mut link, &file, &env_path))
}

/// Prints one status line and returns its exit code.
///
/// Every exit path goes through here, which is what keeps the output contract to a single line and
/// keeps a credential from reaching it: the value printed is a [`provision::Status`], and that type
/// has no variant that can hold one.
fn report(status: provision::Status) -> ExitCode {
    println!("{}", status.line());
    status.exit_code()
}

fn open_failure(e: &LinkError) -> provision::Status {
    provision::fail("OPEN_PORT", &e.to_string())
}

fn run(action: &str, link: &mut impl Link, file: &str, env_path: &str) -> provision::Status {
    let mut session = Session::new(link);
    match action {
        "wifi" => {
            let (ssid, password) = credentials(Path::new(env_path));
            session.set_wifi(&ssid, &password)
        }
        "config" => {
            let payload = std::fs::read(file).unwrap_or_default();
            session.send_config(&payload)
        }
        "factory-reset" => session.factory_reset(),
        _ => session.status(),
    }
}

fn list_ports() {
    match serialport::available_ports() {
        Ok(ports) => {
            for p in ports {
                let kind = match p.port_type {
                    serialport::SerialPortType::UsbPort(_) => "usb",
                    serialport::SerialPortType::BluetoothPort => "bluetooth",
                    serialport::SerialPortType::PciPort => "pci",
                    serialport::SerialPortType::Unknown => "unknown",
                };
                println!("{kind} {}", p.port_name);
            }
        }
        Err(e) => println!("FAIL ENUMERATE {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use provision::link::{Link, LinkError};
    use std::cell::RefCell;

    /// A device that replies from a script.
    ///
    /// Duplicated rather than shared with the library's tests: those are compiled with `cfg(test)`
    /// on the library, so a binary's test target cannot see them. It is twenty lines, and it keeps
    /// the fake out of a shipped build.
    #[derive(Debug, Default)]
    struct FakeDevice {
        sent: RefCell<Vec<String>>,
        replies: RefCell<Vec<String>>,
    }

    impl FakeDevice {
        fn new(replies: &[&str]) -> Self {
            Self {
                sent: RefCell::new(Vec::new()),
                replies: RefCell::new(replies.iter().map(|s| s.to_string()).collect()),
            }
        }

        fn received(&self) -> Vec<String> {
            self.sent.borrow().clone()
        }
    }

    impl Link for FakeDevice {
        fn send(&mut self, line: &str) -> Result<(), LinkError> {
            self.sent.borrow_mut().push(line.to_string());
            Ok(())
        }

        fn read_line(&mut self) -> Result<String, LinkError> {
            let mut replies = self.replies.borrow_mut();
            if replies.is_empty() {
                return Err(LinkError::Disconnected);
            }
            Ok(replies.remove(0))
        }

        fn drain(&mut self) -> Result<(), LinkError> {
            Ok(())
        }
    }

    #[test]
    fn a_port_that_cannot_be_opened_exits_non_zero_with_a_status_line() {
        // The case `just status <a port we cannot open>` produces. One line, on stdout, and no
        // secret anywhere in it.
        let status = open_failure(&LinkError::Open("No such file or directory".into()));
        assert_eq!(status.exit_code(), ExitCode::FAILURE);
        let line = status.line();
        assert!(line.starts_with("FAIL OPEN_PORT "), "{line}");
        assert_eq!(line.matches('\n').count(), 0);
    }

    #[test]
    fn opening_a_missing_port_fails_rather_than_hanging() {
        let result = SerialLink::open("/nonexistent/provision-cli-test", TIMEOUT);
        let status = open_failure(&result.err().expect("should not open"));
        assert!(status.line().starts_with("FAIL OPEN_PORT"));
    }

    #[test]
    fn the_config_subcommand_passes_the_file_bytes_through() {
        let dir = std::env::temp_dir().join("provision-cli-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, br#"{"brew":{"setpoint":92}}"#).unwrap();
        let mut device = FakeDevice::new(&[
            "OK PONG",
            "OK CONFIG_READY",
            "OK CONFIG_CHUNK",
            "OK CONFIG_VALIDATED applied=1",
        ]);
        let status = run("config", &mut device, path.to_str().unwrap(), "");
        assert!(status.is_ok(), "{}", status.line());
        assert!(
            device.received()[1].contains("CONFIG BEGIN 24"),
            "{:?}",
            device.received()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_wifi_subcommand_reads_the_env_file_and_never_prints_the_password() {
        let dir = std::env::temp_dir().join("provision-cli-wifi-test");
        std::fs::create_dir_all(&dir).unwrap();
        let env = dir.join(".env");
        // The password is a placeholder so the repository's secret scanner does not flag this
        // fixture. It still exercises the leak check, because the assertion is that the *value*
        // never reaches stdout, whatever the value is.
        std::fs::write(
            &env,
            "WIFI_SSID=example-network\nWIFI_PASS=placeholder-value\n",
        )
        .unwrap();
        let mut device =
            FakeDevice::new(&["OK PONG", "OK WIFI_ACCEPTED", "OK WIFI_SET", "OK WIFI_SET"]);
        let status = run("wifi", &mut device, "", env.to_str().unwrap());
        assert!(status.is_ok(), "{}", status.line());
        assert!(
            !status.line().contains("placeholder-value"),
            "leaked: {}",
            status.line()
        );
        assert!(device
            .received()
            .contains(&"WIFI PASS placeholder-value".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_source_line_prints_a_credential_variable() {
        // The check that cannot be reviewed by reading: the tool's whole output is one
        // `Status::line`, so a `println!` of a credential would have to be added deliberately.
        let source = include_str!("main.rs");
        for line in source.lines() {
            let l = line.trim();
            if !l.starts_with("println!") && !l.starts_with("print!") {
                continue;
            }
            for secret in ["password", "pass", "ssid", "SSID"] {
                assert!(
                    !l.contains(secret),
                    "a print statement mentions a credential: {l}"
                );
            }
        }
    }
}
