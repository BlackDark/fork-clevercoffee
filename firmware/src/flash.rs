//! The SPI flash, shared by the settings store and firmware updates. Both run on the
//! thread-mode executor; the async mutex only serialises them.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_storage_async::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::partitions::{self, AppPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType};
use esp_storage::{FlashStorage, FlashStorageError};

pub static FLASH: Mutex<CriticalSectionRawMutex, Option<FlashStorage<'static>>> = Mutex::new(None);

pub const SECTOR: u32 = FlashStorage::SECTOR_SIZE;

/// Data partition subtypes of `partitions.csv`.
pub const SUBTYPE_SETTINGS: u8 = 0x40;
pub const SUBTYPE_PROVISION: u8 = 0x41;

pub async fn init(flash: FlashStorage<'static>) {
    *FLASH.lock().await = Some(flash);
}

/// A partition's location.
#[derive(Debug, Clone, Copy)]
pub struct Part {
    pub offset: u32,
    pub len: u32,
}

/// Finds a partition by type.
pub async fn find(kind: PartitionType) -> Option<Part> {
    let mut guard = FLASH.lock().await;
    let flash = guard.as_mut()?;
    let mut buf = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = partitions::read_partition_table(flash, &mut buf).ok()?;
    let e = table.find_partition(kind).ok()??;
    Some(Part {
        offset: e.offset(),
        len: e.len(),
    })
}

pub async fn find_data(subtype: u8) -> Option<Part> {
    let mut guard = FLASH.lock().await;
    let flash = guard.as_mut()?;
    let mut buf = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = partitions::read_partition_table(flash, &mut buf).ok()?;
    table
        .iter()
        .find(|e| e.raw_type() == 1 && e.raw_subtype() == subtype)
        .map(|e| Part {
            offset: e.offset(),
            len: e.len(),
        })
}

/// The OTA slot an update goes to.
pub async fn next_app_slot() -> Option<(Part, AppPartitionSubType)> {
    let slot = {
        let mut guard = FLASH.lock().await;
        let flash = guard.as_mut()?;
        let mut buf = [0u8; PARTITION_TABLE_MAX_LEN];
        let mut ota = esp_bootloader_esp_idf::ota_updater::OtaUpdater::new(flash, &mut buf).ok()?;
        ota.next_partition().ok()?.1
    };
    Some((find(PartitionType::App(slot)).await?, slot))
}

/// Raw NOR access: offsets and lengths word-aligned, writes only clear bits (erase
/// first). The inherent `FlashStorage::write` would erase on its own.
pub async fn read(offset: u32, buf: &mut [u8]) -> Result<(), FlashStorageError> {
    FLASH
        .lock()
        .await
        .as_mut()
        .ok_or(FlashStorageError::IoError)?
        .read_nor(offset, buf)
}

pub async fn write(offset: u32, buf: &[u8]) -> Result<(), FlashStorageError> {
    FLASH
        .lock()
        .await
        .as_mut()
        .ok_or(FlashStorageError::IoError)?
        .write_nor(offset, buf)
}

pub async fn erase(from: u32, to: u32) -> Result<(), FlashStorageError> {
    FLASH
        .lock()
        .await
        .as_mut()
        .ok_or(FlashStorageError::IoError)?
        .erase(from, to)
}

/// One partition as an async NOR flash starting at 0, for `sequential-storage`.
pub struct Region(pub Part);

impl ErrorType for Region {
    type Error = FlashStorageError;
}

impl ReadNorFlash for Region {
    const READ_SIZE: usize = 4;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        read(self.0.offset + offset, bytes).await
    }

    fn capacity(&self) -> usize {
        self.0.len as usize
    }
}

impl NorFlash for Region {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = SECTOR as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        erase(self.0.offset + from, self.0.offset + to).await
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        write(self.0.offset + offset, bytes).await
    }
}
