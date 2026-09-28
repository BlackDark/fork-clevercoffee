//! An in-memory [`ConfigStore`] for host tests.
//!
//! Not part of the library: the only production implementation is `NvsStore`,
//! which is task R3-08 and lives in `cc-hal-esp32`. Keeping this in the test
//! tree means the shipped crate carries no test scaffolding into a 1.8 MB
//! image.
//!
//! It can also be told to fail, because "the store works" is not the interesting
//! case — "what does the firmware do when the store is unavailable" is.

#![allow(dead_code)] // each test binary uses a different subset

use cc_config::{Config, ConfigStore, StoreError};

/// A [`ConfigStore`] backed by an `Option<Config>`.
pub struct MemStore {
    stored: Option<Config>,
    /// When set, every operation fails with this error.
    pub fail_with: Option<StoreError>,
    /// Number of `save` calls, so a test can assert a write actually happened.
    pub saves: usize,
}

impl MemStore {
    /// An empty store, as after a factory reset.
    pub fn empty() -> Self {
        Self {
            stored: None,
            fail_with: None,
            saves: 0,
        }
    }

    /// A store already holding `config`.
    pub fn with(config: Config) -> Self {
        Self {
            stored: Some(config),
            fail_with: None,
            saves: 0,
        }
    }

    /// A store whose every operation fails.
    pub fn failing(error: StoreError) -> Self {
        Self {
            stored: None,
            fail_with: Some(error),
            saves: 0,
        }
    }

    /// The stored value, if any, bypassing the failure injection.
    pub fn peek(&self) -> Option<&Config> {
        self.stored.as_ref()
    }
}

impl ConfigStore for MemStore {
    fn load(&mut self) -> Result<Option<Config>, StoreError> {
        match self.fail_with {
            Some(e) => Err(e),
            None => Ok(self.stored.clone()),
        }
    }

    fn save(&mut self, config: &Config) -> Result<(), StoreError> {
        self.saves += 1;
        if let Some(e) = self.fail_with {
            return Err(e);
        }
        self.stored = Some(config.clone());
        Ok(())
    }

    fn erase_all(&mut self) -> Result<(), StoreError> {
        if let Some(e) = self.fail_with {
            return Err(e);
        }
        self.stored = None;
        Ok(())
    }
}
