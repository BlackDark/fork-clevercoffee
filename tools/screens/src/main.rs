//! Renders every OLED screen (template × language × sample state) to PNG files plus
//! an `index.html` contact sheet. Screens that draw off-panel or miss a glyph are
//! outlined in red.
//!
//! Usage: `cargo run -p screens -- [out-dir]` (default `target/screens`), or `just screens`.

use std::fmt::Write as _;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use cc_core::State;
use cc_core::config::{Choice, Config, DisplayTemplate, Language};
use cc_display::canvas::Frame;
use cc_display::samples::{TEMPLATES, configs, net, scenarios, status};
use cc_display::{Banner, Ota, Rendered, View, render};

const SCALE: usize = 3;

fn main() -> std::io::Result<()> {
    let out = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("target/screens"), PathBuf::from);
    fs::create_dir_all(&out)?;
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>CleverCoffee screens</title>\
         <style>body{font:13px system-ui;background:#222;color:#ddd;margin:16px}\
         h2{margin:24px 0 8px}figure{display:inline-block;margin:4px;vertical-align:top}\
         img{image-rendering:pixelated;border:2px solid #444}img.bad{border-color:#e33}\
         figcaption{max-width:390px;font-size:11px;color:#aaa}</style>\
         <h1>CleverCoffee OLED screens</h1><p>Red border: drawn off-panel or missing glyph.</p>",
    );
    let mut problems = 0;
    for (tname, template) in TEMPLATES {
        for &(lv, lname) in Language::OPTIONS {
            let lang = Language::from_i32(lv).expect("valid option");
            let _ = write!(html, "<h2>{tname} — {lname}</h2>");
            for (cname, cfg) in configs() {
                let cfg = Config {
                    display_template: template,
                    display_language: lang,
                    ..cfg
                };
                for (sname, st) in scenarios() {
                    let view = View {
                        status: &st,
                        cfg: &cfg,
                        net: net(),
                        uptime_ms: 3_723_000,
                        ota: None,
                        banner: None,
                    };
                    problems += emit(
                        &out,
                        &mut html,
                        &format!("{tname}-{lname}-{cname}-{sname}"),
                        &view,
                        template,
                    )?;
                }
            }
            let st = status(State::PidNormal);
            let cfg = Config {
                display_template: template,
                display_language: lang,
                ..Config::default()
            };
            for (name, ota) in [
                ("ota-running", Ota::Running { percent: 57 }),
                ("ota-done", Ota::Done),
                ("ota-failed", Ota::Failed("write error at 0x1a0000")),
            ] {
                let view = View {
                    status: &st,
                    cfg: &cfg,
                    net: net(),
                    uptime_ms: 0,
                    ota: Some(ota),
                    banner: None,
                };
                problems += emit(
                    &out,
                    &mut html,
                    &format!("{tname}-{lname}-{name}"),
                    &view,
                    template,
                )?;
            }
            let banner = Banner {
                line1: "Version",
                line2: "2.0.0",
            };
            let view = View {
                status: &st,
                cfg: &cfg,
                net: net(),
                uptime_ms: 0,
                ota: None,
                banner: Some(banner),
            };
            problems += emit(&out, &mut html, &format!("{tname}-{lname}-boot"), &view, template)?;
        }
    }
    fs::write(out.join("index.html"), html)?;
    println!(
        "wrote {}/index.html ({problems} screens with layout problems)",
        out.display()
    );
    Ok(())
}

fn emit(
    dir: &Path,
    html: &mut String,
    name: &str,
    view: &View<'_>,
    t: DisplayTemplate,
) -> std::io::Result<usize> {
    let mut frame = Frame::default();
    let r = render(&mut frame, view);
    let file = format!("{}.png", name.replace(' ', "_"));
    write_png(&dir.join(&file), &frame, t == DisplayTemplate::Upright)?;
    let bad = r.clipped || r.missing_glyph;
    let class = if bad { " class=bad" } else { "" };
    let note = describe(&r);
    let _ = write!(
        html,
        "<figure><img{class} src=\"{file}\"><figcaption>{name}{note}</figcaption></figure>"
    );
    Ok(usize::from(bad))
}

fn describe(r: &Rendered) -> String {
    let mut s = String::new();
    if let Some((x, y)) = r.first_clip {
        let _ = write!(s, " — off-panel at ({x},{y})");
    }
    if r.missing_glyph {
        s.push_str(" — missing glyph");
    }
    s
}

/// Writes the panel as the user sees it: portrait templates are turned upright.
fn write_png(path: &Path, f: &Frame, portrait: bool) -> std::io::Result<()> {
    let (w, h) = if portrait { (64, 128) } else { (128, 64) };
    let mut data = vec![0u8; w * SCALE * h * SCALE];
    for y in 0..h {
        for x in 0..w {
            // Undo the 90° rotation the renderer applied for portrait screens.
            let on = if portrait { f.get(127 - y, x) } else { f.get(x, y) };
            let v = if on { 0xe8 } else { 0x10 };
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    data[(y * SCALE + dy) * w * SCALE + x * SCALE + dx] = v;
                }
            }
        }
    }
    let file = BufWriter::new(fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, (w * SCALE) as u32, (h * SCALE) as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(std::io::Error::other)?;
    wr.write_image_data(&data).map_err(std::io::Error::other)?;
    Ok(())
}
