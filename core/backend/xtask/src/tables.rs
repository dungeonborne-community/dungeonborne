// xresloader tables (.bytes) <-> balance\*.json by the Configure.pb schema.
//
// .bytes format (org.xresloader.pb.xresloader_datablocks): header{xres_ver, data_ver, count, hash_code="sha256:…",
// data_source…} + repeated bytes data_block (rows) + data_message_type (full name of the row type).
// hash_code = sha256 of the concatenated data_block (verified: 103/103 game originals rebuild byte for byte).
use prost::Message;
use prost_reflect::{
    DescriptorPool, DeserializeOptions, DynamicMessage, Kind, MessageDescriptor, ReflectMessage, SerializeOptions, Value,
};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Schema {
    pub pool: DescriptorPool,
    blocks: MessageDescriptor,
}

impl Schema {
    pub fn load() -> Schema {
        let pool = DescriptorPool::decode(std::fs::read(crate::ws::SCHEMA).expect(crate::ws::SCHEMA).as_slice()).unwrap();
        let blocks = pool
            .all_messages()
            .find(|m| m.name() == "xresloader_datablocks")
            .expect("xresloader_datablocks in Configure.pb");
        Schema { pool, blocks }
    }

    pub fn row_type(&self, name: &str) -> Option<MessageDescriptor> {
        self.pool.get_message_by_name(name)
    }
}

/// A table in memory: row type, header (as is) and rows.
pub struct Table {
    pub row_type: MessageDescriptor,
    pub header: DynamicMessage,
    pub rows: Vec<DynamicMessage>,
}

pub fn decode_table(s: &Schema, bytes: &[u8]) -> Result<(Table, Vec<Vec<u8>>), String> {
    let blocks = DynamicMessage::decode(s.blocks.clone(), bytes).map_err(|e| e.to_string())?;
    let type_name = blocks.get_field_by_name("data_message_type").unwrap().as_str().unwrap_or("").to_string();
    let row_type = s.row_type(&type_name).ok_or(format!("type {} not found in the schema", type_name))?;
    let header = blocks.get_field_by_name("header").unwrap().as_message().unwrap().clone();
    let raw: Vec<Vec<u8>> = blocks
        .get_field_by_name("data_block")
        .unwrap()
        .as_list()
        .unwrap()
        .iter()
        .map(|v| v.as_bytes().unwrap().to_vec())
        .collect();
    let mut rows = Vec::with_capacity(raw.len());
    for (i, r) in raw.iter().enumerate() {
        rows.push(DynamicMessage::decode(row_type.clone(), r.as_slice()).map_err(|e| format!("row {}: {}", i, e))?);
    }
    Ok((Table { row_type, header, rows }, raw))
}

/// Rebuilds the file. `prev` — the previous version of the table (rows + their source bytes): an unchanged row with the same
/// index is written with the source bytes (otherwise map fields are re-encoded in another key order and the file changes without edits).
/// The table did not change at all — the header as is (for the service UeSource_* the hash is not computed by our formula).
pub fn encode_table(s: &Schema, t: &Table, prev: Option<(&[DynamicMessage], &[Vec<u8>])>) -> Vec<u8> {
    let raw: Vec<Vec<u8>> = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| match prev {
            Some((rows, bytes)) if rows.get(i) == Some(r) => bytes[i].clone(),
            _ => r.encode_to_vec(),
        })
        .collect();
    let mut header = t.header.clone();
    let unchanged = matches!(prev, Some((rows, _)) if rows == t.rows.as_slice());
    if !unchanged {
        let mut sha = Sha256::new();
        for r in &raw {
            sha.update(r);
        }
        header.set_field_by_name("count", Value::U32(raw.len() as u32));
        header.set_field_by_name("hash_code", Value::String(format!("sha256:{:x}", sha.finalize())));
    }
    let mut blocks = DynamicMessage::new(s.blocks.clone());
    blocks.set_field_by_name("header", Value::Message(header));
    blocks.set_field_by_name("data_block", Value::List(raw.into_iter().map(|b| Value::Bytes(b.into())).collect()));
    blocks.set_field_by_name("data_message_type", Value::String(t.row_type.full_name().to_string()));
    blocks.encode_to_vec()
}

// ---------- JSON ----------

pub fn to_json(m: &DynamicMessage) -> Json {
    let opts = SerializeOptions::new().use_proto_field_name(true).stringify_64_bit_integers(false);
    let mut v = m.serialize_with_options(serde_json::value::Serializer, &opts).unwrap();
    short_floats(&mut v, &m.descriptor());
    v
}

/// float (f32) in JSON — the shortest form that reads back into the same f32 (0.1 instead of 0.10000000149011612).
fn short_floats(v: &mut Json, desc: &MessageDescriptor) {
    let Json::Object(obj) = v else { return };
    for (key, val) in obj.iter_mut() {
        let Some(field) = desc.get_field_by_name(key).or_else(|| desc.get_field_by_json_name(key)) else { continue };
        let fix = |x: &mut Json, kind: &Kind| match kind {
            Kind::Float => {
                if let Some(n) = x.as_f64().and_then(|f| format!("{}", f as f32).parse::<f64>().ok()) {
                    if let Some(num) = serde_json::Number::from_f64(n) {
                        *x = Json::Number(num);
                    }
                }
            }
            Kind::Message(m) => short_floats(x, m),
            _ => {}
        };
        if field.is_map() {
            if let (Json::Object(map), Kind::Message(entry)) = (val, field.kind()) {
                let vk = entry.map_entry_value_field().kind();
                map.values_mut().for_each(|x| fix(x, &vk));
                // prost-reflect yields a map in HashMap order (random) — sort the keys (numbers by value),
                // so the export is stable and readable (rarity 1,2,3… and not 1,2,4,7,5…)
                let mut entries: Vec<(String, Json)> = std::mem::take(map).into_iter().collect();
                entries.sort_by(|(a, _), (b, _)| match (a.parse::<i64>(), b.parse::<i64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => a.cmp(b),
                });
                map.extend(entries);
            }
        } else if field.is_list() {
            if let Json::Array(arr) = val {
                arr.iter_mut().for_each(|x| fix(x, &field.kind()));
            }
        } else {
            fix(val, &field.kind());
        }
    }
}

pub fn row_from_json(desc: &MessageDescriptor, v: &Json) -> Result<DynamicMessage, String> {
    DynamicMessage::deserialize_with_options(desc.clone(), v, &DeserializeOptions::new().deny_unknown_fields(true))
        .map_err(|e| e.to_string())
}

/// Row key for error messages: the first field (usually id) as is.
fn row_label(v: &Json) -> String {
    match v {
        Json::Object(o) => o.iter().next().map(|(k, x)| format!("{}={}", k, x)).unwrap_or_default(),
        _ => String::new(),
    }
}

pub fn table_to_json(t: &Table) -> Json {
    serde_json::json!({
        "type": t.row_type.full_name(),
        "header": to_json(&t.header),
        "rows": t.rows.iter().map(to_json).collect::<Vec<_>>(),
    })
}

pub fn table_from_json(s: &Schema, name: &str, v: &Json) -> Result<Table, Vec<String>> {
    let type_name = v["type"].as_str().ok_or(vec![format!("{}: no \"type\" field", name)])?;
    let row_type = s.row_type(type_name).ok_or(vec![format!("{}: type {} not found", name, type_name)])?;
    let header_desc = s.blocks.get_field_by_name("header").unwrap().kind().as_message().unwrap().clone();
    let header = row_from_json(&header_desc, &v["header"]).map_err(|e| vec![format!("{}: header: {}", name, e)])?;
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (i, r) in v["rows"].as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().enumerate() {
        match row_from_json(&row_type, r) {
            Ok(m) => rows.push(m),
            Err(e) => errors.push(format!("{}: row #{} ({}): {}", name, i, row_label(r), e)),
        }
    }
    if errors.is_empty() {
        Ok(Table { row_type, header, rows })
    } else {
        Err(errors)
    }
}

pub fn read_json(path: &Path) -> Result<Json, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    // Notepad / PowerShell 5 save UTF-8 with BOM — serde_json does not accept it
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text).map_err(|e| format!("JSON: {}", e))
}

pub fn write_json(path: &Path, v: &Json) {
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

// ---------- commands ----------

pub fn list_tables(dir: &str, ext: &str) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {}", dir, e)).flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some(ext) {
            out.insert(p.file_stem().unwrap().to_string_lossy().to_string(), p);
        }
    }
    out
}

pub fn export(s: &Schema, src: &str, out: &str) {
    std::fs::create_dir_all(out).unwrap();
    for (name, path) in list_tables(src, "bytes") {
        match decode_table(s, &std::fs::read(&path).unwrap()) {
            Ok((t, _)) => {
                write_json(&Path::new(out).join(format!("{}.json", name)), &table_to_json(&t));
                println!("  {:<40} {:>6} rows  {}", name, t.rows.len(), t.row_type.full_name());
            }
            Err(e) => println!("  {:<40} ERROR: {}", name, e),
        }
    }
}

pub fn verify(s: &Schema, src: &str) -> bool {
    let (mut ok, mut sem, mut bad) = (0, 0, 0);
    for (name, path) in list_tables(src, "bytes") {
        let orig = std::fs::read(&path).unwrap();
        let (t, orig_raw) = match decode_table(s, &orig) {
            Ok(x) => x,
            Err(e) => {
                println!("  {:<40} unreadable: {}", name, e);
                bad += 1;
                continue;
            }
        };
        // the full path through JSON, as in real editing
        let back = match table_from_json(s, &name, &table_to_json(&t)) {
            Ok(x) => x,
            Err(errs) => {
                println!("  {:<40} JSON does not build: {}", name, errs[0]);
                bad += 1;
                continue;
            }
        };
        let rebuilt = encode_table(s, &back, Some((&t.rows, &orig_raw)));
        if rebuilt == orig {
            ok += 1;
        } else if back.rows == t.rows {
            sem += 1; // same rows, the wrapper differs (field order of the service UeSource_*)
        } else {
            println!("  {:<40} DIFFERS after round-trip", name);
            bad += 1;
        }
    }
    println!("verify {}: byte for byte {}, by meaning {}, errors {}", src, ok, sem, bad);
    bad == 0
}

/// balance\*.json -> server\*.bytes (only changed ones) and pak_dir\*.bytes (those differing from the game originals).
/// Returns the list of tables for the client pak; on errors in JSON — Err with the list of errors.
pub fn build(s: &Schema, src: &str, server: &str, orig: &str, pak_dir: &Path) -> Result<Vec<String>, Vec<String>> {
    std::fs::create_dir_all(pak_dir).unwrap();
    let originals = list_tables(orig, "bytes");
    let mut errors = Vec::new();
    let (mut built, mut changed_server, mut client) = (0, Vec::new(), Vec::new());
    for (name, path) in list_tables(src, "json") {
        let t = match read_json(&path).map_err(|e| vec![format!("{}: {}", name, e)]).and_then(|v| table_from_json(s, &name, &v)) {
            Ok(t) => t,
            Err(mut e) => {
                errors.append(&mut e);
                continue;
            }
        };
        let server_path = Path::new(server).join(format!("{}.bytes", name));
        let prev = std::fs::read(&server_path).ok().and_then(|p| decode_table(s, &p).ok());
        let same_as_prev = prev.as_ref().map(|(p, _)| p.rows == t.rows).unwrap_or(false);
        // unchanged — the pak gets the same bytes as the server (otherwise the header would come from JSON with the old hash_code)
        let bytes = match (same_as_prev, std::fs::read(&server_path)) {
            (true, Ok(b)) => b,
            _ => encode_table(s, &t, prev.as_ref().map(|(p, raw)| (p.rows.as_slice(), raw.as_slice()))),
        };
        if !same_as_prev {
            std::fs::write(&server_path, &bytes).unwrap();
            changed_server.push(name.clone());
        }
        built += 1;
        if let Some(op) = originals.get(&name) {
            let same_as_orig = decode_table(s, &std::fs::read(op).unwrap()).map(|(o, _)| o.rows == t.rows).unwrap_or(false);
            if !same_as_orig {
                std::fs::write(pak_dir.join(format!("{}.bytes", name)), &bytes).unwrap();
                client.push(name.clone());
            }
        }
    }
    println!("built {} tables; changed on the server: {}", built, join_or_dash(&changed_server));
    println!("into the client pak (differ from the game original): {}", join_or_dash(&client));
    if errors.is_empty() {
        Ok(client)
    } else {
        Err(errors)
    }
}

fn join_or_dash(v: &[String]) -> String {
    if v.is_empty() {
        "—".into()
    } else {
        v.join(", ")
    }
}
