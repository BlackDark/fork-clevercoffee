//! Authentication tests use the `device-tests` feature's header encoder.

use super::*;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;

use cc_domain::secret::Secret;
use cc_protocol::http_auth::WWW_AUTHENTICATE;

/// A `Config` with the given `system.auth.*`, for the auth cases.
fn auth_config(enabled: bool, username: &str, password: &str) -> Arc<Config> {
    let mut config = Config::default();
    config.system.auth.enabled = enabled;
    config.system.auth.username = username.to_string();
    config.system.auth.password = Secret::new(password.to_string());
    Arc::new(config)
}

/// base64, so a test states a credential rather than a blob.
fn basic(user: &str, pass: &str) -> String {
    format!(
        "Basic {}",
        cc_protocol::http_auth::tests_support::encode(format!("{user}:{pass}").as_bytes())
    )
}

#[test]
fn auth_is_inert_when_it_is_switched_off() {
    // The default. `system.auth.enabled` is `false`
    // (`cc-config/src/schema.rs:690-694`), so an operator who has never
    // touched it sees exactly the firmware they had before this existed.
    let auth = Auth::from_config(&auth_config(false, "admin", "admin"));
    assert!(!auth.is_enforced());
    assert!(auth.admits(None), "no header at all must pass");
    assert!(auth.admits(Some("Basic Zm9vOmJhcg==")), "a wrong one too");
}

#[test]
fn auth_admits_the_right_credentials_and_refuses_everything_else() {
    let auth = Auth::from_config(&auth_config(true, "barista", "espresso"));
    assert!(auth.is_enforced());
    assert!(auth.admits(Some(&basic("barista", "espresso"))));
    assert!(!auth.admits(None), "no header");
    assert!(
        !auth.admits(Some(&basic("barista", "wrong"))),
        "wrong password"
    );
    assert!(
        !auth.admits(Some(&basic("wrong", "espresso"))),
        "wrong username"
    );
    assert!(!auth.admits(Some("Basic not-base64!")), "malformed");
    assert!(!auth.admits(Some("Bearer whatever")), "wrong scheme");
}

#[test]
fn auth_with_empty_credentials_serves_the_api_open() {
    // The middleware is installed only when the username AND the password are
    // non-empty, and the else arm is a warning, not a locked door. Deliberate:
    // the alternative locks an operator out of a machine whose only other
    // console is a UART.
    for (username, password) in [("", "espresso"), ("barista", ""), ("", "")] {
        let auth = Auth::from_config(&auth_config(true, username, password));
        assert!(
            !auth.is_enforced(),
            "{username:?}/{password:?} must not challenge"
        );
        assert!(auth.admits(None));
    }
}

#[test]
fn the_challenge_realm_is_the_expected_one() {
    // A realm is the key a browser caches credentials against, so renaming it
    // makes every browser ask for the password again.
    assert_eq!(WWW_AUTHENTICATE, "Basic realm=\"CleverCoffee\"");
}

#[test]
fn the_auth_debug_never_prints_the_configuration() {
    // A derived `Debug` on a struct holding the whole `Config` would print
    // every parameter including the four credentials.
    let auth = Auth::from_config(&auth_config(true, "barista", "espresso"));
    let rendered = format!("{auth:?}");
    assert!(!rendered.contains("espresso"), "{rendered}");
    assert!(!rendered.contains("barista"), "{rendered}");
}
