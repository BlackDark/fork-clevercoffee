extern crate std;

use std::format;
use std::vec::Vec;

use cc_core::config::{Choice, Language};

use super::samples::{TEMPLATES, configs, net, scenarios, status};
use super::*;

#[test]
fn every_screen_renders_inside_the_panel_with_known_glyphs() {
    let mut problems = Vec::new();
    for (tname, template) in TEMPLATES {
        for (lang_v, _) in Language::OPTIONS {
            let lang = Language::from_i32(*lang_v).unwrap();
            for (cname, cfg) in configs() {
                let cfg = Config {
                    display_template: template,
                    display_language: lang,
                    ..cfg
                };
                for (sname, st) in scenarios() {
                    let mut f = Frame::default();
                    let view = View {
                        status: &st,
                        cfg: &cfg,
                        net: net(),
                        uptime_ms: 3_723_000,
                        ota: None,
                        banner: None,
                    };
                    let r = render(&mut f, &view);
                    if r.clipped || r.missing_glyph {
                        problems.push(format!("{tname}/{lang:?}/{cname}/{sname}: {r:?}"));
                    }
                }
            }
            for ota in [
                Ota::Running { percent: 57 },
                Ota::Done,
                Ota::Failed("write error at 0x1a0000 after 1234 bytes"),
            ] {
                let st = status(State::PidNormal);
                let cfg = Config {
                    display_template: template,
                    display_language: lang,
                    ..Config::default()
                };
                let mut f = Frame::default();
                let view = View {
                    status: &st,
                    cfg: &cfg,
                    net: net(),
                    uptime_ms: 0,
                    ota: Some(ota),
                    banner: None,
                };
                let r = render(&mut f, &view);
                if r.clipped || r.missing_glyph {
                    problems.push(format!("{tname}/{lang:?}/ota {ota:?}: {r:?}"));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} screens with problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn standby_long_enough_turns_the_panel_off() {
    let mut st = status(State::Standby);
    st.display_off = true;
    let cfg = Config::default();
    let mut f = Frame::default();
    let view = View {
        status: &st,
        cfg: &cfg,
        net: net(),
        uptime_ms: 0,
        ota: None,
        banner: None,
    };
    assert!(render(&mut f, &view).panel_off);
    assert!(f.bytes.iter().all(|b| *b == 0));
}

#[test]
fn inverted_display_is_rotated_180() {
    let st = status(State::PidNormal);
    let mut a = Frame::default();
    let mut b = Frame::default();
    let cfg = Config::default();
    let inv = Config {
        display_inverted: true,
        ..Config::default()
    };
    render(
        &mut a,
        &View {
            status: &st,
            cfg: &cfg,
            net: net(),
            uptime_ms: 0,
            ota: None,
            banner: None,
        },
    );
    render(
        &mut b,
        &View {
            status: &st,
            cfg: &inv,
            net: net(),
            uptime_ms: 0,
            ota: None,
            banner: None,
        },
    );
    for y in 0..64 {
        for x in 0..128 {
            assert_eq!(a.get(x, y), b.get(127 - x, 63 - y));
        }
    }
}
