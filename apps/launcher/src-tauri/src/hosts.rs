//! One-time `hosts` setup (spec M4). The elevated part is this same exe started as
//! `--hosts-helper install|uninstall`; it takes no paths or data from outside — domains are compiled in.

use std::path::PathBuf;

use crate::error::{bail, Result};
use crate::winutil;

/// The game's own addresses, pointed at the launcher's relay (127.0.0.1). Needed while the server's manifest has no
/// `relay.no_hosts`. With it (the mod ru.rebuild-private brings ServerConfig.json with https://localhost and a directory
/// name that resolves to 127.0.0.1, the INTL login goes to https://localhost through the open INTL config, profile.rs)
/// the game needs no hosts lines, and the settings only offer to remove the block older runs wrote.
pub const DOMAINS: &[&str] = &[
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
    DIR_DOMAIN,
    OLD_DIR_DOMAIN,
];

/// The directory name the server's mod (ru.rebuild-private, ServerConfig.json) gives the game without hosts. Public DNS
/// answers 127.0.0.1; some routers (DNS rebinding protection), VPNs and antiviruses drop such answers — then the game
/// falls back to the official directory and shows its old shutdown notice (12.11). The hosts block maps it too.
pub const DIR_DOMAIN: &str = "dir.228918463-2-1.example.org";

/// The directory name before the move to dungeonborne-community.com: players whose mod still has it keep working
/// through hosts too. Remove once every player has the mod with `DIR_DOMAIN` and the old name leaves DNS.
const OLD_DIR_DOMAIN: &str = "dir.228918463-2-1.dbserver.example.org";

/// Does `host` resolve to this computer the way the game resolves it (system resolver, hosts file included)?
/// Any loopback address counts; a failed lookup does not.
pub fn resolves_locally(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    match (host, 8085u16).to_socket_addrs() {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            !addrs.is_empty() && addrs.iter().any(|a| a.ip().is_loopback())
        }
        Err(_) => false,
    }
}

/// Older versions' block is still in hosts: the settings offer to remove it
pub fn has_block_text(text: &str) -> bool {
    text.lines().any(|l| l.trim() == BEGIN) || text.lines().any(|l| l.starts_with(DISABLED))
}

pub fn has_block() -> bool {
    std::fs::read_to_string(hosts_path()).map(|t| has_block_text(&t)).unwrap_or(false)
}

const BEGIN: &str = "# >>> dungeonborne-launcher";
const END: &str = "# <<< dungeonborne-launcher";
/// Prefix for foreign lines we disabled; uninstall removes exactly this prefix.
const DISABLED: &str = "#[dungeonborne-launcher] ";

pub const HELPER_FLAG: &str = "--hosts-helper";

pub fn hosts_path() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join("drivers").join("etc").join("hosts")
}

fn eol(text: &str) -> &'static str {
    if text.contains("\r\n") || text.is_empty() {
        "\r\n"
    } else {
        "\n"
    }
}

/// Lines outside our marker block.
fn outside_block(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t == BEGIN {
            inside = true;
        } else if t == END {
            inside = false;
        } else if !inside {
            out.push(line);
        }
    }
    out
}

/// An active (uncommented) line that maps one of our domains.
fn maps_our_domain(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with('#') {
        return false;
    }
    let t = t.split('#').next().unwrap_or("");
    t.split_whitespace().skip(1).any(|h| DOMAINS.iter().any(|d| d.eq_ignore_ascii_case(h)))
}

fn finish(mut lines: Vec<String>, nl: &str) -> String {
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let mut s = lines.join(nl);
    s.push_str(nl);
    s
}

pub fn install_text(text: &str) -> String {
    let nl = eol(text);
    let mut lines: Vec<String> = outside_block(text)
        .into_iter()
        .map(|l| if maps_our_domain(l) { format!("{DISABLED}{l}") } else { l.to_string() })
        .collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.push(String::new());
    lines.push(BEGIN.into());
    lines.extend(DOMAINS.iter().map(|d| format!("127.0.0.1 {d}")));
    lines.push(END.into());
    finish(lines, nl)
}

pub fn uninstall_text(text: &str) -> String {
    let nl = eol(text);
    let lines = outside_block(text)
        .into_iter()
        .map(|l| l.strip_prefix(DISABLED).unwrap_or(l).to_string())
        .collect();
    finish(lines, nl)
}

/// Our block is present and complete, and nothing outside it overrides our domains.
pub fn is_installed_text(text: &str) -> bool {
    let mut inside = false;
    let mut mapped = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t == BEGIN {
            inside = true;
        } else if t == END {
            inside = false;
        } else if inside {
            let mut it = t.split_whitespace();
            if it.next() == Some("127.0.0.1") {
                mapped.extend(it.map(str::to_ascii_lowercase));
            }
        } else if maps_our_domain(line) {
            return false;
        }
    }
    DOMAINS.iter().all(|d| mapped.iter().any(|m| m == d))
}

pub fn is_installed() -> bool {
    std::fs::read_to_string(hosts_path()).map(|t| is_installed_text(&t)).unwrap_or(false)
}

/// Runs inside the elevated helper process. Returns the process exit code.
pub fn helper_main(cmd: &str) -> i32 {
    let path = hosts_path();
    let Ok(bytes) = std::fs::read(&path) else { return 2 };
    let Ok(text) = String::from_utf8(bytes.clone()) else { return 3 };
    let new = match cmd {
        "install" => install_text(&text),
        "uninstall" => uninstall_text(&text),
        _ => return 4,
    };
    let backup = path.with_file_name("hosts.dungeonborne-launcher.bak");
    if !backup.exists() && std::fs::write(&backup, &bytes).is_err() {
        return 5;
    }
    if new != text && std::fs::write(&path, new).is_err() {
        return 6;
    }
    let _ = std::process::Command::new("ipconfig").arg("/flushdns").output();
    0
}

/// Starts the helper with UAC and waits for it.
pub async fn run_helper(cmd: &'static str) -> Result<()> {
    let exe = std::env::current_exe()?;
    let code = tokio::task::spawn_blocking(move || -> Result<u32> {
        let p = winutil::shell_execute(&exe, &format!("{HELPER_FLAG} {cmd}"), None, true, true)?;
        Ok(p.wait())
    })
    .await
    .map_err(|e| crate::error::Error::new("internal", e.to_string()))??;
    if code != 0 {
        bail!("hosts_failed", "helper exit code {code}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIG: &str = "# comment\r\n127.0.0.1 localhost\r\n198.51.100.1 sg.intlgame.com\r\n10.0.0.1 other.example\r\n";

    #[test]
    fn leftovers_of_older_versions() {
        assert!(!has_block_text(ORIG), "a hosts file the launcher never touched");
        let old = "127.0.0.1 localhost\r\n#[dungeonborne-launcher] 127.0.0.1 sg.intlgame.com\r\n\r\n# >>> dungeonborne-launcher\r\n\
                   127.0.0.1 sg.intlgame.com\r\n# <<< dungeonborne-launcher\r\n";
        assert!(has_block_text(old));
        let cleaned = uninstall_text(old);
        assert_eq!(cleaned, "127.0.0.1 localhost\r\n127.0.0.1 sg.intlgame.com\r\n", "our block out, the disabled foreign line back");
        assert!(!has_block_text(&cleaned));
    }

    #[test]
    fn install_uninstall_roundtrip() {
        let inst = install_text(ORIG);
        assert!(inst.contains("#[dungeonborne-launcher] 198.51.100.1 sg.intlgame.com"));
        assert!(inst.contains(
            "127.0.0.1 t01.tgf123.com\r\n127.0.0.1 dir.228918463-2-1.example.org\r\n\
             127.0.0.1 dir.228918463-2-1.dbserver.example.org\r\n# <<< dungeonborne-launcher\r\n"
        ));
        assert!(is_installed_text(&inst));
        assert!(!is_installed_text(ORIG));
        assert_eq!(install_text(&inst), inst, "idempotent");
        assert_eq!(uninstall_text(&inst), ORIG);
    }

    #[test]
    fn foreign_override_after_block_breaks_install() {
        let t = format!("{}1.2.3.4 tgf123.com\r\n", install_text(ORIG));
        assert!(!is_installed_text(&t));
        assert!(is_installed_text(&install_text(&t)));
    }

    /// 1.7.2: a block of 1.7.1 lacks the directory name — it counts as not installed, setting it up again adds it
    #[test]
    fn old_block_without_dir_domain_is_reinstalled() {
        let old: String = install_text(ORIG).lines().filter(|l| !l.contains(DIR_DOMAIN)).map(|l| format!("{l}\r\n")).collect();
        assert!(!is_installed_text(&old));
        let again = install_text(&old);
        assert!(is_installed_text(&again));
        assert_eq!(again.matches(DIR_DOMAIN).count(), 1);
    }

    #[test]
    fn localhost_resolves_locally() {
        assert!(resolves_locally("localhost"));
        assert!(!resolves_locally("no-such-name.invalid"));
    }

}
