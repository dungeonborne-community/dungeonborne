// ws - Dungeonborne workspace tool (docs/WORKSPACE_PLAN.md). Run from anywhere inside the workspace.
//
//   ws check                          layout, machine paths, binaries, secrets, member links (before every commit)
//   ws doctor   [--profile=<p>]       what is wrong on this machine: local.toml, home, tools, artifacts, agent env
//   ws bootstrap [--profile=<p>]      same, and fix it: create folders, install tools and artifacts, agent env
//                [--prebuilt|--from-source]   tools of kind "cargo": official release exe (sha256 from tools.lock) or
//                                    cargo install; default - local.toml [setup] build, else prebuilt
//                [--artifact=<name>]   also fetch an on-demand artifact (artifacts.toml manual = true), repeatable
//   ws artifacts push [--apply]       owner: artifacts of this machine -> artifact store (local.toml [store] push_url,
//                                    push_user; password: ws secrets run artifact_store_push -- ...), store_sha256
//                                    into artifacts.toml. ws bootstrap downloads them (ws secrets run artifact_store)
//   ws components list                components (config/components.toml), their members, what is on disk, presets
//   ws components which <path>        which component holds a path (game/ue4ss/Mods/X -> mods) and is it on disk
//   ws components add|remove|set <names|preset|all> [--apply]   put components on disk or take them off (git sparse
//                                    checkout; with what they need); the choice in local.toml [components] installed,
//                                    ws bootstrap then installs only their programs and data (docs/COMPONENTS_PLAN.md)
//   ws dump [--set=<a,b>] [--build=<id>] [--apply]   game data from home/game/original/<build> into
//                                    home/dumps/<build>/export: icons (PNG), monsters, heroes, curves (JSON) by
//                                    tools/dumper/sets.toml; the pak key from the vault session (ws secrets unlock), or
//                                    ws secrets run game_aes_key -- ...; decrypting paks - only on a person's word (docs/GAME_DATA.md)
//   ws dump paks [--only=bpfix,chatfix,servernet,lipatch] [--build=<id>] [--apply]   the patch paks
//                                    built from the game reference into game/bp-patches/build: game/bp-patches/tools/build_paks.py
//   ws members                        members, visibility and links
//   ws preview [<member|name> [--open]]   how a person sees a member's result ([preview] in member.toml): the list,
//                                    or run one in the foreground and print its address (--open: the browser too)
//   ws preview --sync [--apply]       write the previews into .claude/launch.json (Claude Code preview)
//   ws roles [--list=<level>]         what is public / team / owner (member.toml, workspace.toml, memory headers)
//   ws game                           the game reference: state, or the exact steps to get it (docs/GAME_DATA.md)
//   ws game clone <original|build|path> <name> [--apply]   working game copy in home/game/<name> (paks hard-linked)
//   ws game original <steam|depot folder> [--build=<id> --depot= --manifest=] [--apply]   adopt a Steam download as the
//                                    reference; "steam" finds the download_depot folder, numbers from config/game.toml
//   ws game original installed [--apply]   copy the clean files of the game installed by Steam into the reference
//                                    (config/game-<build>.files.tsv; the install is not changed). The copy of the game
//                                    is required: ws doctor errors without it, ws bootstrap --apply takes it itself
//   ws game diff <name from [games]|path> [--build=<id>] [--all]   how an install differs from the original
//   ws runs collect [--apply]         move client crash dumps from C: to home/runs/crashes
//   ws stand [--clients=2] [--apply]  the local test stand (tools/stand): game copies from the reference with the
//                                    launcher's runtime (Goldberg, UE4SS) and mods, [games] / [stand] in local.toml;
//                                    what only a human does (DbTools as administrator, hosts) — checked and named
//   ws secrets list|init|check|run <names|all> -- <command>   secrets from secrets.toml (KeePassXC .kdbx or --vault=env)
//   ws secrets export [--tier=team] [--out=<file>]   new vault with only the records of that tier (own password);
//                                    machine tier: local.toml [vault] tier / db
//   ws secrets unlock [--for=4h] [--idle=60m] [--only=<a,b>]   vault session: the password once; the secrets with
//                                    session = true then go to the commands in their session_for without asking
//                                    (ws dump, ws bootstrap, ws secrets run ...); ws secrets status | lock
//
// Profiles: owner (everything), dev, designer, agent - as in DevKit. `ws bootstrap --profile=<p>` remembers the choice
// in local.toml [setup] profile; without --profile doctor/bootstrap use it, else owner.
// --root=<path> - workspace root explicitly. Exit code 1 - there are errors.
use std::path::{Path, PathBuf};
use ws_core::{Finding, Level, Workspace};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter().find_map(|a| {
            a.strip_prefix(name)
                .and_then(|r| r.strip_prefix('='))
                .map(String::from)
        })
    };
    let has = |name: &str| args.iter().any(|a| a == name);
    let pos: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    let start = flag("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let ws = match Workspace::find(&start) {
        Ok(w) => w,
        Err(e) => fail(&e),
    };
    // --profile, else local.toml [setup] profile (bootstrap remembers the choice), else owner
    let profile = ws_core::bootstrap::choose_profile(&ws, flag("--profile"));
    // programs with kind = "cargo": --prebuilt / --from-source, else local.toml [setup] build, else prebuilt
    let build = ws_core::bootstrap::Build::choose(
        &ws,
        if has("--from-source") {
            Some(ws_core::bootstrap::Build::Source)
        } else if has("--prebuilt") {
            Some(ws_core::bootstrap::Build::Prebuilt)
        } else {
            None
        },
    );
    let findings = match pos.as_slice() {
        ["check"] => ws_core::check::run(&ws),
        ["doctor"] => ws_core::bootstrap::run(
            &ws,
            &ws_core::bootstrap::Options {
                apply: false,
                profile,
                build,
                artifacts: Vec::new(),
            },
        ),
        ["bootstrap"] => ws_core::bootstrap::run(
            &ws,
            &ws_core::bootstrap::Options {
                apply: true,
                profile,
                build,
                // --artifact=<name> (may repeat): also download artifacts with manual = true
                artifacts: args
                    .iter()
                    .filter_map(|a| a.strip_prefix("--artifact="))
                    .map(|n| n.trim_matches('"').to_string())
                    .collect(),
            },
        ),
        ["artifacts", "push"] => ws_core::store::push(&ws, has("--apply")),
        ["components", cmd, rest @ ..] => {
            // names: separate words or comma lists (ws components set site,studio)
            let names: Vec<String> = rest
                .iter()
                .flat_map(|r| r.split(','))
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect();
            ws_core::components::run(&ws, cmd, &names, has("--apply"))
        }
        ["components"] => ws_core::components::run(&ws, "list", &[], false),
        ["dump"] => ws_core::dump::run(
            &ws,
            &ws_core::dump::Options {
                apply: has("--apply"),
                build: flag("--build"),
                sets: flag("--set")
                    .map(|s| {
                        s.split(',')
                            .map(|x| x.trim().to_string())
                            .filter(|x| !x.is_empty())
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        ),
        ["dump", "paks"] => ws_core::dump::paks(
            &ws,
            has("--apply"),
            flag("--build").as_deref(),
            flag("--only").as_deref(),
        ),
        // how a person sees a member's result (docs/PREVIEW.md)
        ["preview"] if has("--sync") => ws_core::preview::sync(&ws, has("--apply")),
        ["preview"] => {
            let (previews, problems) = ws_core::preview::list(&ws);
            for p in &previews {
                println!(
                    "{:<20} {:<16} {}\n    {}",
                    p.name,
                    p.member,
                    p.url(),
                    p.note
                );
            }
            print(&problems);
            println!("run one: cargo ws preview <name> --open");
            return;
        }
        ["preview", key] => {
            let Some(p) = ws_core::preview::find(&ws, key) else {
                fail(&format!(
                    "no [preview] '{key}' (cargo ws preview lists them)"
                ))
            };
            println!(
                "{} -> {}\n  {}\n  Ctrl+C stops it",
                p.member,
                p.url(),
                p.note
            );
            match ws_core::preview::run(&ws, &p, has("--open")) {
                Ok(code) => std::process::exit(code),
                Err(e) => fail(&e),
            }
        }
        ["members"] => {
            for m in &ws.members {
                let deps = if m.depends_on.is_empty() {
                    String::new()
                } else {
                    format!("  -> {}", m.depends_on.join(", "))
                };
                let vis = if m.visibility.is_empty() {
                    "-"
                } else {
                    &m.visibility
                };
                println!("{:<24} {:<9} {:<7} {}{}", m.path, m.kind, vis, m.name, deps);
            }
            return;
        }
        ["roles"] => {
            use ws_core::visibility::Tier;
            let files = ws_core::visibility::classify(&ws).unwrap_or_else(|e| fail(&e));
            if let Some(want) = flag("--list") {
                let want = Tier::parse(&want)
                    .unwrap_or_else(|| fail("--list must be public, team or owner"));
                for (f, t) in &files {
                    if *t == want {
                        println!("{f}");
                    }
                }
                return;
            }
            println!("files by visibility (public < team < owner; docs/WORKSPACE_PLAN.md 13.3):");
            for t in Tier::ALL {
                let all = files.iter().filter(|(_, x)| *x == t).count();
                let mem = files
                    .iter()
                    .filter(|(f, x)| *x == t && f.starts_with("agents/memory/"))
                    .count();
                println!("  {:<7} {all:>5} files ({mem} memory notes)", t.name());
            }
            println!("\nmembers:");
            for t in Tier::ALL {
                let names: Vec<&str> = ws
                    .members
                    .iter()
                    .filter(|m| Tier::parse(&m.visibility) == Some(t))
                    .map(|m| m.path.as_str())
                    .collect();
                println!("  {:<7} {}", t.name(), names.join(", "));
            }
            let unmarked: Vec<&str> = ws
                .members
                .iter()
                .filter(|m| Tier::parse(&m.visibility).is_none())
                .map(|m| m.path.as_str())
                .collect();
            if !unmarked.is_empty() {
                println!("  {:<7} {}", "-", unmarked.join(", "));
            }
            println!("\nws roles --list=public|team|owner - the files of one level");
            return;
        }
        ["game"] => ws_core::game::status(&ws),
        ["game", "clone", src, name] => {
            let home = ws.home();
            let src_path = if Path::new(src).is_absolute() {
                PathBuf::from(src)
            } else if *src == "original" {
                // the newest reference (local.toml [game] original or home/game/original)
                let root = ws_core::game::original_root(&ws);
                let newest = std::fs::read_dir(&root)
                    .ok()
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.path().join("original.toml").is_file())
                    .map(|e| e.path())
                    .max();
                newest.unwrap_or_else(|| fail("no game reference - cargo ws game"))
            } else {
                ws_core::game::original_root(&ws).join(src)
            };
            let dst = home.join("game").join(name);
            match ws_core::game::clone(&src_path, &dst, has("--apply")) {
                Ok((st, notes)) => {
                    print(&notes);
                    println!(
                        "{} -> {}: pak links {}, copies {} ({:.1} GB), already there {}{}",
                        src_path.display(),
                        dst.display(),
                        st.linked,
                        st.copied,
                        st.bytes_copied as f64 / 1e9,
                        st.skipped,
                        if has("--apply") {
                            ""
                        } else {
                            " - dry run; to do it: --apply"
                        }
                    );
                    return;
                }
                Err(e) => fail(&e),
            }
        }
        // the game installed by Steam (steamapps/common): its clean files are copied, the install stays as it is
        ["game", "original", "installed"] => {
            let b = ws_core::game::builds(&ws)
                .into_iter()
                .next()
                .unwrap_or_else(|| fail("no build in config/game.toml"));
            let inst = ws_core::game::installed(&b)
                .unwrap_or_else(|| fail("Dungeonborne is not installed in any Steam library"));
            if inst.buildid != b.id {
                fail(&format!(
                    "{} is build {}, the project needs {} (config/game.toml)",
                    inst.path.display(),
                    inst.buildid,
                    b.id
                ));
            }
            let clean = ws_core::game::clean_files(&ws, &b.id)
                .unwrap_or_else(|| fail(&format!("no config/game-{}.files.tsv", b.id)));
            let dst = ws_core::game::original_root(&ws).join(&b.id);
            match ws_core::game::adopt_installed(&inst.path, &dst, &b, &clean, has("--apply")) {
                Ok(msg) => {
                    println!("{msg}");
                    return;
                }
                Err(e) => fail(&e),
            }
        }
        ["game", "original", src] => {
            // "steam": the download_depot folder in Steam, numbers from config/game.toml (the first build or --build)
            let known = ws_core::game::builds(&ws);
            let from_config = flag("--build")
                .and_then(|b| known.iter().find(|x| x.id == b).cloned())
                .or_else(|| known.first().cloned());
            let (src_path, build, depot, manifest) = if *src == "steam" {
                let b = from_config.unwrap_or_else(|| fail("no build in config/game.toml"));
                let p = ws_core::game::steam_download(&b).unwrap_or_else(|| {
                    fail("Steam is not installed (no SteamPath in the registry)")
                });
                (p, b.id, b.depot, b.manifest)
            } else {
                let build =
                    flag("--build").unwrap_or_else(|| fail("--build=<Steam buildid> is required"));
                (
                    PathBuf::from(src),
                    build,
                    flag("--depot").unwrap_or_default(),
                    flag("--manifest").unwrap_or_default(),
                )
            };
            let dst = ws_core::game::original_root(&ws).join(&build);
            match ws_core::game::adopt_original(
                &src_path,
                &dst,
                &build,
                &depot,
                &manifest,
                has("--apply"),
            ) {
                Ok(msg) => {
                    println!("{msg}");
                    return;
                }
                Err(e) => fail(&e),
            }
        }
        ["game", "diff", install] => {
            let orig_root = ws_core::game::original_root(&ws);
            let build = flag("--build").or_else(|| {
                std::fs::read_dir(&orig_root)
                    .ok()?
                    .flatten()
                    .filter(|e| e.path().join("original.toml").is_file())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .max()
            });
            let Some(build) = build else {
                fail("no original in home/game/original (ws game original ...)")
            };
            let path = ws
                .resolve(
                    &ws.local_str("games", install)
                        .unwrap_or_else(|| install.to_string()),
                )
                .unwrap();
            match ws_core::game::diff(&orig_root.join(&build), &path) {
                Ok((added, changed, missing)) => {
                    let show = |title: &str, v: &[String]| {
                        println!("{title}: {}", v.len());
                        for f in v.iter().take(if has("--all") { usize::MAX } else { 25 }) {
                            println!("  {f}");
                        }
                    };
                    println!("{} vs original {build}", path.display());
                    show("changed (size)", &changed);
                    show("added", &added);
                    show("missing in install", &missing);
                    return;
                }
                Err(e) => fail(&e),
            }
        }
        ["secrets", rest @ ..] => secrets(
            &ws,
            rest,
            &args,
            flag("--db"),
            has("--password-stdin"),
            flag("--vault"),
        ),
        ["stand"] => ws_core::stand::run(
            &ws,
            &ws_core::stand::Options {
                apply: has("--apply"),
                clients: flag("--clients").and_then(|c| c.parse().ok()).unwrap_or(1),
            },
        ),
        ["runs", "collect"] => match ws_core::runs::crashes_dir() {
            Some(src) => ws_core::runs::collect(
                &src,
                &ws.home().join("runs").join("crashes"),
                has("--apply"),
            ),
            None => fail("no LOCALAPPDATA"),
        },
        _ => {
            eprintln!(
                "{}",
                include_str!("main.rs")
                    .lines()
                    // the whole header of comments (take(15) used to cut off runs and secrets)
                    .take_while(|l| l.starts_with("//"))
                    .map(|l| l.trim_start_matches("// ").trim_start_matches("//"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            std::process::exit(2);
        }
    };
    print(&findings);
    let errors = findings.iter().filter(|f| f.level == Level::Error).count();
    let warns = findings.iter().filter(|f| f.level == Level::Warn).count();
    let done = findings.iter().filter(|f| f.level == Level::Done).count();
    println!(
        "{}: errors {errors}, warnings {warns}{}",
        pos.join(" "),
        if done > 0 {
            format!(", done {done}")
        } else {
            String::new()
        }
    );
    if errors > 0 {
        std::process::exit(1);
    }
}

/// ws secrets list | init | check | run <names|all> -- <command>  (secrets.toml, docs/SECRETS.md)
fn secrets(
    ws: &Workspace,
    rest: &[&str],
    args: &[String],
    db: Option<String>,
    pw_stdin: bool,
    vault: Option<String>,
) -> Vec<Finding> {
    use ws_core::secrets as s;
    let m = match s::manifest(ws) {
        Ok(m) => m,
        Err(e) => fail(&e),
    };
    // vault tier of this machine: local.toml [vault] tier (owner | team | public)
    let tier = s::local_tier(ws).unwrap_or_else(|e| fail(&e));
    let opt = |name: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(name).and_then(|r| r.strip_prefix('=')))
            .map(String::from)
    };
    let open = || -> s::Source {
        if vault.as_deref() == Some("env") {
            return s::Source::Env;
        }
        let path = s::db_path(ws, &m, db.as_deref()).unwrap_or_else(|e| fail(&e));
        let pw = s::read_password(
            pw_stdin,
            &format!("Master password for {}: ", path.display()),
        )
        .unwrap_or_else(|e| fail(&e));
        s::Source::open_kdbx(&path, &pw).unwrap_or_else(|e| fail(&e))
    };
    match rest {
        ["list"] => {
            for x in &m.secrets {
                let what = if x.kind == "file" { format!("attachment {}", x.file) } else { format!("field {}", x.field) };
                println!(
                    "{:<22} {:<6} {:<6} {}/{} ({what}) -> env {}{}{}\n    {}  [{}]",
                    x.name,
                    x.kind,
                    x.visibility,
                    x.group,
                    x.title,
                    x.env,
                    if x.required { "" } else { ", optional" },
                    if x.rotate { ", rotate on new PC" } else { "" }.to_string()
                        + &if x.session { format!(", vault session for: {}", x.session_for.join(" | ")) } else { String::new() },
                    x.description,
                    x.used_by.join(", ")
                );
            }
            std::process::exit(0);
        }
        ["init"] => {
            let path = s::db_path(ws, &m, db.as_deref()).unwrap_or_else(|e| fail(&e));
            let pw = s::read_password(pw_stdin, &format!("New master password for {}: ", path.display())).unwrap_or_else(|e| fail(&e));
            if !pw_stdin {
                let again = s::read_password(false, "Repeat: ").unwrap_or_else(|e| fail(&e));
                if again != pw {
                    fail("passwords do not match");
                }
            }
            match s::init(&m, &path, &pw) {
                Ok(msg) => {
                    println!("{msg}\nNext: open it in KeePassXC, fill the entries (fields and attachments as listed by ws secrets list), then run ws secrets check");
                    std::process::exit(0);
                }
                Err(e) => fail(&e),
            }
        }
        ["check"] => {
            if tier != "owner" {
                println!("vault tier: {tier} (local.toml [vault] tier) - records above it are not looked for");
            }
            s::check(&m, &open(), &tier)
        }
        // the vault session (ws_core::vault, docs/SECRETS.md): the password once for the working session
        ["unlock"] => {
            use ws_core::vault as v;
            let dur = |name: &str, default: u64| match opt(name) {
                Some(x) => v::parse_duration(&x).unwrap_or_else(|e| fail(&e)),
                None => default,
            };
            let u = v::Unlock {
                for_secs: dur("--for", v::DEFAULT_FOR),
                idle_secs: dur("--idle", v::DEFAULT_IDLE),
                only: opt("--only").map(|o| o.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect()).unwrap_or_default(),
                agent_args: vec!["secrets".into(), "agent".into()],
            };
            // which secrets, before the password: a wrong --only fails without asking
            let picked = v::session_secrets(&m, &tier, &u.only).unwrap_or_else(|e| fail(&e));
            println!(
                "vault session for: {}",
                picked.iter().map(|x| format!("{} ({})", x.name, x.session_for.join(", "))).collect::<Vec<_>>().join("; ")
            );
            let src = open();
            match v::unlock(ws, &m, &src, &tier, &u) {
                Ok((st, missing)) => {
                    print_session(&st);
                    if !missing.is_empty() {
                        println!("  not in the vault (stay with the password): {}", missing.join(", "));
                    }
                    println!("  close it earlier: cargo ws secrets lock; every use is logged in {}", ws.home().join("runs").join("vault").display());
                    std::process::exit(0);
                }
                Err(e) => fail(&e),
            }
        }
        ["status"] => match ws_core::vault::status(ws) {
            Some(st) => {
                print_session(&st);
                std::process::exit(0);
            }
            None => {
                println!("no vault session (cargo ws secrets unlock)");
                std::process::exit(0);
            }
        },
        ["lock"] => match ws_core::vault::lock(ws) {
            Ok(true) => {
                println!("vault session closed");
                std::process::exit(0);
            }
            Ok(false) => {
                println!("no vault session was open");
                std::process::exit(0);
            }
            Err(e) => fail(&e),
        },
        // started by unlock (a copy of ws.exe in home/cache/vault), not by hand
        ["agent"] => {
            let parent = opt("--parent").and_then(|p| p.parse().ok()).unwrap_or_else(|| fail("--parent=<pid> is required"));
            match ws_core::vault::serve(ws, parent) {
                Ok(()) => std::process::exit(0),
                Err(e) => fail(&e),
            }
        }
        ["run", names, ..] => {
            let cmd: Vec<String> = match args.iter().position(|a| a == "--") {
                Some(i) => args[i + 1..].to_vec(),
                None => fail("a command is needed after --: ws secrets run <names> -- <command>"),
            };
            let names: Vec<String> = names.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
            // an open vault session first (not for "all", --db, --password-stdin, --vault=env); refused or none - the password
            let session = if vault.is_none() && db.is_none() && !pw_stdin && !names.iter().any(|n| n == "all") {
                match ws_core::vault::get(ws, &names, &cmd) {
                    Ok(items) => Some(s::Source::Session(items.into_iter().map(|i| (i.name, i.value)).collect())),
                    Err(e) => {
                        if ws_core::vault::status(ws).is_some() {
                            eprintln!("{e} - asking for the password");
                        }
                        None
                    }
                }
            } else {
                None
            };
            let src = session.unwrap_or_else(&open);
            match s::run(&m, &src, &names, &tier, &ws.home().join("cache").join("tmp"), &cmd) {
                Ok(code) => std::process::exit(code),
                Err(e) => fail(&e),
            }
        }
        // a new vault one tier lower (team): only its entries, its own password; the source one is not changed
        ["export"] => {
            let to = opt("--tier").unwrap_or_else(|| "team".into());
            let src_path = s::db_path(ws, &m, db.as_deref()).unwrap_or_else(|e| fail(&e));
            let out = match opt("--out") {
                Some(p) => PathBuf::from(p),
                None => s::vault_dir(ws).unwrap_or_else(|e| fail(&e)).join(s::export_name(&to)),
            };
            if out == src_path {
                fail("--out is the source vault itself");
            }
            if out.exists() {
                fail(&format!("{} already exists - ws never overwrites a database; move it away first", out.display()));
            }
            let src = open();
            let pw = s::read_password(pw_stdin, &format!("New master password for {} ({to} vault, NOT the owner's): ", out.display()))
                .unwrap_or_else(|e| fail(&e));
            if !pw_stdin {
                let again = s::read_password(false, "Repeat: ").unwrap_or_else(|e| fail(&e));
                if again != pw {
                    fail("passwords do not match");
                }
            }
            match s::export(&m, &src, &to, &out, &pw) {
                Ok(r) => {
                    println!("created {} ({to} vault), checked by reopening with the new password", out.display());
                    println!("  copied   {:>2}: {}", r.copied.len(), r.copied.join(", "));
                    println!("  empty    {:>2}: {}", r.empty.len(), r.empty.join(", "));
                    println!("  withheld {:>2}: above {to}, stay in the owner vault", r.withheld.len());
                    if r.copied.is_empty() {
                        println!("note: nothing copied - no record in secrets.toml has visibility = \"{to}\" with a value yet");
                    }
                    println!(
                        "Next: give the file to the developer, the password by another channel. On their machine local.toml:\n  [vault]\n  db = \"{}\"\n  tier = \"{to}\"\nthen ws secrets check",
                        out.file_name().and_then(|n| n.to_str()).unwrap_or("team.kdbx")
                    );
                    std::process::exit(0);
                }
                Err(e) => fail(&e),
            }
        }
        _ => fail("ws secrets list | init | check | run <names|all> -- <command> | export [--tier=team] [--out=<file>] | unlock [--for=4h] [--idle=60m] [--only=<a,b>] | status | lock   [--db=<file>] [--password-stdin] [--vault=env]"),
    }
}

fn print_session(st: &ws_core::vault::Status) {
    println!(
        "vault session open: {} min left, closes after {} min without use",
        st.left.div_ceil(60),
        st.idle_left.div_ceil(60)
    );
    for (name, cmds) in &st.secrets {
        println!("  {name:<22} for: {}", cmds.join(", "));
    }
}

fn print(findings: &[Finding]) {
    for f in findings {
        let tag = match f.level {
            Level::Error => "ERROR",
            Level::Warn => "warning",
            Level::Done => "done",
            Level::Info => "  ok",
            Level::Note => "  note",
        };
        println!("{tag:<8} [{}] {}: {}", f.rule, f.path, f.msg);
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}
