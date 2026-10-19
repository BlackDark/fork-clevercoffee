//! The parameter-classification tests, moved from `cc-hal-esp32/src/web.rs`.

// `clippy::assert_is_empty` is new in clippy 1.99, which is the channel the
// CI host gate runs on (`CC_RUST_TOOLCHAIN=stable`). It is silenced here, in
// this `#[cfg(test)]` module, and nowhere else: the assertions below are on
// COLLECTIONS, so the lint's suggestion (`assert_eq!(x, "")`) does not
// typecheck, and `assert_eq!(x.len(), 0)` would print a count instead of the
// contents.
#![allow(
    clippy::assert_is_empty,
    reason = "the assertions are on collections, so the suggested \
              assert_eq!(x, \"\") does not typecheck"
)]

use super::*;

use alloc::vec;
use alloc::vec::Vec;

#[test]
fn a_write_that_needs_a_reboot_is_named_rather_than_claimed_applied() {
    // The lie the human reported: "success" for a write that cannot change
    // the running machine. `hardware.switches.brew.enabled` is read once by
    // `SwitchBank::new`, so it is in this list.
    let verdict = classify_parameters(&[("hardware.switches.brew.enabled".into(), "true".into())]);
    assert_eq!(
        verdict.reboot_required(),
        vec!["hardware.switches.brew.enabled"]
    );
    // And the status is still 200: the write *was* accepted and persisted; only
    // the runtime effect is deferred.
    assert_eq!(verdict.response().0, 200);
}

#[test]
fn an_ordinary_parameter_is_not_reported_as_needing_a_reboot() {
    // `pid.enabled` is pushed into the running machine by the control task,
    // so it must NOT appear in this list — otherwise every ordinary write
    // would be reported as deferred and the warning would be worthless.
    let verdict = classify_parameters(&[("pid.enabled".into(), "1".into())]);
    assert!(
        verdict.reboot_required().is_empty(),
        "{:?}",
        verdict.reboot_required()
    );
    assert!(!needs_reboot("pid.enabled"));
    assert!(!needs_reboot("brew.setpoint"));
    assert!(!needs_reboot("pid.regular.kp"));
}

#[test]
fn a_rejected_write_still_names_the_reboot_keys_it_did_accept() {
    // A 400 that applied five of six parameters is still a write, and the
    // reboot keys among the five are still deferred.
    let verdict = classify_parameters(&[
        ("pid.enabled".into(), "1".into()),
        ("hardware.switches.steam.enabled".into(), "true".into()),
        ("pid.regular.kp".into(), "not-a-number".into()),
    ]);
    assert_eq!(verdict.response().0, 400);
    assert_eq!(
        verdict.reboot_required(),
        vec!["hardware.switches.steam.enabled"]
    );
}

#[test]
fn a_write_that_changed_nothing_needs_no_reboot() {
    // `ParameterPost::Nothing` is reached when the request names no parameter
    // *with a value*: a blank field is skipped, not rejected. An unparseable value
    // is `Rejected`, not `Nothing`, so the empty field is what gets here.
    let verdict = classify_parameters(&[("pid.regular.kp".into(), String::new())]);
    assert!(matches!(verdict, ParameterPost::Nothing));
    assert!(verdict.reboot_required().is_empty());

    // And the distinction is real: a bad *value* is a 400, not a no-op.
    let rejected = classify_parameters(&[("pid.regular.kp".into(), "banana".into())]);
    assert!(matches!(rejected, ParameterPost::Rejected { .. }));
    assert_eq!(rejected.response().0, 400);
}

#[test]
fn a_parameter_post_of_the_four_kinds_is_accepted() {
    // One of each kind, plus the wider boolean spellings.
    let verdict = classify_parameters(&[
        ("pid.enabled".into(), "1".into()),
        ("mqtt.port".into(), "1884".into()),
        ("pid.regular.kp".into(), "3.5".into()),
        ("system.hostname".into(), "kettle".into()),
    ]);
    assert_eq!(verdict.clone().into_pairs().len(), 4);
    assert!(
        matches!(verdict, ParameterPost::Updated { .. }),
        "{verdict:?}"
    );
    assert_eq!(verdict.response().0, 200);
}

#[test]
fn an_unknown_key_is_a_400_and_names_nothing_in_the_body() {
    // An unknown key gets `{"error":"Some parameter updates failed"}` and the body
    // names nothing: the reasons go to the log.
    let verdict = classify_parameters(&[("no.such.parameter".into(), "1".into())]);
    assert!(matches!(verdict, ParameterPost::Rejected { .. }));
    assert_eq!(
        verdict.response(),
        (400, "{\"error\":\"Some parameter updates failed\"}")
    );
    assert!(verdict.into_pairs().is_empty(), "nothing was written");
}

#[test]
fn a_value_out_of_range_is_the_same_400_as_an_unknown_key() {
    // `:867-868` — one `hasErrors` flag covers both, so both are one status
    // and one body.
    for raw in ["-1e6", "1e6", "abc", "NaN"] {
        let verdict = classify_parameters(&[("pid.regular.kp".into(), raw.into())]);
        assert_eq!(
            verdict.response().0,
            400,
            "pid.regular.kp={raw:?} should be a 400"
        );
    }
}

#[test]
fn one_rejected_parameter_does_not_lose_the_accepted_ones() {
    // Each pair is applied as the loop reaches it and the error flag is only a
    // flag, so the good pairs still travel to the control task and the answer is
    // still a 400.
    let verdict = classify_parameters(&[
        ("mqtt.port".into(), "1884".into()),
        ("pid.regular.kp".into(), "nope".into()),
        ("pid.enabled".into(), "true".into()),
    ]);
    let ParameterPost::Rejected { accepted, reasons } = &verdict else {
        panic!("expected a rejection, got {verdict:?}");
    };
    assert_eq!(accepted.len(), 2);
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].starts_with("pid.regular.kp:"), "{reasons:?}");
    assert_eq!(verdict.response().0, 400);
}

#[test]
fn a_request_that_names_no_parameter_is_the_third_response() {
    // The third body, for a request whose fields are all valueless (they are
    // skipped) or which names nothing at all.
    for pairs in [
        vec![],
        vec![("pid.enabled".into(), String::new())],
        vec![(String::new(), "1".into())],
    ] {
        let verdict = classify_parameters(&pairs);
        assert_eq!(verdict, ParameterPost::Nothing, "{pairs:?}");
        assert!(verdict.clone().into_pairs().is_empty());
        assert_eq!(
            verdict.response(),
            (
                200,
                "{\"success\":true,\"message\":\"No parameters updated\"}"
            )
        );
    }
}

#[test]
fn the_updated_response_is_the_documented_body() {
    assert_eq!(
        ParameterPost::Updated {
            accepted: Vec::new()
        }
        .response(),
        (
            200,
            "{\"success\":true,\"message\":\"Parameters updated and saved\"}"
        )
    );
}

#[test]
fn enabling_auth_is_reported_as_needing_a_reboot() {
    // The authentication decision is taken once, when the server starts. Enabling
    // `system.auth.enabled` therefore protects nothing until the next boot, and an
    // operator who sets it, sees `200`, and is still serving an open API is exactly
    // the lie this list exists to prevent. `requiresRebootKeys` is where it is
    // said.
    let verdict = classify_parameters(&[("system.auth.enabled".into(), "1".into())]);
    assert_eq!(
        verdict.reboot_required(),
        vec!["system.auth.enabled"],
        "enabling authentication must be reported as needing a reboot"
    );
    // ... and a reboot is not claimed for a parameter that takes effect live.
    let live = classify_parameters(&[("brew.setpoint".into(), "95".into())]);
    assert!(live.reboot_required().is_empty());
}

#[test]
fn the_upload_body_is_bounded_and_the_cap_is_the_wire_limit() {
    // The wire cap is 16 KB. It is a different question from the configuration's
    // own 8 KB policy limit, and a body between them gets the clearer answer.
    assert_eq!(MAX_CONFIG_UPLOAD_BYTES, 16 * 1024);
    // The policy limit underneath it is `cc_config`'s own 8 KB; the two are
    // different questions ("how much will I read off the wire" and "how big a
    // configuration may be") and a body between them gets the clearer answer.
    const {
        assert!(MAX_CONFIG_UPLOAD_BYTES > cc_config::MAX_CONFIG_BYTES);
    }
}
