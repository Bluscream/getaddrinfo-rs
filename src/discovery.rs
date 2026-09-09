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
    if let Ok(val) = std::env::var("HOSTS_DIR") {
        scan_hosts_dir(Path::new(val.trim()), &mut files);
    }

    // 2. Host system files
    let host_hosts = PathBuf::from("/etc/hosts");
    if host_hosts.is_file() {
        files.push(host_hosts);
    }
    scan_hosts_dir(Path::new("/etc/hosts.d"), &mut files);

    // 3. User configuration directories (~/.config/hosts, ~/.config/hosts.d)
    if let Ok(home) = std::env::var("HOME") {
        let home_p = PathBuf::from(home);
        let user_hosts = home_p.join(".config/hosts");
        if user_hosts.is_file() {
            files.push(user_hosts);
        }
        scan_hosts_dir(&home_p.join(".config/hosts.d"), &mut files);
    }

    // 4. Wine / Proton prefix hosts files
    discover_wine_prefix_hosts(&mut files);

    // Deduplicate paths while preserving order
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| seen.insert(p.clone()));

    files
}

/// Discovers hosts and hosts.d files in Wine/Proton prefixes using standard environment
/// variables and current working directory detection.
fn discover_wine_prefix_hosts(files: &mut Vec<PathBuf>) {
    // 1. Check explicit standard WINEPREFIX
    if let Ok(wineprefix) = std::env::var("WINEPREFIX") {
        let prefix = PathBuf::from(wineprefix.trim());
        add_prefix_hosts_candidates(&prefix, files);
    }

    // 2. Check STEAM_COMPAT_DATA_PATH (standard Steam/Proton environment variable)
    if let Ok(compat_path) = std::env::var("STEAM_COMPAT_DATA_PATH") {
        let prefix = PathBuf::from(compat_path.trim()).join("pfx");
        add_prefix_hosts_candidates(&prefix, files);
    }

    // 3. Inspect current working directory hierarchy for an enclosing Wine prefix:
    // Any directory having `drive_c/windows/system32/drivers/etc/` or `pfx/drive_c/...`
    if let Ok(cwd) = std::env::current_dir() {
        let mut curr = Some(cwd.as_path());
        while let Some(dir) = curr {
            if dir.join("drive_c/windows/system32/drivers/etc").is_dir() {
                add_prefix_hosts_candidates(dir, files);
                break;
            }
            if dir.join("pfx/drive_c/windows/system32/drivers/etc").is_dir() {
                add_prefix_hosts_candidates(&dir.join("pfx"), files);
                break;
            }
            curr = dir.parent();
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
