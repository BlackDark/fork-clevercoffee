//! Embeds the built web UI into the firmware image.
//!
//! # Why embedded and not a `LittleFS` partition
//!
//! F25's React SPA is built into `ui/packages/frontend/dist`, gzip-compressed
//! by the `rollup-plugin-gzip` already wired into `vite.config.ts`. Measured on
//! 2026-09-30 (Vite 8.2.2): 199,270 B gzipped, of which one file — the JS
//! bundle — is 182,878 B. The uncompressed bundle is ~715 KB, which does NOT
//! fit anywhere: the app slot had 496,192 B free and the `littlefs` partition
//! is 393,216 B.
//!
//! The gzip bundle fits the app slot with room to spare, so it is embedded with
//! `include_bytes!` rather than uploaded to `LittleFS`. That buys three things a
//! filesystem could not:
//!
//!   * `/ui` cannot fail. There is no mount, no `uploadfs` step, no corrupted
//!     partition and no "the UI 404s after a bad upload" failure mode.
//!   * The asset table is `&'static`, so serving costs **zero RAM**: the bytes
//!     live in flash and `httpd` streams them out of a stack buffer.
//!   * The build is hermetic in the sense that matters: the image on the device
//!     is exactly the bundle in the tree. The C++ needs `pio run -t uploadfs`
//!     as a *second* step that can silently drift from the app image.
//!
//! # What this script does NOT decide
//!
//! MIME types and the SPA fallback are decided in `src/web.rs`, not here, so
//! they are covered by this crate's tests. This script only discovers files,
//! copies them into `OUT_DIR` (so `include_bytes!` has a stable, in-tree path)
//! and emits the table.
//!
//! # When the bundle is missing
//!
//! The build FAILS. A firmware image with no web UI is a silent regression that
//! looks exactly like "the browser is broken", and this crate's whole point is
//! that `/ui` is a real answer. Build it first:
//!
//! ```sh
//! pnpm --filter @clevercoffee/frontend build
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Where `vite build` puts the bundle, relative to this crate.
const DIST_RELATIVE: &str = "../../ui/packages/frontend/dist";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join(DIST_RELATIVE);
    println!("cargo:rerun-if-changed={}", dist.display());

    let dist = match dist.canonicalize() {
        Ok(resolved) => {
            println!("cargo:rerun-if-changed={}", resolved.display());
            resolved
        }
        Err(_) => missing_bundle(&dist),
    };

    let mut assets: BTreeMap<String, Asset> = BTreeMap::new();
    collect(&dist, &dist, &mut assets);

    if assets.is_empty() {
        missing_bundle(&dist);
    }

    let index = assets.contains_key("/index.html");
    if !index {
        fail(&format!(
            "{} has no index.html (nor index.html.gz), so there is no SPA \
             entry point to serve. A Vite build always emits one; this usually \
             means the build was interrupted.",
            dist.display()
        ));
    }

    let out =
        PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_else(|| fail("OUT_DIR is unset")));
    let generated = render(&dist, &assets, &out);
    let file = out.join("ui_bundle.rs");
    std::fs::write(&file, generated)
        .unwrap_or_else(|why| fail(&format!("could not write {}: {why}", file.display())));
}

/// One embeddable file.
struct Asset {
    /// The bytes as they are stored in flash.
    bytes: Vec<u8>,
    /// Whether `bytes` are gzip-compressed and must be sent with
    /// `Content-Encoding: gzip`.
    gzip: bool,
    /// Flattened name inside `OUT_DIR`, so the generated `include_bytes!`
    /// points at a path with no directories to resolve.
    flat_name: String,
}

/// Walk `dir` recursively, recording every file as a URL path.
///
/// `BTreeMap` iteration order is sorted, so the generated table — and therefore
/// the image — does not depend on the order the filesystem happens to hand back
/// directory entries.
fn collect(root: &Path, dir: &Path, into: &mut BTreeMap<String, Asset>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|why| fail(&format!("could not read {}: {why}", dir.display())));

    for entry in entries {
        let entry =
            entry.unwrap_or_else(|why| fail(&format!("could not read {}: {why}", dir.display())));
        let path = entry.path();

        if path.is_dir() {
            collect(root, &path, into);
            continue;
        }

        // `dist/index.html` -> "/index.html".
        let relative = path
            .strip_prefix(root)
            .unwrap_or_else(|_| {
                fail(&format!(
                    "{} is not under {}",
                    path.display(),
                    root.display()
                ))
            })
            .to_string_lossy()
            .replace('\\', "/");
        let (stem, gzip) = match relative.strip_suffix(".gz") {
            Some(stem) => (stem.to_owned(), true),
            None => (relative, false),
        };
        let flat_name = stem.replace('/', "__");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|why| fail(&format!("could not read {}: {why}", path.display())));

        into.insert(
            format!("/{stem}"),
            Asset {
                bytes,
                gzip,
                flat_name,
            },
        );
    }
}

/// Emit the asset table.
fn render(dist: &Path, assets: &BTreeMap<String, Asset>, out_dir: &Path) -> String {
    let total: usize = assets.values().map(|asset| asset.bytes.len()).sum();
    let mut out = String::new();

    // Pass one: stage every file next to the generated module. `include_bytes!`
    // reads them at compile time, so they must exist before the module that
    // names them is even written.
    let mut literals: Vec<(String, String)> = Vec::with_capacity(assets.len());
    for (path, asset) in assets {
        let staged = out_dir.join(&asset.flat_name);
        std::fs::write(&staged, &asset.bytes)
            .unwrap_or_else(|why| fail(&format!("could not stage {}: {why}", staged.display())));
        let compressed = if asset.gzip { "true" } else { "false" };
        // The staged path goes into the generated source as a string literal.
        // Quoted by hand rather than with `{:?}` because `clippy::pedantic`
        // denies `unnecessary_debug_formatting`, and OUT_DIR is a plain
        // filesystem path on every platform this builds for -- it cannot
        // contain a quote or a backslash.
        let staged_literal = staged.to_string_lossy().into_owned();
        literals.push((
            path.clone(),
            format!(
                "UiAsset {{ path: {path:?}, gzip: {compressed}, \
                 bytes: as_slice(include_bytes!(\"{staged_literal}\")) }}"
            ),
        ));
    }

    writeln!(
        out,
        r"// @generated by crates/cc-hal-esp32/build.rs from {}.
// Do not edit; run `pnpm --filter @clevercoffee/frontend build` instead.
//
// One embedded file from the built web UI. `path` is the URL under `/ui` that
// serves it, `gzip` says whether `bytes` are compressed (and so need
// `Content-Encoding: gzip`), and `bytes` are the flash copy the handler streams.
//
// MIME types are NOT here: they are derived from `path` in `web.rs`, where they
// are testable. Deciding them in a build script would put the one thing that
// silently breaks a SPA -- a `.js` served as `text/plain` -- outside the test
// suite.",
        dist.display()
    )
    .unwrap_or_else(|why| fail(&format!("could not format the generated module: {why}")));

    writeln!(
        out,
        r"
/// One embedded file from the built web UI, as it is stored in flash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiAsset {{
    /// The URL under `/ui` that serves this file, e.g. `/index.html`. Never
    /// contains `..`: there is no filesystem, so traversal is unrepresentable.
    pub path: &'static str,
    /// Whether `bytes` are gzip-compressed. When true the handler MUST send
    /// `Content-Encoding: gzip` or the client is handed compressed bytes and
    /// renders a blank page.
    pub gzip: bool,
    /// The file, verbatim.
    pub bytes: &'static [u8],
}}

/// `include_bytes!` yields `&[u8; N]`, and this toolchain cannot slice an
/// array in a `const` initialiser. Coercing through a `const fn` is the
/// supported way to get a slice there, and it copies nothing.
const fn as_slice(bytes: &'static [u8]) -> &'static [u8] {{
    bytes
}}
"
    )
    .unwrap_or_else(|why| fail(&format!("could not format the generated module: {why}")));

    // The SPA entry point, emitted as its own `const` rather than indexed out
    // of `UI_ASSETS`, because indexing a slice is not a `const` operation on
    // this toolchain either.
    let index_literal = literals
        .iter()
        .find(|(path, _)| path == "/index.html")
        .map_or_else(String::new, |(_, literal)| literal.clone());
    writeln!(
        out,
        r"
/// The SPA entry point. Served for `/ui`, `/ui/` and every extensionless path
/// under `/ui`, which is what makes a client-side route survive a page reload.
pub const UI_INDEX: &UiAsset = &{index_literal};

/// Every embedded file, sorted by path so the image is reproducible.
pub const UI_ASSETS: &[UiAsset] = &["
    )
    .unwrap_or_else(|why| fail(&format!("could not format the generated module: {why}")));

    for (path, literal) in &literals {
        // The table names the const rather than repeating the literal, so the
        // entry point's bytes are described exactly once.
        if path == "/index.html" {
            writeln!(out, "    *UI_INDEX,").unwrap_or_else(|why| {
                fail(&format!("could not format the generated module: {why}"))
            });
            continue;
        }
        writeln!(out, "    {literal},")
            .unwrap_or_else(|why| fail(&format!("could not format the generated module: {why}")));
    }

    writeln!(out, "];")
        .unwrap_or_else(|why| fail(&format!("could not format the generated module: {why}")));

    writeln!(
        out,
        r#"
/// How many bytes the web UI adds to the flash image.
pub const UI_TOTAL_BYTES: usize = {total};

/// How many files are embedded. The boot log uses both numbers, because "the UI
/// is served from flash" is only checkable against them.
pub const UI_FILE_COUNT: usize = {};"#,
        assets.len(),
        total = with_separators(total),
    )
    .unwrap_or_else(|why| fail(&format!("could not format the generated module: {why}")));

    out
}

/// Render a byte count with `_` separators.
///
/// The generated module is linted exactly like hand-written source, and
/// `clippy::unreadable_literal` rejects a bare six-digit constant. Emitting
/// `199_270` also means the number in the boot log can be read at a glance.
fn with_separators(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            out.push('_');
        }
        out.push(digit);
    }
    out
}

/// Report a build problem the way a human needs to read it, and stop.
fn missing_bundle(dist: &Path) -> ! {
    fail(&format!(
        "the web UI bundle is missing: {} does not exist.\n\
         \n\
         Build it first, from the repository root:\n\
         \n    pnpm --filter @clevercoffee/frontend build\n\
         \n\
         (run `pnpm install` in `ui/` if that fails). This is not optional: \
         `cargo build` would otherwise succeed and ship a firmware whose `/ui` \
         is a placeholder string.",
        dist.display()
    ))
}

/// Print `message` as a cargo build error and exit.
fn fail(message: &str) -> ! {
    panic!("\n\ncc-hal-esp32 build script failed:\n  {message}\n\n");
}
