//! [`Auth`]: the HTTP Basic check every route on this server goes through.

use alloc::sync::Arc;

use crate::web::http_auth::{self, MAX_CREDENTIAL_BYTES};
use cc_config::Config;
use log::warn;

/// The HTTP Basic credential check for every route on this server.
///
/// # Why this holds the `Arc<Config>` and not a copy of the credential
///
/// `Web::start` is handed the boot-time `Arc<Config>` and the server outlives the
/// call, so the password is read through [`cc_domain::secret::Secret::expose`]
/// per request and **no second copy of it exists in the process**. Cloning it
/// into an owned field would have been one line and one more plaintext
/// credential lying in the heap.
///
/// # What `enforced` means, and why it is decided once
///
/// The decision is made when the server starts, not per request, so a change to
/// `system.auth.enabled` protects nothing until the next reboot. `needs_reboot`
/// reports it in the `POST /api/parameters` answer so the operator is told rather
/// than left guessing.
///
/// **Empty credentials mean no authentication at all**, not a locked door: the
/// server logs "web authentication is enabled but no credentials are set" and
/// serves the API open. Failing closed instead would mean that enabling auth and
/// then not finishing locks an operator out of a machine whose only other console
/// is a UART. The warning is deliberately loud.
#[derive(Clone)]
pub struct Auth {
    config: Arc<Config>,
    enforced: bool,
}

impl core::fmt::Debug for Auth {
    /// Never the configuration.
    ///
    /// A derived `Debug` would print every parameter including the four
    /// credentials; `Secret` redacts its own field but the *rest* of the config
    /// is not what a log line about authentication wants.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Auth")
            .field("enforced", &self.enforced)
            .finish_non_exhaustive()
    }
}

impl Auth {
    /// Decide, once, whether this server challenges.
    #[must_use]
    pub fn from_config(config: &Arc<Config>) -> Self {
        let wanted = config.system.auth.enabled;
        let has_credentials = !config.system.auth.username.is_empty()
            && !config.system.auth.password.expose().is_empty();
        if wanted && !has_credentials {
            warn!(
                "http: web authentication is enabled but no credentials are set; \
                 serving the API unauthenticated"
            );
        }
        Self {
            config: Arc::clone(config),
            enforced: wanted && has_credentials,
        }
    }

    /// Whether requests are challenged at all.
    #[must_use]
    pub fn is_enforced(&self) -> bool {
        self.enforced
    }

    /// Whether this request may proceed.
    ///
    /// `header` is the request's `Authorization` header, or `None`. **It is
    /// never logged and never formatted**, on any path: a rejected request is a
    /// counted and answered, nothing more.
    #[must_use]
    pub fn admits(&self, header: Option<&str>) -> bool {
        if !self.enforced {
            return true;
        }
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        http_auth::authorized(
            header,
            &self.config.system.auth.username,
            self.config.system.auth.password.expose(),
            &mut scratch,
        )
    }
}

#[cfg(test)]
mod tests;
