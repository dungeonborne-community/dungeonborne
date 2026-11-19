// CSV view of the tables for editing in Excel / LibreOffice.
//
// CSV row = table row. The column "<path>#name" next to id is a label (attribute/item/class…), skipped
// on import. Columns are field paths joined with dots (offset.min_value): single nested messages
// are spread into columns, lists and maps — one cell of JSON ([1,2,3], {"1001":{...}}). Empty cell = the field's
// default (0 / false / "" / none). Columns only for fields present in at least one row; a new field —
// add a column with its path by hand.
//
// By default the format suits Russian Excel: separator ';', decimal comma ',', UTF-8 with BOM.
// --plain: ',' and '.' (LibreOffice / VS Code). On import the separator is detected by the header, the decimal mark — any.
use crate::names::Names;
use crate::tables::{self, Schema};
use prost_reflect::{Kind, MessageDescriptor};
use serde_json::{Map, Value as Json};
use std::collections::HashSet;
use std::path::Path;

pub struct Format {
    pub delimiter: u8,
    pub decimal_comma: bool,
}

impl Format {
    pub fn excel_ru() -> Format {
        Format { delimiter: b';', decimal_comma: true }
    }
    pub fn plain() -> Format {
        Format { delimiter: b',', decimal_comma: false }
    }
}

// ---------- export ----------

/// Flat cells of a row: (path, value). An empty nested message stays as the cell "{}" — in proto3
/// the presence of a message matters, without this it would be lost.
fn flatten(obj: &Map<String, Json>, desc: &MessageDescriptor, prefix: &str, out: &mut Vec<(String, Json)>) {
    for (key, val) in obj {
        let path = format!("{}{}", prefix, key);
        let field = desc.get_field_by_name(key);
        match (field.as_ref().map(|f| (f.is_list(), f.is_map(), f.kind())), val) {
            (Some((false, false, Kind::Message(m))), Json::Object(inner)) if !inner.is_empty() => {
                flatten(inner, &m, &format!("{}.", path), out)
            }
            _ => out.push((path, val.clone())),
        }
    }
}

fn cell_text(v: &Json, fmt: &Format) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Bool(b) => b.to_string(),
        Json::Null => String::new(),
        Json::Number(n) => match n.as_f64() {
            // float: "29" instead of "29.0", "0,05" in the Russian format
            Some(f) if n.is_f64() => {
                let s = format!("{}", f);
                if fmt.decimal_comma {
                    s.replace('.', ",")
                } else {
                    s
                }
            }
            _ => n.to_string(),
        },
        other => other.to_string(), // list / map / message — compact JSON
    }
}

/// The last field name in the path ("a.b[3].attribute_type" -> "attribute_type")
fn leaf_field(path: &str) -> &str {
    let last = path.rsplit('.').next().unwrap_or(path);
    last.split('[').next().unwrap_or(last)
}

pub fn export_table(s: &Schema, json_path: &Path, csv_path: &Path, fmt: &Format, names: &Names) -> Result<usize, String> {
    let v = tables::read_json(json_path)?;
    let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or("unknown type")?;
    let rows = v["rows"].as_array().cloned().unwrap_or_default();

    let flat: Vec<Vec<(String, Json)>> = rows
        .iter()
        .map(|r| {
            let mut cells = Vec::new();
            if let Json::Object(o) = r {
                flatten(o, &desc, "", &mut cells);
            }
            cells
        })
        .collect();
    // columns in the order of first appearance (rows are serialized in schema field order)
    let mut columns: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for cells in &flat {
        for (p, _) in cells {
            if seen.insert(p.clone()) {
                columns.push(p.clone());
            }
        }
    }

    // Label column "<column>#name" right after the reference column (if at least one row got a name).
    // Columns with '#' are skipped on import.
    let cell = |cells: &Vec<(String, Json)>, c: &str| cells.iter().find(|(p, _)| p == c).map(|(_, v)| v.clone());
    let named: HashSet<String> = columns
        .iter()
        .filter(|c| flat.iter().any(|cells| cell(cells, c).and_then(|v| names.for_field(leaf_field(c), &v)).is_some()))
        .cloned()
        .collect();
    let mut header: Vec<String> = Vec::new();
    for c in &columns {
        header.push(c.clone());
        if named.contains(c) {
            header.push(format!("{}#name", c));
        }
    }
    let records: Vec<Vec<String>> = flat
        .iter()
        .map(|cells| {
            let mut rec = Vec::new();
            for c in &columns {
                let v = cell(cells, c);
                rec.push(v.as_ref().map(|v| cell_text(v, fmt)).unwrap_or_default());
                if named.contains(c) {
                    rec.push(v.and_then(|v| names.for_field(leaf_field(c), &v)).unwrap_or_default());
                }
            }
            rec
        })
        .collect();
    let header_refs: Vec<&str> = header.iter().map(|s| s.as_str()).collect();
    write_csv(csv_path, fmt, &header_refs, &records)?;
    Ok(rows.len())
}

// ---------- import ----------

fn parse_cell(kind: &Kind, is_list: bool, is_map: bool, text: &str) -> Result<Json, String> {
    if is_list || is_map {
        return serde_json::from_str(text).map_err(|e| format!("JSON expected: {}", e));
    }
    if let Kind::Message(m) = kind {
        // google.protobuf.Timestamp/Duration/wrappers in JSON — a string or a number ("2024-02-02T17:30:00Z"), not an object
        if m.full_name().starts_with("google.protobuf.") {
            return Ok(serde_json::from_str(text).unwrap_or_else(|_| Json::String(text.to_string())));
        }
        return serde_json::from_str(text).map_err(|e| format!("JSON expected: {}", e));
    }
    let num = || text.trim().replace(',', ".");
    match kind {
        Kind::Bool => match text.trim().to_lowercase().as_str() {
            "true" | "1" | "истина" => Ok(Json::Bool(true)),
            "false" | "0" | "ложь" => Ok(Json::Bool(false)),
            _ => Err("true/false expected".into()),
        },
        Kind::Float | Kind::Double => num()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .ok_or("a number expected".into()),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 | Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => {
            num().parse::<i64>().map(|n| Json::Number(n.into())).map_err(|_| "an integer expected".into())
        }
        Kind::Uint32 | Kind::Fixed32 | Kind::Uint64 | Kind::Fixed64 => {
            num().parse::<u64>().map(|n| Json::Number(n.into())).map_err(|_| "an integer ≥ 0 expected".into())
        }
        Kind::Enum(_) => Ok(match text.trim().parse::<i64>() {
            Ok(n) => Json::Number(n.into()),
            Err(_) => Json::String(text.trim().to_string()),
        }),
        _ => Ok(Json::String(text.to_string())), // string / bytes (base64)
    }
}

/// Puts a value at the path a.b.c, creating nested objects; the path is checked against the schema.
fn set_path(obj: &mut Map<String, Json>, desc: &MessageDescriptor, path: &[&str], text: &str) -> Result<(), String> {
    let field = desc.get_field_by_name(path[0]).ok_or(format!("no field '{}' in {}", path[0], desc.name()))?;
    if path.len() == 1 {
        let v = parse_cell(&field.kind(), field.is_list(), field.is_map(), text)?;
        obj.insert(path[0].to_string(), v);
        return Ok(());
    }
    let Kind::Message(m) = field.kind() else { return Err(format!("'{}' is not a message, a nested path is impossible", path[0])) };
    if field.is_list() || field.is_map() {
        return Err(format!("'{}' is a list/map, set as one JSON cell", path[0]));
    }
    let child = obj.entry(path[0].to_string()).or_insert_with(|| Json::Object(Map::new()));
    let Json::Object(child) = child else { return Err(format!("'{}' is set both by a cell and by columns", path[0])) };
    set_path(child, &m, &path[1..], text)
}

fn csv_reader(raw: &[u8]) -> csv::Reader<&[u8]> {
    let raw = raw.strip_prefix("\u{feff}".as_bytes()).unwrap_or(raw);
    let first_line = raw.split(|&b| b == b'\n').next().unwrap_or(&[]);
    let delimiter = if first_line.contains(&b';') { b';' } else { b',' };
    csv::ReaderBuilder::new().delimiter(delimiter).flexible(false).from_reader(raw)
}

fn write_csv(path: &Path, fmt: &Format, header: &[&str], records: &[Vec<String>]) -> Result<(), String> {
    let mut data = Vec::new();
    data.extend_from_slice("\u{feff}".as_bytes()); // BOM — otherwise Excel shows Cyrillic as mojibake
    {
        let mut w = csv::WriterBuilder::new().delimiter(fmt.delimiter).from_writer(&mut data);
        w.write_record(header).map_err(|e| e.to_string())?;
        for r in records {
            w.write_record(r).map_err(|e| e.to_string())?;
        }
        w.flush().map_err(|e| e.to_string())?;
    }
    std::fs::write(path, data).map_err(|e| e.to_string())
}

/// Replaces rows in balance\<table>.json if all rows pass the schema. Returns (rows, changed rows).
fn replace_rows(v: &mut Json, desc: &MessageDescriptor, rows: Vec<Json>, json_path: &Path, line_of: impl Fn(usize) -> String) -> Result<(usize, usize), Vec<String>> {
    let parse_all = |rs: &[Json]| -> Vec<Option<prost_reflect::DynamicMessage>> {
        rs.iter().map(|r| tables::row_from_json(desc, r).ok()).collect()
    };
    let new = parse_all(&rows);
    if let Some(bad) = new.iter().position(|r| r.is_none()) {
        let e = tables::row_from_json(desc, &rows[bad]).unwrap_err();
        return Err(vec![format!("{}: {}", line_of(bad), e)]);
    }
    let old = parse_all(v["rows"].as_array().map(|a| a.as_slice()).unwrap_or(&[]));
    let changed = (0..new.len().max(old.len())).filter(|&i| old.get(i).cloned().flatten() != new.get(i).cloned().flatten()).count();
    v["rows"] = Json::Array(rows);
    tables::write_json(json_path, v);
    Ok((new.len(), changed))
}

// ---------- long format: one CSV row = one value ----------
//
// row;key;title;path;value;name
// 0;101;Strength;data.affix_data[0].rarity_range[3].max_value;12;Rare
//
// row — table row number (from 0). key (first field of the row), title (row name) and name (names of keys/value
// in the path) — labels for filtering in Excel, not read on import (see names.rs).
// path: fields joined with '.', [i] — a list element, [key] — a map element. Empty message — value "{}".
// A table row whose fields are all default — one CSV row with an empty path.

fn walk_long(v: &Json, desc: &MessageDescriptor, prefix: &str, out: &mut Vec<(String, Json)>) {
    let Json::Object(obj) = v else { return };
    for (key, val) in obj {
        let path = format!("{}{}", prefix, key);
        let Some(field) = desc.get_field_by_name(key) else {
            out.push((path, val.clone()));
            continue;
        };
        // a nested message (not empty) — expand it, else a leaf
        let mut leaf_or_walk = |p: String, x: &Json, kind: Kind| match (kind, x) {
            (Kind::Message(m), Json::Object(o)) if !o.is_empty() => walk_long(x, &m, &format!("{}.", p), out),
            _ => out.push((p, x.clone())),
        };
        if field.is_list() {
            if let Json::Array(arr) = val {
                for (i, x) in arr.iter().enumerate() {
                    leaf_or_walk(format!("{}[{}]", path, i), x, field.kind());
                }
            }
        } else if field.is_map() {
            let vk = field.kind().as_message().unwrap().map_entry_value_field().kind();
            if let Json::Object(map) = val {
                for (k, x) in map {
                    leaf_or_walk(format!("{}[{}]", path, k), x, vk.clone());
                }
            }
        } else {
            leaf_or_walk(path, val, field.kind());
        }
    }
}

/// Label of a table row: the first top-level reference field that got a name
/// (affix_value_template: attribute_type 1001 -> first_level_strength)
fn row_title(r: &Json, names: &Names) -> String {
    let Json::Object(o) = r else { return String::new() };
    o.iter().find_map(|(k, v)| names.for_field(k, v)).unwrap_or_default()
}

/// Table (JSON from balance\ or the original) -> long-form values, as in the kit's `data\<table>.csv` (without labels)
pub fn long_values(s: &Schema, v: &Json) -> Result<Vec<balance_kit::Value>, String> {
    let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or("unknown type")?;
    let fmt = Format::plain();
    let mut out = Vec::new();
    for (i, r) in v["rows"].as_array().into_iter().flatten().enumerate() {
        let key = match r {
            Json::Object(o) => o.values().next().map(|x| cell_text(x, &fmt)).unwrap_or_default(),
            _ => String::new(),
        };
        let mut leaves = Vec::new();
        walk_long(r, &desc, "", &mut leaves);
        let mut push = |path: String, value: String| out.push(balance_kit::Value { row: i, key: key.clone(), title: String::new(), path, value, name: String::new() });
        if leaves.is_empty() {
            push(String::new(), String::new());
        }
        for (p, x) in leaves {
            push(p, cell_text(&x, &fmt));
        }
    }
    Ok(out)
}

fn field_kind(kind: &Kind) -> balance_kit::FieldKind {
    use balance_kit::FieldKind as F;
    match kind {
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => F::Int32,
        Kind::Uint32 | Kind::Fixed32 => F::Uint32,
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => F::Int64,
        Kind::Uint64 | Kind::Fixed64 => F::Uint64,
        Kind::Float => F::Float,
        Kind::Double => F::Double,
        Kind::Bool => F::Bool,
        Kind::Enum(_) => F::Enum,
        _ => F::Text,
    }
}

/// Types of editable fields by schema: path with [*] -> type. The walk goes only where the path can still match `editable`,
/// recursive messages are not unfolded.
pub fn field_kinds(desc: &MessageDescriptor, editable: &[String]) -> std::collections::BTreeMap<String, balance_kit::FieldKind> {
    /// Whether anything under `prefix` may match the pattern: compare up to the first '*'
    fn fits(prefix: &str, pattern: &str) -> bool {
        let lit = pattern.split('*').next().unwrap_or("");
        let n = lit.len().min(prefix.len());
        lit.as_bytes()[..n] == prefix.as_bytes()[..n]
    }
    fn walk(desc: &MessageDescriptor, prefix: &str, editable: &[String], stack: &mut Vec<String>, out: &mut std::collections::BTreeMap<String, balance_kit::FieldKind>) {
        if stack.iter().any(|n| n == desc.full_name()) {
            return;
        }
        stack.push(desc.full_name().to_string());
        for f in desc.fields() {
            let mut path = format!("{}{}", prefix, f.name());
            let kind = if f.is_map() {
                path.push_str("[*]");
                f.kind().as_message().unwrap().map_entry_value_field().kind()
            } else {
                if f.is_list() {
                    path.push_str("[*]");
                }
                f.kind()
            };
            if !editable.iter().any(|p| fits(&path, p)) {
                continue;
            }
            match &kind {
                Kind::Message(m) if !m.full_name().starts_with("google.protobuf.") => walk(m, &format!("{}.", path), editable, stack, out),
                other => {
                    if balance_kit::editable(editable, &path) {
                        out.insert(path, field_kind(other));
                    }
                }
            }
        }
        stack.pop();
    }
    let mut out = Default::default();
    walk(desc, "", editable, &mut Vec::new(), &mut out);
    out
}

/// Long-form columns. title and name are labels, not read on import.
const LONG_HEADER: [&str; 6] = ["row", "key", "title", "path", "value", "name"];

/// Ready values (loot template) -> long form. Returns (rows, values)
pub fn write_values(csv_path: &Path, fmt: &Format, values: &[balance_kit::Value]) -> Result<(usize, usize), String> {
    let records: Vec<Vec<String>> = values.iter().map(|v| vec![v.row.to_string(), v.key.clone(), v.title.clone(), v.path.clone(), v.value.clone(), v.name.clone()]).collect();
    write_csv(csv_path, fmt, &LONG_HEADER, &records)?;
    Ok((values.iter().map(|v| v.row + 1).max().unwrap_or(0), values.len()))
}

pub fn export_long(s: &Schema, json_path: &Path, csv_path: &Path, fmt: &Format, names: &Names) -> Result<(usize, usize), String> {
    let v = tables::read_json(json_path)?;
    let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or("unknown type")?;
    let rows = v["rows"].as_array().cloned().unwrap_or_default();
    let mut records = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let key = match r {
            Json::Object(o) => o.values().next().map(|x| cell_text(x, fmt)).unwrap_or_default(),
            _ => String::new(),
        };
        let title = row_title(r, names);
        let mut leaves = Vec::new();
        walk_long(r, &desc, "", &mut leaves);
        if leaves.is_empty() {
            records.push(vec![i.to_string(), key.clone(), title.clone(), String::new(), String::new(), String::new()]);
        }
        for (p, x) in leaves {
            let name = names.for_path(&p, &x);
            records.push(vec![i.to_string(), key.clone(), title.clone(), p, cell_text(&x, fmt), name]);
        }
    }
    write_csv(csv_path, fmt, &LONG_HEADER, &records)?;
    Ok((rows.len(), records.len()))
}

enum Tok<'a> {
    Field(&'a str),
    Index(&'a str),
}

/// "a.b[0].c[3].d" -> Field(a) Field(b) Index(0) Field(c) Index(3) Field(d)
fn tokenize(path: &str) -> Result<Vec<Tok<'_>>, String> {
    let mut out = Vec::new();
    for seg in path.split('.') {
        let (name, mut rest) = match seg.find('[') {
            Some(p) => (&seg[..p], &seg[p..]),
            None => (seg, ""),
        };
        if name.is_empty() {
            return Err(format!("empty field name in '{}'", path));
        }
        out.push(Tok::Field(name));
        while !rest.is_empty() {
            let end = rest.find(']').ok_or(format!("no ']' in '{}'", path))?;
            out.push(Tok::Index(&rest[1..end]));
            rest = &rest[end + 1..];
        }
    }
    Ok(out)
}

fn set_long(obj: &mut Map<String, Json>, desc: &MessageDescriptor, toks: &[Tok], text: &str) -> Result<(), String> {
    let Tok::Field(name) = toks[0] else { return Err("a field name expected".into()) };
    let field = desc.get_field_by_name(name).ok_or(format!("no field '{}' in {}", name, desc.name()))?;
    let rest = &toks[1..];

    // the next step after choosing an element: a leaf or a nested message
    fn place(slot: &mut Json, kind: Kind, rest: &[Tok], text: &str) -> Result<(), String> {
        if rest.is_empty() {
            *slot = parse_cell(&kind, false, false, text)?;
            return Ok(());
        }
        let Kind::Message(m) = kind else { return Err("the path continues after a scalar field".into()) };
        if !slot.is_object() {
            *slot = Json::Object(Map::new());
        }
        set_long(slot.as_object_mut().unwrap(), &m, rest, text)
    }

    if field.is_list() || field.is_map() {
        let Some(Tok::Index(idx)) = rest.first() else {
            return Err(format!("'{}' is a {}, [index] required", name, if field.is_map() { "map" } else { "list" }));
        };
        if field.is_list() {
            let i: usize = idx.parse().map_err(|_| format!("list index '{}' is not a number", idx))?;
            let arr = obj.entry(name.to_string()).or_insert_with(|| Json::Array(Vec::new()));
            let Json::Array(arr) = arr else { return Err(format!("'{}' is not a list", name)) };
            if arr.len() <= i {
                arr.resize(i + 1, Json::Null);
            }
            place(&mut arr[i], field.kind(), &rest[1..], text)
        } else {
            let vk = field.kind().as_message().unwrap().map_entry_value_field().kind();
            let map = obj.entry(name.to_string()).or_insert_with(|| Json::Object(Map::new()));
            let Json::Object(map) = map else { return Err(format!("'{}' is not a map", name)) };
            let slot = map.entry(idx.to_string()).or_insert(Json::Null);
            place(slot, vk, &rest[1..], text)
        }
    } else {
        let slot = obj.entry(name.to_string()).or_insert(Json::Null);
        place(slot, field.kind(), rest, text)
    }
}

/// Skipped list elements (there is [2], no [1]) stay null — that is an error.
fn find_null(v: &Json, path: &str) -> Option<String> {
    match v {
        Json::Null => Some(path.to_string()),
        Json::Array(a) => a.iter().enumerate().find_map(|(i, x)| find_null(x, &format!("{}[{}]", path, i))),
        Json::Object(o) => o.iter().find_map(|(k, x)| find_null(x, &if path.is_empty() { k.clone() } else { format!("{}.{}", path, k) })),
        _ => None,
    }
}

pub fn import_long(s: &Schema, csv_path: &Path, json_path: &Path) -> Result<(usize, usize), Vec<String>> {
    let raw = std::fs::read(csv_path).map_err(|e| vec![e.to_string()])?;
    let mut v = tables::read_json(json_path).map_err(|e| vec![e])?;
    let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or(vec!["unknown type".to_string()])?;
    let mut rdr = csv_reader(&raw);
    let headers: Vec<String> = rdr.headers().map_err(|e| vec![e.to_string()])?.iter().map(|h| h.trim().to_lowercase()).collect();
    let col = |n: &str| headers.iter().position(|h| h == n).ok_or(vec![format!("no column '{}'", n)]);
    let (c_row, c_path, c_value) = (col("row")?, col("path")?, col("value")?);

    let mut rows: Vec<Map<String, Json>> = Vec::new();
    let mut first_line: Vec<usize> = Vec::new(); // for error messages: the CSV line where the table row started
    let mut errors = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let line = i + 2;
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("CSV line {}: {}", line, e));
                continue;
            }
        };
        let Ok(row) = rec.get(c_row).unwrap_or("").trim().parse::<usize>() else {
            errors.push(format!("CSV line {}: row is not a number", line));
            continue;
        };
        if rows.len() <= row {
            rows.resize(row + 1, Map::new());
            first_line.resize(row + 1, line);
        }
        let path = rec.get(c_path).unwrap_or("").trim();
        let text = rec.get(c_value).unwrap_or("");
        if path.is_empty() || text.is_empty() {
            continue; // marker of an empty table row / a default value
        }
        if let Err(e) = tokenize(path).and_then(|t| set_long(&mut rows[row], &desc, &t, text)) {
            errors.push(format!("CSV line {} (row {}, {}): {} (cell: {})", line, row, path, e, text));
        }
    }
    for (i, r) in rows.iter().enumerate() {
        if let Some(p) = find_null(&Json::Object(r.clone()), "") {
            errors.push(format!("row {}: no value for {} (a list element skipped?)", i, p));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let rows: Vec<Json> = rows.into_iter().map(Json::Object).collect();
    replace_rows(&mut v, &desc, rows, json_path, |i| format!("row {} (CSV line {})", i, first_line.get(i).copied().unwrap_or(0)))
}

/// CSV -> rows in balance\<table>.json (type and header are kept). Returns (rows, changed rows).
pub fn import_table(s: &Schema, csv_path: &Path, json_path: &Path) -> Result<(usize, usize), Vec<String>> {
    let raw = std::fs::read(csv_path).map_err(|e| vec![e.to_string()])?;
    let mut v = tables::read_json(json_path).map_err(|e| vec![e])?;
    let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or(vec!["unknown type".to_string()])?;

    let mut rdr = csv_reader(&raw);
    let headers: Vec<String> = rdr.headers().map_err(|e| vec![e.to_string()])?.iter().map(|h| h.trim().to_string()).collect();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("CSV line {}: {}", i + 2, e));
                continue;
            }
        };
        let mut obj = Map::new();
        for (col, text) in headers.iter().zip(rec.iter()) {
            // "#name" — labels, read-only
            if text.is_empty() || col.is_empty() || col.contains('#') {
                continue;
            }
            let path: Vec<&str> = col.split('.').collect();
            if let Err(e) = set_path(&mut obj, &desc, &path, text) {
                errors.push(format!("CSV line {}, column {}: {} (cell: {})", i + 2, col, e, text));
            }
        }
        rows.push(Json::Object(obj));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    replace_rows(&mut v, &desc, rows, json_path, |i| format!("CSV line {}", i + 2))
}

// ---------- balance kit patch (balance_kit, `cargo xtask balance patch`) ----------
//
// A change = (table row, long-form path, was, becomes). "Was" is checked against the current value: if the table
// has been changed meanwhile — a conflict, the edit is not applied. The table changes whole or not at all.

pub struct Change {
    pub row: usize,
    /// first field of the row (like the long-form `key`): rows have shifted — a conflict
    pub key: String,
    pub path: String,
    pub old: String,
    pub new: String,
}

/// Current value at a long-form path ("" — the field's default / no such field)
pub fn value_at(row: &Json, desc: &MessageDescriptor, path: &str) -> String {
    let mut leaves = Vec::new();
    walk_long(row, desc, "", &mut leaves);
    leaves.into_iter().find(|(p, _)| p == path).map(|(_, v)| cell_text(&v, &Format::plain())).unwrap_or_default()
}

/// Equal values: text or numbers (100 = 100.0, 0,5 = 0.5)
pub fn same_value(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a == b {
        return true;
    }
    match (a.replace(',', ".").parse::<f64>(), b.replace(',', ".").parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        _ => false,
    }
}

/// Applies the changes to balance\<table>.json. Ok(changed values) — written (if `write`); Err — nothing.
pub fn apply_changes(s: &Schema, json_path: &Path, changes: &[Change], write: bool) -> Result<usize, Vec<String>> {
    let mut v = tables::read_json(json_path).map_err(|e| vec![e])?;
    let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or(vec!["unknown type".to_string()])?;
    let mut rows = v["rows"].as_array().cloned().unwrap_or_default();
    let mut errors = Vec::new();
    let mut applied = 0;
    for c in changes {
        let at = format!("row {} ({}) {}", c.row, c.key, c.path);
        let Some(row) = rows.get_mut(c.row) else {
            errors.push(format!("{}: no such row (the table has {})", at, v["rows"].as_array().map_or(0, |a| a.len())));
            continue;
        };
        let key = match &*row {
            Json::Object(o) => o.values().next().map(|x| cell_text(x, &Format::plain())).unwrap_or_default(),
            _ => String::new(),
        };
        if !c.key.is_empty() && !same_value(&key, &c.key) {
            errors.push(format!("{}: the row now has {} — the table rows have shifted", at, key));
            continue;
        }
        let cur = value_at(row, &desc, &c.path);
        if !same_value(&cur, &c.old) {
            errors.push(format!("{}: was {}, but now {} — the table has already been changed", at, c.old, if cur.is_empty() { "(empty)" } else { &cur }));
            continue;
        }
        if c.new.trim().is_empty() {
            errors.push(format!("{}: empty new value", at));
            continue;
        }
        if same_value(&cur, &c.new) {
            continue;
        }
        let Json::Object(obj) = row else {
            errors.push(format!("{}: the row is not an object", at));
            continue;
        };
        if let Err(e) = tokenize(&c.path).and_then(|t| set_long(obj, &desc, &t, c.new.trim())) {
            errors.push(format!("{}: {} (value {})", at, e, c.new));
            continue;
        }
        if let Err(e) = tables::row_from_json(&desc, row) {
            errors.push(format!("{}: does not pass the schema: {}", at, e));
            continue;
        }
        applied += 1;
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if write && applied > 0 {
        v["rows"] = Json::Array(rows);
        tables::write_json(json_path, &v);
    }
    Ok(applied)
}
