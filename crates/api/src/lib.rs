//! The HTTP API's data formats, kept compatible with the web UI: response bodies,
//! the nested config file, form decoding and multipart uploads. No allocation.

#![no_std]

pub mod json;
pub mod multipart;

use core::fmt::{self, Write};

use cc_core::Status;
use cc_core::config::{self, Config, Kind, PARAMS, Value};
use cc_core::history::History;
use cc_core::state::State;
use json::Writer;

/// `GET /api/parameters`: every parameter with metadata, as the UI's
/// `ServerParameter` expects (`name`, `label`, `section`, `order`, `helpText`, `type`,
/// `value`, `default`, and `min`/`max` or `options`). Secrets are never sent back.
pub fn write_parameters(cfg: &Config, w: &mut Writer<impl Write>) -> fmt::Result {
    let defaults = Config::default();
    w.begin_array()?;
    for (i, p) in PARAMS.iter().enumerate() {
        w.begin_object()?;
        w.kv_str("name", p.key)?;
        w.kv_str("label", p.label)?;
        w.kv_int("section", i64::from(p.section))?;
        w.kv_int("order", i64::from(p.order))?;
        w.kv_str("helpText", p.help)?;
        w.kv_int("type", i64::from(p.kind.ui_type()))?;
        if p.reboot {
            w.kv_bool("requiresReboot", true)?;
        }
        let secret = matches!(p.kind, Kind::Text { secret: true, .. });
        w.key("value")?;
        write_value(w, if secret { Value::Text("") } else { cfg.get(i) })?;
        w.key("default")?;
        write_value(w, if secret { Value::Text("") } else { defaults.get(i) })?;
        match p.kind {
            Kind::Int { min, max } => {
                w.kv_int("min", i64::from(min))?;
                w.kv_int("max", i64::from(max))?;
            }
            Kind::Double { min, max } => {
                w.kv_num("min", min)?;
                w.kv_num("max", max)?;
            }
            Kind::Choice { options } => {
                w.key("options")?;
                w.begin_array()?;
                for (v, label) in options {
                    w.begin_object()?;
                    w.kv_int("value", i64::from(*v))?;
                    w.kv_str("label", label)?;
                    w.end_object()?;
                }
                w.end_array()?;
            }
            Kind::Bool | Kind::Text { .. } => {}
        }
        if secret {
            w.kv_bool("secret", true)?;
        }
        w.end_object()?;
    }
    w.end_array()
}

fn write_value(w: &mut Writer<impl Write>, v: Value<'_>) -> fmt::Result {
    match v {
        Value::Bool(b) => w.bool(b),
        Value::Int(i) | Value::Choice(i) => w.int(i64::from(i)),
        Value::Double(d) => w.num(d),
        Value::Text(t) => w.str(t),
    }
}

/// `GET /api/parameter-help?param=...`.
pub fn write_parameter_help(key: &str, w: &mut Writer<impl Write>) -> Option<fmt::Result> {
    let p = PARAMS.iter().find(|p| p.key == key)?;
    Some((|| {
        w.begin_object()?;
        w.kv_str("name", p.key)?;
        w.kv_str("helpText", p.help)?;
        w.end_object()
    })())
}

/// Extra live values the core does not own.
#[derive(Debug, Clone, Copy, Default)]
pub struct Live {
    pub uptime_ms: u64,
    pub wifi_connected: bool,
    pub wifi_rssi: i8,
    pub mqtt_connected: bool,
    pub free_heap: u32,
    pub min_free_heap: u32,
}

/// `GET /api/status`. Fields as in the C++, plus a few diagnostics.
pub fn write_status(st: &Status, cfg: &Config, live: &Live, w: &mut Writer<impl Write>) -> fmt::Result {
    w.begin_object()?;
    w.kv_num("temperature", round2(st.temperature))?;
    w.kv_num("setpoint", st.setpoint)?;
    w.kv_num("heaterPower", round2(st.heater_power))?;
    w.kv_int("machineState", i64::from(st.state.code()))?;
    w.kv_str("machineStateName", st.state.name())?;
    w.kv_bool("isStandby", st.state == State::Standby)?;
    w.kv_int("standbyTime", st.standby_remaining_ms as i64)?;
    w.kv_bool("pidEnabled", st.pid_runtime)?;
    w.kv_bool("steamMode", st.state == State::Steam)?;
    w.kv_bool("backflushOn", st.backflush_mode)?;
    w.kv_int("uptime", live.uptime_ms as i64)?;
    w.kv_int("shotsSinceBackflush", i64::from(st.shots_since_backflush))?;
    w.kv_int(
        "backflushReminderThreshold",
        i64::from(cfg.maintenance_backflush_reminder_threshold),
    )?;
    w.kv_bool("backflushReminderDue", st.backflush_reminder_due(cfg))?;
    w.kv_bool("waterTankFull", st.water_tank_full)?;
    w.kv_bool("temperatureValid", st.temperature_valid)?;
    if cfg.hardware_sensors_scale_enabled {
        w.key("weight")?;
        match st.weight {
            Some(g) => w.num(round2(f64::from(g)))?,
            None => w.null()?,
        }
        w.kv_num("brewWeight", round2(f64::from(st.brew_weight)))?;
        w.kv_bool("scaleFault", st.scale_fault)?;
    }
    if cfg.hardware_sensors_pressure_enabled {
        w.key("pressure")?;
        match st.pressure {
            Some(p) => w.num(round2(f64::from(p)))?,
            None => w.null()?,
        }
    }
    w.kv_num("brewTime", round2(st.brew_time_ms / 1000.0))?;
    w.kv_bool("wifiConnected", live.wifi_connected)?;
    w.kv_int("wifiRssi", i64::from(live.wifi_rssi))?;
    w.kv_bool("mqttConnected", live.mqtt_connected)?;
    w.kv_int("freeHeap", i64::from(live.free_heap))?;
    w.kv_int("minFreeHeap", i64::from(live.min_free_heap))?;
    w.end_object()
}

/// `GET /api/temperatures` and the SSE `new_temps` event.
pub fn write_temperatures(st: &Status, w: &mut Writer<impl Write>) -> fmt::Result {
    w.begin_object()?;
    w.kv_num("currentTemp", round2(st.temperature))?;
    w.kv_num("targetTemp", st.setpoint)?;
    w.kv_num("heaterPower", round2(st.heater_power))?;
    w.end_object()
}

/// SSE `weight` event.
pub fn write_weight(st: &Status, w: &mut Writer<impl Write>) -> fmt::Result {
    w.begin_object()?;
    w.kv_num("weight", round2(f64::from(st.weight.unwrap_or(0.0))))?;
    w.kv_num("brewWeight", round2(f64::from(st.brew_weight)))?;
    w.end_object()
}

/// `GET /api/history`: three parallel arrays, oldest first.
pub fn write_history(h: &History, w: &mut Writer<impl Write>) -> fmt::Result {
    w.begin_object()?;
    for (name, f) in [
        (
            "currentTemps",
            (|p| p.temperature) as fn(&cc_core::history::Point) -> f32,
        ),
        ("targetTemps", |p| p.setpoint),
        ("heaterPowers", |p| p.heater_power),
    ] {
        w.key(name)?;
        w.begin_array()?;
        for p in h.iter() {
            w.num2(f64::from(f(p)))?;
        }
        w.end_array()?;
    }
    w.end_object()
}

fn round2(v: f64) -> f64 {
    libm::round(v * 100.0) / 100.0
}

/// Indices of [`PARAMS`] sorted by key, so the nested export can group prefixes.
fn sorted_indices() -> heapless::Vec<u8, 128> {
    let mut v: heapless::Vec<u8, 128> = (0..PARAMS.len() as u8).collect();
    v.sort_unstable_by_key(|&i| PARAMS[i as usize].key);
    v
}

/// `GET /api/config` and the downloadable config file: the configuration as nested
/// objects (`{"brew": {"setpoint": 95, ...}, ...}`). Secrets are included: this is
/// the backup the user restores from.
pub fn write_config(cfg: &Config, w: &mut Writer<impl Write>) -> fmt::Result {
    let mut open: heapless::Vec<&str, 8> = heapless::Vec::new();
    w.begin_object()?;
    for i in sorted_indices() {
        let key = PARAMS[i as usize].key;
        let mut segs: heapless::Vec<&str, 8> = key.split('.').collect();
        let leaf = segs.pop().unwrap_or(key);
        let common = open.iter().zip(segs.iter()).take_while(|(a, b)| a == b).count();
        while open.len() > common {
            open.pop();
            w.end_object()?;
        }
        for s in &segs[common..] {
            w.key(s)?;
            w.begin_object()?;
            let _ = open.push(s);
        }
        w.key(leaf)?;
        write_value(w, cfg.get(i as usize))?;
    }
    for _ in 0..open.len() {
        w.end_object()?;
    }
    w.end_object()
}

/// Why a config file was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    Syntax,
    NotAnObject,
    /// Top-level keys contain dots: the flat format older tools produced.
    FlatKeys,
    Invalid {
        key_index: usize,
        err: config::SetError,
    },
    NothingToImport,
}

impl core::fmt::Display for ImportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ImportError::Syntax => f.write_str("Invalid JSON"),
            ImportError::NotAnObject => f.write_str("JSON body must be a top-level object"),
            ImportError::FlatKeys => f.write_str(
                "Flat dotted-key JSON is not supported. Use nested objects (see Download Config).",
            ),
            ImportError::Invalid { key_index, err } => write!(f, "{}: {err}", PARAMS[*key_index].key),
            ImportError::NothingToImport => f.write_str("No known parameters in the file"),
        }
    }
}

/// Applies a nested config file. All-or-nothing: on any invalid value `cfg` is left
/// unchanged. Unknown keys are ignored (files from newer firmware). Leaves may be plain
/// values or `{"value": ...}` objects, as both formats circulated.
pub fn import_config(src: &[u8], cfg: &mut Config) -> Result<usize, ImportError> {
    use json::Token;
    let mut next = cfg.clone();
    let mut r = json::Reader::new(src);
    if r.next_token().map_err(|_| ImportError::Syntax)? != Some(Token::BeginObject) {
        return Err(ImportError::NotAnObject);
    }
    let mut path: heapless::String<96> = heapless::String::new();
    let mut lens: heapless::Vec<usize, 8> = heapless::Vec::new();
    let mut pending_key: Option<&str> = None;
    let mut wrapped = false; // inside a {"value": x} leaf wrapper
    let mut applied = 0usize;
    let mut buf = [0u8; 128];
    while let Some(tok) = r.next_token().map_err(|_| ImportError::Syntax)? {
        match tok {
            Token::Key(k) => {
                if lens.is_empty() && k.contains('.') {
                    return Err(ImportError::FlatKeys);
                }
                pending_key = Some(k);
            }
            Token::BeginObject => {
                let Some(k) = pending_key.take() else {
                    return Err(ImportError::Syntax);
                };
                let before = path.len();
                if !path.is_empty() {
                    path.push('.').map_err(|_| ImportError::Syntax)?;
                }
                path.push_str(k).map_err(|_| ImportError::Syntax)?;
                if config::find(&path).is_some() {
                    wrapped = true;
                }
                lens.push(before).map_err(|_| ImportError::Syntax)?;
            }
            Token::EndObject => {
                let Some(before) = lens.pop() else { break };
                path.truncate(before);
                wrapped = false;
            }
            Token::BeginArray => r.skip_value(tok).map_err(|_| ImportError::Syntax)?,
            Token::EndArray => return Err(ImportError::Syntax),
            Token::Str(_) | Token::Num(_) | Token::Bool(_) | Token::Null => {
                let k = pending_key.take().unwrap_or("");
                let mut full: heapless::String<96> = path.clone();
                if !wrapped {
                    if !full.is_empty() {
                        full.push('.').map_err(|_| ImportError::Syntax)?;
                    }
                    full.push_str(k).map_err(|_| ImportError::Syntax)?;
                } else if k != "value" {
                    continue;
                }
                let Some(index) = config::find(&full) else {
                    continue;
                };
                let text = match tok {
                    Token::Str(raw) => json::unescape(raw, &mut buf).ok_or(ImportError::Syntax)?,
                    Token::Num(n) => n,
                    Token::Bool(b) => {
                        if b {
                            "true"
                        } else {
                            "false"
                        }
                    }
                    _ => continue,
                };
                next.set_from_str(&full, text)
                    .map_err(|err| ImportError::Invalid {
                        key_index: index,
                        err,
                    })?;
                applied += 1;
            }
        }
    }
    if applied == 0 {
        return Err(ImportError::NothingToImport);
    }
    *cfg = next;
    Ok(applied)
}

/// Iterates `key=value` pairs of an `application/x-www-form-urlencoded` body or query
/// string. Values are percent-decoded into the caller's buffer one pair at a time.
#[derive(Debug)]
pub struct FormPairs<'a> {
    rest: &'a str,
}

impl<'a> FormPairs<'a> {
    pub fn new(s: &'a str) -> Self {
        Self { rest: s }
    }

    /// Next pair, decoded into `key_buf` and `val_buf`. `Err(())` on malformed input or
    /// a too-small buffer.
    #[allow(clippy::result_unit_err)]
    pub fn next_pair<'k, 'v>(
        &mut self,
        key_buf: &'k mut [u8],
        val_buf: &'v mut [u8],
    ) -> Option<Result<(&'k str, &'v str), ()>> {
        loop {
            if self.rest.is_empty() {
                return None;
            }
            let (pair, rest) = self.rest.split_once('&').unwrap_or((self.rest, ""));
            self.rest = rest;
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            return Some(match (percent_decode(k, key_buf), percent_decode(v, val_buf)) {
                (Some(k), Some(v)) => Ok((k, v)),
                _ => Err(()),
            });
        }
    }
}

/// Decodes `%XX` and `+` into `out`.
pub fn percent_decode<'b>(s: &str, out: &'b mut [u8]) -> Option<&'b str> {
    let b = s.as_bytes();
    let (mut i, mut n) = (0, 0);
    while i < b.len() {
        let c = match b[i] {
            b'+' => b' ',
            b'%' => {
                let hex = core::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                i += 2;
                u8::from_str_radix(hex, 16).ok()?
            }
            c => c,
        };
        *out.get_mut(n)? = c;
        n += 1;
        i += 1;
    }
    core::str::from_utf8(&out[..n]).ok()
}

/// Looks up one field of a form body or query string.
pub fn form_value<'b>(s: &str, key: &str, buf: &'b mut [u8]) -> Option<&'b str> {
    let mut kb = [0u8; 64];
    let mut vb = [0u8; 256];
    let mut pairs = FormPairs::new(s);
    while let Some(p) = pairs.next_pair(&mut kb, &mut vb) {
        let Ok((k, v)) = p else { continue };
        if k == key {
            let n = v.len();
            buf.get_mut(..n)?.copy_from_slice(v.as_bytes());
            return core::str::from_utf8(&buf[..n]).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::String;

    fn render(f: impl FnOnce(&mut Writer<String>) -> fmt::Result) -> String {
        let mut w = Writer::new(String::new());
        f(&mut w).unwrap();
        w.into_inner()
    }

    #[test]
    fn parameters_list_matches_the_ui_contract() {
        let s = render(|w| write_parameters(&Config::default(), w));
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), PARAMS.len());
        let sp = arr.iter().find(|p| p["name"] == "brew.setpoint").unwrap();
        assert_eq!(sp["type"], 2);
        assert_eq!(sp["value"], 95);
        assert_eq!(sp["min"], 20);
        assert_eq!(sp["max"], 110);
        let tmpl = arr.iter().find(|p| p["name"] == "display.template").unwrap();
        assert_eq!(tmpl["type"], 5);
        assert_eq!(tmpl["options"][5]["label"], "Modern");
        let pw = arr.iter().find(|p| p["name"] == "mqtt.password").unwrap();
        assert_eq!(pw["value"], "", "secrets are not sent back");
        let pid = arr.iter().find(|p| p["name"] == "pid.enabled").unwrap();
        assert_eq!(pid["type"], 6);
        assert_eq!(pid["value"], false);
    }

    #[test]
    fn mock_server_parameters_are_all_known() {
        // The UI's mock server mirrors what the C++ firmware returned; every key it
        // knows must exist here (or be retired).
        let mock: serde_json::Value = serde_json::from_str(include_str!(
            "../../../ui/packages/mock-server/data/parameters.json"
        ))
        .unwrap();
        for p in mock.as_array().unwrap() {
            let name = p["name"].as_str().unwrap();
            if !name.contains('.') {
                continue; // MQTT pseudo-parameters (STEAM_MODE, TARE_ON, ...)
            }
            assert!(
                config::find(name).is_some() || config::RETIRED_KEYS.contains(&name),
                "mock parameter {name} missing"
            );
        }
    }

    #[test]
    fn config_export_is_nested_and_round_trips() {
        let mut cfg = Config::default();
        cfg.set_from_str("brew.setpoint", "93.5").unwrap();
        cfg.set_from_str("mqtt.password", "p\"w").unwrap();
        let s = render(|w| write_config(&cfg, w));
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["brew"]["setpoint"], 93.5);
        assert_eq!(v["hardware"]["sensors"]["scale"]["calibration2"], 1);
        assert_eq!(v["pid"]["bd"]["kp"], 50);
        let mut back = Config::default();
        assert_eq!(import_config(s.as_bytes(), &mut back), Ok(PARAMS.len()));
        assert_eq!(back, cfg);
    }

    #[test]
    fn import_accepts_value_wrappers_and_ignores_unknown_keys() {
        let mut cfg = Config::default();
        let src = br#"{"brew": {"setpoint": {"value": 92, "min": 20}, "future": 1}, "x": [1, {"a": 2}]}"#;
        assert_eq!(import_config(src, &mut cfg), Ok(1));
        assert_eq!(cfg.brew_setpoint, 92.0);
    }

    #[test]
    fn import_is_all_or_nothing() {
        let mut cfg = Config::default();
        let src = br#"{"brew": {"setpoint": 92}, "pid": {"regular": {"kp": 9999}}}"#;
        assert!(matches!(
            import_config(src, &mut cfg),
            Err(ImportError::Invalid { .. })
        ));
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn import_rejects_flat_keys_and_garbage() {
        let mut cfg = Config::default();
        assert_eq!(
            import_config(br#"{"brew.setpoint": 92}"#, &mut cfg),
            Err(ImportError::FlatKeys)
        );
        assert_eq!(import_config(b"[1]", &mut cfg), Err(ImportError::NotAnObject));
        assert_eq!(import_config(b"{\"a\":", &mut cfg), Err(ImportError::Syntax));
        assert_eq!(import_config(b"{}", &mut cfg), Err(ImportError::NothingToImport));
    }

    #[test]
    fn form_pairs_decode() {
        let mut kb = [0u8; 64];
        let mut vb = [0u8; 64];
        let mut p = FormPairs::new("brew.setpoint=93.5&mqtt.topic=a%2Fb+c&&flag");
        assert_eq!(p.next_pair(&mut kb, &mut vb), Some(Ok(("brew.setpoint", "93.5"))));
        assert_eq!(p.next_pair(&mut kb, &mut vb), Some(Ok(("mqtt.topic", "a/b c"))));
        assert_eq!(p.next_pair(&mut kb, &mut vb), Some(Ok(("flag", ""))));
        assert_eq!(p.next_pair(&mut kb, &mut vb), None);
        let mut b = [0u8; 16];
        assert_eq!(form_value("x=1&value=88.5", "value", &mut b), Some("88.5"));
    }

    #[test]
    fn status_has_the_fields_the_ui_reads() {
        let cfg = Config {
            pid_enabled: true,
            ..Config::default()
        };
        let m = cc_core::Machine::new(&cfg, 3, 0);
        let st = m.status(&cfg);
        let s = render(|w| write_status(&st, &cfg, &Live::default(), w));
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        for k in [
            "temperature",
            "setpoint",
            "heaterPower",
            "machineState",
            "isStandby",
            "standbyTime",
            "pidEnabled",
            "steamMode",
            "uptime",
            "shotsSinceBackflush",
            "backflushReminderThreshold",
            "backflushReminderDue",
        ] {
            assert!(v.get(k).is_some(), "{k}");
        }
        assert_eq!(v["shotsSinceBackflush"], 3);
    }
}
