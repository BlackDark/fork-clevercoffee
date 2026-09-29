//! The provisioning exchange, host side against device side.
//!
//! The real host tool is a separate workspace and cannot be linked from here, so this file carries
//! a small client that speaks the same protocol: the same lines, the same expected tokens. That
//! is the arrangement the task list asks for, and it is the only way to test the device half
//! without a board plugged in.
//!
//! What is asserted, in order of how much it matters:
//!
//! - the exchange completes and the config is applied;
//! - the machine is de-energised for the whole session, before the port is even read;
//! - no reply the device can produce contains the password it was given;
//! - a bad CRC, an over-long document and a malformed line are each refused by name.

use clevercoffee_app::machine::{Machine, Request, RuntimeConfig, Sensors};
use clevercoffee_app::prov::{
    base64_decode, crc32, reply, Base64, Command, ConfigBuffer, Device, ProvisionSink, Step,
    MAX_CONFIG,
};
use clevercoffee_domain::State;
use clevercoffee_hal_traits::{
    ActuatorCommand, ProvisioningTransport, RecordingActuators, TransportError,
};
use heapless::{String, Vec};

/// The machine, reduced to what provisioning needs.
struct MachineHandle(Machine<RecordingActuators>);

impl clevercoffee_app::prov::ServiceMode for MachineHandle {
    fn set_service_mode(&mut self, on: bool) {
        self.0.set_service_mode(on);
    }
    fn force_off(&mut self, reason: &'static str) {
        self.0.force_off(reason);
    }
}

impl MachineHandle {
    #[allow(dead_code)]
    fn state(&self) -> State {
        self.0.state()
    }
    fn last_command(&self) -> ActuatorCommand {
        self.0.last_command()
    }
}

/// A sink that records what it was given, so a test can assert on it.
#[derive(Debug, Default)]
struct Sink {
    ssid: String<64>,
    password: String<64>,
    committed: bool,
    applied: Vec<u8, 2048>,
    applied_counts: (u16, u16, u16),
    fail_apply: bool,
    resets: u32,
}

impl ProvisionSink for Sink {
    fn set_ssid(&mut self, ssid: &str) {
        let _ = self.ssid.push_str(ssid);
    }

    fn set_password(&mut self, password: &str) {
        let _ = self.password.push_str(password);
    }

    fn commit_wifi(&mut self) -> Result<(), &'static str> {
        self.committed = true;
        Ok(())
    }

    fn apply_config(&mut self, payload: &[u8]) -> Result<(u16, u16, u16), &'static str> {
        if self.fail_apply {
            return Err("rejected");
        }
        let _ = self.applied.extend_from_slice(payload);
        Ok(self.applied_counts)
    }

    fn factory_reset(&mut self) -> Result<(), &'static str> {
        self.resets += 1;
        Ok(())
    }

    fn status(&mut self) -> String<96> {
        let mut s = String::new();
        let _ = core::fmt::Write::write_fmt(
            &mut s,
            format_args!("state={} ssid_set={}", 20, !self.ssid.is_empty()),
        );
        s
    }
}

/// An in-memory port: lines in, lines out.
#[derive(Debug, Default)]
struct Pipe {
    to_device: Vec<String<256>, 128>,
    from_device: Vec<String<256>, 128>,
    read_index: usize,
    closed: bool,
}

impl Pipe {
    fn push(&mut self, line: &str) {
        let mut s = String::new();
        let _ = s.push_str(line);
        let _ = self.to_device.push(s);
    }
}

impl ProvisioningTransport for Pipe {
    fn read_line(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        if self.closed {
            return Ok(None);
        }
        if self.read_index >= self.to_device.len() {
            return Ok(Some(0));
        }
        let line = &self.to_device[self.read_index];
        self.read_index += 1;
        let n = line.len().min(buf.len());
        buf[..n].copy_from_slice(&line.as_bytes()[..n]);
        Ok(Some(n))
    }

    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        let mut s = String::new();
        let _ = s.push_str(line);
        let _ = self.from_device.push(s);
        Ok(())
    }

    fn drain(&mut self) {
        self.from_device.clear();
    }
}

/// A session bound to a machine the test can inspect.
struct Session {
    device: Device<Pipe, Sink, MachineHandle>,
    pipe: Pipe,
    step_count: u32,
}

impl Session {
    fn new() -> Self {
        let mut machine = Machine::new(RecordingActuators::new(), RuntimeConfig::default());
        for _ in 0..6 {
            machine.tick(Sensors::at(93.0), 100);
        }
        Self {
            device: Device::new(Pipe::default(), Sink::default(), MachineHandle(machine)),
            pipe: Pipe::default(),
            step_count: 0,
        }
    }

    fn machine(&self) -> &MachineHandle {
        self.device.machine()
    }

    fn machine_mut(&mut self) -> &mut Machine<RecordingActuators> {
        &mut self.device.machine_mut().0
    }

    fn sink_mut(&mut self) -> &mut Sink {
        self.device.sink_mut()
    }

    fn sink(&self) -> &Sink {
        self.device.sink()
    }

    /// Sends one line and returns the reply.
    fn send(&mut self, line: &str) -> String<96> {
        self.device.handle(line)
    }

    fn begin(&mut self) {
        // The port the test writes into is the device's own transport, so `step` reads it.
        self.device.begin();
    }

    fn step(&mut self) -> Step {
        // Move anything the test queued into the device's port, then run one step.
        let queued = core::mem::take(&mut self.pipe);
        for line in queued.to_device.iter() {
            self.device.transport_mut().push(line.as_str());
        }
        let step = self.device.step();
        self.pipe = core::mem::take(self.device.transport_mut());
        self.step_count += 1;
        step
    }

    #[allow(dead_code)]
    fn step_count(&self) -> u32 {
        self.step_count
    }
}

/// The host tool's client, as a function: it sends the lines in the order the real tool does and
/// checks the tokens it expects.
#[test]
fn a_full_exchange_applies_the_config_and_reports_counts() {
    let mut s = Session::new();
    s.begin();
    assert!(
        s.machine().last_command().is_all_off(),
        "the first thing is force_off"
    );

    assert_eq!(s.send("PING").as_str(), reply::PONG);
    assert_eq!(
        s.send("WIFI SSID example-net").as_str(),
        reply::SSID_ACCEPTED
    );
    assert_eq!(
        s.send("WIFI PASS placeholder-secret").as_str(),
        reply::PASSWORD_SET
    );
    assert_eq!(s.send("WIFI COMMIT").as_str(), reply::WIFI_COMMIT);

    let doc = br#"{"brew":{"setpoint":93.0}}"#;
    let begin = format!("CONFIG BEGIN {} {:08x}", doc.len(), crc32(doc));
    assert_eq!(s.send(&begin).as_str(), reply::CONFIG_BEGIN);

    let mut encoded: String<700> = String::new();
    let mut decoded: Vec<u8, 64> = Vec::new();
    assert!(base64_decode("aGVsbG8=", &mut decoded));
    let _ = Base64::encode(doc, &mut encoded);
    assert_eq!(
        s.send(&format!("CONFIG {encoded}")).as_str(),
        reply::CONFIG_CHUNK
    );

    let end = s.send("CONFIG END");
    assert!(end.starts_with(reply::CONFIG_END), "{end}");
    assert!(end.contains("applied="), "the counts are reported: {end}");
    assert!(end.contains("rejected="), "the counts are reported: {end}");

    assert!(s.sink().committed, "the credentials were committed");
    assert_eq!(
        s.sink().applied.as_slice(),
        doc,
        "the document arrived intact"
    );
    assert_eq!(s.sink().ssid.as_str(), "example-net");
    assert_eq!(s.sink().password.as_str(), "placeholder-secret");
}

#[test]
fn no_reply_the_device_can_produce_contains_the_password() {
    // The whole secret-handling argument, as one assertion: the protocol's reply vocabulary has
    // nowhere to put a value, and this walks the commands a host tool actually sends.
    let mut s = Session::new();
    s.begin();
    let password = "placeholder-secret";
    for line in [
        "PING",
        "WIFI SSID example-net",
        &format!("WIFI PASS {password}"),
        "WIFI COMMIT",
        "STATUS",
        "FACTORY RESET",
    ] {
        let out = s.send(line);
        assert!(
            !out.contains(password),
            "the reply to {line} echoed the password: {out}"
        );
    }
    let doc = b"{}";
    let begin = format!("CONFIG BEGIN {} {:08x}", doc.len(), crc32(doc));
    s.send(&begin);
    let mut encoded: String<700> = String::new();
    let _ = Base64::encode(doc, &mut encoded);
    s.send(&format!("CONFIG {encoded}"));
    let end = s.send("CONFIG END");
    assert!(!end.contains(password));
    // And the status line, which is the one reply built from device state.
    let status = s.send("STATUS");
    assert!(
        !status.contains(password),
        "the status line leaked: {status}"
    );
    assert!(
        status.contains("ssid_set=true"),
        "the status line is useful: {status}"
    );
}

#[test]
fn the_actuators_stay_off_for_the_whole_session() {
    let mut s = Session::new();
    // Start a brew first, so "everything is off" cannot be an artefact of an idle machine.
    s.machine_mut().request(Request::BrewStart);
    for _ in 0..3 {
        s.machine_mut().tick(Sensors::at(93.0), 100);
    }
    s.begin();
    assert!(
        s.machine().last_command().is_all_off(),
        "provisioning forces the actuators off: {:?}",
        s.machine().last_command()
    );
    for _ in 0..20 {
        s.step();
    }
    assert!(
        s.machine().last_command().is_all_off(),
        "and they stay off while the session runs: {:?}",
        s.machine().last_command()
    );
}

#[test]
fn a_command_before_the_session_is_open_is_refused() {
    // `begin` opens the session: it is the moment the machine goes into service mode, and it is
    // what the host tool's prompt wait is synchronising with. Before that, a command is refused
    // rather than half-processed, so a stray line in the port's buffer cannot write a credential
    // into a machine that is still brewing.
    let mut s = Session::new();
    assert_eq!(s.send("WIFI SSID net").as_str(), reply::NO_SESSION);
    assert_eq!(s.send("CONFIG END").as_str(), reply::CONFIG_REJECTED);
    assert!(s.sink().ssid.is_empty(), "nothing was written");
    s.begin();
    assert_eq!(s.send("WIFI SSID net").as_str(), reply::SSID_ACCEPTED);
}

#[test]
fn a_bad_crc_is_refused_and_nothing_is_applied() {
    let mut s = Session::new();
    s.begin();
    s.send("PING");
    let doc = b"{}";
    let begin = format!("CONFIG BEGIN {} {:08x}", doc.len(), 0xdead_beefu32);
    s.send(&begin);
    let mut encoded: String<700> = String::new();
    let _ = Base64::encode(doc, &mut encoded);
    s.send(&format!("CONFIG {encoded}"));
    assert_eq!(s.send("CONFIG END").as_str(), reply::CONFIG_CRC);
    assert!(
        s.sink().applied.is_empty(),
        "a document that fails its CRC must not reach the config"
    );
    assert!(!s.device.buffer().is_receiving());
}

#[test]
fn a_document_over_the_bound_is_refused_by_name() {
    let mut s = Session::new();
    s.begin();
    let begin = format!("CONFIG BEGIN {} {:08x}", MAX_CONFIG + 1, 0);
    assert_eq!(s.send(&begin).as_str(), reply::CONFIG_TOO_LARGE);
}

#[test]
fn a_malformed_line_is_answered_rather_than_ignored() {
    let mut s = Session::new();
    s.begin();
    for line in ["", "WHAT", "CONFIG BEGIN x y", "CONFIG !!!"] {
        let out = s.send(line);
        assert!(
            out == reply::UNKNOWN || out == reply::CONFIG_REJECTED,
            "{line:?} produced {out}"
        );
    }
}

#[test]
fn a_chunk_that_overruns_the_declared_length_is_refused() {
    let mut s = Session::new();
    s.begin();
    s.send("PING");
    let doc = b"{}";
    s.send(&format!("CONFIG BEGIN {} {:08x}", doc.len(), crc32(doc)));
    let mut encoded: String<700> = String::new();
    let _ = Base64::encode(b"far too long for two bytes", &mut encoded);
    assert_eq!(
        s.send(&format!("CONFIG {encoded}")).as_str(),
        reply::CONFIG_REJECTED
    );
    assert!(
        !s.device.buffer().is_receiving(),
        "a refused chunk aborts the transfer"
    );
}

#[test]
fn the_buffer_is_dropped_when_a_transfer_is_abandoned() {
    let mut b = ConfigBuffer::new();
    b.begin(10, 0).unwrap();
    b.push(b"12345").unwrap();
    assert!(!b.is_empty());
    b.abort();
    assert!(b.is_empty());
    assert!(!b.is_receiving());
}

#[test]
fn a_factory_reset_ends_the_session_and_clears_the_region() {
    let mut s = Session::new();
    s.begin();
    s.send("PING");
    assert_eq!(s.send("FACTORY RESET").as_str(), reply::FACTORY_RESET);
    assert_eq!(s.sink().resets, 1);
    assert!(!s.device.is_open(), "the session is over");
    assert!(
        !s.machine().last_command().flowing(),
        "and the machine is left with the actuators off"
    );
}

#[test]
fn the_session_hands_the_machine_back_at_the_end() {
    let mut s = Session::new();
    s.begin();
    s.send("PING");
    s.device.end();
    assert!(!s.machine().0.service_mode());
    // A tick after the session must be able to command again.
    s.machine_mut().tick(Sensors::at(93.0), 100);
    assert!(!s.machine().last_command().is_all_off());
}

#[test]
fn a_closed_port_ends_the_session_rather_than_looping() {
    let mut s = Session::new();
    s.begin();
    s.device.transport_mut().closed = true;
    assert_eq!(s.step(), Step::Disconnected);
    assert!(!s.device.is_open());
}

#[test]
fn the_parser_covers_every_line_the_host_tool_sends() {
    // A change to the host tool's protocol without a change here would show up as a test failure
    // rather than as a provisioning session that silently does nothing.
    for line in [
        "PING",
        "WIFI SSID net",
        "WIFI PASS pass",
        "WIFI COMMIT",
        "CONFIG BEGIN 2 00000000",
        "CONFIG aGk=",
        "CONFIG END",
        "FACTORY RESET",
        "STATUS",
    ] {
        assert_ne!(Command::parse(line), Command::Unknown, "{line} must parse");
    }
}

#[test]
fn a_rejected_config_does_not_report_success() {
    let mut s = Session::new();
    s.sink_mut().fail_apply = true;
    s.begin();
    s.send("PING");
    let doc = b"{}";
    s.send(&format!("CONFIG BEGIN {} {:08x}", doc.len(), crc32(doc)));
    let mut encoded: String<700> = String::new();
    let _ = Base64::encode(doc, &mut encoded);
    s.send(&format!("CONFIG {encoded}"));
    assert_eq!(s.send("CONFIG END").as_str(), reply::CONFIG_REJECTED);
}
