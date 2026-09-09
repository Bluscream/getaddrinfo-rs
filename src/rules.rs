//! Rule storage, matching engine, and resolution logic.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::parser::{HostRule, parse_hosts_content};

/// Evaluates whether a given query hostname matches a pattern (handling * and ? wildcards).
pub fn matches_pattern(pattern: &str, hostname: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let hostname = hostname.to_lowercase();

    // Exact match
    if pattern == hostname {
        return true;
    }

    // Special ergonomic domain wildcard: `*.domain.com` matching both `sub.domain.com` and `domain.com`
    if let Some(base) = pattern.strip_prefix("*.")
        && hostname == base
    {
        return true;
    }

    // Fast glob matching for wildcards (* and ?)
    if pattern.contains('*') || pattern.contains('?') {
        return glob_match::glob_match(&pattern, &hostname);
    }

    false
}

/// The result of evaluating a query hostname against loaded rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionOutcome {
    /// Resolved directly to an IP address (e.g. `0.0.0.0` or `127.0.0.1`).
    DirectIp(IpAddr),
    /// Redirected to another hostname (e.g. `google.com` -> `bing.com`).
    Redirect(String),
    /// No matching rule found; should pass through to native system DNS.
    PassThrough,
}

/// An active rule table compiled from loaded hosts files.
#[derive(Debug, Default)]
pub struct RuleTable {
    pub rules: Vec<HostRule>,
    pub file_mtimes: HashMap<PathBuf, SystemTime>,
}

impl RuleTable {
    /// Creates a new, empty rule table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads rules from the provided file paths.
    pub fn load_from_paths(paths: &[PathBuf]) -> Self {
        let mut rules = Vec::new();
        let mut file_mtimes = HashMap::new();

        for p in paths {
            if let Ok(metadata) = std::fs::metadata(p)
                && let Ok(mtime) = metadata.modified()
            {
                file_mtimes.insert(p.clone(), mtime);
            }
            if let Ok(content) = std::fs::read_to_string(p) {
                rules.extend(parse_hosts_content(&content));
            }
        }

        Self {
            rules,
            file_mtimes,
        }
    }

    /// Checks if any of the monitored files have changed since they were last loaded.
    pub fn has_changed(&self, current_paths: &[PathBuf]) -> bool {
        if self.file_mtimes.len() != current_paths.len() {
            return true;
        }
        for p in current_paths {
            match (std::fs::metadata(p).and_then(|m| m.modified()), self.file_mtimes.get(p)) {
                (Ok(current_mtime), Some(&cached_mtime)) => {
                    if current_mtime != cached_mtime {
                        return true;
                    }
                }
                _ => return true,
            }
        }
        false
    }

    /// Resolves a hostname against the loaded rules with redirect loop prevention.
    pub fn resolve(&self, hostname: &str) -> ResolutionOutcome {
        let mut current_target = hostname.to_string();
        let mut visited = std::collections::HashSet::new();
        visited.insert(current_target.to_lowercase());

        // Max hops to prevent infinite redirect loops
        for _ in 0..8 {
            let mut matched = false;
            for rule in &self.rules {
                match rule {
                    HostRule::Address { pattern, ip } => {
                        if matches_pattern(pattern, &current_target) {
                            return ResolutionOutcome::DirectIp(*ip);
                        }
                    }
                    HostRule::Redirect { pattern, target_hostname } => {
                        if matches_pattern(pattern, &current_target) {
                            let next = target_hostname.to_lowercase();
                            if !visited.insert(next.clone()) {
                                // Loop detected, stop following redirect
                                return ResolutionOutcome::Redirect(current_target);
                            }
                            current_target = next;
                            matched = true;
                            break;
                        }
                    }
                }
            }
            if !matched {
                break;
            }
        }

        if current_target != hostname {
            ResolutionOutcome::Redirect(current_target)
        } else {
            ResolutionOutcome::PassThrough
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_matching() {
        assert!(matches_pattern("*.github.io", "test.github.io"));
        assert!(matches_pattern("*.github.io", "sub.test.github.io"));
        assert!(matches_pattern("*.github.io", "github.io"));
        assert!(!matches_pattern("*.github.io", "notgithub.io"));

        assert!(matches_pattern("api-?.service.com", "api-1.service.com"));
        assert!(!matches_pattern("api-?.service.com", "api-12.service.com"));
    }

    #[test]
    fn test_resolution_chaining() {
        let mut table = RuleTable::new();
        table.rules.push(HostRule::Redirect {
            pattern: "google.com".to_string(),
            target_hostname: "bing.com".to_string(),
        });
        table.rules.push(HostRule::Address {
            pattern: "bing.com".to_string(),
            ip: "127.0.0.1".parse().unwrap(),
        });

        assert_eq!(
            table.resolve("google.com"),
            ResolutionOutcome::DirectIp("127.0.0.1".parse().unwrap())
        );
        assert_eq!(
            table.resolve("unknown.org"),
            ResolutionOutcome::PassThrough
        );
    }
}
