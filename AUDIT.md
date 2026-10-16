# Audit

Run 2026-10-15 at `1bd4cd70` (`main`). Docs-only. Verified against
`CC_RUST_TOOLCHAIN=stable just check` (green; 1318 host tests, 112 device tests
registered) and the tools under coverage.

## Scorecard

| Area | Findings | M | L |
| --- | --- | --- | --- |
| code | 1 | 0 | 1 |
| cicd | 1 | 1 | 0 |
| deps | 2 | 1 | 1 |
| security | 0 | 0 | 0 |
| cleanup | 3 | 1 | 2 |
| total | 7 | 3 | 4 |

Top 5 by benefit/effort: CI-1, DEP-1, CLN-1, CODE-1, CLN-2.

## code

### CODE-1 · L · effort S · `crates/cc-display/src/fmt.rs:107,338`

- Evidence: the `# Panics` contract (49-54) says "Never" and the module header
  (28-35) promises an out-of-range fallback. Neither exists.
  `scale_round_half_even` returns `None`, reaching `unreachable!`, for
  `decimals >= 50` (verified: 50, 55, 56, 200 and `u32::MAX` all return `None`).
  For the decimals production actually uses, `n << exponent` overflows `i128`
  and returns silently wrong digits from magnitude >= ~3.4e38 (d=0), ~3.4e37
  (d=1), ~3.4e36 (d=2), ~3.4e29 (d=9). Third symptom: `Formatted = String<24>`
  (338) truncates silently, every push `.ok()`-swallowed, and the sweep
  (521-538) only asserts `len <= 24`, never that the digits are right, so
  `1e21` with 2 decimals renders short. `fmt_oracle.txt` spans magnitude <= ~64
  with decimals 0-3, so no test reaches any of the three branches.
- Fix: implement the documented fallback on `None`; assert the exact rendering at
  the capacity boundary; add a large-magnitude case to `fmt_parity.rs`.
- Benefit: three undocumented behaviours become documented and tested. Not
  reachable from production today: every caller passes a literal `decimals` of
  0, 1 or 2 over sensor- and config-bounded values.

Dropped by review: the 146 identical lines in `cc-display/benches/alloc.rs` and
`cc-machine/benches/alloc.rs` are a real duplication (same md5
`26385f70`), but each copy is the single source for its own bench+test pair, so
the "cannot drift apart" failure mode cannot occur, and the fix would add a
workspace dev crate to satisfy a header sentence.

## cicd

### CI-1 · M · effort S · `.github/workflows/release.yml:31,82,117`

- Evidence: workflow-level `permissions: contents: write` also covers the `guard`
  job, which only fetches refs and runs `git merge-base --is-ancestor`; it has no
  `permissions:` of its own. `zizmor` 1.30.1 (offline) reports
  `excessive-permissions` (high confidence) there plus two low-confidence
  `cache-poisoning` findings at 82 and 117 (`jdx/mise-action` with no `cache:`,
  `actions/setup-node` with `cache: pnpm`), both in the job that builds and
  publishes `firmware.bin`.
- Fix: `contents: read` at workflow level, `contents: write` on `release` only,
  `cache: false` on both actions in that job.
- Benefit: least privilege on a tag-triggered publish path; zizmor-clean.

Note (not a finding): zizmor's `info[superfluous-actions]` at release.yml:197
prefers `gh release` over `softprops/action-gh-release`. The action is
SHA-pinned and is the common choice; swap it or record why it stays, so the
zizmor output is fully accounted for.

Not findings: the duplicated UI block (`rust.yml` 197-204 vs `frontend.yml`) is a
deliberate choice recorded at `docs/operations/ci.md:172`; `ROUTE_PATHS` in
`cc-web/src/help.rs:147` is test-pinned against `web.rs::routes()` by a device
case.

## deps

### DEP-1 · M · effort S · `ui/packages/frontend/package.json:46`

- Evidence: `shadcn-ui@0.9.5` is declared and imported by nothing (only
  `components.json`'s `$schema` and one comment mention shadcn). The CLI ships as
  `shadcn` (latest 4.21.5); `shadcn-ui` is a separate 0.x package, and
  `ui/pnpm-lock.yaml` marks its resolution `deprecated: Package no longer
  supported`.
- Fix: delete the line and re-resolve the lockfile (CI runs
  `pnpm install --frozen-lockfile`).
- Benefit: one fewer installed package, no dependency on a deprecated wrong
  package name.

### DEP-2 · L · effort M · `Cargo.lock`, `ui/pnpm-lock.yaml`

- Evidence: seven advisory ids across five packages, none caught by any CI step:
  `proc-macro-error` 1.0.4 (RUSTSEC-2024-0370) and `unsafe-libyaml` 0.2.11
  (RUSTSEC-2024-0449), both unmaintained, via `ssd1306` -> `maybe-async-cfg` and
  `cc-parity` -> `serde_yaml_ng`; `qs` 6.15.3 (2 GHSA, via express + body-parser
  in the mock server), `brace-expansion` 5.0.8 (4 GHSA, via minimatch),
  `source-map-js` 1.2.1 (1 GHSA, via @tailwindcss/node + postcss). All dev or
  build only; none is linked into the firmware image. All direct Rust deps are
  at latest stable (crates.io check).
- Fix: one advisory-scan step in the `invariants` job. The two Rust advisories
  are unmaintained with no fixed version, so there is nothing to bump; record
  them as accepted.
- Benefit: advisory count becomes a build break rather than a manual audit.

Renovate is configured (weekly, patch automerge, majors separate); both lockfiles
are committed and integrity-pinned.

## security

None. Verified clean: no secrets in the tree or in history (`git log --all -- .env`
and `-S WIFI_PASS` both empty). Every data route funnels through
`crates/cc-hal-esp32/src/web.rs:795` (`web::register`), `/events` included
(`web_async.rs:449-468`). `/ui*` cannot traverse: `resolve_ui` (`web.rs:1087`)
matches a compile-time `UI_ASSETS` table by exact split-on-`?` path. OTA is bounded
(`MAX_FIRMWARE_BYTES`, `MAX_PART_HEADER_BYTES = 256`, 4 KiB streaming chunks) and
rejects an unknown length. `docs/example_config.json` holds only schema-default
placeholders; the live credential lives in the untracked gitignored `.env` and is
never logged or put in `sys.argv` (`justfile:826`).

## cleanup

### CLN-1 · M · effort S · `docs/operations/ci.md:10`, `justfile:482`, `docs/differences.md:110`

- Evidence: AG-RUST-4 says no document states a test count. `ci.md:10` says "1074
  tests"; this audit's `just check` ran 1318 (1247 in cc-web across 50 suites,
  71 in cc-parity across 4). `justfile:482` says "its 67 `#[test]` functions";
  `scripts/device-test-audit.py` reports 112. `differences.md:110` says 5
  machine-readable `ledger` blocks; `rg -c '^```ledger'
  docs/history/divergences.md` says 6.
- Fix: delete the numbers; describe the gate instead. `justfile:439`'s "67 device
  tests" is past-tense history ("made 67 look green"), so it is the weaker half
  of the finding and can keep the number or drop it.
- Benefit: removes claims that drift silently; the justfile understates the
  invariant `test-audit` protects by 45 tests.

### CLN-2 · L · effort S · `justfile:826,896`, `.gitignore:12`

- Evidence: three stale paths from the `docs/rust-migration` -> `docs/history`
  move (`dc046aab`, recorded under AG-REPO-30). `justfile:826` cites
  `.gitignore:21` for `.env`, which is at line 10. `.gitignore:12` cites
  `docs/rust-migration 05 §2`, now
  `docs/archive/migration/05-tooling-and-workflows.md`. `justfile:896`'s
  `parity-test` header says cc-parity's tests read
  `docs/rust-migration/scenarios/` and `intentional-diffs.md`, neither of which
  exists; `crates/cc-parity/tests/runner.rs` now reads `history/`.
- Fix: point all three at the live paths.
- Benefit: three fewer misdirections for a reader chasing a rule.

### CLN-3 · L · effort M · `CONTRIBUTING.md:54` vs `crates/*/src`

- Evidence: the repo states the policy but nothing enforces it. 31 667 of 81 313
  lines under `crates/*/src` are comments (39 %), about 410 k tokens for an agent
  reading the tree. 65 of 140 `src` files open with 40 or more comment lines
  before their first statement, totalling 4 457 lines; the longest is 260
  (`cc-hal-esp32/src/heater.rs`), then 181 (`cc-firmware/src/slots.rs`) and 120
  (`cc-protocol/src/sensor/tsic306/mod.rs`).
- Measured before filing: the long blocks are mostly load-bearing. They cache
  derivation an agent cannot redo without the deleted C++
  (`git show 9fa8c834:<path>`, AG-REPO-27) and ESP-IDF sources; their
  internally checkable claims are machine-asserted
  (`cc-domain/src/heater.rs:382` asserts `CARRIER_HZ * WINDOW_MS == 1_000`) or
  agree with their ADRs; and comments that merely narrate the next line are 115
  lines, 0.36 %. So this is not a defect today, it is an unbounded surface.
- Fix: make `CONTRIBUTING.md:54` ("explain the why, not restate the code",
  "write for the maintainer who does not know the subsystem") a checkable
  convention rather than prose: state the test a comment must pass (does it
  carry a fact the code, an ADR, or a test does not already?) and require that
  anything over the threshold is either that, or split so the answerable part
  loads first. The bulk trim was **not** part of this fix, and was not done at
  the time: the measured waste did not justify an edit campaign.
- Status: implemented as a rule, applied as a sweep. The convention landed as
  `AG-RUST-13` and its `CONTRIBUTING.md` mirror (commit `256059c4`); that rule
  states which comment classes go whatever their truth value and which
  derivation stays. Application is per crate, not one tree-wide trim:
  re-measuring found 146 of 6 218 comment blocks (2 %) holding no token absent
  from the code, 60 of them on `pub` items where `missing_docs` makes a doc
  deletion a build break, which leaves the private remainder as the whole yield.
- Benefit: comment cost stops scaling silently with each feature added, and the
  un-rot checkable claims stop being caught only when a human notices (which is
  what CLN-1 and CLN-2 are).

## Vendored / generated

- `cc-display/tests/golden/` (48 `.ppm`): needed; deliberately committed,
  regenerated by `just snapshot-display`.
- `cc-display/tests/fmt_oracle.txt` (1.9 MB): needed; the `tools/fmt_oracle.c`
  output, kept so the oracle is re-derivable.
- `target/`, `.embuild/`, `scripts/__pycache__/`: untracked and gitignored, zero
  tracked entries.
- Git history (163 MB `.git`): intentional; AG-REPO-27 requires the deleted C++
  tree stay recoverable (`git show 9fa8c834:<path>`).

## Tool coverage

| Tool | Result |
| --- | --- |
| `just check` | green; 1318 host tests, 112 device tests |
| `zizmor` 1.30.1 (offline) | 3 high + 1 info, all in `release.yml` -> CI-1 |
| OSV API (Cargo + npm) | 7 advisories in 5 packages -> DEP-2 |
| crates.io API | direct Rust deps at latest stable |
| npm registry | frontend current; found DEP-1 |

Skipped, not installed: gitleaks, trivy, osv-scanner, semgrep, shellcheck,
hadolint, ansible-lint, tflint, cargo-audit, cargo-deny, cargo-outdated,
pip-audit, actionlint. Their ground was covered another way: secrets via
`git log --all -S`, advisories via the OSV HTTP API, action linting via zizmor.
Branch protection and required checks were not verified; `gh` has no token here.

## review

`review: kept 7 / dropped 1 / merged 3 / added 1`

The one addition is CLN-3 (comment volume), which the reviewer did not raise
because the brief scoped it to verifying existing findings. It was added after
the same measurement, kept at L because the measured waste is small, and its fix
is a convention that bounds later work rather than one bulk edit.

Corrections the review forced, all applied above: the host test count is 1318,
not 2970 (an arithmetic error in the first pass); CODE-1's magnitudes were wrong
and its severity overstated (the `None` panic needs `decimals >= 50`, and silent
wrap begins at ~3.4e37 for the decimals actually called); CODE-2's claimed prose
divergence was an artefact of the first pass's own `sed`, the two files share one
md5, so it is dropped rather than kept; DEP-2's "3 GHSA" undercounts (7 ids in 3
packages) and `cargo update -p` is a no-op for unmaintained crates. Three items
the reviewer raised were merged rather than added: the `String<24>` silent
truncation into CODE-1, `justfile:896`'s stale path into CLN-2, and
release.yml:197's zizmor info into a note under CI-1.
