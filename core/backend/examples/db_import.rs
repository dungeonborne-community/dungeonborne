// Restoring the DB from a no-stop export (`POST /admin/v1/db/export` or the `db_export` line in admin_cmd):
//   cargo run --release --example db_import -- <empty DB folder> <db-<unix>.surql>
// The folder must be new (empty): importing into a live DB would mix records. The server does not touch the folder meanwhile.
// Then — DB_PATH to this folder (or swap db_rocks while the server is stopped).
use surrealdb::engine::local::RocksDb;
use surrealdb::Surreal;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, file] = args.as_slice() else {
        eprintln!("usage: db_import <empty DB folder> <.surql file>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(dir);
    if path.exists() && path.read_dir().map_or(true, |mut d| d.next().is_some()) {
        eprintln!("{dir}: folder is not empty — import only into a new DB");
        std::process::exit(2);
    }
    // the same RocksDB values as the server (src/db.rs rocksdb_defaults)
    for (key, value) in [("SURREAL_ROCKSDB_ENABLE_BLOB_FILES", "false"), ("SURREAL_SYNC_DATA", "true")] {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
    let db = Surreal::new::<RocksDb>(dir.as_str()).await.expect("open the DB");
    db.use_ns("dungeonborne").use_db("gateway").await.expect("ns/db");
    let t = std::time::Instant::now();
    db.import(file).await.expect("import");
    println!("{file} → {dir}: done in {:.1} s", t.elapsed().as_secs_f64());
}
