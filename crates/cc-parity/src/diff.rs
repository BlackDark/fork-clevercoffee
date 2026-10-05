//! Diff two observations, and classify every diff line against the ledger.
//!
//! # The rule the whole task turns on
//!
//! 06 §Definitions says parity is behavioural equivalence *"excluding the
//! divergences listed in `intentional-diffs.md`"*, and that `run.sh` "exits
//! non-zero on any diff not explained by `intentional-diffs.md`". So the
//! classification is not a convenience: it is the definition of the gate.
//!
//! # Declared, not ignored
//!
//! A diff that matches a declared divergence is reported as **expected**, with
//! the ledger entry that explains it, and **counted**. It is not dropped. The
//! report has a line per ledger entry showing how many diffs it accounted for,
//! so a reader sees the ledger rather than a clean pass — and a ledger entry
//! that accounted for *nothing* is itself reported, because a divergence that
//! stopped diverging is either a fix nobody wrote down or a regression in the
//! explanation.
//!
//! # The ledger format
//!
//! A ledger is a JSON file, or a fenced block inside `intentional-diffs.md`.
//! Each entry declares a heading (so a reader can find the prose), and a set of
//! matchers. A matcher is a substring or a regular expression over the diff
//! line. There is deliberately no "match everything" entry: an entry with an
//! empty matcher list is rejected at load, because it would silence the harness
//! completely, which is the failure mode this design exists to prevent.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::observe::Observation;

/// One declared divergence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Divergence {
    /// A short stable id, e.g. `div1`. Used in the report.
    pub id: String,
    /// The heading in `intentional-diffs.md` this entry is the machine-readable
    /// form of. The runner checks that the heading is present in the document,
    /// so a ledger cannot drift away from the prose it claims to encode.
    pub heading: String,
    /// The scenarios whose diffs this entry explains.
    ///
    /// Empty means "any scenario". Scoping matters: divergence #1 (the pump
    /// watchdogs) is only expected in scenarios that run a pump, and an
    /// unexplained diff in `cold_boot` must not be explained by it.
    #[serde(default)]
    pub scenarios: Vec<String>,
    /// Substrings or `/regex/` literals. A diff line matches if any does.
    pub matchers: Vec<String>,
}

impl Divergence {
    /// Whether this entry claims to explain `line` in `scenario`.
    #[must_use]
    pub fn explains(&self, scenario: &str, line: &str) -> bool {
        if !self.scenarios.is_empty() && !self.scenarios.iter().any(|s| s == scenario) {
            return false;
        }
        self.matchers.iter().any(|m| matcher_matches(m, line))
    }
}

/// The set of declared divergences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    /// Every entry, in document order.
    #[serde(default)]
    pub entries: Vec<Divergence>,
}

impl Ledger {
    /// The entry that explains `line` in `scenario`, if any.
    ///
    /// The **first** match wins and the rest are reported too, so a diff claimed
    /// by two entries is visible rather than silently resolved.
    #[must_use]
    pub fn classify(&self, scenario: &str, line: &str) -> Vec<&Divergence> {
        self.entries
            .iter()
            .filter(|e| e.explains(scenario, line))
            .collect()
    }
}

/// Whether a matcher matches a diff line.
///
/// A matcher is either `/regex/` or a plain substring. The regex form exists
/// because the prose in `intentional-diffs.md` is about *patterns* ("the pump
/// watchdogs fire") and a substring of `PumpTimeoutFired` would be enough for
/// that one but not for the general case.
fn matcher_matches(matcher: &str, line: &str) -> bool {
    if let Some(inner) = matcher.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
        return crate::diff::tiny_regex::is_match(inner, line);
    }
    line.contains(matcher)
}

/// One differing line, with what the ledger said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// The line, as it appears in the observation's canonical rendering.
    pub text: String,
    /// Present in the Rust observation and not the C++ baseline (`+`), or the
    /// reverse (`-`).
    pub side: Side,
    /// The ledger entries that claim this line. Empty means unexplained.
    pub explained_by: Vec<String>,
}

/// Which side of the diff a line is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Only in the new (Rust) observation.
    Rust,
    /// Only in the reference (C++) baseline.
    Cpp,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rust => "+",
            Self::Cpp => "-",
        })
    }
}

/// The classification of one diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// A declared divergence explains it.
    Expected {
        /// The ledger entry ids, in order.
        by: Vec<String>,
    },
    /// Nothing explains it. **This is a regression** and it is what makes the
    /// runner exit non-zero.
    Unexplained,
}

/// The result of diffing two observations against a ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Every differing line, in order, with its classification.
    pub lines: Vec<DiffLine>,
    /// How many lines were explained.
    pub expected: usize,
    /// How many were not.
    pub unexplained: usize,
    /// Ledger entry ids that explained at least one line, with the count.
    ///
    /// Reported so a reader sees which parts of the ledger actually did work.
    pub ledger_hits: BTreeMap<String, usize>,
    /// Ledger entry ids that explained nothing.
    ///
    /// A divergence that stopped diverging. Not an error — a fix is a good
    /// outcome — but it is reported, because the entry then needs removing
    /// from the ledger and nobody would otherwise know.
    pub ledger_unused: Vec<String>,
}

impl Verdict {
    /// Whether the run passed: nothing unexplained.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.unexplained == 0
    }
}

/// Diff `rust` against `cpp` under `ledger`.
///
/// The comparison is over the three fields that are comparable across the two
/// firmwares: the state sequence, the effect-name sequence, and the final
/// actuator state. Endpoint bodies and log lines are **not** diffed here — a
/// JSON body contains a temperature, a heap figure and an uptime, none of which
/// two runs share, so a field-by-field diff of one is noise. They are captured
/// (they are what a human reads) and asserted on by the scenario's own
/// `assert.http` block, which is the right place for a keyed comparison.
#[must_use]
pub fn classify(cpp: &Observation, rust: &Observation, ledger: &Ledger) -> Verdict {
    let mut lines: Vec<DiffLine> = Vec::new();

    // ---- the state sequence ------------------------------------------------
    lines.extend(diff_seq(
        "state",
        cpp.states.iter().map(|s| s.as_str()),
        rust.states.iter().map(|s| s.as_str()),
    ));

    // ---- the effect-name sequence -----------------------------------------
    // Names only, not the reduced payloads: the C++ has no effect stream at
    // all, so this is Rust-side instrumentation and comparing payloads across
    // firmwares would be comparing a thing against its absence.
    lines.extend(diff_seq(
        "effect",
        cpp.effects.iter().map(|e| e.name.as_str()),
        rust.effects.iter().map(|e| e.name.as_str()),
    ));

    // ---- the final actuator state ----------------------------------------
    for (field, c, r) in [
        ("pump", cpp.actuators.pump, rust.actuators.pump),
        (
            "water_valve",
            cpp.actuators.water_valve,
            rust.actuators.water_valve,
        ),
        (
            "steam_valve",
            cpp.actuators.steam_valve,
            rust.actuators.steam_valve,
        ),
        (
            "heater_duty_zero",
            cpp.actuators.heater_duty == 0,
            rust.actuators.heater_duty == 0,
        ),
        (
            "emergency_latched",
            cpp.actuators.emergency_latched,
            rust.actuators.emergency_latched,
        ),
    ] {
        if c != r {
            lines.push(DiffLine {
                text: format!(
                    "actuator.{field} cpp={} rust={}",
                    render_bool(c),
                    render_bool(r)
                ),
                side: Side::Rust,
                explained_by: Vec::new(),
            });
        }
    }

    // ---- classify ----------------------------------------------------------
    let mut expected = 0usize;
    let mut unexplained = 0usize;
    let mut ledger_hits: BTreeMap<String, usize> = BTreeMap::new();
    let scenario = rust.scenario.as_str();

    for line in &mut lines {
        let hits = ledger.classify(scenario, &line.text);
        if hits.is_empty() {
            unexplained += 1;
        } else {
            expected += 1;
            line.explained_by = hits.iter().map(|d| d.id.clone()).collect();
            for d in hits {
                *ledger_hits.entry(d.id.clone()).or_insert(0) += 1;
            }
        }
    }

    let ledger_unused = ledger
        .entries
        .iter()
        .filter(|e| !ledger_hits.contains_key(&e.id))
        .map(|e| e.id.clone())
        .collect();

    Verdict {
        lines,
        expected,
        unexplained,
        ledger_hits,
        ledger_unused,
    }
}

fn render_bool(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

/// A longest-common-subsequence diff of two sequences of labels.
///
/// LCS rather than a set difference on purpose: **order is the observation.**
/// `CloseWaterValve` before `DisablePump` and the reverse are the same set and
/// different behaviour, and 09 §15 is a one-loop ordering bug that a set
/// comparison would report as identical.
fn diff_seq<'a>(
    label: &str,
    cpp: impl Iterator<Item = &'a str>,
    rust: impl Iterator<Item = &'a str>,
) -> Vec<DiffLine> {
    let a: Vec<&str> = cpp.collect();
    let b: Vec<&str> = rust.collect();

    // lcs[i][j] = length of the LCS of a[i..] and b[j..]
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(DiffLine {
                text: format!("{label} cpp:{}", a[i]),
                side: Side::Cpp,
                explained_by: Vec::new(),
            });
            i += 1;
        } else {
            out.push(DiffLine {
                text: format!("{label} rust:{}", b[j]),
                side: Side::Rust,
                explained_by: Vec::new(),
            });
            j += 1;
        }
    }
    while i < a.len() {
        out.push(DiffLine {
            text: format!("{label} cpp:{}", a[i]),
            side: Side::Cpp,
            explained_by: Vec::new(),
        });
        i += 1;
    }
    while j < b.len() {
        out.push(DiffLine {
            text: format!("{label} rust:{}", b[j]),
            side: Side::Rust,
            explained_by: Vec::new(),
        });
        j += 1;
    }
    out
}

/// Load a ledger from a JSON string.
///
/// # Errors
///
/// Returns a message if the JSON is malformed, or if an entry has no matchers
/// — an entry that matches everything is not a declaration, it is a mute.
pub fn parse_ledger(text: &str) -> Result<Ledger, String> {
    let ledger: Ledger = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if ledger.entries.is_empty() {
        return Err("the ledger has no entries".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    for e in &ledger.entries {
        if !seen.insert(e.id.as_str()) {
            return Err(format!("duplicate ledger id {:?}", e.id));
        }
        if e.matchers.is_empty() {
            return Err(format!(
                "ledger entry {:?} has no matchers: an entry with no matchers would explain \
                 every diff, which is the opposite of a declaration",
                e.id
            ));
        }
    }
    Ok(ledger)
}

/// Extract the fenced ```ledger blocks from `intentional-diffs.md`.
///
/// The ledger lives *in* the prose document rather than beside it, so it cannot
/// drift: adding a divergence means writing the reasoning and the matchers in
/// one place.
///
/// # Errors
///
/// A message if a block is not valid JSON or violates the rules in
/// [`parse_ledger`].
pub fn ledger_from_markdown(markdown: &str) -> Result<Ledger, String> {
    // Accumulate the whole block before parsing. A ledger entry is one JSON
    // object and is written across two or three lines for readability, so
    // parsing line by line would truncate every one of them.
    let mut entries: Vec<Divergence> = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    let mut fenced = false;
    let mut prose = String::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if !fenced && trimmed.starts_with("```ledger") {
            fenced = true;
            block.clear();
            continue;
        }
        if fenced {
            if trimmed == "```" {
                fenced = false;
                let body = block.join(" ");
                if !body.trim().is_empty() {
                    entries.extend(parse_ledger(&format!("{{\"entries\":[{body}]}}"))?.entries);
                }
                block.clear();
                continue;
            }
            block.push(trimmed);
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    if fenced {
        return Err("an unterminated ```ledger fence".to_string());
    }
    let ledger = Ledger { entries };
    let mut seen = std::collections::BTreeSet::new();
    for e in &ledger.entries {
        if !seen.insert(e.id.as_str()) {
            return Err(format!("duplicate ledger id {:?}", e.id));
        }
        if e.matchers.is_empty() {
            return Err(format!("ledger entry {:?} has no matchers", e.id));
        }
        // Against the **prose**, not the whole document. A `contains` over the
        // raw markdown would find the heading inside the entry's own JSON and
        // pass unconditionally, which is the exact drift the check exists to
        // catch.
        if !prose.contains(&e.heading) {
            return Err(format!(
                "ledger entry {:?} claims heading {:?}, which is not in the document — the \
                 ledger and the prose have drifted",
                e.id, e.heading
            ));
        }
    }
    if ledger.entries.is_empty() {
        return Err("no ```ledger blocks found".to_string());
    }
    Ok(ledger)
}

/// A deliberately tiny regex engine.
///
/// The only patterns the ledger needs are `.*` and literal text — a diff line is
/// a short, structured string like `effect rust:PumpTimeoutFired`, and the
/// matchers that matter are "contains this effect name" and "an actuator line
/// where the two sides disagree". A real regex dependency for that would be
/// more surface than the thing it serves, and this crate is host-only tooling
/// that ships in no image.
///
/// Supported: literal characters, `.` (any character except newline), `*`
/// (zero or more of the previous element), and `\/` for a literal `/`.
pub mod tiny_regex {
    /// Whether `pattern` matches anywhere in `text`.
    #[must_use]
    pub fn is_match(pattern: &str, text: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let t: Vec<char> = text.chars().collect();
        // Backtracking match at every start position. Patterns are one or two
        // characters wide, so this is linear in practice and the exponential
        // worst case is unreachable from a hand-written ledger.
        (0..=t.len()).any(|start| match_here(&p, 0, &t, start))
    }

    fn match_here(p: &[char], mut pi: usize, t: &[char], mut ti: usize) -> bool {
        while pi < p.len() {
            if pi + 1 < p.len() && p[pi + 1] == '*' {
                let atom = p[pi];
                // Greedy: consume as many of the atom as possible, then hand the
                // rest of the pattern the position *after* the run. Back off one
                // atom at a time. The recursion has to be at `ti + count` — at
                // `ti` it would re-check the same position for ever.
                let mut count = 0;
                loop {
                    if match_here(p, pi + 2, t, ti + count) {
                        return true;
                    }
                    if ti + count < t.len() && (atom == '.' || t[ti + count] == atom) {
                        count += 1;
                    } else {
                        return false;
                    }
                }
            }
            if ti >= t.len() {
                return false;
            }
            if p[pi] != '.' && p[pi] != t[ti] {
                return false;
            }
            pi += 1;
            ti += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::Actuators;

    fn obs(scenario: &str, states: &[&str], effects: &[&str]) -> Observation {
        let mut o = Observation::new(scenario, "x");
        o.states = states.iter().map(|s| (*s).to_string()).collect();
        o.effects = effects
            .iter()
            .map(|e| crate::observe::Observed {
                name: (*e).to_string(),
                effect: crate::observe::ObservedEffect::Plain,
                at_ms: 0,
            })
            .collect();
        o
    }

    fn ledger_with(entries: Vec<Divergence>) -> Ledger {
        Ledger { entries }
    }

    fn div(id: &str, scenarios: &[&str], matchers: &[&str]) -> Divergence {
        Divergence {
            id: id.to_string(),
            heading: "x".to_string(),
            scenarios: scenarios.iter().map(|s| (*s).to_string()).collect(),
            matchers: matchers.iter().map(|m| (*m).to_string()).collect(),
        }
    }

    #[test]
    fn identical_observations_produce_no_diff() {
        let a = obs("s", &["PID_NORMAL"], &["EnablePump"]);
        let v = classify(&a, &a, &Ledger::default());
        assert!(v.is_clean(), "{v:?}");
        assert_eq!(v.lines.len(), 0);
    }

    #[test]
    fn a_state_difference_is_unexplained_and_fails() {
        // The core requirement: a synthetic undeclared diff must be caught.
        //
        // A substitution is reported as **two** lines — one removed from the
        // baseline, one added — because a reader needs to see both sides, and
        // because a divergence declared against "the Rust side gained X" is a
        // different declaration from one against "the C++ side lost X".
        let cpp = obs("brew", &["PID_NORMAL", "PID_NORMAL"], &[]);
        let rust = obs("brew", &["PID_NORMAL", "BREW_RUNNING"], &[]);
        let v = classify(&cpp, &rust, &Ledger::default());
        assert!(!v.is_clean());
        assert_eq!(v.unexplained, 2);
        assert_eq!(v.expected, 0);
        assert!(
            v.lines
                .iter()
                .any(|l| l.text.contains("BREW_RUNNING") && l.side == Side::Rust),
            "{v:?}"
        );
        assert!(
            v.lines.iter().any(|l| l.side == Side::Cpp),
            "the removed side must be reported too: {v:?}"
        );
    }

    #[test]
    fn a_declared_divergence_is_reported_as_expected_and_counted() {
        let cpp = obs("brew", &["PID_NORMAL"], &["EnablePump"]);
        let rust = obs("brew", &["PID_NORMAL"], &["EnablePump", "PumpTimeoutFired"]);
        let ledger = ledger_with(vec![div(
            "div1",
            &["brew"],
            &["/effect rust:.*PumpTimeoutFired/"],
        )]);
        let v = classify(&cpp, &rust, &ledger);
        assert!(v.is_clean(), "declared divergence must not fail: {v:?}");
        assert_eq!(v.expected, 1);
        assert_eq!(v.unexplained, 0);
        assert_eq!(v.ledger_hits.get("div1"), Some(&1));
        // And the line is *reported*, not dropped.
        assert_eq!(v.lines.len(), 1);
        assert_eq!(v.lines[0].explained_by, ["div1"]);
    }

    #[test]
    fn a_divergence_outside_its_scenarios_does_not_explain() {
        // div1 is scoped to scenarios that run a pump. A diff in cold_boot must
        // not be excused by it.
        let cpp = obs("cold_boot", &["PID_NORMAL"], &[]);
        let rust = obs("cold_boot", &["PID_NORMAL"], &["PumpTimeoutFired"]);
        let ledger = ledger_with(vec![div(
            "div1",
            &["brew_by_time"],
            &["/effect rust:.*PumpTimeoutFired/"],
        )]);
        let v = classify(&cpp, &rust, &ledger);
        assert!(!v.is_clean());
        assert_eq!(v.unexplained, 1);
    }

    #[test]
    fn a_ledger_entry_that_matched_nothing_is_reported() {
        let cpp = obs("s", &["PID_NORMAL"], &[]);
        let rust = obs("s", &["PID_NORMAL"], &[]);
        let ledger = ledger_with(vec![div("div9", &[], &["never-happens"])]);
        let v = classify(&cpp, &rust, &ledger);
        assert!(v.is_clean());
        assert_eq!(v.ledger_unused, ["div9"]);
    }

    #[test]
    fn an_actuator_difference_is_a_diff() {
        let mut cpp = obs("s", &[], &[]);
        let mut rust = obs("s", &[], &[]);
        cpp.actuators = Actuators::default();
        rust.actuators = Actuators {
            pump: true,
            ..Actuators::default()
        };
        let v = classify(&cpp, &rust, &Ledger::default());
        assert_eq!(v.unexplained, 1);
        assert!(v.lines[0].text.contains("actuator.pump"), "{v:?}");
    }

    #[test]
    fn order_is_observed_not_just_the_set() {
        // 09 §15 is a one-loop ordering bug. A set comparison would call these
        // equal.
        let cpp = obs("s", &[], &["DisablePump", "SafeHardwareShutdown"]);
        let rust = obs("s", &[], &["SafeHardwareShutdown", "DisablePump"]);
        let v = classify(&cpp, &rust, &Ledger::default());
        assert!(!v.is_clean());
        assert_eq!(v.unexplained, 2);
    }

    #[test]
    fn an_omission_from_the_rust_side_is_reported() {
        let cpp = obs("s", &["PID_NORMAL", "BREW_RUNNING"], &[]);
        let rust = obs("s", &["PID_NORMAL"], &[]);
        let v = classify(&cpp, &rust, &Ledger::default());
        assert!(!v.is_clean());
        assert_eq!(v.lines.len(), 1);
        assert_eq!(v.lines[0].side, Side::Cpp);
        assert!(v.lines[0].text.contains("cpp:BREW_RUNNING"), "{v:?}");
    }

    #[test]
    fn a_ledger_entry_with_no_matchers_is_refused() {
        let json = r#"{"entries":[{"id":"x","heading":"h","matchers":[]}]}"#;
        let err = parse_ledger(json).expect_err("must refuse");
        assert!(err.contains("no matchers"), "{err}");
    }

    #[test]
    fn a_duplicate_ledger_id_is_refused() {
        let json = r#"{"entries":[
            {"id":"x","heading":"h","matchers":["a"]},
            {"id":"x","heading":"h","matchers":["b"]}]}"#;
        let err = parse_ledger(json).expect_err("must refuse");
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn an_empty_ledger_is_refused() {
        assert!(parse_ledger(r#"{"entries":[]}"#).is_err());
    }

    #[test]
    fn a_ledger_is_extracted_from_the_markdown() {
        // `r###"…"###` because the document is full of `##` headings and a
        // ``` fence, and a `"##` inside it would close a shorter raw string.
        let md = r###"
# Doc

## 1. Some divergence

Explains a thing.

```ledger
{"id":"div1","heading":"## 1. Some divergence","scenarios":["brew"],
 "matchers":["/effect rust:.*PumpTimeoutFired/"]}
```
"###;
        let ledger = ledger_from_markdown(md).expect("extracts");
        assert_eq!(ledger.entries.len(), 1);
        assert!(ledger.entries[0].explains("brew", "effect rust:PumpTimeoutFired"));
        assert!(!ledger.entries[0].explains("cold_boot", "effect rust:PumpTimeoutFired"));
    }

    #[test]
    fn a_ledger_entry_naming_a_missing_heading_is_refused() {
        // The anti-drift check: the ledger must be the machine-readable form of
        // prose that is actually in the document.
        let md = r###"
## 1. Something else

```ledger
{"id":"div1","heading":"## 9. A heading that was never written","matchers":["a"]}
```
"###;
        let err = ledger_from_markdown(md).expect_err("must refuse");
        assert!(err.contains("drifted"), "{err}");
    }

    #[test]
    fn a_ledger_entry_with_no_matcher_in_the_document_is_refused() {
        let md = r###"
## 1. Something

```ledger
{"id":"div1","heading":"## 1. Something","matchers":[]}
```
"###;
        let err = ledger_from_markdown(md).expect_err("must refuse");
        assert!(err.contains("no matchers"), "{err}");
    }

    #[test]
    fn a_document_with_no_ledger_block_is_refused() {
        // A ledger that silently parsed to nothing would explain nothing, and
        // the runner would then report every diff as a regression without saying
        // why. Failing here is the better outcome.
        let err = ledger_from_markdown("# Doc\n\nNo fences here.\n").expect_err("must refuse");
        assert!(err.contains("no ```ledger blocks"), "{err}");
    }

    #[test]
    fn the_real_intentional_diffs_document_yields_a_usable_ledger() {
        // The ledger lives in the prose document so the two cannot drift. This
        // asserts the real document parses and that every entry has matchers,
        // which is the property `just parity` depends on.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/history/divergences.md"
        );
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{path}: {e}; the ledger lives in the document"));
        let ledger = ledger_from_markdown(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            ledger.entries.len() >= 4,
            "expected the divergences R1-08 seeds, found {}",
            ledger.entries.len()
        );
        for e in &ledger.entries {
            assert!(!e.matchers.is_empty(), "{} has no matchers", e.id);
        }
    }

    // ---- the tiny regex --------------------------------------------------

    #[test]
    fn tiny_regex_matches_a_literal() {
        assert!(tiny_regex::is_match("abc", "xxabcxx"));
        assert!(!tiny_regex::is_match("abc", "xxabxx"));
    }

    #[test]
    fn tiny_regex_dot_is_any_character() {
        assert!(tiny_regex::is_match("a.c", "abc"));
        assert!(tiny_regex::is_match("a.c", "axc"));
        assert!(!tiny_regex::is_match("a.c", "ac"));
    }

    #[test]
    fn tiny_regex_star_is_zero_or_more() {
        assert!(tiny_regex::is_match("ab*c", "ac"));
        assert!(tiny_regex::is_match("ab*c", "abbbc"));
        assert!(!tiny_regex::is_match("ab*c", "abbbd"));
    }

    #[test]
    fn tiny_regex_prefix_and_suffix_with_a_star_in_the_middle() {
        let p = "effect rust:.*PumpTimeoutFired";
        assert!(tiny_regex::is_match(p, "effect rust:PumpTimeoutFired"));
        assert!(tiny_regex::is_match(
            p,
            "effect rust:DisablePump,PumpTimeoutFired"
        ));
        assert!(!tiny_regex::is_match(p, "effect cpp:PumpTimeoutFired"));
    }

    #[test]
    fn tiny_regex_handles_an_empty_pattern() {
        assert!(tiny_regex::is_match("", "anything"));
    }
}
