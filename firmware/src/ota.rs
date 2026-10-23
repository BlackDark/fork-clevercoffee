//! Firmware updates over HTTP into the inactive OTA slot. The heater, pump and valve
//! stay off while an image is written (flash writes stall the CPU for milliseconds).

use core::cell::RefCell;

use alloc::boxed::Box;
use cc_web::{OtaError, OtaStatus};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN;
use log::{info, warn};

use crate::flash::{self, Part, SECTOR};
use crate::shared;

/// First byte of an ESP application image.
const IMAGE_MAGIC: u8 = 0xE9;

struct Session {
    part: Part,
    /// Bytes written to flash.
    written: u32,
    /// The sector being filled.
    sector: Box<[u8; SECTOR as usize]>,
    filled: usize,
}

#[derive(Clone, Copy)]
struct Progress {
    status: &'static str,
    in_progress: bool,
    uploaded: u32,
    total: u32,
    error: Option<&'static str>,
}

static TARGET: Mutex<CriticalSectionRawMutex, RefCell<Option<Part>>> = Mutex::new(RefCell::new(None));
static PROGRESS: Mutex<CriticalSectionRawMutex, RefCell<Progress>> = Mutex::new(RefCell::new(Progress {
    status: "idle",
    in_progress: false,
    uploaded: 0,
    total: 0,
    error: None,
}));
/// Only the HTTP task that began the update touches the session.
static SESSION: embassy_sync::mutex::Mutex<CriticalSectionRawMutex, Option<Session>> =
    embassy_sync::mutex::Mutex::new(None);

/// Boot: confirms the running image (no rollback after a successful start) and
/// finds the slot the next update goes to.
pub async fn init() {
    {
        let mut guard = flash::FLASH.lock().await;
        if let Some(f) = guard.as_mut() {
            let mut buf = [0u8; PARTITION_TABLE_MAX_LEN];
            if let Ok(mut ota) = OtaUpdater::new(f, &mut buf)
                && let Ok(OtaImageState::New | OtaImageState::PendingVerify) = ota.current_ota_state()
            {
                let _ = ota.set_current_ota_state(OtaImageState::Valid);
                info!("ota: new image confirmed");
            }
        }
    }
    match flash::next_app_slot().await {
        Some((part, slot)) => {
            info!("ota: updates go to {slot:?} ({} KB)", part.len / 1024);
            TARGET.lock(|t| *t.borrow_mut() = Some(part));
        }
        None => warn!("ota: no update slot in the partition table"),
    }
}

fn progress(f: impl FnOnce(&mut Progress)) {
    PROGRESS.lock(|p| f(&mut p.borrow_mut()));
}

pub fn status() -> OtaStatus {
    PROGRESS.lock(|p| {
        let p = *p.borrow();
        let max = TARGET.lock(|t| t.borrow().map_or(0, |t| t.len));
        let pct = if max == 0 {
            0
        } else {
            (u64::from(p.uploaded) * 100 / u64::from(p.total.max(1))).min(100)
        };
        OtaStatus {
            status: p.status,
            progress: pct as u8,
            in_progress: p.in_progress,
            uploaded: p.uploaded,
            total: p.total,
            error: p.error,
        }
    })
}

pub fn begin() -> Result<(), OtaError> {
    let part = TARGET.lock(|t| *t.borrow()).ok_or(OtaError::Write)?;
    let busy = PROGRESS.lock(|p| {
        let mut p = p.borrow_mut();
        if p.in_progress {
            return true;
        }
        *p = Progress {
            status: "uploading",
            in_progress: true,
            uploaded: 0,
            total: part.len,
            error: None,
        };
        false
    });
    if busy {
        return Err(OtaError::Busy);
    }
    let Ok(mut s) = SESSION.try_lock() else {
        return Err(OtaError::Busy);
    };
    *s = Some(Session {
        part,
        written: 0,
        sector: Box::new([0xFF; SECTOR as usize]),
        filled: 0,
    });
    shared::with(|s| s.ota_active = true);
    info!("ota: update started");
    Ok(())
}

async fn flush_sector(s: &mut Session) -> Result<(), OtaError> {
    if s.filled == 0 {
        return Ok(());
    }
    if s.written + SECTOR > s.part.len {
        return Err(OtaError::TooLarge);
    }
    let at = s.part.offset + s.written;
    flash::erase(at, at + SECTOR).await.map_err(|_| OtaError::Write)?;
    // Pad the tail to whole words; the rest of the sector stays erased.
    let len = s.filled.next_multiple_of(4);
    s.sector[s.filled..len].fill(0xFF);
    flash::write(at, &s.sector[..len])
        .await
        .map_err(|_| OtaError::Write)?;
    s.written += SECTOR;
    s.filled = 0;
    s.sector.fill(0xFF);
    Ok(())
}

pub async fn write(mut data: &[u8]) -> Result<(), OtaError> {
    let mut guard = SESSION.lock().await;
    let s = guard.as_mut().ok_or(OtaError::Invalid)?;
    if s.written == 0 && s.filled == 0 && data.first().is_some_and(|&b| b != IMAGE_MAGIC) {
        return Err(OtaError::Invalid);
    }
    let n = data.len() as u32;
    while !data.is_empty() {
        let take = (SECTOR as usize - s.filled).min(data.len());
        s.sector[s.filled..s.filled + take].copy_from_slice(&data[..take]);
        s.filled += take;
        data = &data[take..];
        if s.filled == SECTOR as usize {
            flush_sector(s).await?;
        }
    }
    progress(|p| p.uploaded += n);
    Ok(())
}

pub async fn finish() -> Result<(), OtaError> {
    let mut guard = SESSION.lock().await;
    let mut s = guard.take().ok_or(OtaError::Invalid)?;
    if s.written == 0 && s.filled == 0 {
        drop(guard);
        abort(OtaError::Invalid);
        return Err(OtaError::Invalid);
    }
    if let Err(e) = flush_sector(&mut s).await {
        drop(guard);
        abort(e);
        return Err(e);
    }
    let activated = {
        let mut fl = flash::FLASH.lock().await;
        let f = fl.as_mut().ok_or(OtaError::Write)?;
        let mut buf = [0u8; PARTITION_TABLE_MAX_LEN];
        OtaUpdater::new(f, &mut buf).and_then(|mut ota| {
            ota.activate_next_partition()?;
            ota.set_current_ota_state(OtaImageState::New)
        })
    };
    if activated.is_err() {
        drop(guard);
        abort(OtaError::Write);
        return Err(OtaError::Write);
    }
    progress(|p| {
        p.status = "success";
        p.in_progress = false;
    });
    info!("ota: image written ({} KB)", s.written / 1024);
    Ok(())
}

pub fn abort(err: OtaError) {
    if let Ok(mut s) = SESSION.try_lock() {
        *s = None;
    }
    progress(|p| {
        p.status = "error";
        p.in_progress = false;
        p.error = Some(err.message());
    });
    shared::with(|s| s.ota_active = false);
    warn!("ota: update failed: {}", err.message());
}
