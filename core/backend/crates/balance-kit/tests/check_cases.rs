//! Shared examples `fixtures/cases.json`: the same answers are expected from this crate and from `balance_kit/tools/check.py`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use balance_kit::{Issue, Kit, Patch};
use serde_json::{json, Value as Json};

type Found = BTreeSet<(String, Option<u64>)>;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn cases() -> Vec<Json> {
    let text = std::fs::read_to_string(fixtures().join("cases.json")).unwrap();
    serde_json::from_str::<Json>(&text).unwrap().as_array().unwrap().clone()
}

fn kit_dir(case: &Json) -> PathBuf {
    fixtures().join(case["kit"].as_str().unwrap_or("kit"))
}

/// A correct header + the example's overrides
fn patch_json(case: &Json) -> Json {
    let mut p = json!({
        "format": "dbbalance-patch/1", "base": "20260101-aaaaaaaa", "season": null, "author": "test", "comment": "",
        "changes": case["changes"]
    });
    if let Some(header) = case["header"].as_object() {
        for (k, v) in header {
            p[k] = v.clone();
        }
    }
    for k in case["drop"].as_array().into_iter().flatten() {
        p.as_object_mut().unwrap().remove(k.as_str().unwrap());
    }
    p
}

fn expected(case: &Json, field: &str) -> Found {
    case[field].as_array().into_iter().flatten().map(|x| (x[0].as_str().unwrap().to_string(), x[1].as_u64())).collect()
}

fn found(issues: &[Issue]) -> Found {
    issues.iter().map(|i| (i.code.to_string(), i.at.map(|x| x as u64))).collect()
}

#[test]
fn rust_rules() {
    let mut failed = Vec::new();
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let kit = Kit::open(&kit_dir(&case)).unwrap();
        let report = kit.check_patch(&Patch::from_json(&patch_json(&case)).unwrap()).unwrap();
        let (errors, warnings) = (found(&report.errors), found(&report.warnings));
        if errors != expected(&case, "errors") || warnings != expected(&case, "warnings") {
            failed.push(format!("{}: errors {:?}, warnings {:?}", name, errors, warnings));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn python_rules_match() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../content/balance/kit/tools/check.py");
    if Command::new("python").arg("--version").output().is_err() {
        eprintln!("python not found: check.py not checked");
        return;
    }
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mut failed = Vec::new();
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let file = tmp.join(format!("case-{}.json", name));
        std::fs::write(&file, serde_json::to_string(&patch_json(&case)).unwrap()).unwrap();
        let out = Command::new("python").env("PYTHONUTF8", "1").arg(&script).arg(&file).arg("--kit").arg(kit_dir(&case)).arg("--json").output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let Ok(res) = serde_json::from_str::<Json>(&stdout) else {
            failed.push(format!("{}: not JSON: {} {}", name, stdout, String::from_utf8_lossy(&out.stderr)));
            continue;
        };
        let read = |field: &str| -> Found { res[field].as_array().into_iter().flatten().map(|x| (x["code"].as_str().unwrap().to_string(), x["at"].as_u64())).collect() };
        let (errors, warnings) = (read("errors"), read("warnings"));
        if errors != expected(&case, "errors") || warnings != expected(&case, "warnings") {
            failed.push(format!("{}: errors {:?}, warnings {:?}", name, errors, warnings));
        }
        if out.status.success() != errors.is_empty() {
            failed.push(format!("{}: exit code {:?} with errors {:?}", name, out.status.code(), errors));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn old_kit_without_catalog_opens() {
    let kit = Kit::open(&fixtures().join("kit_nocat")).unwrap();
    assert!(kit.catalog.is_none());
    assert_eq!(kit.season(), Some("dead-money"));
    let t = kit.table_data("gear").unwrap().unwrap();
    assert!(t.kinds.is_empty());
    assert!(t.orig.is_none());
    assert!(kit.table_data("nope").unwrap().is_none());
}

#[test]
fn not_a_kit_is_refused() {
    assert!(Kit::open(&fixtures()).is_err());
}
