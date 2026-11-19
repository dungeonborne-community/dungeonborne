//! Packages from tools/make_mods.py through the launcher's own code: download -> verify -> resolve -> lay out in a
//! fake install -> restore. Serve the dist folder first (urls are /mods/<file>):
//!   python -m http.server 8799 --bind 127.0.0.1 --directory dist
//!   cargo test -p dungeonborne-launcher --test mods_dist -- --ignored
//! Or against a running backend (its manifest `mods[]` and `/mods/<file>`):
//!   DBL_MANIFEST=http://127.0.0.1:8090/.well-known/dbserver.json cargo test ... --test mods_dist -- --ignored
//! Leaves nothing behind: the fake install is temporary, downloaded packages are removed from the mod cache.

use dbl::manifest::ModRef;
use dbl::profile;

const BASE: &str = "http://127.0.0.1:8799/.well-known/dbserver.json";

#[tokio::test]
#[ignore]
async fn dist_packages_install_and_restore() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist/mods");
    let index: serde_json::Value = serde_json::from_slice(&std::fs::read(dist.join("index.json")).unwrap()).unwrap();
    let exe = index["exe_sha256"].as_str().unwrap();
    let client = dbl::net::client();
    let (mods, base): (Vec<ModRef>, String) = match std::env::var("DBL_MANIFEST") {
        Ok(url) => {
            let m = dbl::manifest::fetch(&client, &url).await.unwrap();
            assert_eq!(m.mods.len(), index["packages"].as_object().unwrap().len(), "server lists every package");
            (m.mods, url)
        }
        Err(_) => (serde_json::from_slice(&std::fs::read(dist.join("manifest_mods.json")).unwrap()).unwrap(), BASE.into()),
    };
    let base = base.as_str();

    let mut set = Vec::new();
    for r in &mods {
        let p = profile::prepare(&client, r, base, exe, std::sync::Arc::new(|_, _| {})).await.unwrap_or_else(|e| panic!("{}: {e}", r.id));
        println!("{}@{} native={} components={}", p.id, p.version, p.native(), p.meta.components.len());
        set.push(p);
    }
    profile::resolve(&set).unwrap();
    println!("modset {}", profile::modset_sha256(&set));

    // Fake install: exe, UE4SS runtime, vanilla pak, a player's own pak.
    let root = std::env::temp_dir().join(format!("dbl-mods-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let win64 = root.join("Dungeonborne/Binaries/Win64");
    let paks = root.join("Dungeonborne/Content/Paks");
    std::fs::create_dir_all(win64.join("ue4ss/Mods")).unwrap();
    std::fs::create_dir_all(&paks).unwrap();
    std::fs::write(win64.join("Dungeonborne.exe"), b"x").unwrap();
    std::fs::write(win64.join("dwmapi.dll"), b"x").unwrap();
    std::fs::write(win64.join("cacert.pem"), b"ca").unwrap();
    std::fs::write(paks.join("pakchunk0-Windows.pak"), b"vanilla").unwrap();
    let inst = profile::Install::open(root.to_str().unwrap()).unwrap();
    profile::check_runtime(&inst, &set).unwrap();

    let plan = profile::Plan { server_id: "test.dist", mods: &set, ca_pem: None, goldberg: None, isolate_unlisted: false, intl_url: None, client_settings: None };
    profile::apply(&inst, &plan).unwrap();
    let mut laid: Vec<String> = std::fs::read_dir(paks.join("~dbl_test.dist"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    laid.sort();
    for n in &laid {
        println!("  Paks/~dbl_test.dist/{n}");
    }
    let pak_files: usize = set
        .iter()
        .flat_map(|m| m.meta.files.keys())
        .filter(|f| f.starts_with("Paks/"))
        .count();
    assert_eq!(laid.len(), pak_files, "every pak/ucas/utoc laid out");
    // Name order = priority order: zz_ data (10) < rebuild (50) < balance (60) < fixes (70).
    let pos = |needle: &str| laid.iter().position(|n| n.contains(needle)).unwrap();
    assert!(pos("ru-loot") < pos("ru-rebuild-private") && pos("ru-rebuild-private") < pos("ru-balance"));
    assert!(pos("ru-balance") < pos("ru-r14-fix") && pos("ru-balance") < pos("ru-chat-fix"));
    for m in ["DungeonborneR14", "DungeonborneRuClient"] {
        assert!(win64.join("ue4ss/Mods").join(m).join("enabled.txt").is_file(), "{m}");
    }
    assert!(win64.join("ue4ss/Mods/DungeonborneRuClient/native/dbnative.dll").is_file());

    let diff = profile::restore(&inst).unwrap();
    assert!(diff.is_empty(), "{diff:?}");
    assert!(!paks.join("~dbl_test.dist").exists());
    assert_eq!(std::fs::read_dir(win64.join("ue4ss/Mods")).unwrap().count(), 0);

    std::fs::remove_dir_all(&root).unwrap();
    for r in &mods {
        let _ = std::fs::remove_dir_all(dbl::store::data_dir().join("mods").join(&r.id));
    }
}
