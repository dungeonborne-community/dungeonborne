//! Reading a kit folder: `manifest.json`, `kit.json`, the optional `catalog.json`, `data/`, `raw/`.
//! `tools/`, `llm/` and the book are not read or run here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use crate::check::{check, Expect, Report, TableData};
use crate::patch::Patch;
use crate::values::{FieldKind, Value};

pub const KIT_FORMAT: &str = "dbbalance-kit/1";
pub const CATALOG_FORMAT: &str = "dbbalance-catalog/1";

pub struct Kit {
    pub root: PathBuf,
    pub manifest: Json,
    pub config: Json,
    /// `None` — an old kit, without `catalog.json`
    pub catalog: Option<Json>,
}

fn read_json(path: &Path) -> Result<Json, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {}", path.display(), e))?;
    serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(text)).map_err(|e| format!("{}: {}", path.display(), e))
}

/// Long form of a table: `row,key,title,path,value,name`
pub(crate) fn read_values(path: &Path) -> Result<Vec<Value>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let body = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    let mut reader = csv::Reader::from_reader(body);
    let headers = reader.headers().map_err(|e| format!("{}: {}", path.display(), e))?.clone();
    let col = |name: &str| headers.iter().position(|h| h == name).ok_or(format!("{}: no column {}", path.display(), name));
    let (row, key, title, p, value, name) = (col("row")?, col("key")?, col("title")?, col("path")?, col("value")?, col("name")?);
    let mut out = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(|e| format!("{}: {}", path.display(), e))?;
        let get = |i: usize| rec.get(i).unwrap_or("").to_string();
        out.push(Value {
            row: rec.get(row).unwrap_or("").parse().map_err(|_| format!("{}: row is not a number: {:?}", path.display(), rec.get(row)))?,
            key: get(key),
            title: get(title),
            path: get(p),
            value: get(value),
            name: get(name),
        });
    }
    Ok(out)
}

impl Kit {
    pub fn open(root: &Path) -> Result<Kit, String> {
        let manifest_path = root.join("manifest.json");
        if !manifest_path.exists() {
            return Err(format!("does not look like a balance kit: no {}", manifest_path.display()));
        }
        let manifest = read_json(&manifest_path)?;
        if manifest["format"].as_str() != Some(KIT_FORMAT) {
            return Err(format!("kit format {:?}, {} required", manifest["format"], KIT_FORMAT));
        }
        let config = read_json(&root.join("kit.json"))?;
        // The table name becomes a file name: it must not lead outside the kit
        for t in config["table"].as_array().into_iter().flatten() {
            let name = t["name"].as_str().unwrap_or_default();
            if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                return Err(format!("invalid table name: {:?}", t["name"]));
            }
        }
        let catalog_path = root.join("catalog.json");
        let catalog = catalog_path.exists().then(|| read_json(&catalog_path)).transpose()?;
        if let Some(c) = &catalog {
            if c["format"].as_str() != Some(CATALOG_FORMAT) {
                return Err(format!("catalog format {:?}, {} required", c["format"], CATALOG_FORMAT));
            }
        }
        Ok(Kit { root: root.to_path_buf(), manifest, config, catalog })
    }

    pub fn version(&self) -> &str {
        self.manifest["version"].as_str().unwrap_or_default()
    }

    /// Season of the kit; `None` — the server base
    pub fn season(&self) -> Option<&str> {
        self.manifest["season"].as_str()
    }

    pub fn table_names(&self) -> Vec<String> {
        self.config["table"].as_array().into_iter().flatten().filter_map(|t| t["name"].as_str().map(str::to_string)).collect()
    }

    fn table_config(&self, name: &str) -> Option<&Json> {
        self.config["table"].as_array()?.iter().find(|t| t["name"].as_str() == Some(name))
    }

    pub fn table_data(&self, name: &str) -> Result<Option<TableData>, String> {
        let Some(cfg) = self.table_config(name) else { return Ok(None) };
        let orig_path = self.root.join("data").join(format!("{}.orig.csv", name));
        let raw_path = self.root.join("raw").join(format!("{}.json", name));
        let kinds = self
            .catalog
            .as_ref()
            .and_then(|c| c["field_types"][name].as_object())
            .map(|m| m.iter().filter_map(|(p, k)| FieldKind::from_name(k.as_str()?).map(|k| (p.clone(), k))).collect())
            .unwrap_or_default();
        Ok(Some(TableData {
            editable: cfg["editable"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect(),
            values: read_values(&self.root.join("data").join(format!("{}.csv", name)))?,
            orig: orig_path.exists().then(|| read_values(&orig_path)).transpose()?,
            kinds,
            raw_rows: raw_path.exists().then(|| read_json(&raw_path)).transpose()?.and_then(|v| v["rows"].as_array().cloned()),
            ..TableData::default()
        }
        .with_rules(cfg)))
    }

    /// Patch check against the kit: the same as the server does, plus the version and season check
    pub fn check_patch(&self, patch: &Patch) -> Result<Report, String> {
        let mut tables = BTreeMap::new();
        for c in &patch.changes {
            if !tables.contains_key(&c.table) {
                if let Some(t) = self.table_data(&c.table)? {
                    tables.insert(c.table.clone(), t);
                }
            }
        }
        Ok(check(patch, &tables, &Expect { version: Some(self.version()), season: Some(self.season()) }))
    }
}
