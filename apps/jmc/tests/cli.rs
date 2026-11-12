//! Runs the `jmc` binary against the simulated drive.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

struct Run {
    ok: bool,
    out: Value,
    err: String,
}

fn jmc(args: &[&str]) -> Run {
    let o = Command::new(env!("CARGO_BIN_EXE_jmc"))
        .args(args)
        .arg("--compact")
        .env_remove("JMC_PORT")
        .env_remove("JMC_BASELINES")
        .env_remove("JMC_BACKUP_DIR")
        .output()
        .expect("run jmc");
    let stdout = String::from_utf8_lossy(&o.stdout);
    Run {
        ok: o.status.success(),
        out: serde_json::from_str(stdout.trim()).unwrap_or(Value::Null),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jmc-cli-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn s(p: &std::path::Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn read_and_params() {
    let r = jmc(&["--sim", "read", "P02-00", "p3-9"]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.out[0]["value"], "48.0");
    assert_eq!(r.out[1]["code"], "P03-09");
    let r = jmc(&["params", "--group", "0", "--all"]);
    assert!(r.out.as_array().unwrap().iter().any(|p| p["flags"].as_array().unwrap().iter().any(|f| f == "reserved")));
    let r = jmc(&["read", "P02-00"]);
    assert!(!r.ok && r.err.contains("no port"), "{}", r.err);
}

#[test]
fn snapshot_export_import_round_trip() {
    let d = temp("roundtrip");
    let snap = d.join("x.json");
    assert!(jmc(&["--sim", "snapshot", "--out", s(&snap), "--axis", "x"]).ok);
    assert!(!jmc(&["--sim", "snapshot", "--out", s(&snap)]).ok, "refuses to overwrite");
    let xml = d.join("x.xml");
    let r = jmc(&["export-xml", s(&snap), "--out", s(&xml)]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.out["missing"].as_array().unwrap().len(), 0);
    let r = jmc(&["diff", s(&snap), s(&xml)]);
    assert_eq!(r.out["differing"], 0, "{}", r.out);
    let r = jmc(&["import-xml", s(&xml), "--taken", "2023-07-25T14:21:38Z"]);
    assert_eq!(r.out["taken_utc"], "2023-07-25T14:21:38Z");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn write_checks_axis_and_keeps_backups() {
    let d = temp("write");
    let base = d.join("base");
    let backups = d.join("backups");
    // Two baselines: X is factory, Z differs in one parameter.
    assert!(jmc(&["--sim", "snapshot", "--out", s(&base.join("X.json")), "--axis", "X"]).ok);
    let z = d.join("z-drive.json");
    assert!(jmc(&["--sim", "snapshot", "--out", s(&z)]).ok);
    let mut zs: Value = serde_json::from_str(&std::fs::read_to_string(&z).unwrap()).unwrap();
    zs["values"]["P03-09"] = "2000".into();
    zs["axis"] = "Z".into();
    std::fs::write(base.join("Z.json"), zs.to_string()).unwrap();
    std::fs::write(&z, zs.to_string()).unwrap();

    let common = ["--baselines", s(&base), "--backup-dir", s(&backups)];
    let mut args = vec!["--sim", "write", "P02-00=520.0", "--axis", "X"];
    args.extend(common);
    let r = jmc(&args);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.out["results"][0]["after"], "520.0");
    assert_eq!(r.out["results"][0]["persist"], "ram");
    // Same second, second write: its own backup file.
    let r = jmc(&args);
    assert!(r.ok, "{}", r.err);
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 2);

    // A Z drive named as X is refused before anything is written or saved.
    let mut args = vec!["--sim-from", s(&z), "write", "P02-00=520.0", "--axis", "X"];
    args.extend(common);
    let r = jmc(&args);
    assert!(!r.ok && r.err.contains("best match is Z"), "{}", r.err);
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 2);

    for (bad, why) in [
        (vec!["P00-00=1"], "motor data"),
        (vec!["P00-24=1"], "serial settings"),
        (vec!["P02-00=5000"], "outside"),
        (vec!["P02-00=50", "P2-0=51"], "twice"),
        (vec!["P02-00"], "CODE=VALUE"),
    ] {
        let mut args = vec!["--sim", "write", "--axis", "X", "--no-identify-check", "--backup-dir", s(&backups)];
        args.extend(bad);
        let r = jmc(&args);
        assert!(!r.ok && r.err.contains(why), "{why}: {}", r.err);
    }
    let r = jmc(&["--sim", "write", "P02-00=50", "--axis", "X", "--save", "--no-identify-check"]);
    assert!(!r.ok && r.err.contains("--yes"), "{}", r.err);
    let r = jmc(&["--sim", "write", "P02-00=50", "--axis", "X"]);
    assert!(!r.ok && r.err.contains("--baselines"), "{}", r.err);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn status_monitor_and_raw() {
    let r = jmc(&["--sim", "status"]);
    assert_eq!(r.out["servo_on"], false);
    let o = Command::new(env!("CARGO_BIN_EXE_jmc")).args(["--sim", "monitor", "--count", "3", "--interval-ms", "5"]).output().unwrap();
    let lines: Vec<Value> = String::from_utf8_lossy(&o.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["sample"]["bus_v"], 310);
    let r = jmc(&["--sim", "raw-read", "0xC8", "2"]);
    assert_eq!(r.out[0]["param"], "P02-00");
    assert_eq!(r.out[0]["u16"], 480);
    assert!(!jmc(&["--sim", "reset-alarm"]).ok);
}
