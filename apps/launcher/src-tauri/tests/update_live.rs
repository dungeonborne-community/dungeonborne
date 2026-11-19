//! Self-update against a running backend: the release signed by tools/release.py (Python) must verify with the key built
//! into the launcher (Rust), download from `/launcher/<file>` and replace an exe (a temporary stand-in here).
//!   python tools/release.py sign --key <secret> --version 1.1.2-e2e      # -> dist/launcher
//!   backend with probe/auth_test/server.toml ([launcher] dir = ../launcher/dist/launcher)
//!   DBL_MANIFEST=http://127.0.0.1:8090/.well-known/dbserver.json cargo test --test update_live -- --ignored
//! Remove dist/launcher afterwards: a test build must never reach a real server.

use dbl::{manifest, update};

#[tokio::test]
#[ignore]
async fn signed_release_downloads_and_swaps() {
    let url = std::env::var("DBL_MANIFEST").expect("DBL_MANIFEST");
    let client = dbl::net::client();
    let m = manifest::fetch(&client, &url).await.unwrap();
    let rel = m.launcher.clone().expect("server offers a launcher build");
    println!("offered {} ({} bytes)", rel.version, rel.size);
    assert!(update::best(m.launcher.iter()).is_some(), "Python signature verifies with builtin::RELEASE_KEY");

    let dir = std::env::temp_dir().join(format!("dbl-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("dungeonborne-launcher.exe");
    std::fs::write(&exe, b"old build").unwrap();

    let mut seen = 0;
    let started = update::install_at(&client, &rel, &exe, update::CURRENT, dbl::builtin::RELEASE_KEY, |d, _| seen = d)
        .await
        .unwrap();
    assert_eq!(started, exe);
    assert_eq!(seen, rel.size);
    assert_eq!(std::fs::read(dir.join("dungeonborne-launcher.old.exe")).unwrap(), b"old build");
    assert_eq!(dbl::net::sha256_hex(&std::fs::read(&exe).unwrap()), rel.sha256);
    assert!(!dir.join("dungeonborne-launcher.new.exe").exists());

    // a tampered offer is refused before anything is downloaded
    let mut bad = rel.clone();
    bad.size += 1;
    let err = update::install_at(&client, &bad, &exe, update::CURRENT, dbl::builtin::RELEASE_KEY, |_, _| {}).await.unwrap_err();
    assert_eq!(err.code, "update_signature");
    std::fs::remove_dir_all(&dir).unwrap();
}
