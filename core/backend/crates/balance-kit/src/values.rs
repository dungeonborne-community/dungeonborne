//! Long-form values and comparison rules: as in `balance_kit/tools/kitlib.py`.

use std::collections::BTreeMap;

/// One table value (a row of `data/<table>.csv`). An empty `path` — a row whose fields are all default.
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub row: usize,
    pub key: String,
    pub title: String,
    pub path: String,
    pub value: String,
    pub name: String,
}

/// Field type by the game schema: `catalog.json` → `field_types` (kit) or the schema itself (server).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Int32,
    Uint32,
    Int64,
    Uint64,
    Float,
    Double,
    Bool,
    Enum,
    Text,
}

impl FieldKind {
    pub fn name(self) -> &'static str {
        match self {
            FieldKind::Int32 => "int32",
            FieldKind::Uint32 => "uint32",
            FieldKind::Int64 => "int64",
            FieldKind::Uint64 => "uint64",
            FieldKind::Float => "float",
            FieldKind::Double => "double",
            FieldKind::Bool => "bool",
            FieldKind::Enum => "enum",
            FieldKind::Text => "string",
        }
    }

    pub fn from_name(name: &str) -> Option<FieldKind> {
        Some(match name {
            "int32" => FieldKind::Int32,
            "uint32" => FieldKind::Uint32,
            "int64" => FieldKind::Int64,
            "uint64" => FieldKind::Uint64,
            "float" => FieldKind::Float,
            "double" => FieldKind::Double,
            "bool" => FieldKind::Bool,
            "enum" => FieldKind::Enum,
            "string" => FieldKind::Text,
            _ => return None,
        })
    }

    pub fn is_integer(self) -> bool {
        matches!(self, FieldKind::Int32 | FieldKind::Uint32 | FieldKind::Int64 | FieldKind::Uint64)
    }

    pub fn is_number(self) -> bool {
        self.is_integer() || matches!(self, FieldKind::Float | FieldKind::Double)
    }

    /// Bounds of an integer type
    pub fn int_range(self) -> Option<(i128, i128)> {
        match self {
            FieldKind::Int32 => Some((i32::MIN as i128, i32::MAX as i128)),
            FieldKind::Uint32 => Some((0, u32::MAX as i128)),
            FieldKind::Int64 => Some((i64::MIN as i128, i64::MAX as i128)),
            FieldKind::Uint64 => Some((0, u64::MAX as i128)),
            _ => None,
        }
    }
}

/// '*' — any characters (including '.', '[', ']'); the rest literally
pub fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            mark = ti;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

pub fn editable(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|p| wildcard(p, path))
}

/// A finite number written as `[+-]digits[.digits][e[+-]digits]`, comma as a dot. `NaN`, `inf`, `1_000` are not numbers.
pub fn as_number(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - frac;
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

pub fn is_int_text(text: &str) -> bool {
    let t = text.trim();
    let digits = t.strip_prefix('-').unwrap_or(t);
    !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit())
}

/// Equal values: text or numbers (100 = 100.0, 0,5 = 0.5)
pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a == b {
        return true;
    }
    match (as_number(a), as_number(b)) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        _ => false,
    }
}

/// Path without indices: data.affix_data[0].rarity_range[3].max_value -> data.affix_data[*].rarity_range[*].max_value
pub fn generic(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']').map(|c| open + c) else { break };
        let inner = &rest[open + 1..close];
        out.push_str(&rest[..=open]);
        if !inner.is_empty() && inner.bytes().all(|c| c.is_ascii_digit()) {
            out.push('*');
        } else {
            out.push_str(inner);
        }
        out.push(']');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// generic path -> (min, max) over numeric values
pub fn ranges(values: &[Value]) -> BTreeMap<String, (f64, f64)> {
    let mut out: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for v in values.iter().filter(|v| !v.path.is_empty()) {
        if let Some(n) = as_number(&v.value) {
            let e = out.entry(generic(&v.path)).or_insert((n, n));
            e.0 = e.0.min(n);
            e.1 = e.1.max(n);
        }
    }
    out
}

pub fn fmt_range(r: (f64, f64)) -> String {
    if r.0 == r.1 {
        format!("{}", r.0)
    } else {
        format!("{}…{}", r.0, r.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_paths() {
        assert!(wildcard("role_attribute.*", "role_attribute.first_level_strength.fixed_value"));
        assert!(wildcard("data.affix_data[*].rarity_range[*].max_value", "data.affix_data[0].rarity_range[3].max_value"));
        assert!(!wildcard("data.affix_data[*].rarity_range[*].max_value", "data.affix_data[0].rarity_range[3].min_value"));
        assert!(!wildcard("offset.min_value", "offset.ability_set_id"));
        assert!(wildcard("durability", "durability"));
    }

    #[test]
    fn numbers() {
        for ok in ["1", "-1", "+1", "1.5", ".5", "1.", "1,5", " 2 ", "1e5", "1E-5"] {
            assert!(as_number(ok).is_some(), "{ok}");
        }
        for bad in ["", "NaN", "nan", "inf", "-inf", "Infinity", "1_000", "1e", "e5", ".", "-", "1.2.3", "1e999", "١"] {
            assert!(as_number(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn integers() {
        assert!(is_int_text("12") && is_int_text("-12") && is_int_text(" 12 "));
        assert!(!is_int_text("") && !is_int_text("-") && !is_int_text("+1") && !is_int_text("1.0"));
    }

    #[test]
    fn same_values() {
        assert!(same("100", "100.0") && same("0,5", "0.5") && same("a", " a "));
        assert!(!same("1", "2") && !same("a", "b") && !same("", "0"));
    }

    #[test]
    fn generic_paths() {
        assert_eq!(generic("a[0].b[12].c"), "a[*].b[*].c");
        assert_eq!(generic("a[x].b"), "a[x].b");
        assert_eq!(generic("plain"), "plain");
    }
}
