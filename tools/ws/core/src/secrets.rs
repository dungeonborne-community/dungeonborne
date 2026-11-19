//! ws secrets — secrets per the secrets.toml spec (dbsecrets/1, docs/SECRETS.md).
//! No crypto of our own: the default store is a KeePass database (.kdbx, the keepass crate), the env adapter is for CI and
//! servers (a value or a file path in the environment variable named in the spec).
//! An existing database is only read: ws never overwrites it (init creates a new one where there is no file).
use crate::{str_list, Finding, Level, Workspace};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Secret {
    pub name: String,
    pub group: String,
    pub title: String,
    /// "value" | "file"
    pub kind: String,
    /// value: entry field (Password by default)
    pub field: String,
    /// file: attachment name
    pub file: String,
    pub env: String,
    pub used_by: Vec<String>,
    pub description: String,
    pub required: bool,
    pub rotate: bool,
    /// who may receive it: owner | team (no field — owner)
    pub visibility: String,
    /// goes into a vault session (ws secrets unlock, vault.rs): only value kind, never owner
    pub session: bool,
    /// the commands the session gives it to ("ws dump" = `ws dump …` / `cargo ws dump …`)
    pub session_for: Vec<String>,
}

pub struct Manifest {
    pub db_name: String,
    pub secrets: Vec<Secret>,
}

/// Access tiers: public ⊂ team ⊂ owner (like member visibility, visibility.rs).
pub const TIERS: [&str; 3] = ["public", "team", "owner"];

fn rank(tier: &str) -> usize {
    TIERS.iter().position(|t| *t == tier).unwrap_or(2)
}

impl Secret {
    /// The entry belongs in a database of tier `tier` (a team database holds team and public, owner — everything).
    pub fn allowed(&self, tier: &str) -> bool {
        rank(&self.visibility) <= rank(tier)
    }
}

/// Database tier of this machine: local.toml [vault] tier (team — a developer machine with the team database), none — owner.
pub fn local_tier(ws: &Workspace) -> Result<String, String> {
    let t = ws
        .local_str("vault", "tier")
        .unwrap_or_else(|| "owner".into());
    if !TIERS.contains(&t.as_str()) {
        return Err(format!(
            "local.toml [vault] tier = '{t}': must be one of {}",
            TIERS.join(", ")
        ));
    }
    Ok(t)
}

pub fn manifest(ws: &Workspace) -> Result<Manifest, String> {
    let doc = crate::read_toml(&ws.config("secrets.toml"))?;
    let s = |v: &toml::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let mut secrets = Vec::new();
    for v in doc
        .get("secret")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
    {
        let kind = s(v, "kind");
        if kind != "value" && kind != "file" {
            return Err(format!(
                "secrets.toml: {} - kind must be value or file",
                s(v, "name")
            ));
        }
        let session = v.get("session").and_then(|x| x.as_bool()).unwrap_or(false);
        let session_for = str_list(v.get("session_for"));
        let visibility = v
            .get("visibility")
            .and_then(|x| x.as_str())
            .unwrap_or("owner")
            .to_string();
        if session {
            let why = if kind != "value" {
                Some("only value secrets go into a vault session")
            } else if visibility == "owner" {
                Some("owner secrets never go into a vault session (prod, signing): they always ask for the password")
            } else if session_for.is_empty() {
                Some("session = true needs session_for = [\"ws <command>\", ...]")
            } else {
                None
            };
            if let Some(why) = why {
                return Err(format!("secrets.toml: {} - {why}", s(v, "name")));
            }
        }
        secrets.push(Secret {
            name: s(v, "name"),
            group: s(v, "group"),
            title: s(v, "title"),
            field: if s(v, "field").is_empty() {
                "Password".into()
            } else {
                s(v, "field")
            },
            file: s(v, "file"),
            env: s(v, "env"),
            used_by: str_list(v.get("used_by")),
            description: s(v, "description"),
            required: v.get("required").and_then(|x| x.as_bool()).unwrap_or(true),
            rotate: v.get("rotate").and_then(|x| x.as_bool()).unwrap_or(false),
            visibility,
            session,
            session_for,
            kind,
        });
    }
    let db_name = doc
        .get("vault")
        .and_then(|v| v.get("db"))
        .and_then(|x| x.as_str())
        .unwrap_or("secrets.kdbx")
        .to_string();
    Ok(Manifest { db_name, secrets })
}

/// Database folder: local.toml [vault] path.
pub fn vault_dir(ws: &Workspace) -> Result<PathBuf, String> {
    ws.local_str("vault", "path")
        .and_then(|p| ws.resolve(&p))
        .ok_or_else(|| "no local.toml [vault] path (vault folder)".into())
}

/// Database path: --db, else local.toml [vault] path + database name (local.toml [vault] db, else [vault] db of the spec).
pub fn db_path(ws: &Workspace, m: &Manifest, explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(PathBuf::from(p));
    }
    let name = ws
        .local_str("vault", "db")
        .unwrap_or_else(|| m.db_name.clone());
    Ok(vault_dir(ws)?.join(name))
}

/// Default name of an exported database: team.kdbx, public.kdbx.
pub fn export_name(tier: &str) -> String {
    format!("{tier}.kdbx")
}

/// Database password: from the console without echo, or as one line from stdin (--password-stdin — for DevKit and tests).
pub fn read_password(from_stdin: bool, prompt: &str) -> Result<String, String> {
    if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_string());
    }
    rpassword::prompt_password(prompt).map_err(|e| format!("password: {e}"))
}

fn key(password: &str) -> keepass::DatabaseKey {
    keepass::DatabaseKey::new().with_password(password)
}

/// Source of values: a KeePass database or environment variables.
pub enum Source {
    Kdbx(Box<keepass::Database>),
    Env,
    /// values handed out by the vault session (vault.rs): name -> value
    Session(Vec<(String, String)>),
}

impl Source {
    pub fn open_kdbx(path: &Path, password: &str) -> Result<Source, String> {
        let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let db = keepass::Database::open(&mut f, key(password))
            .map_err(|e| format!("{}: cannot open ({e})", path.display()))?;
        Ok(Source::Kdbx(Box::new(db)))
    }

    /// Value (value) or file contents (file). None — no entry/field/attachment, or empty.
    pub fn get(&self, s: &Secret) -> Option<Vec<u8>> {
        match self {
            Source::Session(v) => v
                .iter()
                .find(|(n, _)| n == &s.name)
                .map(|(_, x)| x.clone().into_bytes())
                .filter(|x| !x.is_empty()),
            Source::Env => {
                let v = std::env::var(&s.env).ok().filter(|v| !v.is_empty())?;
                if s.kind == "file" {
                    std::fs::read(v).ok().filter(|b| !b.is_empty())
                } else {
                    Some(v.into_bytes())
                }
            }
            Source::Kdbx(db) => {
                let root = db.root();
                let group = if s.group.is_empty() {
                    Some(root)
                } else {
                    root.group_by_path(&[s.group.as_str()])
                }?;
                let entry = group.entry_by_name(&s.title)?;
                if s.kind == "file" {
                    let a = entry.attachment_by_name(&s.file)?;
                    let data = a.data.get().clone();
                    (!data.is_empty()).then_some(data)
                } else {
                    entry
                        .get(&s.field)
                        .filter(|v| !v.is_empty())
                        .map(|v| v.as_bytes().to_vec())
                }
            }
        }
    }
}

/// ws secrets check: what is not filled. Values are not printed. Entries above the database tier (`tier`) are not looked up.
pub fn check(m: &Manifest, src: &Source, tier: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for s in &m.secrets {
        let at = format!("{}/{}", s.group, s.title);
        if !s.allowed(tier) {
            out.push(Finding::new(
                Level::Info,
                "secrets",
                &s.name,
                format!("{at}: {} only - not in a {tier} vault", s.visibility),
            ));
            continue;
        }
        let (has, miss) = if s.kind == "file" {
            (
                format!("attachment '{}'", s.file),
                format!("attachment '{}'", s.file),
            )
        } else {
            (
                format!("field '{}'", s.field),
                format!("value in field '{}'", s.field),
            )
        };
        match src.get(s) {
            Some(b) => out.push(Finding::new(
                Level::Info,
                "secrets",
                &s.name,
                format!("{at}: {has} present ({} bytes)", b.len()),
            )),
            None => out.push(Finding::new(
                if s.required {
                    Level::Error
                } else {
                    Level::Warn
                },
                "secrets",
                &s.name,
                format!("{at}: missing {miss} - {}", s.description),
            )),
        }
    }
    out
}

fn new_db_guard(path: &Path, password: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "{} already exists - ws never overwrites a database; fill it in KeePassXC",
            path.display()
        ));
    }
    if password.chars().count() < 12 {
        return Err(
            "master password shorter than 12 characters - use a phrase of 4-6 words".into(),
        );
    }
    Ok(())
}

/// Entries per the spec; `fill` — values and attachments from the source database (export), none — empty (init).
fn build_db(
    name: &str,
    secrets: &[&Secret],
    fill: Option<&Source>,
) -> Result<keepass::Database, String> {
    let mut db = keepass::Database::new();
    db.root_mut().name = name.into();
    let mut groups: BTreeMap<String, keepass::db::GroupId> = BTreeMap::new();
    for s in secrets.iter().copied() {
        let gid = match groups.get(&s.group) {
            Some(g) => *g,
            None => {
                let mut root = db.root_mut();
                let mut g = root.add_group();
                g.name = s.group.clone();
                let id = g.id();
                groups.insert(s.group.clone(), id);
                id
            }
        };
        let notes = format!(
            "{}\n\nws secrets: {} ({}), env {}{}; used by: {}",
            s.description,
            s.name,
            if s.kind == "file" {
                format!("attachment '{}'", s.file)
            } else {
                format!("field '{}'", s.field)
            },
            s.env,
            if s.rotate { "; rotate on a new PC" } else { "" },
            s.used_by.join(", ")
        );
        let data = fill.and_then(|src| src.get(s));
        let mut root = db.root_mut();
        let mut g = root.group_mut(gid).ok_or("group vanished")?;
        g.add_entry().edit(|e| {
            e.set_unprotected("Title", s.title.clone());
            e.set_unprotected("Notes", notes);
            if s.kind == "value" {
                let v = data
                    .as_deref()
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_default();
                e.set_protected(s.field.clone(), v);
            } else if let Some(d) = data {
                e.add_attachment(s.file.clone(), keepass::db::Value::protected(d));
            }
        });
    }
    Ok(db)
}

/// Writing the database to disk: first `.kdbx.new`, a check by opening it with the same password, then a rename.
fn save_new(db: &keepass::Database, path: &Path, password: &str) -> Result<Source, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("kdbx.new");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        db.save(&mut f, key(password))
            .map_err(|e| format!("save: {e}"))?;
    }
    let reopened = Source::open_kdbx(&tmp, password);
    if reopened.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    let reopened = reopened?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(reopened)
}

fn group_count(secrets: &[&Secret]) -> usize {
    secrets
        .iter()
        .map(|s| s.group.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

/// ws secrets init: a new database with groups and empty entries per the spec. An existing file is not touched.
pub fn init(m: &Manifest, path: &Path, password: &str) -> Result<String, String> {
    new_db_guard(path, password)?;
    let all: Vec<&Secret> = m.secrets.iter().collect();
    save_new(&build_db("dungeonborne", &all, None)?, path, password)?;
    Ok(format!(
        "created {}: {} groups, {} entries",
        path.display(),
        group_count(&all),
        all.len()
    ))
}

/// Export result: what went in, what is not filled in the source database, what stayed with the owner (names only).
pub struct Export {
    pub copied: Vec<String>,
    pub empty: Vec<String>,
    pub withheld: Vec<String>,
}

/// ws secrets export: a new database of tier `tier` (team, public) — only entries with access not above it, values
/// and attachments from `src`. The source database is not changed; an existing file is not overwritten. After writing, the database
/// is opened again with the new password and every value is compared with the source.
pub fn export(
    m: &Manifest,
    src: &Source,
    tier: &str,
    out: &Path,
    password: &str,
) -> Result<Export, String> {
    if tier == "owner" || !TIERS.contains(&tier) {
        return Err(format!(
            "export is for a lower tier ({}); the owner vault is copied as a file",
            TIERS[..2].join(", ")
        ));
    }
    new_db_guard(out, password)?;
    let (picked, withheld): (Vec<&Secret>, Vec<&Secret>) =
        m.secrets.iter().partition(|s| s.allowed(tier));
    let db = build_db(&format!("dungeonborne-{tier}"), &picked, Some(src))?;
    let copy = save_new(&db, out, password)?;
    let mut res = Export {
        copied: Vec::new(),
        empty: Vec::new(),
        withheld: withheld.iter().map(|s| s.name.clone()).collect(),
    };
    for s in &picked {
        let want = src.get(s);
        if copy.get(s) != want {
            let _ = std::fs::remove_file(out);
            return Err(format!(
                "{}: value differs after export - {} removed",
                s.name,
                out.display()
            ));
        }
        if want.is_some() {
            res.copied.push(s.name.clone());
        } else {
            res.empty.push(s.name.clone());
        }
    }
    Ok(res)
}

/// ws secrets run: secrets into the command's environment. file — into a temp folder (deleted afterwards), the path goes to env.
/// Entries above the database tier (`tier`): "all" skips them, by name — an error.
pub fn run(
    m: &Manifest,
    src: &Source,
    names: &[String],
    tier: &str,
    tmp_root: &Path,
    cmd: &[String],
) -> Result<i32, String> {
    let all = names.iter().any(|n| n == "all");
    let picked: Vec<&Secret> = m
        .secrets
        .iter()
        .filter(|s| s.allowed(tier) && (all || names.contains(&s.name)))
        .collect();
    for n in names.iter().filter(|n| *n != "all") {
        match m.secrets.iter().find(|s| &s.name == n) {
            None => return Err(format!("no secret '{n}' in secrets.toml (ws secrets list)")),
            Some(s) if !s.allowed(tier) => {
                return Err(format!(
                    "'{n}' is {} only; this machine has a {tier} vault (local.toml [vault] tier)",
                    s.visibility
                ))
            }
            _ => {}
        }
    }
    let (exe, args) = cmd.split_first().ok_or("a command is needed after --")?;
    let dir = tmp_root.join(format!("secrets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(exe);
    command.args(args);
    let mut missing = Vec::new();
    for s in picked {
        let Some(data) = src.get(s) else {
            if s.required || !all {
                missing.push(s.name.clone());
            }
            continue;
        };
        if s.kind == "file" {
            let p = dir.join(&s.name).join(&s.file);
            std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&p, &data).map_err(|e| e.to_string())?;
            command.env(&s.env, &p);
        } else {
            command.env(&s.env, String::from_utf8_lossy(&data).to_string());
        }
    }
    if !missing.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("missing in the vault: {}", missing.join(", ")));
    }
    let status = command.status().map_err(|e| format!("{exe}: {e}"));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(status?.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(name: &str, kind: &str) -> Secret {
        Secret {
            name: name.into(),
            group: "Group".into(),
            title: format!("Entry {name}"),
            kind: kind.into(),
            field: "Password".into(),
            file: "key.pem".into(),
            env: format!("WS_TEST_{}", name.to_uppercase()),
            used_by: vec![],
            description: "test".into(),
            required: true,
            rotate: false,
            visibility: "owner".into(),
            session: false,
            session_for: vec![],
        }
    }

    #[test]
    fn init_fill_check_run() {
        let dir = std::env::temp_dir().join(format!("ws-secrets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = Manifest {
            db_name: "t.kdbx".into(),
            secrets: vec![secret("val", "value"), secret("file", "file")],
        };
        let db_path = dir.join("t.kdbx");
        let pw = "test-only-password-0001";
        assert!(init(&m, &db_path, "short").is_err(), "short password");
        init(&m, &db_path, pw).unwrap();
        assert!(
            init(&m, &db_path, pw).is_err(),
            "an existing database is left alone"
        );

        // empty database: both are missing
        let src = Source::open_kdbx(&db_path, pw).unwrap();
        assert!(check(&m, &src, "owner")
            .iter()
            .all(|f| f.level == Level::Error));
        assert!(Source::open_kdbx(&db_path, "wrong-password-0000").is_err());

        // fill it as KeePassXC would: a field and an attachment (in the test — with the same crate, into a separate file)
        let Source::Kdbx(mut db) = src else {
            unreachable!()
        };
        {
            let mut root = db.root_mut();
            let mut g = root.group_by_name_mut("Group").unwrap();
            g.entry_by_name_mut("Entry val")
                .unwrap()
                .set_protected("Password", "s3cr3t");
            g.entry_by_name_mut("Entry file").unwrap().add_attachment(
                "key.pem",
                keepass::db::Value::protected(b"PEM DATA".to_vec()),
            );
        }
        let filled = dir.join("filled.kdbx");
        db.save(&mut std::fs::File::create(&filled).unwrap(), key(pw))
            .unwrap();

        let src = Source::open_kdbx(&filled, pw).unwrap();
        assert_eq!(src.get(&m.secrets[0]).unwrap(), b"s3cr3t");
        assert_eq!(src.get(&m.secrets[1]).unwrap(), b"PEM DATA");
        assert!(check(&m, &src, "owner")
            .iter()
            .all(|f| f.level == Level::Info));

        // run: the value goes to env, the file to a temp folder with its path in env; after the command the folder is gone
        let tmp = dir.join("tmp");
        let probe = dir.join("probe.txt");
        let script = format!(
            "Set-Content -LiteralPath '{}' -Value ($env:WS_TEST_VAL + '|' + (Get-Content -Raw -LiteralPath $env:WS_TEST_FILE) + '|' + $env:WS_TEST_FILE)",
            probe.display()
        );
        let cmd: Vec<String> = ["powershell", "-NoProfile", "-Command", &script]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let code = run(&m, &src, &["all".into()], "owner", &tmp, &cmd).unwrap();
        assert_eq!(code, 0);
        let got = std::fs::read_to_string(&probe).unwrap();
        let parts: Vec<&str> = got.trim().split('|').collect();
        assert_eq!(parts[0], "s3cr3t");
        assert_eq!(parts[1].trim(), "PEM DATA");
        assert!(
            !std::path::Path::new(parts[2]).exists(),
            "the temp secret file is deleted"
        );
        assert!(
            std::fs::read_dir(&tmp).unwrap().next().is_none(),
            "the temp folder is empty"
        );
        assert!(run(&m, &src, &["nope".into()], "owner", &tmp, &cmd).is_err());

        // export: only team entries go to the team database, with the same values; owner entries are not there at all
        let mut m2 = Manifest {
            db_name: "t.kdbx".into(),
            secrets: vec![
                secret("val", "value"),
                secret("file", "file"),
                secret("gap", "value"),
            ],
        };
        m2.secrets[0].visibility = "team".into();
        m2.secrets[2].visibility = "team".into();
        let team = dir.join("team.kdbx");
        let tpw = "team-only-password-0002";
        assert!(
            export(&m2, &src, "owner", &team, tpw).is_err(),
            "owner — a file copy, not an export"
        );
        assert!(export(&m2, &src, "team", &team, "short").is_err());
        let r = export(&m2, &src, "team", &team, tpw).unwrap();
        assert_eq!(r.copied, vec!["val"]);
        assert_eq!(r.empty, vec!["gap"]);
        assert_eq!(r.withheld, vec!["file"]);
        assert!(
            export(&m2, &src, "team", &team, tpw).is_err(),
            "does not overwrite"
        );
        assert!(
            Source::open_kdbx(&team, pw).is_err(),
            "its own password, not the owner's"
        );
        let t = Source::open_kdbx(&team, tpw).unwrap();
        assert_eq!(t.get(&m2.secrets[0]).unwrap(), b"s3cr3t");
        assert!(t.get(&m2.secrets[1]).is_none());
        let Source::Kdbx(tdb) = &t else {
            unreachable!()
        };
        let root = tdb.root();
        let g = root.group_by_path(&["Group"]).unwrap();
        assert!(g.entry_by_name("Entry file").is_none(), "no owner entry");
        assert!(
            g.entry_by_name("Entry gap").is_some(),
            "the empty team entry is there — to be filled"
        );

        // check/run on a machine with tier team: owner — info, not an error; by name — refused
        let f = check(&m2, &t, "team");
        let lvl = |n: &str| &f.iter().find(|x| x.path == n).unwrap().level;
        assert!(*lvl("val") == Level::Info);
        assert!(*lvl("file") == Level::Info);
        assert!(*lvl("gap") == Level::Error);
        let ok: Vec<String> = ["powershell", "-NoProfile", "-Command", "exit 0"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(run(&m2, &t, &["val".into()], "team", &tmp, &ok).unwrap(), 0);
        let e = run(&m2, &t, &["file".into()], "team", &tmp, &ok).unwrap_err();
        assert!(e.contains("owner only"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
