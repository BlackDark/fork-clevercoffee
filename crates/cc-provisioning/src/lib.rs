//! Wi-Fi provisioning: `SoftAP` + DNS interception + a small portal (option P1 in
//! 05 §5).
//!
//! # Rules
//!
//! * The `wifi_provisioning` "USB Serial" transport is **unavailable** on this
//!   hardware: the original ESP32 has no USB peripheral at all, the cable is a
//!   CP2102N UART bridge (05 §5, skill §1 fact 1). Option P3 is not an option.
//! * No credential may be logged at any level, or accepted as a command-line
//!   argument, or accepted over an unauthenticated endpoint. See
//!   `cc_config::Secret<T>` and 05 §5.
//! * The portal only runs while no valid credentials exist, so it cannot collide
//!   with the SPA server for port 80.
//!
//! Owner: R1-06 (feasibility) then R4-11 (implementation). Skeleton only.

#![no_std]
