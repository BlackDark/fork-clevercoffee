//! Compile-time guards on the feature combination.
//!
//! Included by each of the three binaries. Without these, `--features board-esp32s3` on the
//! ESP32 build would silently link the S3 board module and produce a binary that cannot talk to
//! its own chip, which is exactly the kind of mistake a bench should never discover.

/// The board is chosen by the binary, so the binary also asserts that exactly one board feature
/// reached it. `cargo` unions the features of the whole crate, so a caller that passes two
/// boards gets two of these and the build stops.
#[cfg(all(feature = "board-esp32", feature = "board-esp32s3"))]
compile_error!("board-esp32 and board-esp32s3 are mutually exclusive");

#[cfg(all(feature = "board-esp32", feature = "board-esp32c6"))]
compile_error!("board-esp32 and board-esp32c6 are mutually exclusive");

#[cfg(all(feature = "board-esp32s3", feature = "board-esp32c6"))]
compile_error!("board-esp32s3 and board-esp32c6 are mutually exclusive");

#[cfg(not(any(
    feature = "board-esp32",
    feature = "board-esp32s3",
    feature = "board-esp32c6"
)))]
compile_error!("no board feature: enable exactly one of board-esp32, board-esp32s3, board-esp32c6");

/// The provisioning transport is a separate choice, because the S3 and C6 boards expose both a
/// native USB port and a UART bridge and the operator picks by plugging into the right one.
#[cfg(all(feature = "prov-uart", feature = "prov-usb-cdc"))]
compile_error!("prov-uart and prov-usb-cdc are mutually exclusive");

#[cfg(not(any(feature = "prov-uart", feature = "prov-usb-cdc")))]
compile_error!("no provisioning transport: enable exactly one of prov-uart, prov-usb-cdc");

/// The ESP32 has no native USB, so a USB CDC provisioning channel cannot work on it. This is
/// checked here rather than left to a device that silently never receives credentials.
#[cfg(all(feature = "board-esp32", feature = "prov-usb-cdc"))]
compile_error!("the ESP32 has no native USB; use prov-uart on board-esp32");
