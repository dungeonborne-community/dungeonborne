use std::env;
use std::path::PathBuf;

/// Global server configuration.
/// In Kubernetes all parameters can be overridden via environment variables (ENV).
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// IP address for binding sockets ("127.0.0.1" locally or "0.0.0.0" in Docker/K8s)
    pub bind_ip: String,
    /// External IP address of the gateway that DirServer (8085) gives to clients
    pub public_gateway_ip: String,
    /// Gateway port that DirServer gives to the client (6601; in PVE_SNIFF — the sniffer port)
    pub public_gateway_port: u16,
    /// Directory server port (default 8085)
    pub dir_port: u16,
    /// Game gateway port range (default 6601..=6604)
    pub gateway_ports: Vec<u16>,
    /// Path to the SurrealDB database folder
    pub db_path: String,
    /// Path to the assets folder (.pb schemas, packet templates, .bytes tables)
    pub assets_dir: PathBuf,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let bind_ip = env::var("BIND_IP").unwrap_or_else(|_| "127.0.0.1".to_string());
        let public_gateway_ip =
            env::var("PUBLIC_GATEWAY_IP").unwrap_or_else(|_| "127.0.0.1".to_string());
        let dir_port = env::var("DIR_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8085);
        // 11.11: RocksDB, its own folder — the old surreal_saves (SurrealKV) cannot be opened by this engine
        let db_path = env::var("DB_PATH").unwrap_or_else(|_| "db_rocks".to_string());
        let assets_dir = PathBuf::from(env::var("ASSETS_DIR").unwrap_or_else(|_| ".".to_string()));

        Self {
            bind_ip,
            public_gateway_ip,
            public_gateway_port: env::var("PUBLIC_GATEWAY_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(6601),
            dir_port,
            gateway_ports: vec![6601, 6602, 6603, 6604],
            db_path,
            assets_dir,
        }
    }
}