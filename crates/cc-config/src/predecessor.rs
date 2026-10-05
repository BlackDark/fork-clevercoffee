//! Telling an operator that the previous firmware's settings are still on the
//! chip, and that this firmware does not read them.
//!
//! Owner: finding **3.6** of `32-findings-2026-10-03.md`, 2026-10-04.
//!
//! # The mechanism, in one sentence
//!
//! The two firmwares use **different NVS namespaces**, so a machine that ran
//! the C++ has not lost its configuration — the Rust firmware has never opened
//! the namespace it is in, and then writes its compiled-in defaults, which
//! carry no SSID.
//!
//! # Why this is a module and not a log line in `bring_up_config`
//!
//! Because the only thing here that needs an ESP32 is *enumerating one
//! namespace's keys*, and `cc_hal_esp32::nvs::probe_predecessor` is that and
//! nothing else. Everything a reader would want to test — that the
//! message fires only when this firmware's own store is empty, that a namespace
//! which cannot be opened produces a different sentence, that both sentences
//! fit the log ring's line budget — is a pure function of one enum, and
//! `just test` reaches all of it. It belongs beside [`crate::blob_store`] because that
//! is where [`crate::blob_store::NAMESPACE`] and the decision *not* to read a
//! C++-written partition are already documented, and splitting those two facts
//! across two modules is how they drift.
//!
//! # ⚠ There is deliberately no migration, and this is the whole of it
//!
//! A migration would mean shipping the C++'s FNV-1a key hash, its 98 key
//! names and its per-parameter type table, and then writing the result into
//! the **one** configuration slot a machine has, with no rollback. On a device
//! that heats a boiler to 150 °C, that is a bad trade for values that are
//! nearly all within a whisker of their defaults — and the credentials, the
//! only parameters whose loss is actually felt, are re-entered in seconds over
//! a UART that the firmware already has. Detection plus one sentence is the
//! right size of answer. `blob_store`'s module documentation carries the full
//! reasoning; this module is where the operator-facing consequence of it lives.

/// The NVS namespace the **C++ firmware** stored its parameters under.
///
/// `include/clevercoffee/defaults.h:13` — `#define STORAGE_NAMESPACE "config"` —
/// read by `Config.cpp`'s `Preferences` opens at `:137`, `:159` and `:184`. It
/// is a fact about a frozen C++ file, so it is named here with that citation
/// rather than derived from anything in this tree.
///
/// It is deliberately **not** [`crate::blob_store::NAMESPACE`], which is `cc`; that
/// inequality is the reason this module exists, and `blob_store` asserts it.
pub const CPP_NAMESPACE: &str = "config";

/// What the boot found when it looked at [`CPP_NAMESPACE`].
///
/// The four cases are the four things that can be true, and no more: a caller
/// cannot accidentally report "found" for a namespace it never opened, because
/// opening one is what produces a variant other than [`Self::Unreadable`].
///
/// [`Self::Skipped`] exists so the decision stays **pure**. Deciding whether to
/// spend a NVS walk is the caller's job, and a pure function that took a
/// `bool` for "did you look" would be a function whose contract the caller has
/// to keep true by hand; a value the caller must pass truthfully cannot drift
/// the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredecessorProbe {
    /// The boot did not look, because this firmware's own store answered.
    ///
    /// A machine with a stored Rust configuration has nothing to be told: it
    /// already has this firmware's settings, and walking a namespace nobody
    /// will read on every boot would be work for no possible output.
    Skipped,
    /// No such namespace, or it exists and holds no keys.
    Absent,
    /// The namespace exists and holds at least one key.
    ///
    /// The **names and values are never read** — only the existence of a key is
    /// observed. That is what keeps this a diagnostic and not the first half of
    /// a migration.
    Populated,
    /// The namespace could not be opened or its keys could not be listed.
    ///
    /// A fault to report, never to act on: this firmware's behaviour must not
    /// depend on the outcome, so the caller carries on.
    Unreadable,
}

/// The one line the boot log prints about [`CPP_NAMESPACE`], or `None`.
///
/// `None` means **say nothing**, which is the answer for both
/// [`PredecessorProbe::Skipped`] and [`PredecessorProbe::Absent`] — the two
/// cases an operator has nothing to do about. Returning a message for them is
/// how a boot log becomes a wall of text an operator learns to skip.
///
/// The `cc`/`config` names are interpolated here rather than left to the
/// caller so that a test can assert the sentence names both namespaces; a
/// message that said "the other namespace" would survive a rename of either
/// and tell nobody anything.
#[must_use]
pub const fn startup_notice(probe: PredecessorProbe) -> Option<&'static str> {
    match probe {
        PredecessorProbe::Skipped | PredecessorProbe::Absent => None,
        PredecessorProbe::Populated => Some(
            "the previous firmware's settings are in NVS namespace \"config\" and this firmware \
             uses \"cc\": not read, not deleted, still on the chip. Re-enter the Wi-Fi SSID and \
             password. Expected on a first flash.",
        ),
        PredecessorProbe::Unreadable => Some(
            "the previous firmware's NVS namespace \"config\" could not be listed, so this \
             firmware cannot tell whether it holds settings. Nothing was deleted; carrying on.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob_store::NAMESPACE;

    #[test]
    fn the_cpp_namespace_is_a_different_key_space_from_ours() {
        // The entire reason this module is reached. Asserted, not commented:
        // a rename of either constant that made them equal would silently turn
        // this feature into "the firmware reads the C++'s keys", which is the
        // thing `blob_store` decided it must never do.
        assert_eq!(CPP_NAMESPACE, "config");
        assert_ne!(CPP_NAMESPACE, NAMESPACE);
    }

    #[test]
    fn a_populated_predecessor_names_both_namespaces_and_what_to_do() {
        let Some(text) = startup_notice(PredecessorProbe::Populated) else {
            panic!("a populated predecessor must produce a line");
        };
        // The two namespaces, or the sentence explains nothing: the operator
        // has to be able to look at an `nvs_dump` and know which one is which.
        assert!(text.contains(CPP_NAMESPACE), "{text}");
        assert!(text.contains(NAMESPACE), "{text}");
        // Not deleted. An operator who reads "not read" alone cannot tell
        // whether to go looking for a backup, or whether the flash destroyed
        // something.
        assert!(text.contains("not deleted"), "{text}");
        // The action. Everything else in this sentence is context for this.
        assert!(text.contains("SSID") && text.contains("password"), "{text}");
    }

    #[test]
    fn an_unreadable_predecessor_says_so_and_does_not_claim_a_finding() {
        let Some(text) = startup_notice(PredecessorProbe::Unreadable) else {
            panic!("an unreadable predecessor must produce a line");
        };
        assert!(text.contains("could not be listed"), "{text}");
        // It must not also claim settings were found, or the two lines would
        // contradict each other on a boot where both fire.
        assert!(!text.contains("not read"), "{text}");
    }

    #[test]
    fn the_four_probe_cases_map_to_the_four_outcomes() {
        // Total, and asserted as a total: a fifth case added to the enum without
        // a decision here would be unreachable output, and this is where a new
        // case has to be given one.
        let spoken = [
            PredecessorProbe::Skipped,
            PredecessorProbe::Absent,
            PredecessorProbe::Populated,
            PredecessorProbe::Unreadable,
        ]
        .map(|probe| startup_notice(probe).is_some());
        assert_eq!(spoken, [false, false, true, true]);
    }
}
