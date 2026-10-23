//! Settings in the `cc_cfg` partition: one map entry per parameter (key = parameter
//! key, value = [`cc_core::config::stored`] bytes) plus the shot counter. Only changed
//! values are written, so a slider in the UI costs a few bytes, not a page.
//!
//! The storage task also performs restarts, so a pending save is never lost.

use cc_core::Config;
use cc_core::config::{PARAMS, stored};
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use heapless::String;
use log::{info, warn};
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

use crate::flash::{self, Region};
use crate::shared::{self, RestartKind};

type Key = String<64>;
type Map = MapStorage<Key, Region, Cache<Uncached, Uncached, Uncached, Key>>;

const SHOTS_KEY: &str = "!shots";
/// Changes are collected this long before writing.
const SAVE_DELAY: Duration = Duration::from_secs(2);

pub struct Store {
    map: Map,
    saved: Config,
    saved_shots: u32,
}

fn key(s: &str) -> Key {
    Key::try_from(s).unwrap_or_default()
}

impl Store {
    pub async fn open() -> Option<Self> {
        let part = flash::find_data(flash::SUBTYPE_SETTINGS).await?;
        let config = MapConfig::try_new(0..part.len).ok()?;
        Some(Self {
            map: MapStorage::new(Region(part), config, Cache::new_uncached()),
            saved: Config::default(),
            saved_shots: 0,
        })
    }

    /// Reads the stored settings over the defaults. Unknown keys (parameters a later
    /// firmware removed) are skipped; so are values that no longer fit their type.
    pub async fn load(&mut self) -> (Config, u32) {
        let mut cfg = Config::default();
        let mut shots = 0u32;
        let mut buf = [0u8; 256];
        let mut skipped = 0usize;
        match self.map.fetch_all_items(&mut buf).await {
            Ok(mut items) => loop {
                let mut item = [0u8; 256];
                match items.next::<&[u8]>(&mut item).await {
                    Ok(Some((k, v))) => {
                        if k.as_str() == SHOTS_KEY {
                            if let Ok(b) = v.try_into() {
                                shots = u32::from_le_bytes(b);
                            }
                        } else if let Some(i) = cc_core::config::find(&k) {
                            match stored::decode(PARAMS[i].kind, v).map(|v| cfg.set(i, v)) {
                                Some(Ok(())) => {}
                                _ => skipped += 1,
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        warn!("settings: read error {e:?}");
                        break;
                    }
                }
            },
            Err(e) => warn!("settings: cannot open ({e:?}), using defaults"),
        }
        if skipped > 0 {
            warn!("settings: {skipped} stored values did not fit and were reset");
        }
        self.saved = cfg.clone();
        self.saved_shots = shots;
        (cfg, shots)
    }

    /// Writes what differs from the last saved state.
    pub async fn save(&mut self, cfg: &Config, shots: u32) {
        let mut buf = [0u8; 256];
        let mut written = 0;
        for (i, p) in PARAMS.iter().enumerate() {
            let v = cfg.get(i);
            if v == self.saved.get(i) {
                continue;
            }
            let mut val = [0u8; 128];
            let Some(n) = stored::encode(&v, &mut val) else {
                continue;
            };
            match self.map.store_item(&mut buf, &key(p.key), &&val[..n]).await {
                Ok(()) => written += 1,
                Err(e) => {
                    warn!("settings: write {} failed: {e:?}", p.key);
                    return;
                }
            }
        }
        self.saved = cfg.clone();
        if shots != self.saved_shots {
            let b = shots.to_le_bytes();
            if let Err(e) = self.map.store_item(&mut buf, &key(SHOTS_KEY), &&b[..]).await {
                warn!("settings: write shots failed: {e:?}");
                return;
            }
            self.saved_shots = shots;
            written += 1;
        }
        if written > 0 {
            info!("settings: saved {written} values");
        }
    }

    pub async fn erase(&mut self) {
        if let Err(e) = self.map.erase_all().await {
            warn!("settings: erase failed: {e:?}");
        }
        self.saved = Config::default();
        self.saved_shots = 0;
    }
}

/// Applies the provisioning blob written by `just provision`, then erases it.
pub async fn apply_provisioning(cfg: &mut Config) -> bool {
    let Some(part) = flash::find_data(flash::SUBTYPE_PROVISION).await else {
        return false;
    };
    let mut blob = alloc::vec![0u8; part.len as usize];
    if flash::read(part.offset, &mut blob).await.is_err() {
        return false;
    }
    let Some((n, err)) = cc_api::provision::apply(&blob, cfg) else {
        return false;
    };
    info!("provisioning: applied {n} settings");
    if let Some((k, e)) = err {
        warn!("provisioning: {k} refused: {e}");
    }
    if let Err(e) = flash::erase(part.offset, part.offset + part.len).await {
        warn!("provisioning: erase failed: {e:?}");
    }
    true
}

#[embassy_executor::task]
pub async fn run(mut store: Store) -> ! {
    loop {
        match select(shared::PERSIST.wait(), shared::RESTART.wait()).await {
            Either::First(()) => {
                Timer::after(SAVE_DELAY).await;
                persist(&mut store).await;
            }
            Either::Second(kind) => {
                // Let the HTTP response go out first.
                Timer::after(Duration::from_millis(500)).await;
                shared::with(|s| s.ota_active = true); // actuators off
                match kind {
                    RestartKind::Plain => persist(&mut store).await,
                    RestartKind::FactoryReset => store.erase().await,
                    RestartKind::WifiReset => {
                        let (mut cfg, shots) =
                            shared::with(|s| (s.cfg.clone(), s.machine.status(&s.cfg).shots_since_backflush));
                        cfg.system_wifi_ssid.clear();
                        cfg.system_wifi_password.clear();
                        store.save(&cfg, shots).await;
                    }
                }
                Timer::after(Duration::from_millis(100)).await;
                warn!("restarting ({kind:?})");
                esp_hal::system::software_reset();
            }
        }
    }
}

async fn persist(store: &mut Store) {
    let (cfg, shots) = shared::with(|s| {
        s.config_dirty = false;
        s.shots_dirty = false;
        (s.cfg.clone(), s.machine.status(&s.cfg).shots_since_backflush)
    });
    store.save(&cfg, shots).await;
}
