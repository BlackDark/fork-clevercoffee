//! The network task, per board.
//!
//! The connection shape, the bridge and the tests for them are in `clevercoffee_app::net`, where
//! they run on the host. What is left here is what a board contributes: the port, and the Wi-Fi
//! association that produces a listener.
//!
//! # What is not here
//!
//! `embassy-net`'s listener and `esp-radio`'s association. Both are build-verified in the spikes
//! and the credentials come from the provisioning channel, but a radio cannot be exercised without
//! one, and a hand-written association nobody has run is worse than a missing one. The contract
//! they plug into is [`clevercoffee_app::net::Connection`]: a socket in, requests out, a bounded
//! number of connections, and a stalled client dropped rather than waited on (D39).

/// The port the C++ firmware served on, and the one this port serves on.
pub use clevercoffee_app::net::HTTP_PORT;

/// The connection bounds, re-exported so a board writes one import.
pub use clevercoffee_app::net::{CONNECTION_IDLE_MS, MAX_CONNECTIONS, READ_CHUNK};
