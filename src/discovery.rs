//! Discovers all relevant HOSTS files and `hosts.d/` drop-in directories across the host
//! system, active Wine/Proton prefixes, and user configuration.

use std::path::{Path, PathBuf};

/// Gathers all existing hosts files and snippet files that should be loaded.
pub fn discover_hosts_files() -> Vec<PathBuf> {
    let mut files = Vec::new();

    // 1. Explicit environment overrides
    if let Ok(val) = std::env::var("HOSTS_FILE") {
        let p = PathBuf::from(val.trim());
        if p.is_file() {
            files.push(p);
        }
    }
    if let Ok(val) = std::env::var("LVR_HOSTS") {
        let p = PathBuf::from(val.trim());
        if p.is_file() {
            files.push(p);
        }
    }
    if let Ok(val) = std::env::var("HOSTS_DIR") {
        scan_hosts_dir(Path::new(val.trim()), &mut files);
    }
    if let Ok(val) = std::env::var("LVR_HOSTS_DIR") {
        scan_hosts_dir(Path::new(val.trim()), &mut files);
    }

    // 2. Host system files
    let host_hosts = PathBuf::from("/etc/hosts");
    if host_hosts.is_file() {
        files.push(host_hosts);
    }
    scan_hosts_dir(Path::new("/etc/hosts.d"), &mut files);

    // 3. XDG Runtime and Cache directories for fast LVRIPC / daemon sync
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let runtime_hosts = PathBuf::from(runtime.trim()).join("lvr/hosts");
        if runtime_hosts.is_file() {
            files.push(runtime_hosts);
        }
        scan_hosts_dir(&PathBuf::from(runtime.trim()).join("lvr/hosts.d"), &mut files);
    }

    // 4. User configuration and cache directories (~/.config/hosts.d, ~/.cache/lvr/hosts, etc.)
    if let Ok(home) = std::env::var("HOME") {
        let home_p = PathBuf::from(home);
        let user_hosts = home_p.join(".config/hosts");
        if user_hosts.is_file() {
            files.push(user_hosts);
        }
        let lvr_user_hosts = home_p.join(".config/lvr/hosts");
        if lvr_user_hosts.is_file() {
            files.push(lvr_user_hosts);
        }
        let lvr_cache_hosts = home_p.join(".cache/lvr/hosts");
        if lvr_cache_hosts.is_file() {
            files.push(lvr_cache_hosts);
        }
        scan_hosts_dir(&home_p.join(".config/hosts.d"), &mut files);
        scan_hosts_dir(&home_p.join(".config/lvr/hosts.d"), &mut files);
        scan_hosts_dir(&home_p.join(".cache/lvr/hosts.d"), &mut files);
    }

    // 5. Fallback path
    let tmp_hosts = PathBuf::from("/tmp/lvr_hosts");
    if tmp_hosts.is_file() {
        files.push(tmp_hosts);
    }

    // 6. Wine / Proton prefix hosts files
    discover_wine_prefix_hosts(&mut files);

    // Deduplicate paths while preserving order
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| seen.insert(p.clone()));

    files
}

/// Discovers hosts and hosts.d files in Wine/Proton prefixes.
fn discover_wine_prefix_hosts(files: &mut Vec<PathBuf>) {
    // Check WINEPREFIX
    if let Ok(wineprefix) = std::env::var("WINEPREFIX") {
        let prefix = PathBuf::from(wineprefix.trim());
        add_prefix_hosts_candidates(&prefix, files);
    }

    // Check STEAM_COMPAT_DATA_PATH
    if let Ok(compat_path) = std::env::var("STEAM_COMPAT_DATA_PATH") {
        let prefix = PathBuf::from(compat_path.trim()).join("pfx");
        add_prefix_hosts_candidates(&prefix, files);
    }

    // Check known common Proton compatdata prefixes (e.g. VRChat 438100)
    let candidates = [
        "/run/media/system/Data/Games/Steam/steamapps/compatdata/438100/pfx",
        "/run/media/system/Data/SteamLibrary/steamapps/compatdata/438100/pfx",
    ];

    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_dir() {
            add_prefix_hosts_candidates(&p, files);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let steam_roots = [
            format!("{home}/.local/share/Steam/steamapps/compatdata/438100/pfx"),
            format!("{home}/.steam/steam/steamapps/compatdata/438100/pfx"),
            format!("{home}/.var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/compatdata/438100/pfx"),
        ];
        for s in steam_roots {
            let p = PathBuf::from(s);
            if p.is_dir() {
                add_prefix_hosts_candidates(&p, files);
            }
        }
    }
}

/// Adds the `hosts` file and any `hosts.d/` snippets inside a Wine prefix to the file list.
fn add_prefix_hosts_candidates(prefix: &Path, files: &mut Vec<PathBuf>) {
    let drivers_etc = prefix.join("drive_c/windows/system32/drivers/etc");
    let hosts_file = drivers_etc.join("hosts");
    if hosts_file.is_file() {
        files.push(hosts_file);
    }
    scan_hosts_dir(&drivers_etc.join("hosts.d"), files);
}

/// Recursively or flatly scans a directory for non-hidden snippet files.
fn scan_hosts_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut snippet_paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip hidden files and common editor backup files
                if !name.starts_with('.') && !name.ends_with('~') && !name.ends_with(".bak") {
                    snippet_paths.push(path);
                }
            }
        }
        snippet_paths.sort();
        files.extend(snippet_paths);
    }
}
