//! The web UI embedded in flash by `build.rs`: asset lookup and chunked serving.

#![allow(
    clippy::wildcard_imports,
    reason = "split out of `web` for size: it shares the parent's imports and \
              private helpers"
)]

use super::*;

// The built React SPA, embedded by `build.rs`. The table is `&'static` and the
// bytes live in flash, so serving the UI costs no RAM beyond the httpd send
// buffer — which is the reason the bundle is embedded rather than mounted: see
// `build.rs` for the size arithmetic that decided it.
mod bundle {
    include!(concat!(env!("OUT_DIR"), "/ui_bundle.rs"));
}

use bundle::UiAsset;
pub(super) use bundle::{UI_ASSETS, UI_FILE_COUNT, UI_INDEX, UI_TOTAL_BYTES};

/// The chunk size the embedded-file writer streams at.
///
/// The same 512 B [`respond`] uses, and for the same reason: the httpd task's
/// stack is 8 KB and the send path buffers, so a large chunk is a large
/// allocation on a machine whose heap is the scarce resource. At 512 B the
/// 183 KB bundle is 357 `httpd_resp_send_chunk` calls, which is nothing.
pub(super) const UI_CHUNK_BYTES: usize = 512;

/// What `GET /ui...` resolved to.
///
/// The three cases are distinct because conflating them is what makes a broken
/// SPA hard to diagnose: serving `index.html` for a missing `.js` produces a
/// `200`, a correct-looking HTML body, and a browser console full of MIME
/// errors — which looks like a broken app rather than a missing file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTarget {
    /// Serve this embedded file.
    File(&'static UiAsset),
    /// No such file, but the path has no extension: it is a client-side route,
    /// so serve the SPA shell and let the router sort it out.
    SpaRoute,
    /// No such file, and it names an extension: the caller sends a 404.
    Missing,
}

/// Resolve a request URI to an embedded asset.
///
/// `/ui` and `/ui/` are the shell. Everything else is looked up under `/ui`, and
/// an unknown extensionless path falls back to the shell — that is what makes a
/// deep link like `/ui/config/behavior` survive a page reload.
///
/// The lookup is a linear scan over a `&'static` table comparing borrowed
/// strings, so a request allocates nothing. Path traversal is unrepresentable
/// rather than filtered: there is no filesystem, and a path is either a key in
/// the table or it is nothing.
///
/// The query string is stripped before the extension test. Testing the URL
/// *including* the query would treat an SPA route with `?x=1.5` as an asset
/// request and 404 it.
#[must_use]
pub fn resolve_ui(uri: &str) -> UiTarget {
    let path = uri.split_once('?').map_or(uri, |(path, _)| path);
    let Some(rest) = path.strip_prefix("/ui") else {
        return UiTarget::Missing;
    };
    // `/ui*` is the registered template, so `/uixyz` reaches this handler too
    // and must not be treated as a child of `/ui`.
    if !rest.is_empty() && !rest.starts_with('/') {
        return UiTarget::Missing;
    }
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        return UiTarget::File(UI_INDEX);
    }
    if let Some(asset) = UI_ASSETS
        .iter()
        .find(|asset| asset.path.strip_prefix('/') == Some(rest))
    {
        return UiTarget::File(asset);
    }
    if rest.contains('.') {
        UiTarget::Missing
    } else {
        UiTarget::SpaRoute
    }
}

/// Write one embedded file, with its MIME type and encoding.
///
/// `esp-idf-svc` 0.53.0 cannot send a `Content-Length`: the `httpd_resp_set_len`
/// call is commented out in `server.rs:1056-1058`, so every response it writes
/// is chunked. The terminating zero-length chunk below is therefore not
/// optional — without it the browser waits for a body that never ends. This is
/// the same reason [`respond`] ends with `conn.write(&[])`.
///
/// Compressed assets are sent with `Content-Encoding: gzip` unconditionally
/// rather than negotiated. Only the gzip form is embedded, so there is no
/// identity variant to fall back to, and every browser advertises gzip. The
/// ceiling: a client that cannot decode gzip cannot load the UI, and `curl`
/// needs `--compressed`.
fn write_ui_file(
    conn: &mut EspHttpConnection<'_>,
    asset: &'static UiAsset,
) -> Result<(), EspError> {
    let mime = mime_for(asset.path);
    // Cache policy: the shell must not be cached or a rebuilt UI is unreachable
    // until the entry expires, while Vite's content-hashed asset names make the
    // rest safe to pin.
    let cache = if asset.path == UI_INDEX.path {
        "no-cache, no-store, must-revalidate"
    } else {
        "max-age=604800"
    };

    if asset.gzip {
        conn.initiate_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", mime),
                ("Content-Encoding", "gzip"),
                ("Cache-Control", cache),
            ],
        )?;
    } else {
        conn.initiate_response(
            200,
            Some("OK"),
            &[("Content-Type", mime), ("Cache-Control", cache)],
        )?;
    }

    for chunk in asset.bytes.chunks(UI_CHUNK_BYTES) {
        let _ = conn.write(chunk);
    }
    let _ = conn.write(&[]);
    Ok(())
}

/// `GET /ui` and everything under it.
///
/// Registered once, on the template `/ui*`. ESP-IDF's
/// `httpd_uri_match_wildcard` treats a trailing `*` as
/// "prefix", and a template *without* `*` or `?` still requires an exact length
/// match — so enabling [`Configuration::uri_match_wildcard`] for this one route
/// leaves all 23 exact `/api/*` handlers exactly as strict as they were.
pub(super) fn serve_ui(mut req: Request<&mut EspHttpConnection<'_>>) -> Result<(), EspError> {
    let target = resolve_ui(req.uri());
    let conn = req.connection();
    match target {
        UiTarget::File(asset) => write_ui_file(conn, asset),
        UiTarget::SpaRoute => write_ui_file(conn, UI_INDEX),
        UiTarget::Missing => {
            // "File not found", deliberately not the shell: a missing asset must
            // be visible as a 404 rather than smuggled in as HTML the browser
            // then fails to parse as JavaScript.
            conn.initiate_response(404, Some("Not Found"), &[("Content-Type", "text/plain")])?;
            conn.write_all(b"File not found")
        }
    }
}
