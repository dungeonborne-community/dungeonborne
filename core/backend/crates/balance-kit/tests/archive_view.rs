//! Taking a kit (zip safety) and the interface view on the `fixtures/kit` examples.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use balance_kit::{draft_fits, import, BulkSpec, KitView, Limits, Patch, Query};
use serde_json::{json, Value as Json};
use zip::write::SimpleFileOptions;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn tmp(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn add_dir(z: &mut zip::ZipWriter<fs::File>, dir: &Path, prefix: &str) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let name = format!("{}{}", prefix, e.file_name().to_string_lossy());
        if e.path().is_dir() {
            add_dir(z, &e.path(), &format!("{}/", name));
        } else {
            z.start_file(name, SimpleFileOptions::default()).unwrap();
            z.write_all(&fs::read(e.path()).unwrap()).unwrap();
        }
    }
}

/// The example kit in a zip (with a folder inside, as from `balance kit`) plus `extra`
fn kit_zip(dir: &Path, extra: impl FnOnce(&mut zip::ZipWriter<fs::File>)) -> PathBuf {
    let path = dir.join("kit.zip");
    let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    add_dir(&mut z, &fixtures().join("kit"), "my-kit/");
    extra(&mut z);
    z.finish().unwrap();
    path
}

fn code_of(src: &Path, dest: &Path, limits: Limits) -> &'static str {
    import(src, dest, limits).expect_err("the kit should have been rejected").code
}

#[test]
fn zip_imports_only_data() {
    let dir = tmp("zip_ok");
    let dest = dir.join("kits");
    let src = kit_zip(&dir, |z| {
        for name in ["my-kit/balance.xlsx", "my-kit/tools/check.py", "my-kit/llm/PROMPT.md", "my-kit/data/evil.exe"] {
            z.start_file(name, SimpleFileOptions::default()).unwrap();
            z.write_all(b"x").unwrap();
        }
        z.start_file("my-kit/icons/100.png", SimpleFileOptions::default()).unwrap();
        z.write_all(b"\x89PNG\r\n\x1a\nrest").unwrap();
        z.start_file("my-kit/icons/fake.png", SimpleFileOptions::default()).unwrap();
        z.write_all(b"<svg onload=alert(1)>").unwrap();
    });
    let r = import(&src, &dest, Limits::default()).unwrap();
    assert_eq!(r.version, "20260101-aaaaaaaa");
    assert_eq!(r.root, dest.join("20260101-aaaaaaaa"));
    assert_eq!(r.skipped, 4);
    assert_eq!(r.bad_icons, 1);
    assert!(r.root.join("manifest.json").is_file());
    assert!(r.root.join("data/gear.csv").is_file());
    assert!(r.root.join("icons/100.png").is_file());
    for gone in ["balance.xlsx", "tools", "llm", "data/evil.exe", "icons/fake.png"] {
        assert!(!r.root.join(gone).exists(), "{gone}");
    }
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1, "the temp folder was not removed");
    // Opening again replaces the copy
    assert!(import(&src, &dest, Limits::default()).is_ok());
}

#[test]
fn folder_imports_only_data() {
    let dir = tmp("folder_ok");
    let r = import(&fixtures().join("kit"), &dir.join("kits"), Limits::default()).unwrap();
    assert!(r.root.join("kit.json").is_file());
    assert_eq!(r.bad_icons, 0);
}

#[test]
fn dangerous_paths_are_refused() {
    for (i, bad) in ["../evil.txt", "my-kit/../../evil.txt", "/abs/evil.txt", "C:\\evil.txt", "my-kit\\..\\..\\evil.txt"].iter().enumerate() {
        let dir = tmp(&format!("zip_bad_{}", i));
        let src = kit_zip(&dir, |z| {
            z.start_file(*bad, SimpleFileOptions::default()).unwrap();
            z.write_all(b"x").unwrap();
        });
        assert_eq!(code_of(&src, &dir.join("kits"), Limits::default()), "kit_unsafe", "{bad}");
        assert!(!dir.join("evil.txt").exists());
        assert_eq!(fs::read_dir(dir.join("kits")).unwrap().count(), 0, "no files must remain after a refusal");
    }
}

#[test]
fn symlink_is_refused() {
    let dir = tmp("zip_link");
    let src = kit_zip(&dir, |z| {
        z.add_symlink("my-kit/data/link.csv", "../../../secret", SimpleFileOptions::default()).unwrap();
    });
    assert_eq!(code_of(&src, &dir.join("kits"), Limits::default()), "kit_unsafe");
}

#[test]
fn bomb_is_refused() {
    let dir = tmp("zip_bomb");
    let src = kit_zip(&dir, |z| {
        z.start_file("my-kit/data/bomb.csv", SimpleFileOptions::default().large_file(false)).unwrap();
        let chunk = vec![b'0'; 1 << 20];
        for _ in 0..8 {
            z.write_all(&chunk).unwrap();
        }
    });
    assert_eq!(code_of(&src, &dir.join("a"), Limits { ratio: 10, ..Limits::default() }), "kit_too_large");
    assert_eq!(code_of(&src, &dir.join("b"), Limits { file: 1 << 20, ..Limits::default() }), "kit_too_large");
    assert_eq!(code_of(&src, &dir.join("c"), Limits { total: 1 << 20, ..Limits::default() }), "kit_too_large");
    assert_eq!(code_of(&src, &dir.join("d"), Limits { entries: 3, ..Limits::default() }), "kit_too_large");
}

#[test]
fn not_a_kit_is_refused() {
    let dir = tmp("zip_not");
    let p = dir.join("x.zip");
    let mut z = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    z.start_file("readme.txt", SimpleFileOptions::default()).unwrap();
    z.finish().unwrap();
    assert_eq!(code_of(&p, &dir.join("kits"), Limits::default()), "kit_not_found");

    let garbage = dir.join("y.zip");
    fs::write(&garbage, b"not a zip").unwrap();
    assert_eq!(code_of(&garbage, &dir.join("kits"), Limits::default()), "kit_damaged");
    assert_eq!(code_of(&dir.join("nope.zip"), &dir.join("kits"), Limits::default()), "kit_not_found");
}

/// A copy of the example in which files may be changed
fn edited_kit(name: &str, file: &str, edit: impl FnOnce(String) -> String) -> PathBuf {
    let dir = tmp(name);
    let src = dir.join("kit");
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for e in fs::read_dir(from).unwrap().flatten() {
            if e.path().is_dir() {
                copy(&e.path(), &to.join(e.file_name()));
            } else {
                fs::copy(e.path(), to.join(e.file_name())).unwrap();
            }
        }
    }
    copy(&fixtures().join("kit"), &src);
    let path = src.join(file);
    fs::write(&path, edit(fs::read_to_string(&path).unwrap())).unwrap();
    src
}

#[test]
fn table_name_cannot_leave_the_kit() {
    let src = edited_kit("bad_table", "kit.json", |t| t.replace("\"name\": \"gear\"", "\"name\": \"../../secret\""));
    let dest = src.parent().unwrap().join("kits");
    assert_eq!(code_of(&src, &dest, Limits::default()), "kit_invalid");
}

#[test]
fn version_cannot_leave_the_kits_folder() {
    let src = edited_kit("bad_version", "manifest.json", |t| t.replace("20260101-aaaaaaaa", "../../evil"));
    let dest = src.parent().unwrap().join("kits");
    assert_eq!(code_of(&src, &dest, Limits::default()), "kit_invalid");
    assert!(!src.parent().unwrap().parent().unwrap().join("evil").exists());
}

fn query(extra: Json) -> Query {
    Query::from_json(&extra)
}

fn view() -> KitView {
    KitView::load(&fixtures().join("kit")).unwrap()
}

#[test]
fn overview_counts() {
    let v = view();
    let o = v.overview();
    assert_eq!(o["version"], "20260101-aaaaaaaa");
    let gear = &o["tables"][0];
    assert_eq!(gear["rows"], 3);
    assert_eq!(gear["has_orig"], true);
    // Only in row 0: name, unit, count, mode, negative (the original lacks them) and power (5 with 10 in the original).
    // Rows 1 and 2 are not in the original, they count separately
    assert_eq!(gear["changed"], 6, "{gear}");
    assert_eq!(gear["new_rows"], 2);
    assert_eq!(o["tables"][1]["has_orig"], false);
    assert_eq!(o["tables"][1]["changed"], 0);
}

#[test]
fn wide_shows_editable_columns_by_default() {
    let v = view();
    let r = v.query("gear", &query(json!({"view": "wide"})), 0, 10).unwrap();
    assert_eq!(r["total"], 3);
    let paths: Vec<&str> = r["columns"].as_array().unwrap().iter().map(|c| c["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"power.fixed_value") && !paths.contains(&"name") && !paths.contains(&"item_id"), "{paths:?}");
    assert_eq!(r["has_all_fields"], true);

    let all = v.query("gear", &query(json!({"view": "wide", "all_fields": true})), 0, 10).unwrap();
    let paths: Vec<&str> = all["columns"].as_array().unwrap().iter().map(|c| c["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"name") && paths.contains(&"item_id"));

    // A reference table: nothing to edit, so everything is visible
    let info = v.query("info", &query(json!({})), 0, 10).unwrap();
    assert_eq!(info["columns"].as_array().unwrap().len(), 1);
    assert_eq!(info["has_all_fields"], false);
}

#[test]
fn cells_carry_the_original() {
    let v = view();
    let r = v.query("gear", &query(json!({"view": "wide"})), 0, 10).unwrap();
    let col = r["columns"].as_array().unwrap().iter().position(|c| c["path"] == "power.fixed_value").unwrap();
    assert_eq!(r["rows"][0]["cells"][col], json!({"v": "5", "o": "10"}));
    let rate = r["columns"].as_array().unwrap().iter().position(|c| c["path"] == "rate").unwrap();
    assert_eq!(r["rows"][0]["cells"][rate], json!({"v": "0.5"}));
    assert_eq!(r["rows"][2]["cells"][rate], Json::Null);
    assert_eq!(r["rows"][1]["new"], true);
}

#[test]
fn long_view_lists_values() {
    let v = view();
    let r = v.query("gear", &query(json!({})), 0, 100).unwrap();
    assert_eq!(r["long"], true, "layout of the gear table — long");
    assert!(r["rows"].as_array().unwrap().iter().all(|x| x["field"]["editable"] == true));
    let all = v.query("gear", &query(json!({"all_fields": true})), 0, 100).unwrap();
    assert!(all["total"].as_u64() > r["total"].as_u64());
    let wide = v.query("affix", &query(json!({"view": "long"})), 0, 100).unwrap();
    assert_eq!(wide["long"], true);
}

#[test]
fn search_filters_and_paging() {
    let v = view();
    let total = |q: Json| v.query("gear", &query(q), 0, 100).unwrap()["total"].as_u64().unwrap();
    assert_eq!(total(json!({"view": "wide", "search": "щит"})), 1);
    assert_eq!(total(json!({"view": "wide", "search": "ЩИТ"})), 1);
    assert_eq!(total(json!({"view": "wide", "search": "200"})), 1);
    assert_eq!(total(json!({"view": "wide", "search": "меч 100"})), 2);
    assert_eq!(total(json!({"view": "wide", "only_diff": true})), 3);

    // A field was searched for, not a row: its columns remain for all rows
    let by_field = v.query("gear", &query(json!({"view": "wide", "search": "durability_nope"})), 0, 10).unwrap();
    assert_eq!(by_field["total"], 0);
    let by_field = v.query("gear", &query(json!({"view": "wide", "search": "rate"})), 0, 10).unwrap();
    assert_eq!(by_field["total"], 3);
    assert_eq!(by_field["columns_matched"], true);
    assert_eq!(by_field["columns"].as_array().unwrap().len(), 1);

    let page = v.query("gear", &query(json!({"view": "wide"})), 1, 1).unwrap();
    assert_eq!(page["rows"].as_array().unwrap().len(), 1);
    assert_eq!(page["rows"][0]["row"], 1);
    assert!(page.get("columns").is_none(), "columns are needed only with the first page");
}

#[test]
fn sorting_is_numeric_with_empty_last() {
    let v = view();
    let first = v.query("gear", &query(json!({"view": "wide"})), 0, 10).unwrap();
    let col = first["columns"].as_array().unwrap().iter().find(|c| c["path"] == "attack.offset.min_value").unwrap()["id"].clone();
    let col = col.to_string();
    let rows = |desc: bool| -> Vec<u64> {
        let r = v.query("gear", &query(json!({"view": "wide", "sort": col, "desc": desc})), 0, 10).unwrap();
        r["rows"].as_array().unwrap().iter().map(|x| x["row"].as_u64().unwrap()).collect()
    };
    assert_eq!(rows(false), [1, 0, 2]);
    assert_eq!(rows(true), [0, 1, 2], "an empty row stays at the end");
}

#[test]
fn unknown_table_is_an_error() {
    assert!(view().query("nope", &query(json!({})), 0, 1).is_err());
}

#[test]
fn icon_names_are_whitelisted() {
    let dir = tmp("icons");
    let r = import(&fixtures().join("kit"), &dir, Limits::default()).unwrap();
    fs::create_dir_all(r.root.join("icons")).unwrap();
    fs::write(r.root.join("icons/100.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    let v = KitView::load(&r.root).unwrap();
    assert!(v.icon_path("100.png").is_some());
    for bad in ["../manifest.json", "..\\manifest.json", "nope.png", "100.jpg", "100"] {
        assert!(v.icon_path(bad).is_none(), "{bad}");
    }
    let row = &v.query("gear", &query(json!({"view": "wide"})), 0, 1).unwrap()["rows"][0];
    assert_eq!(row["icon"], "100.png");
}

#[test]
fn old_kit_without_catalog_opens_in_view() {
    let v = KitView::load(&fixtures().join("kit_nocat")).unwrap();
    assert_eq!(v.overview()["has_catalog"], false);
    assert_eq!(v.query("gear", &query(json!({})), 0, 10).unwrap()["total"], 1);
}

// ------------------------------------------------------------------------------------------- edits

fn set(v: &KitView, table: &str, row: usize, path: &str, new: &str) -> Json {
    v.set_values(&[(table.to_string(), row, path.to_string(), new.to_string())])
}

fn wide_cell(v: &KitView, table: &str, row: usize, path: &str) -> Json {
    let r = v.query(table, &query(json!({"view": "wide", "all_fields": true})), 0, 100).unwrap();
    let col = r["columns"].as_array().unwrap().iter().position(|c| c["path"] == path).unwrap();
    r["rows"].as_array().unwrap().iter().find(|x| x["row"] == row).unwrap()["cells"][col].clone()
}

#[test]
fn edits_overlay_the_table() {
    let v = view();
    let st = set(&v, "gear", 0, "power.fixed_value", "7");
    assert_eq!((st["count"].as_u64(), st["errors"].as_u64(), st["changed"].as_u64()), (Some(1), Some(0), Some(1)));
    assert_eq!(wide_cell(&v, "gear", 0, "power.fixed_value"), json!({"v": "7", "o": "10", "k": "5"}));

    // Editing a value back to the kit value removes the edit
    let st = set(&v, "gear", 0, "power.fixed_value", " 5.0 ");
    assert_eq!(st["count"], 0);
    assert_eq!(wide_cell(&v, "gear", 0, "power.fixed_value"), json!({"v": "5", "o": "10"}));

    // An empty value too
    set(&v, "gear", 0, "power.fixed_value", "7");
    assert_eq!(set(&v, "gear", 0, "power.fixed_value", "")["count"], 0);
}

#[test]
fn locked_cells_are_refused() {
    let v = view();
    for (table, row, path, code) in [("gear", 0, "name", "field_locked"), ("gear", 99, "rate", "row_missing"), ("nope", 0, "rate", "table_unknown"), ("info", 0, "count", "field_locked")] {
        let st = set(&v, table, row, path, "1");
        assert_eq!(st["count"], 0, "{table} {path}");
        assert_eq!(st["rejected"][0]["code"], code, "{table} {path}");
    }
}

#[test]
fn issues_are_marked_on_cells() {
    let v = view();
    let st = set(&v, "gear", 0, "power.fixed_value", "5.5");
    assert_eq!(st["errors"], 1);
    assert_eq!(wide_cell(&v, "gear", 0, "power.fixed_value")["e"], "new_not_integer");

    let st = set(&v, "gear", 0, "power.fixed_value", "20");
    assert_eq!((st["errors"].as_u64(), st["warnings"].as_u64()), (Some(0), Some(1)));
    assert_eq!(wide_cell(&v, "gear", 0, "power.fixed_value")["w"], "ratio_big");

    // Min/max pair: fixed by a second edit
    let st = set(&v, "gear", 0, "attack.offset.min_value", "30");
    assert_eq!(st["errors"], 1);
    let st = set(&v, "gear", 0, "attack.offset.max_value", "40");
    assert_eq!(st["errors"], 0);
}

#[test]
fn undo_and_redo() {
    let v = view();
    assert_eq!(v.status()["can_undo"], false);
    set(&v, "gear", 0, "rate", "0.6");
    v.set_values(&[("gear".into(), 0, "rate".into(), "0.7".into()), ("gear".into(), 0, "power.fixed_value".into(), "6".into())]);
    assert_eq!(v.status()["count"], 2);
    let st = v.undo();
    assert_eq!(st["count"], 1, "the batch is undone as a whole");
    assert_eq!(wide_cell(&v, "gear", 0, "rate")["v"], "0.6");
    assert_eq!(v.undo()["count"], 0);
    assert_eq!(v.status()["can_undo"], false);
    let st = v.redo();
    assert_eq!(st["count"], 1);
    assert_eq!(v.redo()["count"], 2);
    assert_eq!(v.status()["can_redo"], false);
    assert_eq!(wide_cell(&v, "gear", 0, "rate")["v"], "0.7");
    // A new edit clears redo
    v.undo();
    set(&v, "gear", 0, "count", "4");
    assert_eq!(v.status()["can_redo"], false);
    // Nothing changed: no undo step appears
    let before = v.status();
    set(&v, "gear", 0, "count", "4");
    assert_eq!(v.status(), before);
}

#[test]
fn only_mine_filter() {
    let v = view();
    let total = |v: &KitView, q: Json| v.query("gear", &query(q), 0, 100).unwrap()["total"].as_u64().unwrap();
    assert_eq!(total(&v, json!({"view": "wide", "only_mine": true})), 0);
    set(&v, "gear", 1, "attack.offset.max_value", "5");
    assert_eq!(total(&v, json!({"view": "wide", "only_mine": true})), 1, "the list is recounted after an edit");
    assert_eq!(total(&v, json!({"view": "long", "only_mine": true})), 1);
    v.undo();
    assert_eq!(total(&v, json!({"view": "wide", "only_mine": true})), 0);
}

#[test]
fn patch_passes_the_same_rules() {
    let v = view();
    assert_eq!(v.patch_json("a", "", "", "").unwrap_err().0, "patch_empty");
    set(&v, "gear", 0, "power.fixed_value", "5.5");
    assert_eq!(v.patch_json("a", "", "", "").unwrap_err().0, "patch_has_errors");
    set(&v, "gear", 0, "power.fixed_value", "6");
    set(&v, "gear", 2, "rate", "1,5");
    let p = v.patch_json("Ник", "зачем", "2026-11-06 12:00", "balance-studio/0").unwrap();
    assert_eq!(p["format"], "dbbalance-patch/1");
    assert_eq!(p["base"], "20260101-aaaaaaaa");
    assert_eq!(p["season"], Json::Null);
    assert_eq!(p["changes"][0], json!({"table": "gear", "row": 0, "key": "100", "title": "Меч", "path": "power.fixed_value", "old": "5", "new": "6"}));
    assert_eq!(p["changes"][1], json!({"table": "gear", "row": 2, "key": "200", "title": "Щит", "path": "rate", "old": "", "new": "1.5"}));
    let report = v.kit.check_patch(&Patch::from_json(&p).unwrap()).unwrap();
    assert!(report.ok(), "{:?}", report.errors);
}

fn patch_of(changes: Json, extra: Json) -> Patch {
    let mut p = json!({"format": "dbbalance-patch/1", "base": "20260101-aaaaaaaa", "season": null, "author": "llm", "comment": "из патча", "changes": changes});
    for (k, x) in extra.as_object().unwrap() {
        p[k] = x.clone();
    }
    Patch::from_json(&p).unwrap()
}

#[test]
fn import_applies_what_fits() {
    let v = view();
    let p = patch_of(
        json!([
            {"table": "gear", "row": 0, "key": "100", "path": "power.fixed_value", "old": "5", "new": "6"},
            {"table": "gear", "row": 0, "key": "100", "path": "rate", "old": "0.5", "new": "abc"},
            {"table": "gear", "row": 0, "key": "100", "path": "name", "old": "Sword", "new": "X"},
            {"table": "gear", "row": 0, "key": "100", "path": "count", "old": "999", "new": "4"},
            {"table": "nope", "row": 0, "path": "x", "old": "1", "new": "2"}
        ]),
        json!({}),
    );
    let r = v.import_patch(&p).unwrap();
    assert_eq!(r["applied"], 2);
    let codes: Vec<&str> = r["rejected"].as_array().unwrap().iter().map(|x| x["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["field_locked", "old_mismatch", "table_unknown"]);
    assert_eq!(r["errors"], 1, "a type error is visible on the cell, it can be fixed");
    assert_eq!(wide_cell(&v, "gear", 0, "rate")["e"], "new_not_number");
    assert_eq!(r["comment"], "из патча");
    assert_eq!(v.undo()["count"], 0, "the whole patch is undone in one step");
}

#[test]
fn import_refuses_foreign_header() {
    let v = view();
    assert_eq!(v.import_patch(&patch_of(json!([]), json!({"season": "dead-money"}))).unwrap_err().0, "patch_invalid");
    assert_eq!(v.import_patch(&patch_of(json!([]), json!({"format": "x/1"}))).unwrap_err().0, "patch_invalid");
    let other = v.import_patch(&patch_of(json!([]), json!({"base": "old"}))).unwrap();
    assert_eq!(other["patch_warnings"][0]["code"], "base_differs");
}

#[test]
fn draft_survives_reload() {
    let v = view();
    set(&v, "gear", 0, "power.fixed_value", "6");
    set(&v, "gear", 0, "rate", "abc");
    v.set_comment("черновик");
    let d = v.draft_json();
    assert_eq!(d["format"], "dbbalance-draft/1");

    let w = view();
    assert_eq!(w.load_draft(&d), 0);
    let st = w.status();
    assert_eq!((st["count"].as_u64(), st["errors"].as_u64(), st["comment"].as_str()), (Some(2), Some(1), Some("черновик")));
    assert_eq!(st["can_undo"], false);
    assert_eq!(wide_cell(&w, "gear", 0, "power.fixed_value")["k"], "5");
}

#[test]
fn draft_of_other_kit_or_changed_row_is_not_applied() {
    let v = view();
    set(&v, "gear", 0, "power.fixed_value", "6");
    let d = v.draft_json();

    let w = view();
    let mut other = d.clone();
    other["base"] = json!("other");
    w.load_draft(&other);
    assert_eq!(w.status()["count"], 0, "a draft of another version moves only through moving edits");

    let mut bad_hash = d.clone();
    bad_hash["changes"][0]["hash"] = json!("0000");
    assert_eq!(w.load_draft(&bad_hash), 1);
    let mut bad_old = d.clone();
    bad_old["changes"][0]["old"] = json!("100");
    assert_eq!(w.load_draft(&bad_old), 1);
    assert_eq!(w.status()["count"], 0);
}

#[test]
fn row_hash_ignores_editable_values_only() {
    // The hash does not change with editable values: in the draft it is compared with the kit, not with the edit
    let v = view();
    let a = v.draft_json();
    assert_eq!(a["changes"].as_array().unwrap().len(), 0);
    set(&v, "gear", 0, "rate", "0.7");
    let h1 = v.draft_json()["changes"][0]["hash"].clone();
    set(&v, "gear", 0, "rate", "0.9");
    assert_eq!(v.draft_json()["changes"][0]["hash"], h1);
    assert_eq!(v.draft_json()["changes"][0]["n"], 0);
    set(&v, "gear", 1, "attack.offset.max_value", "5");
    let changes = v.draft_json()["changes"].clone();
    assert_eq!(changes[0]["n"], 0);
    assert_eq!(changes[1]["n"], 1, "the second row with the same key");
    assert_ne!(changes[0]["hash"], changes[1]["hash"]);
}

// ------------------------------------------------------------------------------------------- screens

fn screens() -> KitView {
    KitView::load(&fixtures().join("kit_screens")).unwrap()
}

#[test]
fn features_follow_the_tables() {
    assert_eq!(screens().overview()["features"], json!({"items": true, "curves": true, "loot": false}));
    assert_eq!(view().overview()["features"], json!({"items": false, "curves": false, "loot": false}));
    assert!(view().get_item("100", "", false).is_err());
    assert!(view().class_curves("", false).is_err());
}

#[test]
fn item_list_filters() {
    let v = screens();
    let n = |q: Json| v.list_items(&q)["items"].as_array().unwrap().len();
    assert_eq!(n(json!({})), 3, "a potion without properties is an item too, the service entry 999 without a name is not");
    assert_eq!(n(json!({"search": "щит"})), 1);
    assert_eq!(n(json!({"kind": "EQUIP_WEAPON"})), 1);
    assert_eq!(n(json!({"rarity": "NORMAL"})), 1);
    assert_eq!(n(json!({"only_mine": true})), 0);
    set(&v, "gear_template", 0, "durability", "120");
    assert_eq!(n(json!({"only_mine": true})), 1, "a durability edit also counts as an item edit");
    assert_eq!(v.list_items(&json!({}))["rarities"][0]["id"], "NORMAL", "rarities in order");
}

#[test]
fn item_card_gathers_everything() {
    let v = screens();
    let it = v.get_item("100", "", false).unwrap();
    assert_eq!((it["title"].as_str(), it["rarity"].as_str(), it["kind"].as_str()), (Some("Меч"), Some("UNCOMMON"), Some("EQUIP_WEAPON")));

    let attack = it["base"].as_array().unwrap().iter().find(|p| p["attr"] == "attack").unwrap();
    assert_eq!(attack["label"], "Урон");
    assert_eq!(attack["min"]["cell"]["v"], "10");
    assert_eq!(attack["roll"]["values"], json!(["10", "12", "14"]));
    let crit = it["base"].as_array().unwrap().iter().find(|p| p["attr"] == "crit").unwrap();
    assert_eq!((crit["fixed"]["cell"]["v"].as_str(), crit["fixed"]["row_unit"].as_str()), (Some("0.05"), Some("доля")));

    assert_eq!(it["durability"]["cell"]["v"], "100");
    assert_eq!(it["market"][0]["product"], "5001");
    let price = it["market"][0]["fields"].as_array().unwrap().iter().find(|f| f["path"].as_str().unwrap().ends_with("cost_price")).unwrap();
    assert_eq!((price["cell"]["v"].as_str(), price["currency"].as_str()), (Some("50"), Some("Золото")));

    let affix = &it["affix"];
    assert_eq!((affix["group"].as_str(), affix["shared"].as_u64(), affix["rarity"].as_str()), (Some("101"), Some(2), Some("UNCOMMON")));
    assert_eq!(affix["rows"][0]["title"], "Сила");
    assert_eq!(affix["rows"][0]["major"]["min"]["cell"]["v"], "12", "item rarity: 4 × 3");
    assert_eq!(affix["rows"][0]["major"]["roll"]["count"], 7, "12…18, step 1");
    assert_eq!(affix["rows"][1]["unit"], "доля");

    let normal = v.get_item("100", "NORMAL", false).unwrap();
    assert_eq!(normal["affix"]["rows"][0]["major"]["min"]["cell"]["v"], "8", "another rarity: 4 × 2");
    assert!(v.get_item("998", "", false).is_err());

    // Durability is not set: the field exists and is empty, it can be set. No product
    let shield = v.get_item("200", "", false).unwrap();
    assert_eq!(shield["durability"]["cell"], Json::Null);
    assert_eq!(shield["durability"]["editable"], true);
    assert_eq!(shield["market"].as_array().unwrap().len(), 0);
}

#[test]
fn item_card_shows_edits_and_new_roll() {
    let v = screens();
    let path = "data.fixed_rule[0].attribute.attack.offset.max_value";
    set(&v, "affix_template", 0, path, "20");
    let it = v.get_item("100", "", false).unwrap();
    let attack = it["base"].as_array().unwrap().iter().find(|p| p["attr"] == "attack").unwrap();
    assert_eq!((attack["max"]["cell"]["v"].as_str(), attack["max"]["cell"]["k"].as_str()), (Some("20"), Some("14")));
    assert_eq!(attack["roll"]["count"], 6, "10…20, step 2");
    let bad = set(&v, "affix_template", 0, path, "1");
    assert_eq!(bad["errors"], 1);
    let it = v.get_item("100", "", false).unwrap();
    let attack = it["base"].as_array().unwrap().iter().find(|p| p["attr"] == "attack").unwrap();
    assert_eq!(attack["max"]["cell"]["e"], "min_gt_max");
    assert_eq!(attack["roll"]["count"], Json::Null);
}

#[test]
fn labels_name_the_rarity_and_the_affix_kind() {
    let v = screens();
    let r = v.query("affix_value_template", &query(json!({"view": "wide"})), 0, 1).unwrap();
    let col = r["columns"].as_array().unwrap().iter().find(|c| c["path"] == "data.affix_data[1].rarity_range[3].max_value").unwrap();
    assert_eq!(col["label"], "макс. · Редкое · дополн.");
}

#[test]
fn goods_without_an_item_name_use_the_catalog() {
    let v = screens();
    let r = v.query("market_product_item", &query(json!({"view": "wide"})), 0, 10).unwrap();
    assert_eq!(r["rows"][0]["title"], "Меч");
    assert_eq!(r["rows"][1]["title"], "Набор");
}

#[test]
fn class_curves_skip_service_and_single_level() {
    let v = screens();
    let c = v.class_curves("", false).unwrap();
    let ids: Vec<&str> = c["classes"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["400001", "400002"]);
    assert_eq!(c["path"], "role_attribute.strength.fixed_value", "the attribute that changes comes first");
    assert_eq!(c["attrs"][0]["varies"], true);
    assert_eq!(c["attrs"].as_array().unwrap().last().unwrap()["varies"], false);
    let f = &c["classes"][0];
    assert_eq!(f["title"], "Боец");
    assert_eq!(f["levels"][2]["level"].as_f64(), Some(3.0));
    assert_eq!(f["levels"][2]["ref"]["cell"]["v"], "13");

    set(&v, "role_level_upgrade", 2, "role_attribute.strength.fixed_value", "15");
    let c = v.class_curves("role_attribute.strength.fixed_value", false).unwrap();
    assert_eq!(c["classes"][0]["levels"][1]["ref"]["cell"], json!({"v": "15", "k": "12"}));
    let crit = v.class_curves("role_attribute.crit.fixed_value", false).unwrap();
    assert_eq!(crit["classes"][0]["levels"][0]["ref"]["cell"]["v"], "0.1");
}

fn bulk(extra: Json) -> BulkSpec {
    let mut s = json!({"table": "affix_value_template", "query": {"view": "wide"}, "paths": ["data.affix_data[0].rarity_range[2].max_value"], "op": "mul", "value": 1.5});
    for (k, x) in extra.as_object().unwrap() {
        s[k] = x.clone();
    }
    BulkSpec::from_json(&s).unwrap()
}

#[test]
fn bulk_preview_changes_nothing() {
    let v = screens();
    let p = v.preview_bulk(&bulk(json!({}))).unwrap();
    assert_eq!(p["total"], 2);
    assert_eq!((p["changes"][0]["old"].as_str(), p["changes"][0]["new"].as_str()), (Some("12"), Some("18")));
    assert_eq!(p["changes"][1]["new"], "0.24", "a fraction without the noise of addition");
    assert_eq!(v.status()["count"], 0);
    assert_eq!(v.status()["can_undo"], false);
}

#[test]
fn bulk_apply_is_one_undo_step() {
    let v = screens();
    let st = v.apply_bulk(&bulk(json!({}))).unwrap();
    assert_eq!((st["changed"].as_u64(), st["count"].as_u64()), (Some(2), Some(2)));
    assert_eq!(v.undo()["count"], 0);
    assert_eq!(v.status()["can_undo"], false);
}

#[test]
fn bulk_operations_and_percent() {
    let v = screens();
    let min = "data.affix_data[0].rarity_range[2].min_value";
    // Strength: points, Chance: fraction. The increment and the value for a fraction are in percentage points
    let p = v.preview_bulk(&bulk(json!({"op": "add", "value": 5, "paths": [min]}))).unwrap();
    assert_eq!((p["changes"][0]["new"].as_str(), p["changes"][1]["new"].as_str()), (Some("13"), Some("0.15")));
    let p = v.preview_bulk(&bulk(json!({"op": "set", "value": 20, "paths": [min]}))).unwrap();
    assert_eq!((p["changes"][0]["new"].as_str(), p["changes"][1]["new"].as_str()), (Some("20"), Some("0.2")));
    assert!(p["errors"].as_u64().unwrap() >= 1, "20 is greater than max 12: {p}");
    // Integer fields are rounded
    let p = v.preview_bulk(&BulkSpec::from_json(&json!({"table": "market_product_item", "query": {"view": "wide"}, "paths": ["product.cost_options[0].cost_items[0].cost_price"], "op": "mul", "value": 0.8})).unwrap()).unwrap();
    assert_eq!((p["changes"][0]["new"].as_str(), p["changes"][1]["new"].as_str()), (Some("40"), Some("79")));
}

#[test]
fn bulk_pairs_move_together() {
    let v = screens();
    let min = "data.affix_data[0].rarity_range[2].min_value";
    let alone = v.preview_bulk(&bulk(json!({"paths": [min], "op": "mul", "value": 3}))).unwrap();
    assert_eq!(alone["total"], 2);
    assert!(alone["errors"].as_u64().unwrap() > 0, "min above max");
    let both = v.preview_bulk(&bulk(json!({"paths": [min], "op": "mul", "value": 3, "pairs": true}))).unwrap();
    assert_eq!(both["total"], 4);
    assert_eq!(both["errors"], 0, "{both}");
}

#[test]
fn bulk_respects_filter_selection_and_empty_cells() {
    let v = screens();
    let by_rows = v.preview_bulk(&bulk(json!({"rows": [1]}))).unwrap();
    assert_eq!(by_rows["total"], 1);
    assert_eq!(by_rows["changes"][0]["title"], "Шанс");
    let by_search = v.preview_bulk(&bulk(json!({"query": {"view": "wide", "search": "сила"}}))).unwrap();
    assert_eq!(by_search["total"], 1);
    // A bulk edit does not fill an empty cell
    let spec = BulkSpec::from_json(&json!({"table": "gear_template", "query": {"view": "wide"}, "paths": ["durability"], "op": "set", "value": 50})).unwrap();
    let p = v.preview_bulk(&spec).unwrap();
    assert_eq!((p["total"].as_u64(), p["skipped"].as_u64(), p["empty"].as_u64()), (Some(1), Some(0), Some(1)));
    // An already equal value is skipped too, the edit is computed from the current value
    set(&v, "gear_template", 0, "durability", "50");
    assert_eq!(v.preview_bulk(&spec).unwrap()["total"], 0);
    let twice = BulkSpec::from_json(&json!({"table": "gear_template", "query": {"view": "wide"}, "paths": ["durability"], "op": "mul", "value": 2})).unwrap();
    assert_eq!(v.preview_bulk(&twice).unwrap()["changes"][0]["new"], "100", "50 × 2, not 100 × 2");
}

#[test]
fn bulk_refuses_locked_and_bad_input() {
    let v = screens();
    assert!(v.preview_bulk(&bulk(json!({"paths": ["data.unit"]}))).is_err());
    assert!(v.preview_bulk(&bulk(json!({"paths": ["nope"]}))).is_err());
    assert!(v.preview_bulk(&bulk(json!({"table": "nope"}))).is_err());
    assert!(BulkSpec::from_json(&json!({"op": "pow", "value": 1})).is_err());
    assert!(BulkSpec::from_json(&json!({"op": "mul"})).is_err());
}

#[test]
fn bulk_fields_list_what_can_be_changed() {
    let v = screens();
    let f = v.bulk_fields("affix_value_template", &json!({"view": "wide"}), &json!(null)).unwrap();
    assert_eq!(f["rows"], 2);
    assert_eq!(f["fields"].as_array().unwrap().len(), 8);
    assert!(f["fields"].as_array().unwrap().iter().all(|x| x["editable"] == true));
    let one = v.bulk_fields("gear_template", &json!({"view": "wide"}), &json!([1])).unwrap();
    assert_eq!((one["rows"].as_u64(), one["fields"].as_array().unwrap().len()), (Some(1), 0), "row 1 has no durability");
}

// ------------------------------------------------------------------------------------------- moving to a new kit

const STRENGTH: &str = "role_attribute.strength.fixed_value";
const AFFIX_MAX: &str = "data.affix_data[0].rarity_range[2].max_value";

/// A copy of the kit with another version; `edit` changes the data files by table
fn next_kit(name: &str, edit: impl Fn(&str, String) -> String) -> PathBuf {
    let dir = tmp(name);
    let src = dir.join("kit");
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for e in fs::read_dir(from).unwrap().flatten() {
            if e.path().is_dir() {
                copy(&e.path(), &to.join(e.file_name()));
            } else {
                fs::copy(e.path(), to.join(e.file_name())).unwrap();
            }
        }
    }
    copy(&fixtures().join("kit_screens"), &src);
    let manifest = src.join("manifest.json");
    fs::write(&manifest, fs::read_to_string(&manifest).unwrap().replace("20260101-cccccccc", "20260202-dddddddd")).unwrap();
    for e in fs::read_dir(src.join("data")).unwrap().flatten() {
        let table = e.file_name().to_string_lossy().trim_end_matches(".csv").to_string();
        let text = fs::read_to_string(e.path()).unwrap();
        fs::write(e.path(), edit(&table, text)).unwrap();
    }
    src
}

/// Replaces the value of one path in one CSV row
fn replace_value(text: String, row: usize, path: &str, new: &str) -> String {
    let prefix = format!("{},", row);
    text.lines()
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            if l.trim_start_matches('\u{feff}').starts_with(&prefix) && f.get(3) == Some(&path) {
                format!("{},{},{},{},{},{}", f[0], f[1], f[2], f[3], new, f[5])
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// A draft of the old kit: Fighter, level 2 (row 2), strength 12 -> 15, and something else
fn old_draft(edits: &[(&str, usize, &str, &str)]) -> Json {
    let v = screens();
    for (table, row, path, new) in edits {
        let st = set(&v, table, *row, path, new);
        assert_eq!(st["rejected"].as_array().unwrap().len(), 0, "{st}");
    }
    v.draft_json()
}

fn ids(plan: &Json, group: &str) -> Vec<u64> {
    plan[group].as_array().unwrap().iter().map(|x| x["id"].as_u64().unwrap()).collect()
}

#[test]
fn unchanged_value_moves_by_itself() {
    let draft = old_draft(&[("role_level_upgrade", 2, STRENGTH, "15")]);
    let v = KitView::load(&next_kit("rb_same", |_, t| t)).unwrap();
    let plan = v.plan_rebase(&draft, false);
    assert_eq!(ids(&plan, "auto"), [0]);
    assert_eq!(plan["auto"][0]["title"], "Боец");
    assert_eq!(plan["auto"][0]["context"][0]["value"], "2", "it is clear which level this is");
    assert_eq!(v.status()["count"], 0, "the plan changes nothing");
    let r = v.apply_rebase(&draft, &json!({})).unwrap();
    assert_eq!((r["applied"].as_u64(), r["count"].as_u64()), (Some(1), Some(1)));
    assert_eq!(wide_cell(&v, "role_level_upgrade", 2, STRENGTH), json!({"v": "15", "k": "12"}));
    assert_eq!(v.undo()["count"], 0, "the whole move is undone in one step");
}

#[test]
fn row_number_shift_is_followed() {
    // The new kit got a new row before all rows: the numbers shifted by 1
    let draft = old_draft(&[("role_level_upgrade", 2, STRENGTH, "15")]);
    let path = next_kit("rb_shift", |table, t| {
        if table != "role_level_upgrade" {
            return t;
        }
        let mut out = vec!["row,key,title,path,value,name".to_string(), "0,300000,,role_type_id,300000,x".to_string(), format!("0,300000,,{},1,", STRENGTH)];
        for l in t.lines().skip(1) {
            let (n, rest) = l.split_once(',').unwrap();
            out.push(format!("{},{}", n.parse::<usize>().unwrap() + 1, rest));
        }
        out.join("\n") + "\n"
    });
    let v = KitView::load(&path).unwrap();
    assert_eq!(ids(&v.plan_rebase(&draft, false), "auto"), [0]);
    v.apply_rebase(&draft, &json!({})).unwrap();
    assert_eq!(wide_cell(&v, "role_level_upgrade", 3, STRENGTH), json!({"v": "15", "k": "12"}), "the edit moved together with the row");
    assert_eq!(wide_cell(&v, "role_level_upgrade", 2, STRENGTH)["v"], "11");
}

#[test]
fn insertion_inside_a_key_group_does_not_move_a_change_to_another_row() {
    // One key for many rows, the values match: by key and index the edit would land in the wrong place
    let draft = old_draft(&[("affix_value_template", 1, AFFIX_MAX, "0.2")]);
    let path = next_kit("rb_group", |table, t| {
        if table != "affix_value_template" {
            return t;
        }
        // A new row before "Шанс" with the same values but another attribute
        let lines: Vec<&str> = t.lines().collect();
        let mut out = vec![lines[0].to_string()];
        let row1: Vec<&str> = lines[1..].iter().copied().filter(|l| l.starts_with("1,")).collect();
        for l in lines[1..].iter().filter(|l| l.starts_with("0,")) {
            out.push(l.to_string());
        }
        for l in &row1 {
            out.push(l.replacen("1,", "1,", 1).replace(",attribute_type,2001,", ",attribute_type,3001,").replace("Шанс", "Новое"));
        }
        for l in &row1 {
            out.push(l.replacen("1,", "2,", 1));
        }
        out.join("\n") + "\n"
    });
    let v = KitView::load(&path).unwrap();
    let plan = v.plan_rebase(&draft, false);
    assert_eq!(ids(&plan, "auto"), [0], "{plan}");
    assert_eq!(plan["auto"][0]["row"], 2, "the edit stayed with its row");
    assert_eq!(plan["auto"][0]["title"], "Шанс");
    v.apply_rebase(&draft, &json!({})).unwrap();
    assert_eq!(wide_cell(&v, "affix_value_template", 1, AFFIX_MAX)["k"], Json::Null, "a foreign row was not touched");
    assert_eq!(wide_cell(&v, "affix_value_template", 2, AFFIX_MAX)["v"], "0.2");
}

#[test]
fn value_already_in_the_new_kit_needs_no_change() {
    let draft = old_draft(&[("gear_template", 0, "durability", "150")]);
    let v = KitView::load(&next_kit("rb_already", |table, t| if table == "gear_template" { replace_value(t, 0, "durability", "150") } else { t })).unwrap();
    let plan = v.plan_rebase(&draft, false);
    assert_eq!(ids(&plan, "already"), [0]);
    let r = v.apply_rebase(&draft, &json!({})).unwrap();
    assert_eq!((r["applied"].as_u64(), r["count"].as_u64()), (Some(0), Some(0)));
}

#[test]
fn changed_value_is_a_conflict_decided_by_the_person() {
    let draft = old_draft(&[("gear_template", 0, "durability", "150")]);
    let new_kit = || KitView::load(&next_kit("rb_conflict", |table, t| if table == "gear_template" { replace_value(t, 0, "durability", "120") } else { t })).unwrap();
    let v = new_kit();
    let plan = v.plan_rebase(&draft, false);
    assert_eq!(ids(&plan, "conflicts"), [0]);
    let c = &plan["conflicts"][0];
    assert_eq!((c["old"].as_str(), c["server"].as_str(), c["new"].as_str(), c["partial"].as_bool()), (Some("100"), Some("120"), Some("150"), Some(false)));

    assert!(v.apply_rebase(&draft, &json!({})).is_err(), "not moved without a decision");
    assert_eq!(v.status()["count"], 0);

    let mine = new_kit();
    mine.apply_rebase(&draft, &json!({"0": "mine"})).unwrap();
    assert_eq!(wide_cell(&mine, "gear_template", 0, "durability"), json!({"v": "150", "k": "120"}), "\"was\" in the patch will come from the new kit");

    let server = new_kit();
    let r = server.apply_rebase(&draft, &json!({"0": "server"})).unwrap();
    assert_eq!((r["applied"].as_u64(), r["count"].as_u64()), (Some(0), Some(0)));
}

#[test]
fn row_found_only_by_key_and_number_is_a_conflict() {
    // A non-editable value of the row changed: the hash does not match, only key and index remain
    let draft = old_draft(&[("gear_template", 0, "durability", "150")]);
    let v = KitView::load(&next_kit("rb_partial", |table, t| {
        if table == "gear_template" {
            t.replace("0,100,Меч,item_id,100,Меч\n", "0,100,Меч,item_id,100,Меч\n0,100,Меч,new_locked,7,\n")
        } else {
            t
        }
    }))
    .unwrap();
    let plan = v.plan_rebase(&draft, false);
    assert_eq!(ids(&plan, "conflicts"), [0], "{plan}");
    assert_eq!(plan["conflicts"][0]["partial"], true);
    assert_eq!(plan["auto"].as_array().unwrap().len(), 0, "not applied automatically, even if the value is the same");
}

#[test]
fn missing_row_field_or_table_is_lost_and_kept() {
    let draft = old_draft(&[
        ("gear_template", 0, "durability", "150"),
        ("market_product_item", 0, "product.cost_options[0].cost_items[0].cost_price", "60"),
        ("role_level_upgrade", 2, STRENGTH, "15"),
        ("role_level_upgrade", 5, STRENGTH, "30"),
    ]);
    // Class 400002 is gone, the merchant price became locked
    let path = next_kit("rb_lost", |table, t| if table == "role_level_upgrade" { t.lines().filter(|l| !l.contains(",400002,")).collect::<Vec<_>>().join("\n") + "\n" } else { t });
    let kit_json = path.join("kit.json");
    let text = fs::read_to_string(&kit_json).unwrap();
    let closed = text.replace("\"product.cost_options[*].cost_items[*].cost_price\",", "");
    assert_ne!(text, closed);
    fs::write(&kit_json, closed).unwrap();
    let v = KitView::load(&path).unwrap();
    let plan = v.plan_rebase(&draft, false);
    let reasons: Vec<&str> = plan["lost"].as_array().unwrap().iter().map(|x| x["reason"].as_str().unwrap()).collect();
    assert_eq!(reasons, ["field_gone", "row_gone"], "{plan}");
    assert_eq!(plan["lost"][1]["title"], "Жрец", "the name is taken from the draft");
    assert_eq!(ids(&plan, "auto").len(), 2);
    let r = v.apply_rebase(&draft, &json!({})).unwrap();
    assert_eq!(r["applied"], 2);
    assert_eq!(r["lost"].as_array().unwrap().len(), 2, "lost edits are returned so they stay in the old draft");
    assert_eq!(r["lost"][0]["new"], "60");

    // The table is gone entirely
    let mut other = draft.clone();
    other["changes"][0]["table"] = json!("nope");
    assert_eq!(v.plan_rebase(&other, false)["lost"][0]["reason"], "table_gone");
}

#[test]
fn comment_moves_with_the_changes() {
    let v = screens();
    set(&v, "gear_template", 0, "durability", "150");
    v.set_comment("зачем");
    let draft = v.draft_json();
    let w = KitView::load(&next_kit("rb_comment", |_, t| t)).unwrap();
    assert_eq!(w.apply_rebase(&draft, &json!({})).unwrap()["comment"], "зачем");
}

#[test]
fn only_a_newer_kit_of_the_same_season_takes_a_draft() {
    let d = |base: &str, season: Json, extra: Json| {
        let mut x = json!({"format": "dbbalance-draft/1", "base": base, "season": season, "comment": "", "changes": [{"table": "t"}]});
        for (k, v) in extra.as_object().unwrap() {
            x[k] = v.clone();
        }
        x
    };
    assert!(draft_fits(&d("20261104-aaaa", Json::Null, json!({})), "20261106-bbbb", None));
    assert!(draft_fits(&d("20261106-aaaa", Json::Null, json!({})), "20261106-bbbb", None), "same day, other data");
    assert!(draft_fits(&d("20261104-aaaa", json!("dead-money"), json!({})), "20261106-bbbb", Some("dead-money")));
    assert!(!draft_fits(&d("20261106-bbbb", Json::Null, json!({})), "20261106-bbbb", None), "the same version: this is its own draft");
    assert!(!draft_fits(&d("20261108-aaaa", Json::Null, json!({})), "20261106-bbbb", None), "not moved onto an older kit");
    assert!(!draft_fits(&d("20261104-aaaa", json!("dead-money"), json!({})), "20261106-bbbb", None), "another season");
    assert!(!draft_fits(&d("20261104-aaaa", Json::Null, json!({})), "20261106-bbbb", Some("dead-money")));
    assert!(!draft_fits(&d("20261104-aaaa", Json::Null, json!({"changes": []})), "20261106-bbbb", None), "empty");
    assert!(!draft_fits(&d("20261104-aaaa", Json::Null, json!({"rebased_to": "x"})), "20261106-bbbb", None), "leftovers of an already moved draft");
    assert!(!draft_fits(&d("20261104-aaaa", Json::Null, json!({"format": "x"})), "20261106-bbbb", None));
}

#[test]
fn overview_speaks_the_language_of_the_interface() {
    let dir = tmp("en_texts");
    let src = dir.join("kit");
    fs::create_dir_all(src.join("data")).unwrap();
    for f in ["manifest.json", "catalog.json"] {
        fs::copy(fixtures().join("kit_screens").join(f), src.join(f)).unwrap();
    }
    fs::write(src.join("data/gear_template.csv"), "row,key,title,path,value,name\n0,100,Меч,durability,100,\n").unwrap();
    fs::write(src.join("kit.json"), r#"{"title": "Баланс", "title_en": "Balance", "notes": ["заметка"], "notes_en": ["note"], "table": [
        {"name": "gear_template", "title": "Прочность", "title_en": "Durability", "about": "Описание", "about_en": "About", "where": "сервер", "where_en": "server", "layout": "wide", "icon": "", "editable": ["durability"]}]}"#).unwrap();
    let v = KitView::load(&src).unwrap();
    let ru = v.overview_in(false);
    assert_eq!((ru["title"].as_str(), ru["notes"][0].as_str(), ru["tables"][0]["title"].as_str()), (Some("Баланс"), Some("заметка"), Some("Прочность")));
    let en = v.overview_in(true);
    assert_eq!((en["title"].as_str(), en["notes"][0].as_str()), (Some("Balance"), Some("note")));
    assert_eq!((en["tables"][0]["title"].as_str(), en["tables"][0]["about"].as_str(), en["tables"][0]["where"].as_str()), (Some("Durability"), Some("About"), Some("server")));
    // No translation: an old kit shows the Russian texts in English mode, not emptiness
    let old = view().overview_in(true);
    assert_eq!(old["tables"][0]["title"], "Предметы");
}

// ------------------------------------------------------------------------------------------- economy

#[test]
fn item_card_has_the_economy_of_the_item() {
    let v = screens();
    let it = v.get_item("100", "", false).unwrap();
    assert_eq!(it["sell"]["cell"]["v"], "40");
    let trade = it["trade"].as_array().unwrap();
    assert_eq!(trade.len(), 2, "a row per market");
    assert_eq!(trade[0]["market"], "2");
    assert_eq!(trade[0]["fields"][0]["cell"]["v"], "900");
    assert_eq!((it["dismantle"][0]["cell"]["v"].as_str(), it["dismantle"][0]["item"].as_str()), (Some("10"), Some("Трава")));
    assert_eq!(it["craft"].as_array().unwrap().len(), 0, "the sword is not crafted");
}

#[test]
fn item_without_properties_has_a_card_too() {
    let v = screens();
    let it = v.get_item("300", "", false).unwrap();
    assert_eq!((it["title"].as_str(), it["kind"].as_str()), (Some("Зелье"), Some("ITEM_CONSUMABLE")));
    assert_eq!(it["base"].as_array().unwrap().len(), 0);
    assert!(it.get("affix").is_none());
    assert_eq!(it["sell"]["cell"]["v"], "5");
    let fields = it["craft"][0]["fields"].as_array().unwrap();
    assert_eq!(it["craft"][0]["recipe"], "101");
    let cost = fields.iter().find(|f| f["path"] == "item_cost[0].count").unwrap();
    assert_eq!((cost["cell"]["v"].as_str(), cost["item"].as_str()), (Some("3"), Some("Трава")));
}

#[test]
fn a_change_in_any_table_marks_the_item() {
    let v = screens();
    let mine = |v: &KitView, id: &str| v.list_items(&json!({}))["items"].as_array().unwrap().iter().find(|x| x["id"] == id).unwrap()["mine"].as_bool().unwrap();
    assert!(!mine(&v, "100"));
    set(&v, "trade_commodity", 0, "price_def", "950");
    assert!(mine(&v, "100"));
    v.undo();
    set(&v, "gear_dismantle", 0, "reward[0].count", "12");
    assert!(mine(&v, "100"));
    v.undo();
    assert!(!mine(&v, "300"));
    set(&v, "craft_table", 0, "product[0].count", "2");
    assert!(mine(&v, "300"), "a recipe marks what it gives");
}

#[test]
fn rows_of_maps_and_rarities_get_names() {
    let v = screens();
    let r = v.query("map_exp", &query(json!({})), 0, 10).unwrap();
    assert_eq!(r["rows"][0]["title"], "Склеп");
    let paths: Vec<&str> = r["columns"].as_array().unwrap().iter().map(|c| c["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["alive_time", "exp"], "the time tier is visible next to the experience");
    let en = v.query("map_exp", &query(json!({"lang": "en"})), 0, 10).unwrap();
    assert_eq!(en["rows"][0]["title"], "Sinner's End");
    let ins = v.query("gear_insurance_item", &query(json!({})), 0, 10).unwrap();
    assert_eq!(ins["rows"][0]["title"], "Редкое");
}

#[test]
fn tables_know_their_group() {
    let o = screens().overview();
    let group = |name: &str| o["tables"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap()["group"].clone();
    assert_eq!(group("item_type"), "economy");
    assert_eq!(group("map_exp"), "progress");
    assert_eq!(group("gear_template"), "", "old kits without groups");
}
