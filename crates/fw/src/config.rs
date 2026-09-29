//! Reading the config region out of flash.
//!
//! One function, shared by the three binaries, because the partition layout is a property of the
//! image rather than of the chip. The board's partition table is read, the `config` partition is
//! found, its bytes are copied into the app's region type, and the app's loader turns those bytes
//! into the machine's configuration.
//!
//! # What happens when there is no partition
//!
//! The machine boots on its compiled defaults and says so in the log. A firmware image written
//! without the partition table — which is what `just build` produces before `just flash` writes
//! one — has no config region at all, and a machine that refused to start because of that would
//! be indistinguishable from a broken one.
//!
//! # Why this is not on the control path
//!
//! Flash reads block for milliseconds and an erase for tens. The control tick is one millisecond
//! and the safety tick must not slip, so the region is read once, here, before the loop starts,
//! and nothing on the control path ever touches flash. A configuration change from the web API is
//! written by the network task and takes effect on the next boot, which is the contract the C++
//! firmware had and the one the frontend's `restart: true` hint already expects.
//!
//! # The size
//!
//! The app's region is two 32 KB slots, so the partition must be at least 64 KB. A table that says
//! otherwise is one this firmware did not write, and reading past its end would be a buffer
//! overrun, so the length is checked before a single byte is copied.

use clevercoffee_app::store::{self, Source};
use clevercoffee_storage::Partition;
use esp_storage::FlashStorage;

/// The partition name the region is stored under. Matches `partitions-rust.csv`.
pub const CONFIG_PARTITION: &str = "config";

/// Why a config read did not produce the user's configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReadFailure {
    /// The bootloader reported no partition table, so the image was written without one.
    NoPartitionTable,
    /// The table has no `config` partition.
    NoConfigPartition,
    /// The partition is smaller than the region this firmware reads, which means the table and the
    /// firmware disagree and copying it would overrun a buffer.
    TooSmall { found: usize, needed: usize },
    /// A read or write to the flash peripheral failed.
    FlashError,
}

impl ReadFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            ReadFailure::NoPartitionTable => "no partition table",
            ReadFailure::NoConfigPartition => "no config partition",
            ReadFailure::TooSmall { .. } => "config partition too small",
            ReadFailure::FlashError => "flash error",
        }
    }
}

/// Reads the config partition and builds the machine's configuration.
///
/// Never fails. An unreadable region is a machine on its compiled defaults, not a machine that will
/// not start: the C++ firmware fell back the same way, and a machine that will not boot because
/// its configuration is corrupt cannot be fixed over the network either.
pub fn read(flash: esp_hal::peripherals::FLASH) -> store::Loaded {
    match region(flash) {
        Ok(region) => store::load(&region),
        Err(_) => store::defaults(Source::DefaultsNoRegion),
    }
}

/// The config partition's bytes, or why there are none.
pub fn region(flash: esp_hal::peripherals::FLASH) -> Result<Partition, ReadFailure> {
    let mut storage = FlashStorage::new(flash);
    let mut table_buf = [0u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN];
    let table =
        esp_bootloader_esp_idf::partitions::read_partition_table(&mut storage, &mut table_buf)
            .map_err(|_| ReadFailure::NoPartitionTable)?;
    let entry = table
        .iter()
        .find(|p| p.label_as_str() == CONFIG_PARTITION)
        .ok_or(ReadFailure::NoConfigPartition)?;

    let needed = core::mem::size_of::<Partition>();
    if (entry.len() as usize) < needed {
        return Err(ReadFailure::TooSmall {
            found: entry.len() as usize,
            needed,
        });
    }

    // A `FlashRegion` is the bootloader's view of one partition: it applies the partition's offset
    // and its own bounds check, so a table that disagrees with the firmware fails here rather than
    // reading past the end of flash.
    let mut region_view = entry.as_flash_region(&mut storage);
    let mut region = Partition::blank();
    region_view
        .read(0, region.as_bytes_mut())
        .map_err(|_| ReadFailure::FlashError)?;
    Ok(region)
}
