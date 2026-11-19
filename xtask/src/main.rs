//! `cargo xtask` — developer kit build tasks (cargo-xtask pattern; docs/WORKSPACE_PLAN.md, section 13).
//! Order from scratch: rustup → `cargo xtask setup --apply` (programs for building windows + ws + DevKit) → DevKit / `cargo ws
//! bootstrap` (everything else). Programs are installed into home per tools.lock (ws-core), PATH changes only for our own builds.
//!
//! Choice "ready-made or from source" (`--prebuilt` / `--from-source`, else local.toml [setup] build, else prebuilt):
//! - third-party programs (trunk, tauri-cli) — the official release, sha256 pinned in tools.lock, or `cargo install --locked`;
//! - ours (ws, DevKit) — a `cargo xtask dist` release signed with the owner's key (public key — xtask/dist.pub), or
//!   a build here. No key, no release for this commit, or uncommitted edits — it is built here.
use minisign::{KeyPair, PublicKeyBox, SecretKeyBox, SignatureBox};
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use ws_core::bootstrap::{self, Build};
use ws_core::{Finding, Level, Workspace};

const HELP: &str = "\
cargo xtask <command>

  setup [--apply] [--prebuilt|--from-source]
        tools that build the windows (trunk, tauri-cli, tailwindcss; versions and sha256 from tools.lock, into
        DB_HOME/tools), then ws and DevKit. Without --apply - only the plan.
        --prebuilt     official release exe for tools, signed dist (cargo xtask dist) for ws and DevKit
        --from-source  build everything here (cargo install --locked, cargo build)
        default: local.toml [setup] build = \"prebuilt\" | \"source\", else prebuilt
        --only=tools   only the tools; --only=ours  only ws and DevKit (bootstrap.ps1 -Step tools / devkit)
        --tool=<a,b>   only these tools from tools.lock (retry one failed download)
  devkit            build DevKit.exe here (needs setup)
  ci                fmt, clippy, test, ws check (needs setup: DevKit's page is built first)
  dist [--sign] [--allow-dirty]
        build ws and DevKit, put them with SHA256SUMS into DB_HOME/out/dist/<tag or commit>; --sign signs SHA256SUMS
        with the key file in DIST_SIGNING_KEY (ws secrets run dist_signing_key -- cargo xtask dist --sign)
  dist push
        signed dists from DB_HOME/out/dist -> artifact store (local.toml [store] push_url/push_user;
        ws secrets run artifact_store_push -- cargo xtask dist push). setup takes a dist from there when the current
        commit has none of its own but ws and DevKit sources are unchanged since a pushed one
  dist keygen --secret-out <file outside the workspace>
        new signing key: public -> xtask/dist.pub (commit it), secret -> the file (put it into KeePassXC, delete it)
  pack designer [--sign] [--allow-dirty]
        the designer's package, no git needed on their side: Balance Studio + balance kit of the active season +
        guide + AGENTS.md + SHA256SUMS (signed with --sign, key as for dist) -> DB_HOME/out/pack/<name>.zip
  export <public|team> [--allow-dirty]
        an edition, one way, no history (docs/WORKSPACE_PLAN.md 13.3): public = only public files, team = everything
        but owner files (cargo ws roles); replacements and overlay from export/<edition>.toml, then every line is
        checked (owner markers, IPs, game files, machine paths) and ws check runs in the copy
        -> DB_HOME/out/export/<edition>-<tag or commit>. Pushes nothing.
        --update=<repo>     then make it one new commit in <repo> (a clean clone of that edition's repository): its
                            files are replaced with the export's, the tree is checked to be identical, the commit is
                            by the neutral author in UTC. Pushes nothing: `git -C <repo> push` on the owner's word.
        --message=<text>    message of that commit (default: \"Update <edition> edition\")

After setup: run DevKit, or `cargo ws bootstrap --profile <owner|dev|designer|agent> --apply` for the rest.
";

/// Programs from tools.lock without which the DevKit window cannot be built (nor the launcher and Studio).
const BUILD_TOOLS: [&str; 3] = ["trunk", "tauri-cli", "tailwindcss"];

/// A machine without Visual Studio: bootstrap.ps1 wrote into CARGO_HOME's config.toml build settings via the cargo-xwin CRT/SDK and
/// `[env] DBDEV_XWIN = "1"` (cargo passes it to `cargo xtask` too). Then the Tauri windows also need LLVM (clang-cl,
/// llvm-rc) — it is installed together with the build programs.
fn xwin() -> bool {
    std::env::var_os("DBDEV_XWIN").is_some_and(|v| v == "1")
}

fn build_tools() -> Vec<&'static str> {
    let mut list = BUILD_TOOLS.to_vec();
    if xwin() {
        // first: it is the C compiler of such a machine, and trunk / tauri-cli built --from-source compile C (ring)
        // (19.11, clean run: LLVM came last and trunk failed without clang-cl)
        list.insert(0, "llvm");
    }
    list
}
/// What goes into a `dist` release.
const DIST_FILES: [&str; 3] = ["ws.exe", "DevKit.exe", "dist.toml"];
const DIST_NEED: [&str; 2] = ["ws.exe", "DevKit.exe"];
const PUBKEY: &str = "xtask/dist.pub";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let ws = match find_workspace() {
        Ok(ws) => ws,
        Err(e) => return fail(&format!("workspace not found: {e}")),
    };
    let explicit = if has("--from-source") {
        Some(Build::Source)
    } else if has("--prebuilt") {
        Some(Build::Prebuilt)
    } else {
        None
    };
    let build = Build::choose(&ws, explicit);
    let res = match args.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("setup") => {
            let opt = |k: &str| {
                args.iter()
                    .find_map(|a| a.strip_prefix(k)?.strip_prefix('=').map(String::from))
            };
            setup(&ws, has("--apply"), build, opt("--only"), opt("--tool"))
        }
        Some("devkit") => devkit(&ws).map(|_| ()),
        Some("ci") => ci(&ws),
        Some("dist") if args.get(1).map(String::as_str) == Some("keygen") => {
            let out = args.iter().skip_while(|a| *a != "--secret-out").nth(1);
            match out {
                Some(p) => keygen(&ws, Path::new(p)),
                None => Err("dist keygen needs --secret-out <file>".into()),
            }
        }
        Some("dist") if args.get(1).map(String::as_str) == Some("push") => dist_push(&ws),
        Some("dist") => dist(&ws, has("--sign"), has("--allow-dirty")),
        Some("pack") if args.get(1).map(String::as_str) == Some("designer") => {
            pack_designer(&ws, has("--sign"), has("--allow-dirty"))
        }
        Some("pack") => Err("pack: only `pack designer` for now".into()),
        Some("export")
            if args
                .get(1)
                .is_some_and(|e| ws_core::export::EDITIONS.contains(&e.as_str())) =>
        {
            let opt = |k: &str| {
                args.iter()
                    .find_map(|a| a.strip_prefix(k)?.strip_prefix('=').map(String::from))
            };
            export_edition(
                &ws,
                &args[1],
                has("--allow-dirty"),
                opt("--update").map(PathBuf::from),
                opt("--message"),
            )
        }
        Some("export") => Err(format!(
            "export: `export <{}>`",
            ws_core::export::EDITIONS.join("|")
        )),
        Some(cmd) => Err(format!("unknown command '{cmd}'\n\n{HELP}")),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("xtask: {msg}");
    ExitCode::FAILURE
}

/// Root: from the current folder upwards, else the folder above this crate.
fn find_workspace() -> Result<Workspace, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    Workspace::find(&cwd).or_else(|_| Workspace::find(Path::new(env!("CARGO_MANIFEST_DIR"))))
}

fn print_findings(list: &[Finding]) -> usize {
    let mut errors = 0;
    for f in list {
        let tag = match f.level {
            Level::Error => {
                errors += 1;
                "ERROR"
            }
            Level::Warn => "WARN ",
            Level::Done => "DONE ",
            Level::Info => "ok   ",
            Level::Note => "note ",
        };
        println!("{tag} [{}] {}: {}", f.rule, f.path, f.msg);
    }
    errors
}

// ------------------------------------------------------------------------------------------------------- setup

/// `only`: "tools" — only programs from tools.lock, "ours" — only ws and DevKit (steps of bootstrap.ps1 -Step tools /
/// devkit); `tool`: only these programs, comma-separated (retrying one failed download).
fn setup(
    ws: &Workspace,
    apply: bool,
    build: Build,
    only: Option<String>,
    tool: Option<String>,
) -> Result<(), String> {
    let only = only.as_deref();
    if !matches!(only, None | Some("tools" | "ours")) {
        return Err("setup --only=tools|ours".into());
    }
    let mut errors = 0;
    if only != Some("ours") {
        let names: Vec<&str> = match &tool {
            Some(t) => t
                .split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .collect(),
            None => build_tools(),
        };
        println!(
            "== tools ({}{}; tools.lock -> {})",
            build.name(),
            if xwin() {
                ", no Visual Studio: + LLVM"
            } else {
                ""
            },
            ws.home().join("tools").display()
        );
        errors = print_findings(&bootstrap::tools_named(ws, &names, apply, build));
        if only == Some("tools") || tool.is_some() {
            if !apply && errors == 0 {
                println!("\nnothing changed - add --apply to do it");
            }
            return match (errors, apply) {
                (0, _) => Ok(()),
                (n, true) => Err(format!("{n} tool(s) failed to install")),
                (n, false) => Err(format!("{n} tool(s) missing")),
            };
        }
    }

    println!("\n== ws and DevKit ({})", build.name());
    let ours = match build {
        Build::Prebuilt => ours_prebuilt(ws, apply)?,
        Build::Source => None,
    };
    if !apply {
        if ours.is_none() {
            println!(
                "plan: build ws and DevKit here into {}",
                target_dir(ws).display()
            );
        }
        println!("\nnothing changed - run `cargo xtask setup --apply` to do it");
        return if errors == 0 {
            Ok(())
        } else {
            Err(format!("{errors} tool(s) missing"))
        };
    }
    if errors > 0 {
        return Err(format!("{errors} tool(s) failed to install"));
    }
    let (devkit_exe, ws_exe) = match ours {
        Some(paths) => paths,
        None => {
            run(cargo(ws).args(["build", "--locked", "--release", "-p", "ws"]))?;
            let built_dk = devkit(ws)?;
            let built_ws = target_dir(ws).join("release").join("ws.exe");
            // Into home\tools, like a ready release: the build cache (home\cache\target) can be deleted without losing DevKit and ws
            let (ws_dst, dk_dst) = (
                ws_core::native(&ws.home().join("tools/ws/ws.exe")),
                ws_core::native(&ws.home().join("tools/devkit/DevKit.exe")),
            );
            for (from, to) in [(&built_ws, &ws_dst), (&built_dk, &dk_dst)] {
                if let Some(dir) = to.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                std::fs::copy(from, to).map_err(|e| {
                    format!(
                        "{} -> {}: {e} (is DevKit running? close it)",
                        from.display(),
                        to.display()
                    )
                })?;
            }
            (dk_dst, ws_dst)
        }
    };
    println!(
        "\nready:\n  DevKit  {}\n  ws      {}",
        devkit_exe.display(),
        ws_exe.display()
    );
    println!("next: run DevKit (Overview -> Fix), or `cargo ws bootstrap --profile <owner|dev|designer|agent> --apply`");
    Ok(())
}

/// Ready ws and DevKit from the `dist` release for the current commit: SHA256SUMS signature with the key from xtask/dist.pub, file
/// hashes. None — no release (build here); an error — a release exists but fails the check (install nothing).
fn ours_prebuilt(ws: &Workspace, apply: bool) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let fallback = |why: String| {
        println!("no prebuilt ws/DevKit: {why} - building here");
        Ok(None)
    };
    let Ok(pk) = std::fs::read_to_string(ws.root.join(PUBKEY)) else {
        return fallback(format!(
            "{PUBKEY} is missing (no signing key yet: cargo xtask dist keygen)"
        ));
    };
    if dirty(ws) {
        return fallback("uncommitted changes in tracked files".into());
    }
    let mut id = dist_id(ws)?;
    let mut dir = dist_source(ws).join(&id);
    if !dir.join("SHA256SUMS.minisig").is_file() {
        // no release for this commit — a suitable one from the artifact store (an earlier commit where ws and DevKit are the same)
        match store_dist(ws, apply)? {
            Some((sid, sdir)) => (id, dir) = (sid, sdir),
            None => {
                // 18.11: without the store password the store was not asked at all - say so, not "not there"
                let store = match ws_core::store::Store::open(ws, false) {
                    Ok(_) => "or in the artifact store".to_string(),
                    Err(ws_core::store::Missing::NotConfigured) => {
                        "(the artifact store is not set up: local.toml [store])".to_string()
                    }
                    Err(ws_core::store::Missing::NoPassword(_)) => "(the artifact store was not asked: no password - \
                         open the vault session, cargo ws secrets unlock, or cargo ws secrets run artifact_store -- \
                         cargo xtask setup ...)"
                        .to_string(),
                };
                return fallback(format!("no signed dist {id} in {} {store}", dir.display()));
            }
        }
    }
    if !apply && !dir.join("SHA256SUMS.minisig").is_file() {
        // plan: the release is in the artifact store, not downloaded yet (18.11: the plan used to say "building here")
        let (ws_dst, dk_dst) = (
            ws_core::native(&ws.home().join("tools/ws/ws.exe")),
            ws_core::native(&ws.home().join("tools/devkit/DevKit.exe")),
        );
        println!(
            "plan: check its signature, copy ws.exe -> {}, DevKit.exe -> {}",
            ws_dst.display(),
            dk_dst.display()
        );
        return Ok(Some((dk_dst, ws_dst)));
    }
    verify_dir(&dir, &pk, &trusted("dist", &id), &DIST_NEED)
        .map_err(|e| format!("dist {id} in {} failed verification: {e}", dir.display()))?;
    // the commit comes from the signed dist.toml (the release index in the store is not signed): ws and DevKit have not changed since
    let commit = dist_commit(&dir)?;
    if !dist_usable(ws, &commit) {
        return fallback(format!(
            "dist {id} ({commit:.10}): ws or DevKit sources changed since - building here"
        ));
    }
    println!("ok    signed dist {id}: {}", dir.display());
    let (ws_dst, dk_dst) = (
        ws_core::native(&ws.home().join("tools/ws/ws.exe")),
        ws_core::native(&ws.home().join("tools/devkit/DevKit.exe")),
    );
    if !apply {
        println!(
            "plan: copy ws.exe -> {}, DevKit.exe -> {}",
            ws_dst.display(),
            dk_dst.display()
        );
        return Ok(Some((dk_dst, ws_dst)));
    }
    for (name, dst) in [("ws.exe", &ws_dst), ("DevKit.exe", &dk_dst)] {
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::copy(dir.join(name), dst).map_err(|e| format!("{name}: {e}"))?;
        println!("DONE  {name} -> {}", dst.display());
    }
    Ok(Some((dk_dst, ws_dst)))
}

/// What ws.exe and DevKit.exe depend on: a release of another commit will do if nothing here has changed since.
const DIST_INPUTS: [&str; 6] = [
    "tools/ws",
    "apps/devkit",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo",
];
/// Release index in the store: lines "<id> <commit>", newest at the bottom. Not signed — each release is signed.
const DIST_INDEX: &str = "dist-index.txt";

/// The release is built from `commit`, which is an ancestor of HEAD, and DIST_INPUTS have not changed since.
fn dist_usable(ws: &Workspace, commit: &str) -> bool {
    let head = git(ws, &["rev-parse", "HEAD"]).unwrap_or_default();
    if commit == head {
        return true;
    }
    let mut diff = vec!["diff", "--quiet", commit, "HEAD", "--"];
    diff.extend(DIST_INPUTS);
    git(ws, &["merge-base", "--is-ancestor", commit, "HEAD"]).is_ok() && git(ws, &diff).is_ok()
}

fn dist_commit(dir: &Path) -> Result<String, String> {
    let doc = ws_core::read_toml(&dir.join("dist.toml"))?;
    doc.get("commit")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "dist.toml: no commit".into())
}

/// A suitable release from the store (local.toml [store], password — ws secrets run artifact_store): the newest from
/// the index that fits HEAD; it is downloaded and unpacked into dist_source/<id>. No store or password — None.
fn store_dist(ws: &Workspace, apply: bool) -> Result<Option<(String, PathBuf)>, String> {
    let Ok(store) = ws_core::store::Store::open(ws, false) else {
        return Ok(None);
    };
    let tmp = ws.home().join("cache/tmp");
    let idx = tmp.join(format!("{DIST_INDEX}.part"));
    if !store.get(DIST_INDEX, &idx)? {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&idx).unwrap_or_default();
    let _ = std::fs::remove_file(&idx);
    for line in text.lines().rev() {
        let mut it = line.split_whitespace();
        let (Some(id), Some(commit)) = (it.next(), it.next()) else {
            continue;
        };
        if !dist_usable(ws, commit) {
            continue;
        }
        let dir = dist_source(ws).join(id);
        if !dir.join("SHA256SUMS.minisig").is_file() {
            if !apply {
                println!("plan: download signed dist {id} from the artifact store");
                // the caller plans the copy; the signature is checked after the download (with --apply)
                return Ok(Some((id.to_string(), dir)));
            }
            let part = tmp.join(format!("dist-{id}.tar.part"));
            if !store.get(&format!("dist-{id}.tar"), &part)? {
                return Err(format!(
                    "artifact store: {DIST_INDEX} lists {id}, but dist-{id}.tar is missing"
                ));
            }
            ws_core::store::install(&part, &dir, true)?;
            println!(
                "DONE  signed dist {id} from the artifact store -> {}",
                dir.display()
            );
        }
        return Ok(Some((id.to_string(), dir)));
    }
    Ok(None)
}

/// `cargo xtask dist push` (owner; ws secrets run artifact_store_push -- …): signed releases from
/// home/out/dist that are not yet in the store index go there (tar), the index is extended.
fn dist_push(ws: &Workspace) -> Result<(), String> {
    let store = ws_core::store::Store::open(ws, true).map_err(|_| {
        "artifact store: local.toml [store] push_url/push_user and ARTIFACT_STORE_PUSH_PASSWORD (ws secrets run artifact_store_push -- cargo xtask dist push)".to_string()
    })?;
    let pk = std::fs::read_to_string(ws.root.join(PUBKEY)).map_err(|e| format!("{PUBKEY}: {e}"))?;
    let tmp = ws.home().join("cache/tmp/dist-push");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let idx = tmp.join(DIST_INDEX);
    let mut index = if store.get(DIST_INDEX, &idx)? {
        std::fs::read_to_string(&idx).unwrap_or_default()
    } else {
        String::new()
    };
    let listed: Vec<String> = index
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(String::from))
        .collect();
    let mut dists: Vec<(std::time::SystemTime, String, PathBuf)> =
        std::fs::read_dir(dist_source(ws))
            .map_err(|e| format!("{}: {e}", dist_source(ws).display()))?
            .flatten()
            .filter(|e| e.path().join("SHA256SUMS.minisig").is_file())
            .map(|e| {
                let t = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (t, e.file_name().to_string_lossy().to_string(), e.path())
            })
            .collect();
    dists.sort();
    let mut pushed = 0;
    for (_, id, dir) in dists {
        if listed.contains(&id) || id.ends_with("-dirty") {
            continue;
        }
        verify_dir(&dir, &pk, &trusted("dist", &id), &DIST_NEED)
            .map_err(|e| format!("dist {id}: {e} - not pushed"))?;
        let commit = dist_commit(&dir)?;
        let (tar, _, _) = ws_core::store::object_for(&dir, &tmp)?;
        store.put(&format!("dist-{id}.tar"), &tar)?;
        index.push_str(&format!("{id} {commit}\n"));
        println!("DONE  dist {id} ({commit:.10}) -> artifact store");
        pushed += 1;
    }
    if pushed == 0 {
        println!(
            "nothing to push: every signed dist in {} is already in the store",
            dist_source(ws).display()
        );
    } else {
        std::fs::write(&idx, &index).map_err(|e| e.to_string())?;
        store.put(DIST_INDEX, &idx)?;
        println!("DONE  {DIST_INDEX}: {} dist(s)", index.lines().count());
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// Where to take releases from: local.toml [dist] source (a folder; "home:" is expanded), else home/out/dist.
fn dist_source(ws: &Workspace) -> PathBuf {
    ws.local_str("dist", "source")
        .and_then(|p| ws.resolve(&p))
        .unwrap_or_else(|| ws.home().join("out").join("dist"))
}

// ------------------------------------------------------------------------------------------------------- devkit, ci

fn devkit(ws: &Workspace) -> Result<PathBuf, String> {
    println!("\n== DevKit");
    let tauri = tool(ws, "tauri-cli")?;
    let mut c = Command::new(tauri);
    c.args(["build", "--no-bundle"])
        .current_dir(ws.root.join("apps/devkit/src-tauri"));
    run(env(ws, &mut c)?)?;
    let exe = target_dir(ws).join("release").join("DevKit.exe");
    exe.is_file()
        .then_some(exe.clone())
        .ok_or_else(|| format!("build finished but {} is missing", exe.display()))
}

fn ci(ws: &Workspace) -> Result<(), String> {
    let mut failed: Vec<&str> = Vec::new();
    let mut step = |name: &'static str, r: Result<(), String>| match &r {
        Ok(()) => println!("-- {name}: ok"),
        Err(e) => {
            println!("-- {name}: FAILED ({e})");
            failed.push(name);
        }
    };
    // tauri-build requires the ready DevKit page (ui/dist) already at compile time
    println!("== DevKit page (trunk build)");
    let mut c = Command::new(tool(ws, "trunk")?);
    c.arg("build").current_dir(ws.root.join("apps/devkit/ui"));
    step("page", env(ws, &mut c).and_then(run));
    println!("\n== fmt");
    step("fmt", run(cargo(ws).args(["fmt", "--all", "--check"])));
    println!("\n== clippy");
    step(
        "clippy",
        run(cargo(ws).args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ])),
    );
    println!("\n== test");
    step(
        "test",
        run(cargo(ws).args(["test", "--workspace", "--locked"])),
    );
    println!("\n== ws check");
    let found: Vec<Finding> = ws_core::check::run(ws)
        .into_iter()
        .filter(|f| !matches!(f.level, Level::Info))
        .collect();
    let errors = print_findings(&found);
    step(
        "ws check",
        if errors == 0 {
            Ok(())
        } else {
            Err(format!("{errors} error(s)"))
        },
    );
    if failed.is_empty() {
        println!("\nci: all passed");
        Ok(())
    } else {
        Err(format!("ci failed: {}", failed.join(", ")))
    }
}

// ------------------------------------------------------------------------------------------------------- dist

/// Release: ws and DevKit built here + dist.toml + SHA256SUMS (+ signature) in home/out/dist/<id>.
fn dist(ws: &Workspace, sign: bool, allow_dirty: bool) -> Result<(), String> {
    let is_dirty = dirty(ws);
    if is_dirty && !allow_dirty {
        return Err("uncommitted changes in tracked files - commit first (or --allow-dirty for a local test)".into());
    }
    let id = format!("{}{}", dist_id(ws)?, if is_dirty { "-dirty" } else { "" });
    // the key is checked before the long build
    let key = if sign { Some(signing_key(ws)?) } else { None };
    run(cargo(ws).args(["build", "--locked", "--release", "-p", "ws"]))?;
    let devkit_exe = devkit(ws)?;
    let out = ws.home().join("out").join("dist").join(&id);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::copy(
        target_dir(ws).join("release").join("ws.exe"),
        out.join("ws.exe"),
    )
    .map_err(|e| e.to_string())?;
    std::fs::copy(devkit_exe, out.join("DevKit.exe")).map_err(|e| e.to_string())?;
    let rustc = Command::new("rustc")
        .arg("--version")
        .current_dir(&ws.root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let meta = format!(
        "# cargo xtask dist (docs/WORKSPACE_PLAN.md, section 13)\nid = \"{id}\"\ncommit = \"{}\"\nrustc = \"{rustc}\"\n",
        git(ws, &["rev-parse", "HEAD"])?
    );
    std::fs::write(out.join("dist.toml"), meta).map_err(|e| e.to_string())?;
    let sums = write_sums(&out, &DIST_FILES)?;
    println!("\ndist {id}: {}", out.display());
    match key {
        Some((sk, pk)) => {
            sign_sums(&out, &sk, &trusted("dist", &id))?;
            verify_dir(&out, &pk, &trusted("dist", &id), &DIST_NEED)?;
            println!("signed: SHA256SUMS.minisig (checked with {PUBKEY})");
        }
        None => println!("unsigned - setup --prebuilt accepts only signed dists (--sign)"),
    }
    print!("{}", String::from_utf8_lossy(&sums));
    Ok(())
}

/// Secret key from the file in DIST_SIGNING_KEY (ws secrets run puts the database attachment into a temp file) + public key
/// from xtask/dist.pub — we sign only with a key that matches the public one in git.
fn signing_key(ws: &Workspace) -> Result<(minisign::SecretKey, String), String> {
    let path = std::env::var_os("DIST_SIGNING_KEY").ok_or(
        "DIST_SIGNING_KEY is not set: ws secrets run dist_signing_key -- cargo xtask dist --sign",
    )?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("DIST_SIGNING_KEY: {e}"))?;
    let sk = SecretKeyBox::from_string(&text)
        .and_then(|b| b.into_unencrypted_secret_key())
        .map_err(|e| format!("DIST_SIGNING_KEY: {e}"))?;
    let pk = std::fs::read_to_string(ws.root.join(PUBKEY)).map_err(|e| format!("{PUBKEY}: {e}"))?;
    Ok((sk, pk))
}

fn keygen(ws: &Workspace, secret_out: &Path) -> Result<(), String> {
    let pub_path = ws.root.join(PUBKEY);
    if pub_path.exists() {
        return Err(format!(
            "{PUBKEY} already exists - a new key would invalidate all signed dists"
        ));
    }
    let abs = std::path::absolute(secret_out).map_err(|e| e.to_string())?;
    if abs.starts_with(&ws.root) {
        return Err(
            "--secret-out must be outside the workspace (the secret key must never reach git)"
                .into(),
        );
    }
    if abs.exists() {
        return Err(format!("{} already exists", abs.display()));
    }
    let kp = KeyPair::generate_unencrypted_keypair().map_err(|e| e.to_string())?;
    let pk = kp.pk.to_box().map_err(|e| e.to_string())?.into_string();
    let sk = kp
        .sk
        .to_box(Some("dungeonborne dist signing key"))
        .map_err(|e| e.to_string())?
        .into_string();
    std::fs::write(&abs, sk).map_err(|e| format!("{}: {e}", abs.display()))?;
    std::fs::write(&pub_path, pk).map_err(|e| format!("{PUBKEY}: {e}"))?;
    println!("public key: {} (commit it)", pub_path.display());
    println!("secret key: {}", abs.display());
    println!("next: attach the secret key file to the KeePassXC entry of `dist_signing_key` (secrets.toml), then delete it");
    Ok(())
}

/// SHA256SUMS: "<sha256>  <name>", one line per file.
fn write_sums(dir: &Path, names: &[&str]) -> Result<Vec<u8>, String> {
    let mut sums = String::new();
    for name in names {
        let h =
            ws_core::sha256_file(&dir.join(name)).ok_or_else(|| format!("{name}: cannot read"))?;
        sums.push_str(&format!("{h}  {name}\n"));
    }
    std::fs::write(dir.join("SHA256SUMS"), &sums).map_err(|e| e.to_string())?;
    Ok(sums.into_bytes())
}

/// The trusted comment of the signature binds it to the kind and id of the release: a ws/DevKit release signature will not fit the
/// designer package, a signature of one commit will not fit another. kind: "dist" (ws + DevKit), "pack-designer".
fn trusted(kind: &str, id: &str) -> String {
    format!("dungeonborne-{kind} {id}")
}

fn sign_sums(dir: &Path, sk: &minisign::SecretKey, tc: &str) -> Result<(), String> {
    let sums = std::fs::read(dir.join("SHA256SUMS")).map_err(|e| e.to_string())?;
    let sig =
        minisign::sign(None, sk, Cursor::new(sums), Some(tc), None).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("SHA256SUMS.minisig"), sig.into_string()).map_err(|e| e.to_string())
}

/// Checks SHA256SUMS: signature with the public key, trusted comment `tc`, hash of every listed file, files `need` in the list.
fn verify_dir(dir: &Path, pubkey: &str, tc_want: &str, need: &[&str]) -> Result<(), String> {
    let sums = std::fs::read(dir.join("SHA256SUMS")).map_err(|e| format!("SHA256SUMS: {e}"))?;
    let sig_text = std::fs::read_to_string(dir.join("SHA256SUMS.minisig"))
        .map_err(|e| format!("signature: {e}"))?;
    let sig = SignatureBox::from_string(&sig_text).map_err(|e| format!("signature: {e}"))?;
    let pk = PublicKeyBox::from_string(pubkey)
        .and_then(|b| b.into_public_key())
        .map_err(|e| format!("{PUBKEY}: {e}"))?;
    minisign::verify(&pk, &sig, Cursor::new(&sums), true, false, false)
        .map_err(|e| format!("signature: {e}"))?;
    let tc = sig.trusted_comment().map_err(|e| e.to_string())?;
    if tc != tc_want {
        return Err(format!("signature is for '{tc}', expected '{tc_want}'"));
    }
    let text = String::from_utf8(sums).map_err(|e| e.to_string())?;
    let mut seen = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (want, name) = line
            .split_once("  ")
            .ok_or_else(|| format!("bad SHA256SUMS line: {line}"))?;
        if name.contains(['/', '\\']) || name.contains("..") {
            return Err(format!("bad file name in SHA256SUMS: {name}"));
        }
        if ws_core::sha256_file(&dir.join(name)).as_deref() != Some(want) {
            return Err(format!("{name}: sha256 does not match SHA256SUMS"));
        }
        seen.push(name.to_string());
    }
    for need in need {
        if !seen.iter().any(|n| n == need) {
            return Err(format!("{need} is not in SHA256SUMS"));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------------- pack

/// Designer package (docs/PACKAGES.md, plan 13.4 item 5): Balance Studio, the balance kit of the active season (backend
/// xtask `balance kit`), instructions, AGENTS.md for the designer's AI, SHA256SUMS (+ signature) — a folder and a zip in
/// home/out/pack. No secrets and no git in the package.
fn pack_designer(ws: &Workspace, sign: bool, allow_dirty: bool) -> Result<(), String> {
    let is_dirty = dirty(ws);
    if is_dirty && !allow_dirty {
        return Err("uncommitted changes in tracked files - commit first (or --allow-dirty for a local test)".into());
    }
    let id = format!("{}{}", dist_id(ws)?, if is_dirty { "-dirty" } else { "" });
    let key = if sign { Some(signing_key(ws)?) } else { None };

    println!("\n== Balance Studio");
    let studio = ws.root.join("apps/studio");
    let conf = std::fs::read_to_string(studio.join("src-tauri/tauri.conf.json"))
        .map_err(|e| e.to_string())?;
    let version = serde_json::from_str::<serde_json::Value>(&conf)
        .ok()
        .and_then(|v| v["version"].as_str().map(String::from))
        .ok_or("apps/studio/src-tauri/tauri.conf.json: no version")?;
    let mut c = Command::new(tool(ws, "tauri-cli")?);
    c.args(["build", "--no-bundle"])
        .current_dir(studio.join("src-tauri"));
    run(env(ws, &mut c)?)?;
    let studio_exe = target_dir(ws).join("release").join("balance-studio.exe");

    println!("\n== balance kit (cargo xtask balance kit in core/backend)");
    let season = std::fs::read_to_string(ws.root.join("content/seasons/active.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let kit_dir = match &season {
        Some(s) => ws.root.join("content/seasons").join(s).join("kit"),
        None => ws.root.join("content/balance/kit/out"),
    };
    let started = std::time::SystemTime::now();
    let mut k = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    k.args(["xtask", "balance", "kit"])
        .current_dir(ws.root.join("core/backend"))
        .env("CARGO_TARGET_DIR", target_dir(ws));
    run(&mut k)?;
    let kit_zip = std::fs::read_dir(&kit_dir)
        .map_err(|e| format!("{}: {e}", kit_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "zip"))
        .filter_map(|p| {
            std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| (t, p))
        })
        .filter(|(t, _)| *t >= started)
        .max()
        .map(|(_, p)| p)
        .ok_or_else(|| format!("balance kit: no new zip in {}", kit_dir.display()))?;

    println!("\n== package");
    let name = format!("DungeonborneBalance-{version}-{id}");
    let packs = ws.home().join("out").join("pack");
    let out = packs.join(&name);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let exe_name = format!("BalanceStudio-{version}.exe");
    let kit_name = kit_zip.file_name().unwrap().to_string_lossy().to_string();
    let mut files: Vec<(String, PathBuf)> =
        vec![(exe_name.clone(), studio_exe), (kit_name.clone(), kit_zip)];
    // names in the package — Latin only: PowerShell 5.1 reads SHA256SUMS as ANSI and breaks Cyrillic
    for (dst, src) in [
        ("README.md", "GUIDE.md"),
        ("CHANGELOG.md", "CHANGELOG.md"),
        ("AGENTS.md", "AGENTS.md"),
    ] {
        files.push((dst.to_string(), studio.join("docs").join(src)));
    }
    if ws.root.join(PUBKEY).is_file() {
        files.push(("signing-key.pub".into(), ws.root.join(PUBKEY)));
    }
    for (dst, src) in &files {
        std::fs::copy(src, out.join(dst)).map_err(|e| format!("{}: {e}", src.display()))?;
    }
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    write_sums(&out, &names)?;
    let tc = trusted("pack-designer", &id);
    match key {
        Some((sk, pk)) => {
            sign_sums(&out, &sk, &tc)?;
            verify_dir(&out, &pk, &tc, &[&exe_name, &kit_name])?;
            println!("signed: SHA256SUMS.minisig ({tc})");
        }
        None => println!(
            "unsigned (--sign to sign; AGENTS.md tells the designer's AI to expect a signature)"
        ),
    }
    let zip = packs.join(format!("{name}.zip"));
    let _ = std::fs::remove_file(&zip);
    let mut z = Command::new(windows_tar());
    z.arg("-a")
        .arg("-cf")
        .arg(&zip)
        .arg("-C")
        .arg(&packs)
        .arg(&name);
    run(&mut z)?;
    let mb = std::fs::metadata(&zip)
        .map(|m| m.len() as f64 / 1048576.0)
        .unwrap_or(0.0);
    println!(
        "\npackage {name}:\n  folder {}\n  zip    {} ({mb:.1} MB)",
        out.display(),
        zip.display()
    );
    for n in &names {
        println!("    {n}");
    }
    Ok(())
}

/// tar.exe from Windows (bsdtar: does zip by extension); Git Bash may slip in its own GNU tar without zip.
fn windows_tar() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32").join("tar.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("tar"))
}

// ------------------------------------------------------------------------------------------------- export public

/// Edition in DB_HOME/out/export/<edition>-<id>: ws_core::export (edition-level files, replacements, files on top,
/// a check of every line), then in the copy `git init` + `git add`, the first commit from the neutral author from
/// export/<edition>.toml (without it — no commit) and `ws check` with its own rules. Nothing is sent outside.
fn export_edition(
    ws: &Workspace,
    edition: &str,
    allow_dirty: bool,
    update: Option<PathBuf>,
    message: Option<String>,
) -> Result<(), String> {
    if update.is_some() && dirty(ws) {
        return Err("--update needs a committed workspace (no --allow-dirty)".into());
    }
    let mut id = dist_id(ws)?;
    if dirty(ws) {
        if !allow_dirty {
            return Err(
                "uncommitted changes - commit first, or --allow-dirty for a trial export".into(),
            );
        }
        id.push_str("-dirty");
    }
    let out = ws
        .home()
        .join("out")
        .join("export")
        .join(format!("{edition}-{id}"));
    let spec_file = ws_core::export::spec_file(edition);
    let rep = ws_core::export::run(ws, edition, &out, &id)?;
    println!(
        "export {}: {} files ({} with replacements, {} from the overlay), {} excluded by {spec_file}",
        out.display(),
        rep.files,
        rep.scrubbed,
        rep.overlay,
        rep.excluded
    );
    if !rep.leaks.is_empty() {
        for l in rep.leaks.iter().take(80) {
            println!("  LEAK {}:{}  {}", l.path, l.line, l.what);
        }
        if rep.leaks.len() > 80 {
            println!("  ... and {} more", rep.leaks.len() - 80);
        }
        return Err(format!(
            "{} finding(s) in the export - fix the source or add a replacement to {spec_file}; the folder is \
             kept for inspection, do not publish it",
            rep.leaks.len()
        ));
    }
    let g = |args: &[&str]| -> Result<(), String> {
        let st = Command::new("git")
            .args(["-c", "safe.directory=*", "-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(&out)
            .status()
            .map_err(|e| format!("git: {e}"))?;
        st.success()
            .then_some(())
            .ok_or(format!("git {} failed in the export", args.join(" ")))
    };
    g(&["init", "-q", "-b", "main"])?;
    let mark = ws_core::export::MARK;
    std::fs::rename(out.join(mark), out.join(".git").join(mark))
        .map_err(|e| format!("{mark}: {e}"))?;
    g(&["add", "-A"])?;
    // Author from the edition spec: the owner's personal name, mail and time zone do not get into the copy. It is also the git config
    // of the copy, so that later commits in it do not take the machine's global settings either.
    let author = ws_core::export::spec(ws, edition)?.author;
    if let Some((name, email)) = &author {
        g(&["config", "user.name", name])?;
        g(&["config", "user.email", email])?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let date = format!("{now} +0000");
        let st = Command::new("git")
            .args(["-c", "safe.directory=*", "-c", "core.autocrlf=false"])
            .args(["commit", "-q", "-m"])
            .arg(format!(
                "Dungeonborne community framework: {edition} edition"
            ))
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .current_dir(&out)
            .status()
            .map_err(|e| format!("git: {e}"))?;
        if !st.success() {
            return Err("git commit failed in the export".into());
        }
    }
    // ws check by the rules of the export itself (its workspace.toml: the edition's own in the overlay or the shared one)
    run(cargo(ws)
        .args([
            "run",
            "--quiet",
            "--release",
            "--package",
            "ws",
            "--",
            "check",
        ])
        .arg(format!("--root={}", out.display())))?;
    println!(
        "ok: no leaks, ws check passes in the copy. It is a git repo with {}.\n\
         Next (owner): read it, build it (`cargo xtask setup --apply`, `cargo xtask ci` inside), then publish only on \
         the owner's word.",
        match &author {
            Some((n, e)) => format!("one commit by {n} <{e}> (UTC)"),
            None => "everything staged and no commit".into(),
        }
    );
    match update {
        Some(repo) => update_edition(
            &out,
            &repo,
            &spec_file,
            author.as_ref(),
            message
                .clone()
                .unwrap_or(format!("Update {edition} edition"))
                .as_str(),
        ),
        None => Ok(()),
    }
}

/// git in a folder, stdout output; without line-ending autoconversion (as in the export).
fn git_in(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let mut c = Command::new("git");
    c.args(["-c", "safe.directory=*", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir);
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(format!(
            "git {} in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&o.stderr).trim()
        ))
    }
}

/// Export `out` → a new commit in a clone of the published repository `repo`: files are replaced by the export's files,
/// the tree is checked against the export tree, the commit — neutral author, UTC. History is not rewritten, no push is made.
fn update_edition(
    out: &Path,
    repo: &Path,
    spec_file: &str,
    author: Option<&(String, String)>,
    message: &str,
) -> Result<(), String> {
    let (name, email) = author.ok_or(format!(
        "--update needs [export] author and author_email in {spec_file}"
    ))?;
    if !repo.join(".git").is_dir() {
        return Err(format!(
            "{}: not a git clone of the published repository",
            repo.display()
        ));
    }
    if !git_in(repo, &["status", "--porcelain"], &[])?.is_empty() {
        return Err(format!(
            "{}: has changes or untracked files - clean it first",
            repo.display()
        ));
    }
    println!("\n== update {}", repo.display());
    let split = |s: String| -> Vec<String> {
        s.split('\0')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    };
    // old files go away (and emptied folders), new ones come from the export per its git ls-files
    for rel in split(git_in(repo, &["ls-files", "-z"], &[])?) {
        let p = repo.join(&rel);
        std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut d = p.parent();
        while let Some(dir) = d {
            if dir == repo || std::fs::remove_dir(dir).is_err() {
                break;
            }
            d = dir.parent();
        }
    }
    for rel in split(git_in(out, &["ls-files", "-z"], &[])?) {
        let (from, to) = (out.join(&rel), repo.join(&rel));
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    git_in(repo, &["add", "-A"], &[])?;
    let want = git_in(out, &["rev-parse", "HEAD^{tree}"], &[])?;
    let got = git_in(repo, &["write-tree"], &[])?;
    if want != got {
        return Err(format!(
            "tree {got} in {} differs from the export's {want} - not committed, look at `git -C {} status`",
            repo.display(),
            repo.display()
        ));
    }
    if git_in(repo, &["rev-parse", "HEAD^{tree}"], &[])? == want {
        println!(
            "nothing to update: {} already has this tree",
            repo.display()
        );
        return Ok(());
    }
    git_in(repo, &["config", "user.name", name], &[])?;
    git_in(repo, &["config", "user.email", email], &[])?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let date = format!("{now} +0000");
    git_in(
        repo,
        &["commit", "-q", "-m", message],
        &[("GIT_AUTHOR_DATE", &date), ("GIT_COMMITTER_DATE", &date)],
    )?;
    let stat = git_in(repo, &["diff", "--shortstat", "HEAD~1", "HEAD"], &[])?;
    let head = git_in(repo, &["log", "-1", "--format=%h %an <%ae> %ad"], &[])?;
    println!("committed {head}\n  {stat}\n  tree identical to the export ({want})");
    println!(
        "push only on the owner's word: git -C {} push",
        repo.display()
    );
    Ok(())
}

// ------------------------------------------------------------------------------------------------------- helpers

/// Release name: a tag on HEAD, else the short commit hash.
fn dist_id(ws: &Workspace) -> Result<String, String> {
    git(ws, &["describe", "--tags", "--exact-match", "HEAD"])
        .or_else(|_| git(ws, &["rev-parse", "--short=10", "HEAD"]))
}

/// There are uncommitted edits in tracked files — by content, not by `git status`: cargo tauri build
/// rewrites src-tauri/Cargo.toml with the same text, and status considers it changed (17.11, because of this a clean install
/// built ws and DevKit itself instead of the signed release).
fn dirty(ws: &Workspace) -> bool {
    git(ws, &["diff", "--quiet", "HEAD", "--"]).is_err()
}

fn git(ws: &Workspace, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git")
        .args(["-c", "safe.directory=*"])
        .args(args)
        .current_dir(&ws.root)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ))
    }
}

/// A program from tools.lock installed in home; else — a hint about setup.
fn tool(ws: &Workspace, name: &str) -> Result<PathBuf, String> {
    bootstrap::tool_exe(ws, name)
        .ok_or_else(|| format!("{name} is not installed - run `cargo xtask setup --apply`"))
}

/// Shared target: CARGO_TARGET_DIR, else home/cache/target (as for agents and DevKit).
fn target_dir(ws: &Workspace) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| ws.home().join("cache").join("target"))
}

fn cargo(ws: &Workspace) -> Command {
    let mut c = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    c.current_dir(&ws.root)
        .env("CARGO_TARGET_DIR", target_dir(ws));
    c
}

/// Build environment for windows: trunk / tauri-cli / tailwindcss folders first in PATH (only for this command).
fn env<'a>(ws: &Workspace, c: &'a mut Command) -> Result<&'a mut Command, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for name in build_tools() {
        dirs.extend(tool(ws, name)?.parent().map(Path::to_path_buf));
    }
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path: OsString = std::env::join_paths(dirs).map_err(|e| e.to_string())?;
    Ok(c.env("PATH", path)
        .env("NO_COLOR", "true")
        .env("CARGO_TARGET_DIR", target_dir(ws)))
}

fn run(c: &mut Command) -> Result<(), String> {
    let st = c
        .status()
        .map_err(|e| format!("{:?}: {e}", c.get_program()))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("{:?} exit code {:?}", c.get_program(), st.code()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Release signature: passes as is, fails on a replaced file, a foreign id, a foreign key.
    #[test]
    fn dist_signature_roundtrip() {
        let dir = std::env::temp_dir().join(format!("xtask-dist-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in [
            ("ws.exe", "ws"),
            ("DevKit.exe", "devkit"),
            ("dist.toml", "id = \"t\""),
        ] {
            std::fs::write(dir.join(name), body).unwrap();
        }
        write_sums(&dir, &DIST_FILES).unwrap();
        let kp = KeyPair::generate_unencrypted_keypair().unwrap();
        let pk = kp.pk.to_box().unwrap().into_string();
        let tc = trusted("dist", "abc");
        sign_sums(&dir, &kp.sk, &tc).unwrap();
        assert!(verify_dir(&dir, &pk, &tc, &DIST_NEED).is_ok());
        assert!(
            verify_dir(&dir, &pk, &trusted("pack-designer", "abc"), &DIST_NEED).is_err(),
            "signature of another release kind"
        );
        assert!(
            verify_dir(&dir, &pk, &trusted("dist", "other"), &DIST_NEED).is_err(),
            "signature of another release"
        );
        let other = KeyPair::generate_unencrypted_keypair()
            .unwrap()
            .pk
            .to_box()
            .unwrap()
            .into_string();
        assert!(
            verify_dir(&dir, &other, &tc, &DIST_NEED).is_err(),
            "foreign key"
        );
        std::fs::write(dir.join("DevKit.exe"), "tampered").unwrap();
        assert!(
            verify_dir(&dir, &pk, &tc, &DIST_NEED).is_err(),
            "replaced file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
