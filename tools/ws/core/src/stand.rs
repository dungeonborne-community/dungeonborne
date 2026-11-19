//! ws stand — the local test stand of this workspace (tools/stand/README.md), assembled from what the kit has:
//!
//! - game copies `home/game/stand-server`, `stand-client` (and `stand-client2` with `--clients=2`) from the reference
//!   (`ws game clone`: paks are hard links, about 1 GB per copy);
//! - the launcher's client runtime in each (artifact `launcher bundle client.zip`: Goldberg `steam_api64.dll`, UE4SS,
//!   the mod loader) and a Goldberg identity of its own (`steam_settings/force_steamid.txt`, `force_account_name.txt`);
//! - the mods of the profile (`cargo xtask mods install --profile=server|client`);
//! - `local.toml [games] steam / client / client-e` and `[stand] host_open_id`, when they are not set yet.
//!
//! A workspace whose `[games]` already names its own installs (the owner's) is left as it is. What only a human can do
//! is checked and named: the DbTools task (administrator, `ops/machine/install-dbtools.ps1 -Apply` — several workspaces
//! on one machine are fine) and the `hosts` block. The port lock is one folder for the machine (`tools/stand/paths.ps1`).
use crate::{Finding, Level, Workspace};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Options {
    pub apply: bool,
    /// 1 or 2 clients
    pub clients: usize,
}

struct Copy {
    name: &'static str,
    /// local.toml [games] key the stand scripts use
    key: &'static str,
    profile: &'static str,
    steam_id: &'static str,
    /// Goldberg's LAN port: every copy its own, or two copies on one PC see each other as one
    listen_port: u16,
}

/// Goldberg Steam IDs of the stand copies, the same on every machine. They must be valid individual Steam IDs
/// (76561197960265728 + account number): with a smaller one (76561190000001001.., 18.11) Goldberg fell back to the
/// machine's shared account.
#[cfg(test)]
const STEAM_ID_BASE: u64 = 76561197960265728;
/// IDs ws stand wrote before 18.11 evening (invalid): a host_open_id with one of them is replaced.
const OLD_STAND_IDS: [&str; 3] = [
    "76561190000001001",
    "76561190000001002",
    "76561190000001003",
];
/// Line 1 of the battle server's native_data.txt: the backend's DS HTTP (DS_AGENT_PORT 9091 of run_local.ps1).
const NATIVE_DATA_URL: &str = "http://127.0.0.1:9091/ds/battle_user_data?open_id=";
const COPIES: [Copy; 3] = [
    Copy {
        name: "stand-server",
        key: "steam",
        profile: "server",
        steam_id: "76561198000001001",
        listen_port: 47611,
    },
    Copy {
        name: "stand-client",
        key: "client",
        profile: "client",
        steam_id: "76561198000001002",
        listen_port: 47612,
    },
    Copy {
        name: "stand-client2",
        key: "client-e",
        profile: "client",
        steam_id: "76561198000001003",
        listen_port: 47613,
    },
];

const WIN64: &str = "Dungeonborne/Binaries/Win64";
const DBTOOLS_CONFIG: &str = r"C:\ProgramData\DbTools\dbtools.config.ps1";

/// The game's addresses the stand client must reach on this machine (the launcher's `hosts.rs` DOMAINS).
const HOSTS_DOMAINS: [&str; 11] = [
    "sg.intlgame.com",
    "intlgame.com",
    "openapi.smartlink.intlgame.com",
    "tdm.deltaverse-intl.com",
    "f.tgf123.com",
    "sg.api.tgf123.com",
    "tgf123.com",
    "dir.228918463-2-1.dmpplat.com",
    "tconnd-7101-702.dmpplat.com",
    "t01.tgf123.com",
    "dir.228918463-2-1.example.org",
];

fn f(level: Level, path: impl Into<String>, msg: impl Into<String>) -> Finding {
    Finding::new(level, "stand", path, msg)
}

fn same_path(a: &Path, b: &Path) -> bool {
    let k = |p: &Path| crate::normalize(p).display().to_string().to_lowercase();
    k(a) == k(b)
}

/// The stand work folder: local.toml [stand] work, else home/runs/stand (as tools/stand/paths.ps1).
pub fn work_dir(ws: &Workspace) -> PathBuf {
    ws.local_str("stand", "work")
        .and_then(|p| ws.resolve(&p))
        .unwrap_or_else(|| ws.home().join("runs").join("stand"))
}

/// The port lock folder: local.toml [stand] locks, else one folder for the machine (as tools/stand/paths.ps1).
pub fn lock_dir(ws: &Workspace) -> PathBuf {
    ws.local_str("stand", "locks")
        .and_then(|p| ws.resolve(&p))
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into()),
            )
            .join("DungeonborneStand")
            .join("locks")
        })
}

pub fn run(ws: &Workspace, opt: &Options) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut human: Vec<String> = Vec::new();
    if !ws.root.join("tools/stand/README.md").is_file() {
        out.push(f(
            Level::Error,
            "tools/stand",
            "the stand scripts are not on disk: cargo ws components add stand --apply",
        ));
        return out;
    }
    // 1. the reference everything is copied from
    let Some(b) = crate::game::builds(ws).into_iter().next() else {
        out.push(f(Level::Error, "config/game.toml", "no build listed"));
        return out;
    };
    let reference = crate::game::original_root(ws).join(&b.id);
    if !reference.join("original.toml").is_file() {
        out.push(f(
            Level::Error,
            "game reference",
            format!("no reference {} - cargo ws bootstrap --apply takes your copy of the game (docs/GAME_DATA.md)", reference.display()),
        ));
        return out;
    }
    let bundle = ws.root.join("apps/launcher/bundle/client.zip");
    let have_bundle = bundle.is_file();
    if !have_bundle {
        out.push(f(
            Level::Error,
            "apps/launcher/bundle/client.zip",
            "no launcher client bundle (Goldberg, UE4SS): artifact \"launcher bundle client.zip\" - cargo ws bootstrap --apply with the artifact store; without it: cargo xtask setup --apply --tool=goldberg,ue4ss, then python apps/launcher/tools/make_client_bundle.py --upstream",
        ));
    }
    // the launcher's bundle unpacked once (home/cache/tmp/stand-bundle) with its file list (bundle.json: path -> sha256)
    let runtime = if have_bundle {
        unpack_bundle(ws, &bundle)
    } else {
        None
    };
    if have_bundle && runtime.is_none() {
        out.push(f(
            Level::Error,
            "apps/launcher/bundle/client.zip",
            "could not unpack it or read its bundle.json",
        ));
    }
    // 2. the copies
    let local = ws.config("local.toml");
    let n = 1 + opt.clients.clamp(1, 2);
    let mut server_ours = false;
    let mut managed: Vec<PathBuf> = Vec::new(); // Win64 folders of the copies ws stand looks after
    for c in &COPIES[..n] {
        let dst = ws.home().join("game").join(c.name);
        if let Some(own) = ws.local_str("games", c.key).and_then(|p| ws.resolve(&p)) {
            if !same_path(&own, &dst) {
                out.push(f(
                    Level::Info,
                    format!("[games] {}", c.key),
                    format!(
                        "{} - your own install; ws stand leaves it as it is",
                        own.display()
                    ),
                ));
                continue;
            }
        }
        if c.key == "steam" {
            server_ours = true;
        }
        let label = format!("home/game/{}", c.name);
        // a. the copy
        if !dst.join(WIN64).join("Dungeonborne.exe").is_file() {
            if !opt.apply {
                out.push(f(
                    Level::Warn,
                    &label,
                    format!(
                        "missing - ws stand --apply makes it from {}",
                        reference.display()
                    ),
                ));
                continue;
            }
            match crate::game::clone(&reference, &dst, true) {
                Ok((st, _)) => out.push(f(
                    Level::Done,
                    &label,
                    format!(
                        "copied from the reference: paks linked {}, files copied {} ({:.1} GB)",
                        st.linked,
                        st.copied,
                        st.bytes_copied as f64 / 1e9
                    ),
                )),
                Err(e) => {
                    out.push(f(Level::Error, &label, e));
                    continue;
                }
            }
        }
        let win64 = dst.join(WIN64);
        managed.push(win64.clone());
        // b. the launcher's runtime: Goldberg, UE4SS, the mod loader — every file of the bundle by its sha256 in
        // bundle.json (18.11: a read-only steam_api64.dll of the reference stayed, and a check by UE4SS.dll alone said
        // "there"; the game then ran with Steam's own steam_api64.dll and could not log in)
        match &runtime {
            None => {}
            Some((dir, files)) => {
                let stale: Vec<&String> = files
                    .iter()
                    .filter(|(rel, sha)| {
                        // cacert.pem: the stand adds its own certificate at the end (stand_tls) — the bundle's
                        // content as the beginning is enough
                        if rel.ends_with("cacert.pem") {
                            let have = std::fs::read(dst.join(rel)).unwrap_or_default();
                            let want = std::fs::read(dir.join(rel)).unwrap_or_default();
                            return !(!want.is_empty() && have.starts_with(&want));
                        }
                        crate::sha256_file(&dst.join(rel)).as_deref() != Some(sha.as_str())
                    })
                    .map(|(rel, _)| rel)
                    .collect();
                if !stale.is_empty() {
                    if !opt.apply {
                        out.push(f(
                            Level::Warn,
                            &label,
                            format!("the launcher's runtime is not in place ({} file(s), e.g. {}) - ws stand --apply", stale.len(), stale[0]),
                        ));
                    } else {
                        let failed: Vec<String> = stale
                            .iter()
                            .filter_map(|rel| {
                                put_file(&dir.join(rel), &dst.join(rel))
                                    .err()
                                    .map(|e| format!("{rel}: {e}"))
                            })
                            .collect();
                        out.push(if failed.is_empty() {
                            f(Level::Done, &label, format!("the launcher's client runtime (Goldberg, UE4SS, mod loader): {} file(s) put in", stale.len()))
                        } else {
                            f(Level::Error, &label, format!("the launcher's runtime: {}", failed.join("; ")))
                        });
                    }
                }
            }
        }
        // c. the Goldberg identity of this copy. The launcher's Goldberg keeps its account where local_save.txt points
        // (<folder>/settings/user_steam_id.txt, account_name.txt, listen_port.txt — as the owner's client); it does not
        // read steam_settings/force_steamid.txt (18.11: every stand copy logged in with the machine's shared Goldberg
        // account, the host and the guest were one player). The force_* files stay for builds that read them.
        let profile = "stand_profile";
        let wanted: [(PathBuf, String); 7] = [
            (win64.join("local_save.txt"), profile.to_string()),
            (
                win64
                    .join(profile)
                    .join("settings")
                    .join("user_steam_id.txt"),
                c.steam_id.to_string(),
            ),
            (
                win64
                    .join(profile)
                    .join("settings")
                    .join("account_name.txt"),
                c.name.to_string(),
            ),
            (
                win64.join(profile).join("settings").join("listen_port.txt"),
                c.listen_port.to_string(),
            ),
            (
                win64.join(profile).join("settings").join("language.txt"),
                "english".to_string(),
            ),
            (
                win64.join("steam_settings").join("force_steamid.txt"),
                c.steam_id.to_string(),
            ),
            (
                win64.join("steam_settings").join("force_account_name.txt"),
                c.name.to_string(),
            ),
        ];
        let missing: Vec<&(PathBuf, String)> = wanted
            .iter()
            .filter(|(path, v)| {
                std::fs::read_to_string(path).ok().as_deref().map(str::trim) != Some(v.as_str())
            })
            .collect();
        if !missing.is_empty() {
            if opt.apply {
                let res: Result<(), std::io::Error> = missing.iter().try_for_each(|(path, v)| {
                    std::fs::create_dir_all(path.parent().unwrap())?;
                    std::fs::write(path, v)
                });
                out.push(match res {
                    Ok(_) => f(
                        Level::Done,
                        &label,
                        format!(
                            "Goldberg account {} ({}, LAN port {})",
                            c.name, c.steam_id, c.listen_port
                        ),
                    ),
                    Err(e) => f(Level::Error, &label, format!("Goldberg account: {e}")),
                });
            } else {
                out.push(f(
                    Level::Warn,
                    &label,
                    format!(
                        "no Goldberg account of its own yet - ws stand --apply sets {} ({})",
                        c.name, c.steam_id
                    ),
                ));
            }
        }
        // c2. the battle server's link to the backend: DungeonborneRuServer reads the backend URL from line 1 of
        // DungeonborneRuClient\native_data.txt and only then sends the heartbeat (POST /ds/slot) the allocator waits
        // for; "realkey" is a flag word (docs/MOD_SETTINGS.md), as ops/deploy/robot/install_gameserver.ps1 writes it.
        // 18.11 (found by the newcomer test): without it a room's "Start" failed with -4628, no battle server.
        // native_init.txt = 1: the mod leaves HasInitedPlayerData alone, so the game's own InitPlayerGameData builds the
        // hero (as on the owner's battle server). 18.11: without it the server logged "LyraPlayerState Has Inited!" and
        // dropped every client ~1 s later: "Client attempted to create sub-object" (BACKLOG, 02.11).
        // An existing file is left alone - its flags may be someone's own.
        if c.profile == "server" {
            let native_data = format!("{NATIVE_DATA_URL}\r\nrealkey\r\n");
            let files: [(&str, &str, &str, &str); 2] = [
                (
                    "native_data.txt",
                    &native_data,
                    "the battle server reports to the local backend",
                    "the battle server would not report to the backend (a room's Start fails)",
                ),
                (
                    "native_init.txt",
                    "1",
                    "the game builds the players' heroes itself",
                    "the battle server would drop every client entering a match",
                ),
            ];
            for (name, text, done, missing) in files {
                let path = win64.join(r"ue4ss\Mods\DungeonborneRuClient").join(name);
                if path.exists() {
                    continue;
                }
                if opt.apply {
                    let res = std::fs::create_dir_all(path.parent().unwrap())
                        .and_then(|_| std::fs::write(&path, text));
                    out.push(match res {
                        Ok(_) => f(Level::Done, &label, format!("{name}: {done}")),
                        Err(e) => f(Level::Error, &label, format!("{name}: {e}")),
                    });
                } else {
                    out.push(f(
                        Level::Warn,
                        &label,
                        format!("no {name} - {missing}; ws stand --apply writes it"),
                    ));
                }
            }
        }
        // d. local.toml [games]
        if ws.local_str("games", c.key).is_none() {
            let line = format!("{} = 'home:game/{}'   # ws stand", c.key, c.name);
            if opt.apply {
                out.push(
                    match crate::bootstrap::set_local_line(&local, "games", c.key, &line) {
                        Ok(_) => f(Level::Done, "local.toml [games]", line),
                        Err(e) => f(Level::Error, "local.toml [games]", e),
                    },
                );
            } else {
                out.push(f(
                    Level::Warn,
                    "local.toml [games]",
                    format!("no {} - ws stand --apply writes: {line}", c.key),
                ));
            }
        }
        // e. the mods of the profile (a build of the mod packages; needs the components with core/backend)
        if opt.apply {
            if !ws.root.join("core/backend/xtask").is_dir() {
                out.push(f(Level::Warn, &label, "mods not installed: core/backend is not on disk (cargo ws components add server --apply)"));
            } else {
                let ok = Command::new("cargo")
                    .args([
                        "xtask",
                        "mods",
                        "install",
                        &format!("--profile={}", c.profile),
                    ])
                    .arg(format!("--game={}", dst.display()))
                    .current_dir(ws.root.join("core/backend"))
                    .status()
                    .is_ok_and(|s| s.success());
                out.push(if ok {
                    f(
                        Level::Done,
                        &label,
                        format!("mods of the {} profile installed", c.profile),
                    )
                } else {
                    f(
                        Level::Error,
                        &label,
                        format!(
                            "cargo xtask mods install --profile={} failed (output above)",
                            c.profile
                        ),
                    )
                });
            }
        }
        // f. enable the profile's UE4SS mods the way the launcher does (enabled.txt in the mod's folder): mods install
        // lays the files out but leaves enabling alone (18.11, found by the newcomer test — clients started without RuClient)
        if let Some(found) = enable_profile_mods(ws, &win64, c.profile, opt.apply) {
            out.push(f(found.0, &label, found.1));
        }
        // g. the client copies also get the autotest driver (run.ps1 and the other scripts start them with -dbclient)
        if c.profile == "client" {
            if let Some(found) = autotest_mod(ws, &win64, opt.apply) {
                out.push(f(found.0, &label, found.1));
            }
        }
    }
    // 3. the host of the stand: the server copy's Goldberg Steam ID
    let host_stale = ws
        .local_str("stand", "host_open_id")
        .is_some_and(|h| OLD_STAND_IDS.contains(&h.trim()));
    if server_ours && (ws.local_str("stand", "host_open_id").is_none() || host_stale) {
        let line = format!(
            "host_open_id = '{}'   # ws stand: Goldberg account of stand-server",
            COPIES[0].steam_id
        );
        if opt.apply {
            out.push(
                match crate::bootstrap::set_local_line(&local, "stand", "host_open_id", &line) {
                    Ok(_) => f(Level::Done, "local.toml [stand]", line),
                    Err(e) => f(Level::Error, "local.toml [stand]", e),
                },
            );
        } else {
            out.push(f(
                Level::Warn,
                "local.toml [stand]",
                if host_stale {
                    format!(
                        "host_open_id is an old invalid stand ID - ws stand --apply writes: {line}"
                    )
                } else {
                    format!("no host_open_id - ws stand --apply writes: {line}")
                },
            ));
        }
    }
    // 3b. the stand's own TLS for the local login (HTTPS 443)
    out.extend(stand_tls(ws, &managed, opt.apply));
    // 4. the backend build the stand runs
    // where the stand scripts look (tools/stand/paths.ps1 Enter-Backend): CARGO_TARGET_DIR, else core/backend/target;
    // and the shared build cache of the root .cargo/config.toml
    let built = [
        std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
        Some(ws.root.join("core/backend/target")),
        Some(ws.home().join("cache").join("target")),
    ]
    .into_iter()
    .flatten()
    .any(|t| t.join("release/dbserver-rs.exe").is_file());
    if !built {
        out.push(f(Level::Note, "backend", "not built yet: cargo build --release -p dbserver-rs in core/backend (docs/CONTRIBUTING.md, step 6)"));
    }
    // 5. what only a human can do: the DbTools task (administrator) and hosts
    let work = work_dir(ws);
    let task = Command::new("schtasks")
        .args(["/Query", "/TN", "DbTools"])
        .output()
        .is_ok_and(|o| o.status.success());
    let config = std::fs::read_to_string(DBTOOLS_CONFIG).unwrap_or_default();
    let registered = dbtools_knows(&config, &work);
    if task && registered {
        out.push(f(
            Level::Info,
            "DbTools",
            format!("the task knows this workspace ({})", work.display()),
        ));
    } else {
        out.push(f(
            Level::Warn,
            "DbTools",
            if task {
                "the task does not know this workspace yet"
            } else {
                "no DbTools task on this machine"
            },
        ));
        human.push(
            "PowerShell \"Run as administrator\", in the workspace folder: powershell -ExecutionPolicy Bypass -File \
             ops\\machine\\install-dbtools.ps1 -Apply (adds this workspace; other workspaces on the machine stay)"
                .into(),
        );
    }
    // every name of the block, from the hosts file itself (18.11: only sg.intlgame.com was checked, and a missing
    // dir.* line went unnoticed; resolving each name instead waits for DNS timeouts)
    let missing: Vec<&str> = match std::fs::read_to_string(hosts_file()) {
        Ok(text) => {
            let local = local_hosts(&text);
            HOSTS_DOMAINS
                .iter()
                .copied()
                .filter(|d| !local.iter().any(|h| h.eq_ignore_ascii_case(d)))
                .collect()
        }
        Err(_) => HOSTS_DOMAINS
            .iter()
            .copied()
            .filter(|d| !resolves_locally(d))
            .collect(),
    };
    if missing.is_empty() {
        out.push(f(
            Level::Info,
            "hosts",
            "the game's addresses go to this machine",
        ));
    } else {
        out.push(f(
            Level::Warn,
            "hosts",
            format!(
                "{} of the game's addresses do not go to 127.0.0.1 (e.g. {}) - the stand client cannot reach the local backend",
                missing.len(),
                missing[0]
            ),
        ));
        let block: Vec<String> = missing.iter().map(|d| format!("127.0.0.1 {d}")).collect();
        human.push(format!(
            "add to C:\\Windows\\System32\\drivers\\etc\\hosts (Notepad as administrator; inside the dungeonborne-launcher block if \
             you have one; remove it when you do not use the stand):\n\
             # >>> dungeonborne-launcher\n{}\n# <<< dungeonborne-launcher",
            block.join("\n")
        ));
    }
    // 6. the port lock: one for the machine
    let locks = lock_dir(ws);
    if ws.local_str("stand", "locks").is_some() {
        out.push(f(
            Level::Warn,
            "local.toml [stand] locks",
            format!(
                "{}: a lock folder of this workspace only - another workspace on this PC would not see its locks (the \
                 game's ports are the machine's). Remove the line to use the machine's folder (tools/stand/paths.ps1), \
                 or set the same folder in every workspace here",
                locks.display()
            ),
        ));
    }
    let held: Vec<String> = std::fs::read_dir(&locks)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            std::fs::read_to_string(e.path())
                .ok()
                .map(|t| t.lines().next().unwrap_or("").to_string())
        })
        .collect();
    out.push(f(
        Level::Info,
        "port lock",
        if held.is_empty() {
            format!("{} - free", locks.display())
        } else {
            format!("{} - held: {}", locks.display(), held.join("; "))
        },
    ));
    for (i, h) in human.iter().enumerate() {
        out.push(f(Level::Note, format!("human, step {}", i + 1), h.clone()));
    }
    out
}

/// The launcher's client bundle unpacked into home/cache/tmp/stand-bundle, and its files (bundle.json: path -> sha256).
fn unpack_bundle(ws: &Workspace, bundle: &Path) -> Option<(PathBuf, Vec<(String, String)>)> {
    let dir = ws.home().join("cache").join("tmp").join("stand-bundle");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let tar = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
        .join("System32")
        .join("tar.exe");
    let ok = Command::new(tar)
        .arg("-xf")
        .arg(bundle)
        .arg("-C")
        .arg(&dir)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return None;
    }
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("bundle.json")).ok()?).ok()?;
    let files = doc["files"]
        .as_object()?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    Some((dir, files))
}

/// Clears the read-only attribute of a copy's file (files copied from the reference keep it). Only for files the copy
/// owns: a pak linked to the reference shares its data and is never written.
fn make_writable(p: &Path) -> Result<(), String> {
    if let Ok(m) = std::fs::metadata(p) {
        let mut perm = m.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            std::fs::set_permissions(p, perm).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Copies `src` over `dst`, also over a read-only file (files copied from the reference keep that attribute).
fn put_file(src: &Path, dst: &Path) -> Result<(), String> {
    make_writable(dst)?;
    std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The autotest driver of the stand's clients: the mod DungeonborneRuServer (catalog `ue4ss.server-hooks`, profiles
/// server and dev-client — not client). With -dbclient it logs in, presses the match button and writes
/// dbclient-status.json for run.ps1; without the flag it does nothing. 19.11 (clean run): a client made from the client
/// profile never reported, run.ps1 timed out and the client stayed running (under ACE only the mod can quit it).
const AUTOTEST_MOD: &str = "DungeonborneRuServer";

fn autotest_mod(ws: &Workspace, win64: &Path, apply: bool) -> Option<(Level, String)> {
    fn files(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                files(&p, base, out);
            } else if let Ok(rel) = p.strip_prefix(base) {
                out.push(rel.to_path_buf());
            }
        }
    }
    let src = ws.root.join("game/ue4ss/Mods").join(AUTOTEST_MOD);
    if !src.is_dir() {
        return Some((
            Level::Warn,
            format!(
                "no {} (component mods): run.ps1 cannot drive this client",
                src.display()
            ),
        ));
    }
    let dst = win64.join("ue4ss/Mods").join(AUTOTEST_MOD);
    let mut list = Vec::new();
    files(&src, &src, &mut list);
    let stale: Vec<&PathBuf> = list
        .iter()
        .filter(|rel| std::fs::read(src.join(rel)).ok() != std::fs::read(dst.join(rel)).ok())
        .collect();
    let enabled = dst.join("enabled.txt").is_file();
    if stale.is_empty() && enabled {
        return None;
    }
    if !apply {
        return Some((
            Level::Warn,
            format!("the autotest mod {AUTOTEST_MOD} is missing or old - ws stand --apply"),
        ));
    }
    for rel in &stale {
        if let Err(e) = put_file(&src.join(rel), &dst.join(rel)) {
            return Some((Level::Error, format!("{AUTOTEST_MOD}: {e}")));
        }
    }
    if let Err(e) = std::fs::write(dst.join("enabled.txt"), "") {
        return Some((Level::Error, format!("{AUTOTEST_MOD}/enabled.txt: {e}")));
    }
    Some((
        Level::Done,
        format!(
            "the autotest mod {AUTOTEST_MOD} (run.ps1, -dbclient): {} file(s) put in, enabled",
            stale.len()
        ),
    ))
}

/// The UE4SS mods of a profile (home/out/mods/<profile>/manifest.json, written by `cargo xtask mods build|install`)
/// that are on disk in the copy but not enabled; with `apply` an `enabled.txt` is put in each (as the launcher does).
/// None — nothing to say (no manifest yet, or all enabled).
fn enable_profile_mods(
    ws: &Workspace,
    win64: &Path,
    profile: &str,
    apply: bool,
) -> Option<(Level, String)> {
    let manifest = std::fs::read_to_string(
        ws.home()
            .join("out")
            .join("mods")
            .join(profile)
            .join("manifest.json"),
    )
    .ok()?;
    let doc: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let mods = win64.join("ue4ss").join("Mods");
    let off: Vec<String> = doc["items"]
        .as_array()?
        .iter()
        .filter(|it| it["kind"] == "ue4ss")
        .filter_map(|it| it["name"].as_str().map(String::from))
        .filter(|n| mods.join(n).is_dir() && !mods.join(n).join("enabled.txt").is_file())
        .collect();
    if off.is_empty() {
        return None;
    }
    if !apply {
        return Some((
            Level::Warn,
            format!(
                "mods of the {profile} profile not enabled: {} - ws stand --apply",
                off.join(", ")
            ),
        ));
    }
    let failed: Vec<&String> = off
        .iter()
        .filter(|n| std::fs::write(mods.join(n).join("enabled.txt"), b"").is_err())
        .collect();
    Some(if failed.is_empty() {
        (
            Level::Done,
            format!("mods of the {profile} profile enabled: {}", off.join(", ")),
        )
    } else {
        (
            Level::Error,
            format!(
                "could not enable: {}",
                failed
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })
}

/// Does `host` resolve to this computer (system resolver, hosts included)? As the launcher checks it.
/// The login names the stand's certificate is for (the game's INTL login, as the production certificate).
#[cfg_attr(not(feature = "stand-tls"), allow(dead_code))]
const TLS_NAMES: [&str; 8] = [
    "sg.intlgame.com",
    "intlgame.com",
    "openapi.smartlink.intlgame.com",
    "f.tgf123.com",
    "sg.api.tgf123.com",
    "tgf123.com",
    "tdm.deltaverse-intl.com",
    "localhost",
];

/// The stand's TLS files: `<stand work folder>/tls/cert.pem`, `key.pem` (home, never the workspace or git).
pub fn tls_paths(ws: &Workspace) -> (PathBuf, PathBuf) {
    let dir = work_dir(ws).join("tls");
    (dir.join("cert.pem"), dir.join("key.pem"))
}

/// The kind of key the stand makes now; a certificate of another kind (the ECDSA one of 8c2251e) is made again.
const TLS_KIND: &str = "rsa2048";
/// The line before the stand's certificate in a copy's cacert.pem (to find and replace it).
const TLS_MARK: &str = "# Dungeonborne stand (this machine only), ws stand";

/// A self-signed certificate for the login names, made on this machine. RSA 2048, like the production certificate:
/// the client's login SDK did not take an ECDSA P-256 one (18.11 — TLS checked fine with OpenSSL, the client still
/// failed with TGFSDKLoginFailedEvent and never reached the backend).
#[cfg(not(feature = "stand-tls"))]
fn make_tls() -> Result<(String, String), String> {
    Err("this build has no stand TLS (ws-core feature stand-tls): use the ws binary (cargo ws stand)".into())
}

#[cfg(feature = "stand-tls")]
fn make_tls() -> Result<(String, String), String> {
    let mut params =
        rcgen::CertificateParams::new(TLS_NAMES.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
    params.distinguished_name.push(
        rcgen::DnType::OrganizationName,
        "Dungeonborne stand (this machine only)",
    );
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "sg.intlgame.com");
    use rsa::pkcs8::EncodePrivateKey;
    let rsa_key =
        rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).map_err(|e| e.to_string())?;
    let key_pem = rsa_key
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| e.to_string())?;
    let key = rcgen::KeyPair::from_pem_and_sign_algo(&key_pem, &rcgen::PKCS_RSA_SHA256)
        .map_err(|e| e.to_string())?;
    let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
    Ok((cert.pem(), key_pem.to_string()))
}

/// cacert.pem without any stand certificate ws stand added before (the mark line and the certificate after it).
fn strip_stand_certs(text: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for line in text.split_inclusive('\n') {
        if line.trim_end() == TLS_MARK {
            skip = true;
            continue;
        }
        if skip {
            if line.trim_end() == "-----END CERTIFICATE-----" {
                skip = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

/// The stand's own TLS for the local login (18.11, owner's decision): never the production certificate, which every
/// player's client trusts. ws stand makes a certificate for this machine, keeps it in the stand work folder (the
/// backend of the stand takes it through TLS_CERT / TLS_KEY, tools/stand/run_local.ps1) and adds it to the trusted
/// list (`cacert.pem`) of the stand copies only — the launcher's bundle and the players are not touched.
fn stand_tls(ws: &Workspace, managed: &[PathBuf], apply: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    if managed.is_empty() {
        return out; // own installs ([games]): their login certificate is the owner's business
    }
    let (cert, key) = tls_paths(ws);
    let kind_file = cert.with_file_name("kind.txt");
    let current = std::fs::read_to_string(&kind_file).is_ok_and(|k| k.trim() == TLS_KIND);
    if !(cert.is_file() && key.is_file() && current) {
        if !apply {
            out.push(f(Level::Warn, "stand TLS", format!("no certificate for the local login - ws stand --apply makes one for this machine ({})", cert.display())));
            return out;
        }
        let made = make_tls().and_then(|(c, k)| {
            std::fs::create_dir_all(cert.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&cert, c).map_err(|e| e.to_string())?;
            std::fs::write(&key, k).map_err(|e| e.to_string())?;
            std::fs::write(&kind_file, TLS_KIND).map_err(|e| e.to_string())
        });
        match made {
            Ok(()) => out.push(f(
                Level::Done,
                "stand TLS",
                format!(
                    "a certificate of this machine for the local login: {}",
                    cert.display()
                ),
            )),
            Err(e) => {
                out.push(f(Level::Error, "stand TLS", e));
                return out;
            }
        }
    }
    let pem = std::fs::read_to_string(&cert).unwrap_or_default();
    let pem = pem.trim();
    for win64 in managed {
        let ca = win64.join("cacert.pem");
        let Ok(text) = std::fs::read_to_string(&ca) else {
            continue; // no runtime yet: said above
        };
        if text.contains(pem) {
            continue;
        }
        let text = strip_stand_certs(&text); // an older stand certificate goes
        let label = ca.display().to_string();
        if !apply {
            out.push(f(
                Level::Warn,
                label,
                "does not trust the stand's certificate yet - ws stand --apply",
            ));
            continue;
        }
        let sep = if text.ends_with('\n') { "" } else { "\n" };
        // 19.11 (clean run): a copy of a read-only reference has a read-only cacert.pem — os error 5 here
        let written = make_writable(&ca).and_then(|_| {
            std::fs::write(&ca, format!("{text}{sep}\n{TLS_MARK}\n{pem}\n"))
                .map_err(|e| e.to_string())
        });
        out.push(match written {
            Ok(_) => f(Level::Done, label, "trusts the stand's certificate"),
            Err(e) => f(Level::Error, label, e.to_string()),
        });
    }
    out
}

fn hosts_file() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
        .join(r"System32\drivers\etc\hosts")
}

/// The names a hosts file sends to this machine (127.0.0.1 or ::1), comments dropped.
fn local_hosts(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.split('#').next().unwrap_or("");
            let mut it = l.split_whitespace();
            let ip = it.next()?;
            matches!(ip, "127.0.0.1" | "::1").then(|| it.map(String::from).collect::<Vec<_>>())
        })
        .flatten()
        .collect()
}

fn resolves_locally(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    (host, 8085u16)
        .to_socket_addrs()
        .map(|a| a.into_iter().any(|x| x.ip().is_loopback()))
        .unwrap_or(false)
}

/// A Windows path for comparing as text: `\`, lower case, no `.` / `..` parts, no trailing `\`.
fn path_key(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in p.split(['\\', '/']) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            x => parts.push(x),
        }
    }
    parts.join("\\").to_lowercase()
}

/// Whether the DbTools config (dbtools.config.ps1) lists this stand work folder: its `WorkDir = '…'` lines, compared
/// as normalized paths (19.11: an administrator window without DB_HOME registered 'X:\ws\..\home\runs\stand').
fn dbtools_knows(config: &str, work: &Path) -> bool {
    let want = path_key(&work.display().to_string());
    let re = regex::Regex::new(r"WorkDir\s*=\s*'([^']+)'").unwrap();
    let found = re.captures_iter(config).any(|c| path_key(&c[1]) == want);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbtools_paths_compare_normalized() {
        let config = "$Workspaces = @(\n    @{ WorkDir = 'X:\\PublicTest\\workspace\\..\\home\\runs\\stand'; Installs = @{} }\n)\n";
        assert!(dbtools_knows(
            config,
            Path::new("X:/PublicTest/home/runs/stand")
        ));
        assert!(dbtools_knows(
            config,
            Path::new(r"x:\publictest\home\runs\stand\")
        ));
        assert!(!dbtools_knows(
            config,
            Path::new(r"X:\Other\home\runs\stand")
        ));
    }

    #[test]
    fn reads_hosts_names() {
        let text = "# c\n127.0.0.1 a.example b.example # x\n#127.0.0.1 off.example\n10.0.0.1 far.example\n::1 six.example\n";
        assert_eq!(local_hosts(text), ["a.example", "b.example", "six.example"]);
    }

    #[test]
    fn ids_and_keys() {
        // the stand scripts use these [games] keys; the IDs are 17-digit Steam IDs and distinct
        assert_eq!(
            COPIES.iter().map(|c| c.key).collect::<Vec<_>>(),
            ["steam", "client", "client-e"]
        );
        for c in &COPIES {
            assert!(c.steam_id.len() == 17 && c.steam_id.starts_with("7656119"));
            // a valid individual Steam ID: Goldberg ignores a smaller one
            assert!(c.steam_id.parse::<u64>().unwrap() > STEAM_ID_BASE);
        }
        assert_ne!(COPIES[1].steam_id, COPIES[2].steam_id);
        assert!(HOSTS_DOMAINS.contains(&"sg.intlgame.com"));
    }

    #[test]
    #[cfg(feature = "stand-tls")]
    fn makes_a_tls_pair() {
        let (cert, key) = make_tls().unwrap();
        assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(key.contains("PRIVATE KEY"));
        // RSA, like the production certificate (the client's SDK refused ECDSA)
        let key_pair = rcgen::KeyPair::from_pem(&key).unwrap();
        assert!(key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256));
    }

    #[test]
    fn replaces_an_older_stand_cert() {
        let ca = format!("-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n\n{TLS_MARK}\n-----BEGIN CERTIFICATE-----\nOLD\n-----END CERTIFICATE-----\n");
        let s = strip_stand_certs(&ca);
        assert!(s.contains("AAA") && !s.contains("OLD") && !s.contains(TLS_MARK));
    }
}
