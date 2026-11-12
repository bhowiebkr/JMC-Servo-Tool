//! Window tests without a window: frames run headless against the simulated drive.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use jmc_core::drive::{Drive, Persist, WriteOptions};
use jmc_core::link::Link;
use jmc_core::params::Schema;
use jmc_core::sim::SimDrive;
use jmc_core::snapshot::Snapshot;

use crate::app::{App, Compare, PendingWrite, Tab, Values};
use crate::live::{sample_rate, samples_csv};
use crate::settings::Settings;
use crate::table::{Filter, rows};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("jmc-gui-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A baseline folder holding the simulated drive's factory values as axis X.
fn sim_baselines(dir: &Path) {
    let schema = Schema::model512();
    let mut d = Drive::new(Link::new(Box::new(SimDrive::new(schema)), 1), schema);
    let all = d.read_all(|_, _| {}).unwrap();
    let mut s = Snapshot::from_raw(schema, "test", &all.values);
    s.axis = Some("X".into());
    std::fs::create_dir_all(dir).unwrap();
    s.save(&dir.join("X-test.json")).unwrap();
}

fn frame(ctx: &egui::Context, app: &mut App) {
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
    // No renderer here: drop the font texture uploads on purpose.
    out.textures_delta.clear();
}

/// Draw frames until `done` holds or the time runs out.
fn run_until(ctx: &egui::Context, app: &mut App, done: impl Fn(&App) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(20);
    while Instant::now() < end {
        frame(ctx, app);
        if done(app) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn log_text(app: &App) -> String {
    app.log.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
}

fn sim_app(dir: &Path) -> (egui::Context, App) {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut app = App::new(ctx.clone(), None);
    app.settings.sim = true;
    app.settings.sim_from = None;
    app.settings.baselines_dir = dir.join("baselines").display().to_string();
    app.settings.backup_dir = dir.join("backups").display().to_string();
    app.load_baselines();
    (ctx, app)
}

fn connect(ctx: &egui::Context, app: &mut App) {
    app.connect();
    let ok = run_until(ctx, app, |a| a.connected.is_some() && a.values.from_drive && a.busy.is_none());
    assert!(ok, "no connection; log:\n{}", log_text(app));
}

#[test]
fn connects_reads_and_writes_through_the_window() {
    let dir = temp_dir("write");
    sim_baselines(&dir.join("baselines"));
    let (ctx, mut app) = sim_app(&dir);
    assert!(app.baselines_note.contains("1 baselines: X"), "{}", app.baselines_note);
    connect(&ctx, &mut app);
    assert!(app.values.raw.len() > 400, "read {} values", app.values.raw.len());
    let id = app.identify.as_ref().expect("identification after reading");
    assert!(id.consistent_with("X"));

    let schema = Schema::model512();
    let def = schema.get("P02-00").unwrap();
    app.select(def);
    assert!(app.write_block(def).unwrap().contains("Choose the axis"));
    app.settings.axis = "Z".into();
    assert!(app.write_block(def).unwrap().contains("No baseline for axis Z"));
    app.settings.axis = "X".into();
    assert_eq!(app.write_block(def), None);
    app.edit_text = "123.4".into();
    frame(&ctx, &mut app);

    // The dialog stays up until answered.
    app.pending_write = Some(PendingWrite {
        code: def.code.clone(),
        name: def.name.clone(),
        from: def.format(app.values.raw[&def.code]),
        to: "123.4".into(),
    });
    frame(&ctx, &mut app);
    assert!(app.pending_write.is_some());
    app.confirm_write();
    assert!(app.pending_write.is_none());
    let ok = run_until(&ctx, &mut app, |a| a.last_report.is_some() && a.busy.is_none());
    assert!(ok, "no write report; log:\n{}", log_text(&app));
    let r = app.last_report.as_ref().unwrap();
    assert!(r.ok, "{r:?}");
    assert!(r.backup.starts_with(dir.join("backups")), "{}", r.backup.display());
    assert!(r.backup.exists());
    assert_eq!(app.values.raw.get("P02-00"), Some(&1234));
    assert_eq!(app.edit_text, "123.4");
    assert!(log_text(&app).contains("RAM only"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_is_blocked_with_a_reason() {
    let dir = temp_dir("block");
    sim_baselines(&dir.join("baselines"));
    let (ctx, mut app) = sim_app(&dir);
    let schema = Schema::model512();
    let gain = schema.get("P02-00").unwrap();
    app.settings.axis = "X".into();
    assert!(app.write_block(gain).unwrap().contains("Connect"));

    connect(&ctx, &mut app);
    assert!(app.write_block(schema.get("P00-21").unwrap()).unwrap().contains("Not writable here"));
    if let Some(p) = schema.real().find(|d| d.needs_power_cycle() && Drive::check_write(d, WriteOptions { persist: Persist::Ram, allow_protected: false }).is_ok()) {
        assert!(app.write_block(p).unwrap().contains("power cycle"), "{}", p.code);
    }

    // Values from a file are never written.
    let file = dir.join("baselines").join("X-test.json");
    app.open_values_file(&file);
    assert!(!app.values.from_drive);
    assert!(app.write_block(gain).unwrap().contains("Read the drive first"));

    // A drive that does not match the chosen axis.
    app.read_all();
    assert!(run_until(&ctx, &mut app, |a| a.values.from_drive && a.busy.is_none()));
    for code in ["P02-01", "P02-02"] {
        let v = app.values.raw.get_mut(code).unwrap();
        *v = v.wrapping_add(1);
    }
    let mut z = Snapshot::from_raw(schema, "test", &app.values.raw);
    z.axis = Some("Z".into());
    z.save(&dir.join("baselines").join("Z-test.json")).unwrap();
    app.read_all();
    assert!(run_until(&ctx, &mut app, |a| a.values.from_drive && a.busy.is_none()));
    app.load_baselines();
    app.settings.axis = "Z".into();
    let why = app.write_block(gain).unwrap();
    assert!(why.contains("does not look like axis Z"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn live_monitor_samples_and_draws() {
    let dir = temp_dir("live");
    sim_baselines(&dir.join("baselines"));
    let (ctx, mut app) = sim_app(&dir);
    connect(&ctx, &mut app);
    app.settings.axis = "X".into();
    app.tab = Tab::Live;
    app.settings.monitor_ms = 20;
    app.settings.channels = vec!["following_error".into(), "speed".into(), "position".into()];
    app.set_monitor(true);
    let ok = run_until(&ctx, &mut app, |a| a.samples.len() >= 5);
    assert!(ok, "no samples; log:\n{}", log_text(&app));
    let why = app.write_block(Schema::model512().get("P02-00").unwrap()).unwrap();
    assert!(why.contains("Stop the live monitor"), "{why}");
    app.set_monitor(false);
    frame(&ctx, &mut app);
    assert!(!app.monitoring);
    let csv = samples_csv(&app.samples);
    assert_eq!(csv.lines().count(), app.samples.len() + 1);
    assert!(csv.starts_with("t_s,pos_cmd,"));
    assert!(sample_rate(&app.samples, 10.0).unwrap() > 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_view_draws() {
    let dir = temp_dir("draw");
    sim_baselines(&dir.join("baselines"));
    let (ctx, mut app) = sim_app(&dir);
    for tab in [Tab::Parameters, Tab::Live] {
        app.tab = tab;
        frame(&ctx, &mut app);
    }
    connect(&ctx, &mut app);
    app.settings.axis = "X".into();
    app.select(Schema::model512().get("P01-01").unwrap());
    app.confirm_reset = true;
    app.show_spares = true;
    app.search = "gain".into();
    for _ in 0..3 {
        frame(&ctx, &mut app);
    }
    assert!(app.confirm_reset, "the reset dialog waits for an answer");
    app.confirm_reset = false;
    app.settings.sim = false;
    app.disconnect();
    assert!(run_until(&ctx, &mut app, |a| a.connected.is_none()));
    frame(&ctx, &mut app);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A frame on a 1400x900 screen with these input events; returns the accessibility tree egui built.
fn frame_with(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> Option<egui::accesskit::TreeUpdate> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.draw(ui));
    out.textures_delta.clear();
    out.platform_output.accesskit_update
}

#[test]
fn clicking_a_row_text_selects_it() {
    // Labels that can be selected would take the click; the row must get it wherever it lands.
    let dir = temp_dir("click");
    let (ctx, mut app) = sim_app(&dir);
    app.search = "P02-00".into();
    ctx.enable_accesskit();
    frame_with(&ctx, &mut app, vec![]);
    let tree = frame_with(&ctx, &mut app, vec![]).expect("an accessibility tree");
    let name = Schema::model512().get("P02-00").unwrap().name.as_str();
    let b = tree
        .nodes
        .iter()
        .find(|(_, n)| n.value() == Some(name) || n.label() == Some(name))
        .and_then(|(_, n)| n.bounds())
        .expect("the row's name label");
    let pos = egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
    assert!(app.selected.is_none());
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame_with(&ctx, &mut app, vec![egui::Event::PointerMoved(pos)]);
    frame_with(&ctx, &mut app, vec![press(true)]);
    frame_with(&ctx, &mut app, vec![press(false)]);
    frame_with(&ctx, &mut app, vec![]);
    assert_eq!(app.selected.as_deref(), Some("P02-00"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn units_and_sentences() {
    let schema = Schema::model512();
    assert!(schema.real().all(|d| !d.unit.contains("---")), "no placeholder units");
    assert_eq!(crate::table::unit_text(schema.get("P02-00").unwrap()), "1/s");
    assert_eq!(crate::table::applies_text(schema.get("P03-08").unwrap()), Some("Read-only"));
    assert_eq!(crate::app::sentence("written to RAM only"), "Written to RAM only");
    assert_eq!(crate::app::sentence(""), "");
}

#[test]
fn table_filters() {
    let schema = Schema::model512();
    let mut values = Values::default();
    for d in schema.real() {
        values.raw.insert(d.code.clone(), d.default.and_then(|v| d.to_raw(v).ok()).unwrap_or(0));
    }
    let mut compare = Compare { label: "test".into(), raw: values.raw.clone() };
    *compare.raw.get_mut("P02-00").unwrap() += 1;
    let all = |f: &Filter| rows(schema, &values, Some(&compare), f);
    let f = Filter { search: "", group: None, only_changed: false, show_spares: false };
    let shown = all(&f);
    assert!(shown.iter().all(|d| !d.placeholder()));
    assert!(all(&Filter { show_spares: true, ..f }).len() > shown.len());
    let one = all(&Filter { search: "p2-0", ..f });
    assert!(one.iter().any(|d| d.code == "P02-00"));
    let exact = all(&Filter { search: "P02-00", ..f });
    assert_eq!(exact.first().map(|d| d.code.as_str()), Some("P02-00"));
    let changed = all(&Filter { only_changed: true, ..f });
    assert_eq!(changed.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), ["P02-00"]);
    assert!(all(&Filter { group: Some(2), ..f }).iter().all(|d| d.group == 2));
}

#[test]
fn settings_survive_a_restart() {
    let dir = temp_dir("settings");
    let path = dir.join("gui.json");
    assert_eq!(Settings::load(&path), Settings::default());
    let s = Settings { port: "COM7".into(), axis: "YL".into(), monitor_ms: 100, ..Settings::default() };
    s.save(&path).unwrap();
    assert_eq!(Settings::load(&path), s);
    std::fs::write(&path, "not json").unwrap();
    assert_eq!(Settings::load(&path), Settings::default());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every accessibility label on the screen.
fn labels(ctx: &egui::Context, app: &mut App) -> Vec<String> {
    ctx.enable_accesskit();
    frame_with(ctx, app, vec![]);
    let tree = frame_with(ctx, app, vec![]).expect("an accessibility tree");
    tree.nodes.iter().filter_map(|(_, n)| n.label().or(n.value()).map(str::to_string)).collect()
}

/// Click the widget whose accessibility label (or value) is `label`. False if there is none.
fn click_label(ctx: &egui::Context, app: &mut App, label: &str) -> bool {
    ctx.enable_accesskit();
    frame_with(ctx, app, vec![]);
    let Some(tree) = frame_with(ctx, app, vec![]) else { return false };
    let Some(b) = tree
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(label) || n.value() == Some(label))
        .and_then(|(_, n)| n.bounds())
    else {
        return false;
    };
    let pos = egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
    let press = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(ctx, app, vec![egui::Event::PointerMoved(pos)]);
    frame_with(ctx, app, vec![press(true)]);
    frame_with(ctx, app, vec![press(false)]);
    frame_with(ctx, app, vec![egui::Event::PointerGone]);
    true
}

#[test]
fn more_window_follows_links_and_goes_back() {
    let dir = temp_dir("more");
    let (ctx, mut app) = sim_app(&dir);
    ctx.set_embed_viewports(true);
    let schema = Schema::model512();
    // "More…" on the About card opens the selected parameter.
    app.select(schema.get("P02-00").unwrap());
    assert!(click_label(&ctx, &mut app, "More…"), "the About card has a More button");
    assert!(app.more.open);
    assert_eq!(app.more.current(), Some("P02-00"));
    let labels = labels(&ctx, &mut app);
    assert!(labels.iter().any(|l| l == "Try a value"), "the page has its drawing");
    let target = format!("P02-10  {}", schema.get("P02-10").unwrap().name);
    assert!(labels.contains(&target), "P02-00 has P02-10 under See also");
    // The Topics menu opens a topic page in the same window.
    assert!(click_label(&ctx, &mut app, "Topics"));
    assert!(click_label(&ctx, &mut app, "Filters"));
    assert_eq!(app.more.current(), Some("topic:filters"));
    // A code in the page's text is a link to that parameter.
    assert!(click_label(&ctx, &mut app, "P08-19"), "the filters page links to P08-19");
    assert_eq!(app.more.current(), Some("P08-19"));
    assert!(click_label(&ctx, &mut app, "← Back"));
    assert_eq!(app.more.current(), Some("topic:filters"));
    assert!(click_label(&ctx, &mut app, "Forward →"));
    assert_eq!(app.more.current(), Some("P08-19"));
    // "Select in table" selects it in the main window.
    assert!(click_label(&ctx, &mut app, "Select in table"));
    assert_eq!(app.selected.as_deref(), Some("P08-19"));
    assert!(click_label(&ctx, &mut app, "Close"));
    assert!(!app.more.open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_help_page_draws() {
    // Every parameter's drawing, with the drive's values and with odd "Try a value" values
    // (the ends of the range, so zeros and limits too), and every topic page.
    let dir = temp_dir("pages");
    let (ctx, mut app) = sim_app(&dir);
    ctx.set_embed_viewports(true);
    connect(&ctx, &mut app);
    let schema = Schema::model512();
    let mut keys: Vec<String> = schema.real().map(|d| d.code.clone()).collect();
    keys.extend(jmc_core::docs::topics().map(|p| p.key.clone()));
    for (i, key) in keys.iter().enumerate() {
        app.more.show(key);
        frame_with(&ctx, &mut app, vec![]);
        if let Ok(d) = schema.get(key) {
            let s = 10f64.powi(i32::from(d.decimals));
            let end = if i % 2 == 0 { d.max } else { d.min };
            app.more.trial.insert(key.clone(), end as f64 / s);
            frame_with(&ctx, &mut app, vec![]);
        }
    }
    assert!(app.more.open);
    app.settings.sim = false;
    app.disconnect();
    assert!(run_until(&ctx, &mut app, |a| a.connected.is_none()));
    // With no values at all: the default and the trial value only.
    app.values = Values::default();
    for key in ["P02-00", "P02-10", "P08-20", "P08-30", "P03-10", "P04-06", "P01-04"] {
        app.more.show(key);
        frame_with(&ctx, &mut app, vec![]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Pictures of the window and of one More page for each kind of drawing, to look at by eye:
/// `cargo test -p jmc-servo-tool pictures -- --ignored`. They go to the folder named in
/// `JMC_SHOTS`, else to a new temporary folder. Nothing opens on the screen.
#[test]
#[ignore = "draws with the GPU and writes PNG files"]
fn more_window_pictures() {
    let out = std::env::var_os("JMC_SHOTS").map_or_else(|| temp_dir("shots"), PathBuf::from);
    std::fs::create_dir_all(&out).unwrap();
    let Some(mut shot) = crate::shots::Shooter::new([1500, 1000], 1.0) else {
        eprintln!("no GPU adapter: no pictures");
        return;
    };
    let dir = temp_dir("pictures");
    sim_baselines(&dir.join("baselines"));
    let (ctx, mut app) = sim_app(&dir);
    ctx.set_embed_viewports(true);
    app.settings.axis = "X".into();
    app.connect();
    let end = Instant::now() + Duration::from_secs(20);
    while !(app.connected.is_some() && app.values.from_drive && app.busy.is_none()) {
        assert!(Instant::now() < end, "no connection; log:\n{}", log_text(&app));
        shot.frame(&ctx, vec![], |ui| app.draw(ui));
        std::thread::sleep(Duration::from_millis(5));
    }
    let schema = Schema::model512();
    app.select(schema.get("P02-00").unwrap());
    for _ in 0..3 {
        shot.frame(&ctx, vec![], |ui| app.draw(ui));
    }
    shot.save(&out.join("00-window.png"), None).unwrap();

    // One parameter for each kind of drawing, then the topics.
    let mut seen = Vec::new();
    let mut keys = Vec::new();
    for d in schema.real() {
        let drawing = jmc_core::docs::page(&d.code)
            .and_then(|p| p.drawing.clone())
            .unwrap_or_else(|| jmc_core::docs::drawing_for(d));
        if !seen.contains(&drawing.kind) {
            seen.push(drawing.kind);
            keys.push(d.code.clone());
        }
    }
    for code in ["P02-10", "P08-30"] {
        if !keys.iter().any(|k| k == code) {
            keys.push(code.into());
        }
    }
    keys.extend(jmc_core::docs::topics().map(|p| p.key.clone()));
    for (i, key) in keys.iter().enumerate() {
        app.more.show(key);
        if let Ok(d) = schema.get(key)
            && d.choices.is_empty()
        {
            // A "Try a value" a quarter of the way from the default to the top of the range (near
            // the bottom when the default is the top).
            let s = 10f64.powi(i32::from(d.decimals));
            let start = d.default.unwrap_or(d.min) as f64;
            let (lo, hi) = (d.min as f64, d.max as f64);
            let v = if start >= hi { lo + (hi - lo) / 10.0 } else { start + (hi - start) / 4.0 };
            app.more.trial.insert(key.clone(), v.round() / s);
        }
        for _ in 0..12 {
            shot.frame(&ctx, vec![], |ui| app.draw(ui));
        }
        let rect = ctx.top_layer_id().and_then(|l| ctx.memory(|m| m.area_rect(l.id)));
        let name = format!("{:02}-{}.png", i + 1, key.replace(':', "-"));
        shot.save(&out.join(name), rect).unwrap();
    }
    eprintln!("pictures in {}", out.display());
    app.settings.sim = false;
    app.disconnect();
    let end = Instant::now() + Duration::from_secs(20);
    while app.connected.is_some() && Instant::now() < end {
        shot.frame(&ctx, vec![], |ui| app.draw(ui));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
