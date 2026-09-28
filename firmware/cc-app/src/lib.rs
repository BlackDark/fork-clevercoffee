//! Task wiring and the outward-facing surfaces: HTTP, MQTT, OTA, provisioning
//! and logging.
//!
//! Holds no control logic — that lives in `cc-domain`, which is host-testable.
//!
//! See `docs/rust-migration/architecture.md` section 6.5.
