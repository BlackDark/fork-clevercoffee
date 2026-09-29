//! The config region, as the firmware sees it.
//!
//! The region format itself is `clevercoffee-storage`'s; what is missing, and is here, is the
//! three lines between it and the machine: read the region, parse the document, validate it, and
//! build a [`RuntimeConfig`]. Every one of those four steps can fail, and each has a defined
//! outcome, because the alternative is a machine that boots on whatever came out of flash.
//!
//! The outcomes, in the order they are checked:
//!
//! 1. **No valid slot** — compiled defaults, and a note. Not an error: a machine with defaults
//!    brews, and a machine that refuses to boot because its config is corrupt cannot be fixed
//!    over the network either. This is the C++ behaviour and it is right.
//! 2. **A payload that does not parse** — defaults, and a named reason.
//! 3. **A payload with a rejected or unknown field** — defaults, and the counts. Applying the
//!    fields that happened to be valid is D13's exact shape, so it does not happen here.
//! 4. **A clean payload** — the machine's configuration, and the document it came from, kept so
//!    `/api/config` can show the user what is actually set rather than a reconstruction.

use clevercoffee_config::json;
use clevercoffee_config::{Report, ResolvedDoc};
use clevercoffee_storage::Partition;

use crate::config_rt;
use crate::machine::RuntimeConfig;

/// Why the machine is running on the configuration it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The region held a clean document.
    Region,
    /// The region held nothing usable and the machine is on its compiled defaults.
    DefaultsNoRegion,
    /// The region held something that did not parse.
    DefaultsUnparseable,
    /// The region held a document with a rejected or unknown field.
    DefaultsRejected,
}

impl Source {
    /// A short word for a log line, and for the status endpoint.
    pub const fn as_str(self) -> &'static str {
        match self {
            Source::Region => "region",
            Source::DefaultsNoRegion => "defaults:no_region",
            Source::DefaultsUnparseable => "defaults:unparseable",
            Source::DefaultsRejected => "defaults:rejected",
        }
    }

    /// Whether the machine is running on the user's configuration.
    pub const fn is_user_config(self) -> bool {
        matches!(self, Source::Region)
    }
}

/// The machine's configuration and where it came from.
#[derive(Debug)]
pub struct Loaded {
    pub config: RuntimeConfig,
    pub source: Source,
    /// The validation report, which is meaningful only when the payload parsed.
    pub report: Report,
    /// The document, kept for the export path. `None` when nothing parsed.
    pub document: Option<ResolvedDoc>,
}

impl Loaded {
    /// The one-line summary a boot log wants.
    pub fn log_line(&self) -> heapless::String<320> {
        use core::fmt::Write;
        let mut s = heapless::String::new();
        let _ = write!(s, "config from {} ", self.source.as_str());
        let _ = s.push_str(config_rt::summary(&self.config).as_str());
        if !self.source.is_user_config() {
            let _ = write!(
                s,
                " (accepted {} rejected {} unknown {})",
                self.report.accepted, self.report.rejected, self.report.unknown
            );
        }
        s
    }
}

/// Reads the config region and builds the machine's configuration.
///
/// Takes the partition by value because the region it reads borrows from it, and a caller that
/// wants to write afterwards has to build a new one. The alternative is a borrow that outlives
/// the machine, which on a 320 KB part is a bad trade for one field read.
pub fn load(partition: &Partition) -> Loaded {
    let (outcome, _region) = partition.read();
    if !outcome.has_config() {
        // No valid slot, a version this firmware does not read, or a region that is all zeroes.
        // All three are the same thing to the machine: compiled defaults, and a log line saying
        // which. A downgrade must not overwrite a newer machine's configuration, which is why an
        // unreadable version is not treated as a write candidate either.
        return on_defaults(Source::DefaultsNoRegion, Report::default(), None);
    }
    match partition.payload() {
        Some(p) => from_payload(p),
        None => on_defaults(Source::DefaultsNoRegion, Report::default(), None),
    }
}

/// Builds the machine's configuration from a payload, with the four-step ladder above.
pub fn from_payload(payload: &[u8]) -> Loaded {
    let Ok(document) = json::parse(payload) else {
        return on_defaults(Source::DefaultsUnparseable, Report::default(), None);
    };
    let doc = document.into_resolved();
    let report = clevercoffee_config::validate(&doc, false).report;
    if !report.is_applicable() {
        // Nothing is applied, not even the fields that passed. That is the whole point of the
        // transaction, and it is the defect D13 recorded.
        return on_defaults(Source::DefaultsRejected, report, Some(doc));
    }
    Loaded {
        config: config_rt::build(&doc),
        source: Source::Region,
        report,
        document: Some(doc),
    }
}

/// A machine on its compiled defaults, with a reason and nothing applied.
pub fn defaults(source: Source) -> Loaded {
    on_defaults(source, Report::default(), None)
}

fn on_defaults(source: Source, report: Report, document: Option<ResolvedDoc>) -> Loaded {
    Loaded {
        config: RuntimeConfig::default(),
        source,
        report,
        document,
    }
}

/// The payload to write back, from the machine's configuration and the document it was read from.
///
/// The round trip the `/api/config` download route and the USB export both need: the user's own
/// document with the machine's values folded back in, rather than a document built from the
/// thirty-odd fields the machine knows about, which would drop the other sixty.
pub fn payload_for(loaded: &Loaded, updated: &RuntimeConfig) -> heapless::String<16384> {
    let doc = match &loaded.document {
        Some(d) => config_rt::resolve(d, updated),
        None => {
            // Nothing was stored, so the document is built from the machine's own values. A
            // first-boot export is a legitimate thing for a user to want.
            let empty = ResolvedDoc::new();
            config_rt::resolve(&empty, updated)
        }
    };
    clevercoffee_config::import::export(&|key| {
        doc.value(key).and_then(|v| match v {
            clevercoffee_config::import::DocValue::Bool(b) => {
                Some(clevercoffee_config::Value::Bool(b))
            }
            clevercoffee_config::import::DocValue::Int(i) => {
                Some(clevercoffee_config::Value::Int(i))
            }
            clevercoffee_config::import::DocValue::Number(n) => {
                Some(clevercoffee_config::Value::Number(n))
            }
            clevercoffee_config::import::DocValue::Text { .. }
            | clevercoffee_config::import::DocValue::TextTooLong { .. } => {
                // A text value is the only shape the exporter takes as a borrow, and the
                // document's own storage is a fixed array rather than a `str`. Rather than copy
                // through a temporary the exporter cannot see, text parameters are emitted by the
                // exporter's own schema walk, which is where the hostname and the credentials
                // live.
                None
            }
            clevercoffee_config::import::DocValue::Enum(e) => {
                Some(clevercoffee_config::Value::Enum(e))
            }
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"brew":{"setpoint":92.0,"by_time":{"target_time":30}}}"#;

    #[test]
    fn a_clean_payload_becomes_the_machines_configuration() {
        let loaded = from_payload(GOOD.as_bytes());
        assert_eq!(loaded.source, Source::Region);
        assert!(loaded.source.is_user_config());
        assert_eq!(loaded.config.setpoint_c, 92.0);
        assert_eq!(loaded.config.brew_target_time_ms, 30_000);
        assert!(
            loaded.document.is_some(),
            "the document is kept for the export path"
        );
    }

    #[test]
    fn a_payload_with_a_rejected_field_falls_back_to_defaults_and_applies_nothing() {
        let loaded = from_payload(br#"{"brew":{"setpoint":400.0}}"#);
        assert_eq!(loaded.source, Source::DefaultsRejected);
        assert!(loaded.report.rejected > 0);
        assert_eq!(
            loaded.config.setpoint_c,
            RuntimeConfig::default().setpoint_c,
            "not one field from a rejected document is applied"
        );
    }

    #[test]
    fn an_unparseable_payload_falls_back_to_defaults_with_a_named_reason() {
        for bad in [&b""[..], b"{", b"not json at all", b"[1,2,3]"] {
            let loaded = from_payload(bad);
            assert_eq!(loaded.source, Source::DefaultsUnparseable, "{bad:?}");
            assert_eq!(
                loaded.config.setpoint_c,
                RuntimeConfig::default().setpoint_c
            );
        }
    }

    #[test]
    fn an_erased_region_falls_back_to_defaults_rather_than_refusing_to_boot() {
        // A machine with defaults brews. A machine that will not boot because its config is
        // corrupt cannot be fixed over the network either, and the C++ firmware's behaviour here
        // was right.
        let erased = Partition::blank();
        let loaded = load(&erased);
        assert_eq!(loaded.source, Source::DefaultsNoRegion);
        assert_eq!(
            loaded.config.setpoint_c,
            RuntimeConfig::default().setpoint_c
        );
    }

    #[test]
    fn a_region_holding_a_real_document_is_read_and_applied() {
        // The whole path: flash bytes, region header, CRC, JSON, validation, machine values.
        let partition = Partition::with_payload(GOOD.as_bytes());
        let loaded = load(&partition);
        assert_eq!(loaded.source, Source::Region, "{}", loaded.log_line());
        assert_eq!(loaded.config.setpoint_c, 92.0);
        assert_eq!(loaded.config.brew_target_time_ms, 30_000);
    }

    #[test]
    fn a_region_holding_a_rejected_document_does_not_apply_any_of_it() {
        let partition = Partition::with_payload(br#"{"brew":{"setpoint":400.0}}"#);
        let loaded = load(&partition);
        assert_eq!(loaded.source, Source::DefaultsRejected);
        assert_eq!(
            loaded.config.setpoint_c,
            RuntimeConfig::default().setpoint_c
        );
        assert!(loaded.report.rejected > 0);
    }

    #[test]
    fn the_boot_log_line_names_the_source_and_the_values() {
        let loaded = from_payload(GOOD.as_bytes());
        let line = loaded.log_line();
        assert!(line.contains("config from region"), "{line}");
        assert!(line.contains("setpoint=92"), "{line}");

        let rejected = from_payload(br#"{"brew":{"setpoint":400.0}}"#);
        let line = rejected.log_line();
        assert!(line.contains("defaults:rejected"), "{line}");
        assert!(
            line.contains("rejected"),
            "the counts are in the log: {line}"
        );
    }

    #[test]
    fn an_export_round_trips_through_the_loader() {
        // The migration path's other direction: what the machine would write, read back, produces
        // the same configuration. A round trip that changes a setpoint is a user's machine
        // quietly drifting every time they save their settings.
        let loaded = from_payload(GOOD.as_bytes());
        let payload = payload_for(&loaded, &loaded.config);
        let again = from_payload(payload.as_bytes());
        assert_eq!(again.source, Source::Region, "{}", again.log_line());
        assert_eq!(again.config.setpoint_c, loaded.config.setpoint_c);
        assert_eq!(
            again.config.brew_target_time_ms,
            loaded.config.brew_target_time_ms,
            "{}",
            again.log_line()
        );
    }

    #[test]
    fn an_export_from_a_machine_with_no_stored_document_still_produces_a_usable_one() {
        let loaded = from_payload(b"");
        let payload = payload_for(&loaded, &loaded.config);
        let again = from_payload(payload.as_bytes());
        assert!(!payload.is_empty());
        assert!(
            again.config.setpoint_c > 0.0,
            "a first-boot export must not produce a zero setpoint: {}",
            again.log_line()
        );
    }

    #[test]
    fn a_secret_is_not_written_into_an_export() {
        // D14: the C++ returned four plaintext passwords from four endpoints. The export path
        // goes through the same redacting exporter the API uses, and this is the test that says so
        // for this path specifically.
        let loaded = from_payload(GOOD.as_bytes());
        let payload = payload_for(&loaded, &loaded.config);
        // The credentials and the default passwords are emitted as empty strings; the auth
        // *username* is not a secret and is still readable, which is what the C++ did.
        for secret in ["\"password\": \"\"", "\"ssid\": \"\""] {
            assert!(
                payload.contains(secret),
                "{secret} should be present and empty"
            );
        }
        assert!(!payload.contains("otapass"), "{}", payload);
        assert!(!payload.contains("example-net"), "{}", payload);
        assert!(payload.contains("\"username\": \"admin\""), "{}", payload);
    }
}
