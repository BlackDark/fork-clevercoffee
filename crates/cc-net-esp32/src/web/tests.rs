#![allow(
    clippy::wildcard_imports,
    reason = "the tests exercise the parent's private items, and the `cfg(test)` \
              exception does not cover the device-tests build"
)]

use super::events::SSE_MAILBOX_DEPTH;
use super::spa::{UI_ASSETS, UI_INDEX};
use super::*;

#[cfg_attr(test, test)]
pub fn every_api_route_is_registered() {
    // 20 /api/* routes, `/api/config/download`, the four OTA routes, and
    // `/`, `/ui` and `/events`.
    //
    // This list is the parity contract with `ui/packages/frontend/src/lib/
    // routes.ts`, the frontend's own `API_ROUTES` table. A route the UI can
    // call and this server does not register is a 404 in the browser and a
    // blank page in the UI, so the omission is the bug this test exists to
    // prevent.
    let routes = routes();
    for expected in [
        "/api/status",
        "/api/health",
        "/api/temperatures",
        "/api/history",
        "/api/nvs-debug",
        "/api/parameter-help",
        "/api/config",
        "/api/config/download",
        "/api/parameters",
        "/api/setpoint",
        "/api/steam",
        "/api/pid",
        "/api/backflush",
        "/api/sleep",
        "/api/wake",
        "/api/scale/tare",
        "/api/scale/calibration",
        "/api/maintenance/reset-backflush-counter",
        "/api/wifi-reset",
        "/api/factory-reset",
        "/api/restart",
        "/api/ota/status",
        "/api/ota/firmware",
        "/api/ota/filesystem",
        "/api/ota/url",
    ] {
        assert!(
            routes.iter().any(|(path, _)| *path == expected),
            "{expected} is missing"
        );
    }
}

/// The frontend's `API_ROUTES` table, verbatim from
/// `ui/packages/frontend/src/lib/routes.ts`.
///
/// Transcribed here rather than parsed at build time so that adding a route
/// to one side and forgetting the other is a **test failure** and not a 404
/// somebody finds by clicking. `getApiRoute` prefixes `/api`, so each entry
/// is compared with that prefix applied.
#[cfg_attr(test, test)]
pub fn every_route_the_frontend_calls_is_registered() {
    const FRONTEND: &[&str] = &[
        "/api/steam",
        "/api/pid",
        "/api/backflush",
        "/api/setpoint",
        "/api/wake",
        "/api/sleep",
        "/api/config",
        "/api/config/download",
        "/api/parameters",
        "/api/parameter-help",
        "/api/status",
        "/api/health",
        "/api/temperatures",
        "/api/history",
        "/api/scale/tare",
        "/api/scale/calibration",
        "/api/ota/status",
        "/api/ota/firmware",
        "/api/ota/filesystem",
        "/api/ota/url",
        "/api/restart",
        "/api/factory-reset",
        "/api/wifi-reset",
        "/api/nvs-debug",
        "/api/maintenance/reset-backflush-counter",
    ];
    let routes = routes();
    for path in FRONTEND {
        assert!(
            routes.iter().any(|(registered, _)| registered == path),
            "the UI calls {path} but no handler is registered for it"
        );
    }
}

// ==================================================== the setpoint route

/// Every `/api/` route the table advertises is a path the JSON `404`
/// claims.
///
/// Finding 3.8. The JSON `404` decides JSON-vs-text with
/// `path.starts_with("/api/")`. If a route were ever advertised as `/api` —
/// no trailing slash — the 404 would not cover it and a client would get
/// plain text from a route the boot log claims exists. That is a whole-table
/// property rather than a property of one entry, which is why it is asserted
/// over the table and not per route.
#[cfg_attr(test, test)]
pub fn every_advertised_api_route_is_covered_by_the_json_404() {
    for (path, _) in routes() {
        // A wildcard answers the requests it matches, so it never reaches
        // the 404 and the rule cannot apply to it. The two registered
        // wildcards are checked for their own method in
        // `wildcard_matching_leaves_the_api_routes_exact`.
        if path.ends_with('*') {
            continue;
        }
        if path.starts_with("/api") {
            assert!(
                cc_net::web::help::wants_json_not_found(path),
                "{path} is advertised but the JSON 404 does not cover it"
            );
        }
    }
}

/// `/api/parameter-help` is registered for `GET` and nothing else.
///
/// Finding 3.4. The route existed and was wrong in the worst way — it
/// answered `200` with an error object — so the registration was never in
/// doubt and no route test could have caught it. What is worth pinning is
/// the method list, and a `POST` to it falls through to the JSON `404`
/// rather than being answered.
#[cfg_attr(test, test)]
pub fn every_registered_route_is_in_the_raw_handlers_own_table() {
    // `cc_net::web::help::ROUTE_PATHS` is a compile-time list, because the `extern
    // "C"` handler that consults it cannot capture one. `routes()` is the
    // runtime list the server actually registers, and this is what stops the
    // two drifting: a path in `routes()` and missing from `ROUTE_PATHS` would
    // answer a `404` where a `405` is correct, and the only way that happens
    // is someone adding a route here and not there.
    for (path, _) in routes() {
        assert!(
            cc_net::web::help::ROUTE_PATHS.contains(&path),
            "{path} is registered but the raw 404/405 handler does not know it"
        );
    }
    for path in cc_net::web::help::ROUTE_PATHS {
        assert!(
            routes().iter().any(|(registered, _)| registered == path),
            "{path} is in the raw handler's table but no handler is registered"
        );
    }
}

#[cfg_attr(test, test)]
pub fn the_parameter_help_route_is_a_get_and_nothing_else() {
    let entries: Vec<&'static str> = routes()
        .iter()
        .filter(|(path, _)| *path == "/api/parameter-help")
        .map(|(path, _)| *path)
        .collect();
    // Exactly one entry — so no second method is advertised for it, which is
    // what makes the assertion below mean "and nothing else".
    assert_eq!(entries, vec!["/api/parameter-help"]);
    assert!(routes().contains(&("/api/parameter-help", Method::Get)));
}

// ==================================================== the toggle routes

// ==================================================== OTA (deferred)

// ============================================ POST /api/parameters honesty

#[cfg_attr(test, test)]
pub fn the_route_table_fits_the_servers_handler_budget() {
    // If this ever grows past MAX_URI_HANDLERS the server will fail to start
    // with ESP_ERR_HTTPD_HANDLERS_FULL, at boot, which is a bad place to find
    // out.
    assert!(routes().len() <= MAX_URI_HANDLERS);
}

// ================================================ POST /api/config/upload

#[cfg_attr(test, test)]
pub fn the_config_upload_route_is_registered() {
    // `ui/packages/frontend/src/pages/SystemPage.tsx:182` posts here. It was
    // a live button with no route behind it, so every operator who clicked
    // "Upload configuration" got a 404.
    assert!(
        routes().contains(&("/api/config/upload", Method::Post)),
        "the upload route must be in the table"
    );
}

// ============================================================== CORS

#[cfg_attr(test, test)]
pub fn the_advertised_options_handler_is_a_wildcard_over_the_api() {
    // It used to be advertised as `("/api/status", Method::Options)` with no
    // `fn_handler` behind it anywhere, so a preflight 404'd while the boot
    // log claimed the route existed. See `register_preflight`.
    let options: Vec<&str> = routes()
        .iter()
        .filter(|(_, method)| *method == Method::Options)
        .map(|(uri, _)| *uri)
        .collect();
    assert_eq!(options, vec!["/api*"], "one real preflight route");
}

// ==================================================== POST /api/parameters

#[cfg_attr(test, test)]
pub fn the_parameter_route_is_registered_for_both_methods() {
    // Registered once per method rather than as `HTTP_ANY`; two
    // registrations is the closest `esp-idf-svc` can get, because its
    // `Method` has no "any" (embedded-svc 0.29 `http.rs:17-52`).
    let routes = routes();
    assert!(routes.contains(&("/api/parameters", Method::Get)));
    assert!(routes.contains(&("/api/parameters", Method::Post)));
}

#[cfg_attr(test, test)]
pub fn the_redirect_and_the_ui_are_routes() {
    let routes = routes();
    assert!(
        routes.contains(&("/", Method::Get)),
        "the / -> /ui/ redirect"
    );
    assert!(
        routes.contains(&("/ui*", Method::Get)),
        "the UI is one wildcard route: the shell, its assets and its \
         client-side routes all reach the same handler"
    );
}

#[cfg_attr(test, test)]
pub fn the_sse_stream_is_a_route() {
    assert!(routes().contains(&("/events", Method::Get)));
}

// ---- SSE framing ---------------------------------------------------

#[cfg_attr(test, test)]
pub fn an_sse_frame_ends_with_a_blank_line() {
    // The WHATWG parser dispatches on the blank line. Without it the
    // browser buffers the frame and delivers nothing, forever, with no
    // error — the single most common way a hand-rolled SSE endpoint is
    // "broken".
    let frame = Sse::frame("new_temps", "{\"currentTemp\":90.0}");
    assert!(frame.starts_with("event: new_temps\n"), "{frame}");
    assert!(frame.contains("data: {"), "{frame}");
    assert!(frame.ends_with("\n\n"), "{frame}");
}

#[cfg_attr(test, test)]
pub fn a_keepalive_is_a_comment_and_carries_no_event() {
    let frame = Sse::keepalive();
    assert!(frame.starts_with(':'), "{frame}");
    assert!(frame.ends_with("\n\n"), "{frame}");
    assert!(!frame.contains("event:"), "{frame}");
}

#[cfg_attr(test, test)]
pub fn the_default_sse_mode_is_the_chunked_one() {
    // Chunked framing is the only mode left: it is the one that keeps
    // `send_wait_timeout` and the only one that can be written from a
    // detached request.
    assert_eq!(SseMode::default(), SseMode::Chunked);
    assert_eq!(Sse::new(SseMode::default()).mode(), SseMode::Chunked);
}

#[cfg_attr(test, test)]
pub fn the_client_cap_leaves_sockets_for_the_api() {
    // The bound that keeps one browser tab from starving `/api/*`. ESP-IDF's
    // httpd has a fixed session pool and an SSE client holds its session for
    // as long as it is connected, so the cap is a socket budget, not a
    // politeness limit. ADR-0002's reproducer fires 6-10 parallel API
    // requests; this asserts at least three sockets stay available.
    const { assert!(SSE_MAX_CLIENTS < MAX_OPEN_SOCKETS) };
    const { assert!(MAX_OPEN_SOCKETS - SSE_MAX_CLIENTS >= 3) };
}

#[cfg_attr(test, test)]
pub fn a_fresh_stream_has_no_clients_and_no_counters() {
    let sse = Sse::new(SseMode::Chunked);
    assert_eq!(sse.connected_now(), 0);
    assert_eq!(sse.clients(), 0);
    assert_eq!(sse.rejected(), 0);
    assert_eq!(sse.sent(), 0);
    assert_eq!(sse.dropped(), 0);
    assert_eq!(sse.pushed(), 0);
    assert_eq!(sse.dropped_frames(), 0);
}

/// How many frames the mailbox is holding.
///
/// A `usize` rather than the `Result`: the on-target build has no `std`, so
/// `MutexGuard` is not `PartialEq` there and comparing the `Result` directly
/// would compile for the host and fail for the device. That asymmetry is
/// exactly what `just lint-esp32` exists to catch, and this is the shape it
/// caught.
fn mailbox_len(sse: &Sse) -> usize {
    sse.mailbox.lock().map_or(0, |m| m.len())
}

#[cfg_attr(test, test)]
pub fn a_push_counts_and_a_full_mailbox_counts_the_drop() {
    // `Sse::broadcast` must not block its caller: the caller is the control
    // task. A mailbox that filled is counted, not waited on.
    let sse = Sse::new(SseMode::Chunked);
    for i in 0..SSE_MAILBOX_DEPTH {
        sse.broadcast(Sse::frame("weight", &format!("{{\"n\":{i}}}")));
    }
    sse.note_push_attempt();
    assert_eq!(sse.pushed(), 1);
    assert_eq!(sse.dropped_frames(), 0);
    assert_eq!(mailbox_len(&sse), SSE_MAILBOX_DEPTH);

    // One more than the mailbox holds: the oldest is discarded and counted.
    sse.broadcast(Sse::frame("weight", "{\"n\":99}"));
    assert_eq!(sse.dropped_frames(), 1);
    assert_eq!(mailbox_len(&sse), SSE_MAILBOX_DEPTH);
}

#[cfg_attr(test, test)]
pub fn the_broadcaster_cadence_fits_under_the_poll_interval() {
    // The broadcaster can only be as responsive as its poll, and the poll
    // must not be the thing that makes the loop a busy-wait.
    const { assert!(SSE_POLL_MS < SSE_EVENT_INTERVAL_MS) };
    const { assert!(SSE_POLL_MS <= 100) };
}

#[cfg_attr(test, test)]
pub fn the_keepalive_interval_is_under_the_nat_floor() {
    // 15 s against the common 30 s NAT UDP timeout; a silent stream is
    // dropped by the router long before the browser notices.
    const { assert!(SSE_KEEPALIVE_MS <= 15_000) };
    const { assert!(SSE_KEEPALIVE_MS > SSE_EVENT_INTERVAL_MS) };
}

// ---- the heap guard -------------------------------------------------

#[cfg_attr(test, test)]
pub fn the_large_response_limit_is_above_19kb() {
    // ADR-0002 measured `/api/parameters?filter=all` at 19 KB. A limit
    // below that would 503 the endpoint that serves it.
    const { assert!(MAX_JSON_BYTES > 19 * 1024) };
    const {
        assert!(MAX_JSON_BYTES < 64 * 1024, "and not so large it is the OOM");
    };
}

#[cfg_attr(test, test)]
pub fn the_shared_telemetry_round_trips() {
    let shared = Shared::new();
    let t = Telemetry {
        temperature_c: 88.0,
        machine_state: 33,
        ..Telemetry::default()
    };
    shared.publish(t.clone());
    assert_eq!(shared.snapshot(), t);
}

#[cfg_attr(test, test)]
pub fn a_read_does_not_consume_the_snapshot() {
    // `Snapshot::get` was `Cell::take`, so reading it left `T::default()`
    // behind and the *second* read of a live value returned zeroes. Reading
    // twice and requiring the two to be equal is the only assertion that
    // fails on a destructive read -- one that only checks "the read returns
    // what was set" passes either way.
    let shared = Shared::new();
    let published = Telemetry {
        machine_state: 33,
        temperature_c: 88.0,
        pid_enabled: true,
        ..Telemetry::default()
    };
    shared.publish(published.clone());
    let first = shared.snapshot();
    let second = shared.snapshot();
    assert_eq!(first, second, "the first read consumed the snapshot");
    assert_eq!(second, published, "the second read saw an emptied slot");
}

#[cfg_attr(test, test)]
pub fn the_radio_fields_survive_a_machine_publish() {
    // The two-publisher contract, and the reason the control task publishes
    // the radio *after* the machine telemetry. `publish` replaces the whole
    // slot, so a machine publish that ran second would erase the radio's
    // four fields and `/api/status` would go back to `wifiAssociated: false`
    // — which is the bug this ordering exists to prevent.
    let shared = Shared::new();
    // The radio publishes first — a read-modify-write, exactly as
    // `network::publish_radio` does it.
    let mut radio = shared.snapshot();
    radio.wifi_associated = true;
    radio.signal = 4;
    radio.ip = heapless::String::<15>::try_from("10.0.0.7").ok();
    shared.publish(radio);
    // …and the control task's publish is the one that must not run second.
    let before = shared.snapshot();
    shared.publish(telemetry_with_radio_untouched(&before));
    let after = shared.snapshot();
    assert!(
        after.wifi_associated,
        "the machine publish erased the radio's association flag"
    );
    assert_eq!(after.signal, 4);
    assert_eq!(after.ip, before.ip);
}

/// What a control task's `publish` looks like when it is careful: it carries
/// the machine fields and *reuses* the radio's, because the two are separate
/// publishers and a whole-slot replace is the only thing `Shared` offers.
fn telemetry_with_radio_untouched(previous: &Telemetry) -> Telemetry {
    Telemetry {
        machine_state: 20,
        temperature_c: 93.5,
        signal: previous.signal,
        wifi_associated: previous.wifi_associated,
        wifi_offline: previous.wifi_offline,
        ip: previous.ip.clone(),
        ..Telemetry::default()
    }
}

#[cfg_attr(test, test)]
pub fn a_reboot_request_is_one_shot() {
    let shared = Shared::new();
    shared.reboot_requested.store(true, Ordering::SeqCst);
    assert!(shared.take_reboot_request());
    assert!(
        !shared.take_reboot_request(),
        "a second take must not reboot again"
    );
}

#[cfg_attr(test, test)]
pub fn the_ui_shell_and_its_assets_are_embedded_and_gzipped() {
    // The size assertion is the one that matters: the bundle is embedded rather
    // than mounted, and it only fits because it is compressed. In a `const`
    // block so a build that outgrows the app partition fails HERE rather
    // than as an OTA that will not fit on a machine that is already wired
    // up.
    const _: () = assert!(
        UI_TOTAL_BYTES < 400 * 1024,
        "the embedded web UI no longer fits the app partition"
    );
    const _: () = assert!(UI_FILE_COUNT >= 3, "index.html, one JS and one CSS bundle");

    // A JS file served as the wrong type is a blank page behind a 200, so
    // the embedded assets are checked for the pairing that causes it.
    for asset in UI_ASSETS {
        assert!(
            mime_for(asset.path) != "text/plain",
            "{} would be served as text/plain",
            asset.path
        );
        assert!(!asset.bytes.is_empty(), "{} is empty", asset.path);
    }
    let shell = resolve_ui("/ui");
    assert_eq!(shell, UiTarget::File(UI_INDEX));
    assert_eq!(UI_INDEX.path, "/index.html");
    assert!(UI_INDEX.gzip, "the shell must be gzip-encoded");
}

#[cfg_attr(test, test)]
pub fn a_client_side_route_serves_the_shell_but_a_missing_asset_does_not() {
    // A reload on a deep link must boot the app, not 404.
    assert_eq!(resolve_ui("/ui/config/behavior"), UiTarget::SpaRoute);
    assert_eq!(resolve_ui("/ui/system"), UiTarget::SpaRoute);
    // A missing script must be a 404. Serving the shell here is the failure
    // that produces "200, HTML where JS was expected, blank screen".
    assert_eq!(resolve_ui("/ui/assets/missing.js"), UiTarget::Missing);
    // A query string is not part of the extension test.
    assert_eq!(resolve_ui("/ui/system?x=1.5"), UiTarget::SpaRoute);
    // `/ui*` also matches `/uixyz`, which is not a child of `/ui`.
    assert_eq!(resolve_ui("/uixyz"), UiTarget::Missing);
    // And the real asset is found.
    let js = UI_ASSETS
        .iter()
        .find(|a| mime_for(a.path) == "application/javascript")
        .map_or_else(|| panic!("no embedded JS"), |a| a.path);
    assert_eq!(
        resolve_ui(&alloc::format!("/ui{js}")),
        UiTarget::File(
            UI_ASSETS
                .iter()
                .find(|a| a.path == js)
                .unwrap_or(&UI_ASSETS[0])
        )
    );
}

#[cfg_attr(test, test)]
pub fn wildcard_matching_leaves_the_api_routes_exact() {
    // `/ui*` only works because the server matches wildcards, and that
    // switch is global. `httpd_uri_match_wildcard` requires an exact length
    // match for a template with no `*`/`?` (httpd_uri.c:57-60), so this
    // asserts the invariant the whole `/api/*` surface depends on.
    // Exactly two wildcards are registered and both are deliberate: the UI
    // shell (`/ui*`), and the `/api*` preflight, which answers `Options` and
    // therefore cannot serve the wrong body to a `GET`. Anything else with a
    // `*` would match a real route by prefix.
    let mut wildcards: Vec<&str> = routes()
        .iter()
        .filter(|(uri, _)| uri.ends_with('*'))
        .map(|(uri, _)| *uri)
        .collect();
    wildcards.sort_unstable();
    assert_eq!(wildcards, ["/api*", "/ui*"], "wildcard routes changed");
    for (uri, method) in routes() {
        if !uri.ends_with('*') {
            continue;
        }
        // Each wildcard is paired with the one method it may answer, so
        // neither can start serving a body to a request it was not meant for.
        let allowed = match uri {
            "/ui*" => Method::Get,
            "/api*" => Method::Options,
            other => panic!("unexpected wildcard route {other}"),
        };
        assert_eq!(method, allowed, "{uri} answers the wrong method");
    }
    assert!(configuration().uri_match_wildcard);
}
