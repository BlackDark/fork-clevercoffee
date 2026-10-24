//! Every endpoint over real HTTP: the picoserve service on a tokio socket, a fake
//! backend wrapping the real `Machine`.

#![allow(clippy::needless_pass_by_value, clippy::unused_async_trait_impl)]

use std::cell::RefCell;
use std::time::Duration;

use cc_api::Live;
use cc_core::history::History;
use cc_core::{Command, Config, Inputs, Machine, Reply, Status};
use cc_web::{Api, Asset, Backend, OtaError, OtaStatus, StreamSlots};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

struct Fake {
    cfg: RefCell<Config>,
    machine: RefCell<Machine>,
    history: RefCell<History>,
    ota: RefCell<Vec<u8>>,
    ota_done: RefCell<bool>,
    restarted: RefCell<bool>,
}

thread_local! {
    /// Free heap the fake reports; tests run on one thread (current_thread runtime).
    static FAKE_HEAP: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

impl Fake {
    fn new(cfg: Config) -> Self {
        let mut m = Machine::new(&cfg, 0, 0);
        let mut c = cfg.clone();
        m.tick(
            &mut c,
            &Inputs {
                now_ms: 10,
                temperature: Some(93.0),
                water_tank: true,
                ..Inputs::default()
            },
        );
        Self {
            cfg: RefCell::new(c),
            machine: RefCell::new(m),
            history: RefCell::new(History::new()),
            ota: RefCell::new(Vec::new()),
            ota_done: RefCell::new(false),
            restarted: RefCell::new(false),
        }
    }
}

impl Backend for Fake {
    fn read<R>(&self, f: impl FnOnce(&Config, &Status, &Live) -> R) -> R {
        let cfg = self.cfg.borrow();
        let st = self.machine.borrow().status(&cfg);
        f(&cfg, &st, &Live::default())
    }
    fn history<R>(&self, f: impl FnOnce(&History) -> R) -> R {
        f(&self.history.borrow())
    }
    fn command(&self, cmd: Command) -> Reply {
        let mut ev = heapless::Vec::new();
        self.machine
            .borrow_mut()
            .command(cmd, &mut self.cfg.borrow_mut(), &mut ev)
    }
    fn update_config(&self, f: &mut dyn FnMut(&mut Config) -> bool) {
        f(&mut self.cfg.borrow_mut());
    }
    fn restart(&self) {
        *self.restarted.borrow_mut() = true;
    }
    fn factory_reset(&self) {}
    fn wifi_reset(&self) {}
    fn asset(&self, path: &str) -> Option<Asset> {
        match path {
            "index.html" => Some(Asset {
                bytes: b"<html>",
                content_type: "text/html",
                gzip: true,
                immutable: false,
            }),
            "assets/app.js" => Some(Asset {
                bytes: b"js",
                content_type: "application/javascript",
                gzip: true,
                immutable: true,
            }),
            _ => None,
        }
    }
    async fn sleep_ms(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms / 20)).await;
    }
    fn heap_free(&self) -> usize {
        FAKE_HEAP.with(std::cell::Cell::get).unwrap_or(100_000)
    }
    fn deadline(&self, ms: u64) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        Box::pin(tokio::time::sleep(Duration::from_millis(ms)))
    }
    fn ota_status(&self) -> OtaStatus {
        OtaStatus {
            status: "idle",
            progress: 0,
            in_progress: false,
            uploaded: 0,
            total: 0,
            error: None,
        }
    }
    fn ota_begin(&self) -> Result<(), OtaError> {
        self.ota.borrow_mut().clear();
        Ok(())
    }
    async fn ota_write(&self, data: &[u8]) -> Result<(), OtaError> {
        self.ota.borrow_mut().extend_from_slice(data);
        Ok(())
    }
    async fn ota_finish(&self) -> Result<(), OtaError> {
        *self.ota_done.borrow_mut() = true;
        Ok(())
    }
    fn ota_abort(&self, _: OtaError) {}

    #[cfg(feature = "bench")]
    fn debug_hang(&self) {
        *self.restarted.borrow_mut() = true;
    }
}

struct Resp {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&self.body)))
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim())
        })
    }
}

/// Runs `f` against a server on an ephemeral port.
async fn with_server<F, Fut>(cfg: Config, f: F) -> &'static Fake
where
    F: FnOnce(u16) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let fake: &'static Fake = Box::leak(Box::new(Fake::new(cfg)));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(async move {
                let streams: &'static _ = Box::leak(Box::new(StreamSlots::new(1)));
                let app: &'static _ = Box::leak(Box::new(Api::new(fake, streams).router()));
                static CONFIG: picoserve::Config = picoserve::Config::const_default();
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    tokio::task::spawn_local(async move {
                        let mut buf = vec![0u8; 4096];
                        let _ = picoserve::Server::new_tokio(app, &CONFIG, &mut buf)
                            .serve(stream)
                            .await;
                    });
                }
            });
            f(port).await;
        })
        .await;
    fake
}

async fn request(port: u16, raw: &[u8]) -> Resp {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    s.write_all(raw).await.unwrap();
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut out)).await;
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").expect("header end") + 4;
    let head = String::from_utf8_lossy(&out[..split]).into_owned();
    let status = head[9..12].parse().unwrap();
    Resp {
        status,
        head,
        body: out[split..].to_vec(),
    }
}

async fn get(port: u16, path: &str) -> Resp {
    request(
        port,
        format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .await
}

async fn post(port: u16, path: &str, ct: &str, body: &[u8]) -> Resp {
    let mut raw = format!(
        "POST {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: {ct}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    raw.extend_from_slice(body);
    request(port, &raw).await
}

fn ready() -> Config {
    Config {
        pid_enabled: true,
        ..Config::default()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn status_and_reads() {
    with_server(ready(), |port| async move {
        let r = get(port, "/api/status").await;
        assert_eq!(r.status, 200);
        let v = r.json();
        assert_eq!(v["machineState"], 20);
        assert_eq!(v["pidEnabled"], true);
        assert_eq!(r.header("Access-Control-Allow-Origin"), Some("*"));

        let r = get(port, "/api/parameters?filter=all").await;
        assert_eq!(r.status, 200);
        assert_eq!(r.json().as_array().unwrap().len(), cc_core::config::PARAMS.len());

        assert_eq!(get(port, "/api/health").await.status, 200);
        assert_eq!(get(port, "/api/temperatures").await.json()["targetTemp"], 95);
        assert!(get(port, "/api/history").await.json()["currentTemps"].is_array());

        let r = get(port, "/api/parameter-help?param=brew.setpoint").await;
        assert_eq!(r.json()["name"], "brew.setpoint");
        assert_eq!(get(port, "/api/parameter-help?param=nope").await.status, 404);
        assert_eq!(get(port, "/api/parameter-help").await.status, 422);

        let r = get(port, "/api/config/download").await;
        assert_eq!(r.status, 200);
        assert!(r.header("Content-Disposition").unwrap().contains("config.json"));
        assert_eq!(r.json()["brew"]["setpoint"], 95);
    })
    .await;
}

// Without a length a keep-alive client waits for the server to close the connection
// (1 s on the device) before it sees the end of an empty body.
#[tokio::test(flavor = "current_thread")]
async fn health_has_a_content_length() {
    with_server(ready(), |port| async move {
        let r = get(port, "/api/health").await;
        assert_eq!(r.status, 200);
        assert_eq!(r.header("Content-Length"), Some("0"));
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn writes_parameters_and_setpoint() {
    let fake = with_server(ready(), |port| async move {
        let r = post(
            port,
            "/api/parameters",
            "application/x-www-form-urlencoded",
            b"brew.setpoint=93.5&pid.use_ponm=true",
        )
        .await;
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        let r = post(
            port,
            "/api/parameters",
            "application/x-www-form-urlencoded",
            b"brew.setpoint=500",
        )
        .await;
        assert_eq!(r.status, 400);
        assert!(r.json()["message"].as_str().unwrap().contains("brew.setpoint"));
        let r = post(
            port,
            "/api/setpoint?value=88.5",
            "application/x-www-form-urlencoded",
            b"",
        )
        .await;
        assert_eq!(r.status, 200);
        let r = post(
            port,
            "/api/setpoint",
            "application/x-www-form-urlencoded",
            b"value=999",
        )
        .await;
        assert_eq!(r.status, 400);
    })
    .await;
    let cfg = fake.cfg.borrow();
    assert_eq!(cfg.brew_setpoint, 88.5);
    assert!(cfg.pid_use_ponm);
}

#[tokio::test(flavor = "current_thread")]
async fn toggles() {
    with_server(ready(), |port| async move {
        let empty = "application/x-www-form-urlencoded";
        assert_eq!(
            post(port, "/api/steam", empty, b"").await.json()["steamMode"],
            true
        );
        assert_eq!(
            post(port, "/api/pid", empty, b"").await.json()["pidEnabled"],
            false
        );
        assert_eq!(
            post(port, "/api/backflush", empty, b"").await.json()["backflushOn"],
            true
        );
        let r = post(port, "/api/maintenance/reset-backflush-counter", empty, b"").await;
        assert_eq!(r.json()["shotsSinceBackflush"], 0);
        assert_eq!(
            post(port, "/api/scale/tare", empty, b"").await.status,
            404,
            "scale disabled"
        );
        assert_eq!(post(port, "/api/wake", empty, b"").await.status, 200);
        assert_eq!(post(port, "/api/sleep", empty, b"").await.status, 200);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn errors_are_json() {
    with_server(ready(), |port| async move {
        let r = get(port, "/api/nope").await;
        assert_eq!(r.status, 404);
        assert_eq!(r.json()["error"], "API endpoint not found");
        let r = get(port, "/api/steam").await;
        assert_eq!(r.status, 405);
        let r = request(
            port,
            b"OPTIONS /api/status HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert_eq!(r.status, 204);
        let r = post(
            port,
            "/api/ota/url",
            "application/x-www-form-urlencoded",
            b"url=x",
        )
        .await;
        assert_eq!(r.status, 410);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn basic_auth_when_enabled() {
    let cfg = Config {
        system_auth_enabled: true,
        ..ready()
    };
    with_server(cfg, |port| async move {
        assert_eq!(get(port, "/api/status").await.status, 401);
        // admin:admin
        let r = request(port, b"GET /api/status HTTP/1.1\r\nHost: x\r\nConnection: close\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n").await;
        assert_eq!(r.status, 200);
        let r = request(port, b"GET /api/status HTTP/1.1\r\nHost: x\r\nConnection: close\r\nAuthorization: Basic YWRtaW46d3Jvbmc=\r\n\r\n").await;
        assert_eq!(r.status, 401);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn config_upload() {
    let fake = with_server(ready(), |port| async move {
        let r = post(
            port,
            "/api/config/upload",
            "application/json",
            br#"{"brew":{"setpoint":91}}"#,
        )
        .await;
        assert_eq!(r.status, 200);
        assert_eq!(r.json()["restart"], true);
        let r = post(
            port,
            "/api/config/upload",
            "application/json",
            br#"{"brew.setpoint":91}"#,
        )
        .await;
        assert_eq!(r.status, 400);
        assert_eq!(r.json()["success"], false);
    })
    .await;
    assert_eq!(fake.cfg.borrow().brew_setpoint, 91.0);
}

#[tokio::test(flavor = "current_thread")]
async fn firmware_upload_streams_the_file_part() {
    let image: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let mut body = Vec::new();
    body.extend_from_slice(b"--BOUNDARY\r\nContent-Disposition: form-data; name=\"firmware\"; filename=\"fw.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(&image);
    body.extend_from_slice(b"\r\n--BOUNDARY--\r\n");
    let fake = with_server(ready(), |port| async move {
        let r = post(
            port,
            "/api/ota/firmware",
            "multipart/form-data; boundary=BOUNDARY",
            &body,
        )
        .await;
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        assert_eq!(r.json()["success"], true);
        assert_eq!(get(port, "/api/ota/status").await.json()["status"], "idle");
    })
    .await;
    assert_eq!(*fake.ota.borrow(), image);
    assert!(*fake.ota_done.borrow());
    assert!(*fake.restarted.borrow());
}

// picoserve allows 3 s for a whole request body; an image takes the ESP32 30 to 60 s.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn slow_firmware_upload_is_not_cut_off() {
    let image: Vec<u8> = (0..8_000u32).map(|i| (i % 253) as u8).collect();
    let mut body = Vec::new();
    body.extend_from_slice(
        b"--B\r\nContent-Disposition: form-data; name=\"firmware\"; filename=\"fw.bin\"\r\n\r\n",
    );
    body.extend_from_slice(&image);
    body.extend_from_slice(b"\r\n--B--\r\n");
    let fake = with_server(ready(), |port| async move {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let head = format!(
            "POST /api/ota/firmware HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: multipart/form-data; boundary=B\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        s.write_all(head.as_bytes()).await.unwrap();
        for chunk in body.chunks(body.len() / 4 + 1) {
            tokio::time::sleep(Duration::from_millis(1_500)).await;
            s.write_all(chunk).await.unwrap();
        }
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.unwrap();
        let out = String::from_utf8_lossy(&out);
        assert!(out.starts_with("HTTP/1.1 200"), "{out}");
    })
    .await;
    assert_eq!(*fake.ota.borrow(), image);
}

#[tokio::test(flavor = "current_thread")]
async fn ui_is_served_from_the_firmware() {
    with_server(ready(), |port| async move {
        let r = get(port, "/").await;
        assert_eq!(r.status, 302);
        assert_eq!(r.header("Location"), Some("/ui/"));
        for path in ["/ui/", "/ui", "/ui/settings"] {
            let r = get(port, path).await;
            assert_eq!(r.status, 200, "{path}");
            assert_eq!(r.body, b"<html>");
            assert_eq!(r.header("Content-Encoding"), Some("gzip"));
            assert_eq!(r.header("Cache-Control"), Some("no-cache"));
        }
        let r = get(port, "/ui/assets/app.js").await;
        assert!(r.header("Cache-Control").unwrap().contains("immutable"));
        assert_eq!(get(port, "/ui/missing.png").await.status, 404);
        let probe = get(port, "/generate_204").await;
        assert_eq!(probe.status, 302);
        assert_eq!(probe.header("Location"), Some("/ui/"));
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn server_sent_events() {
    with_server(ready(), |port| async move {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(b"GET /events HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut seen = String::new();
        let mut buf = [0u8; 1024];
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while !seen.contains("event:new_temps") && tokio::time::Instant::now() < deadline {
            let n = tokio::time::timeout(Duration::from_secs(1), s.read(&mut buf))
                .await
                .unwrap_or(Ok(0))
                .unwrap();
            seen.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        assert!(seen.contains("text/event-stream"), "{seen}");
        assert!(seen.contains("event:hello"));
        assert!(seen.contains("event:new_temps"));
        assert!(seen.contains("\"targetTemp\":95"));

        // The test server allows one stream: a second is turned away, not queued.
        assert_eq!(get(port, "/events").await.status, 503);
        // Closing the first frees its slot once the server notices (next event).
        drop(s);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let mut again = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            again
                .write_all(b"GET /events HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
            let n = again.read(&mut buf).await.unwrap();
            if String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200") {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "slot not freed");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
}

#[cfg(feature = "bench")]
#[tokio::test(flavor = "current_thread")]
async fn bench_hang_route() {
    let fake = with_server(ready(), |port| async move {
        assert_eq!(post(port, "/api/debug/hang", "text/plain", b"").await.status, 200);
    })
    .await;
    assert!(*fake.restarted.borrow());
}

#[tokio::test(flavor = "current_thread")]
async fn low_heap_answers_503_but_health() {
    with_server(ready(), |port| async move {
        assert_eq!(get(port, "/api/status").await.status, 200);
        FAKE_HEAP.with(|h| h.set(Some(cc_web::MIN_FREE_HEAP - 1)));
        let r = get(port, "/api/parameters").await;
        assert_eq!(r.status, 503);
        assert_eq!(r.header("Retry-After"), Some("1"));
        assert_eq!(r.json()["success"], false);
        assert_eq!(get(port, "/api/health").await.status, 200);
        FAKE_HEAP.with(|h| h.set(None));
        assert_eq!(get(port, "/api/parameters").await.status, 200);
    })
    .await;
}
