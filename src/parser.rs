//! Extended HOSTS file parser supporting standard IP mappings, multiple hostnames per line,
//! wildcards (* and ?), hostname-to-hostname redirects, and inline comments.
//! Uses the established `hostfile` crate for standard line parsing, with extensions for
//! wildcards, redirects, and comment preservation.

use std::net::IpAddr;
use std::str::FromStr;

/// A parsed entry from a hosts or hosts.d snippet file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRule {
    /// Maps a hostname/pattern to an IP address (e.g. `0.0.0.0 *.github.io` or `127.0.0.1 localhost`).
    Address {
        pattern: String,
        ip: IpAddr,
    },
    /// Redirects matching hostnames to another target hostname (e.g. `google.com bing.com`).
    Redirect {
        pattern: String,
        target_hostname: String,
    },
}

/// A line in a hosts file, retaining comments and formatting for lossless round-tripping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostLine {
    /// A blank line or whitespace only.
    Empty,
    /// A pure comment line (starts with `#`).
    Comment(String),
    /// A line containing parsed rules, along with optional inline comment.
    Entry {
        raw_text: String,
        rules: Vec<HostRule>,
        inline_comment: Option<String>,
    },
}

/// Parses a single line from a hosts file into a `HostLine`.
pub fn parse_line(line: &str) -> HostLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return HostLine::Empty;
    }

    if trimmed.starts_with('#') {
        return HostLine::Comment(line.to_string());
    }

    // Split inline comments
    let (content_part, comment_part) = match line.find('#') {
        Some(idx) => (&line[..idx], Some(line[idx + 1..].trim().to_string())),
        None => (line, None),
    };

    let content_trimmed = content_part.trim();
    if content_trimmed.is_empty() {
        return match comment_part {
            Some(c) => HostLine::Comment(format!("#{c}")),
            None => HostLine::Empty,
        };
    }

    // 1. Try standard parser using `hostfile::HostEntry`
    if let Ok(entry) = hostfile::HostEntry::from_str(content_trimmed) {
        let mut rules = Vec::new();
        for name in entry.names {
            let lower = name.trim().to_lowercase();
            if !lower.is_empty() {
                rules.push(HostRule::Address {
                    pattern: lower,
                    ip: entry.ip,
                });
            }
        }
        return HostLine::Entry {
            raw_text: line.to_string(),
            rules,
            inline_comment: comment_part,
        };
    }

    // 2. Extension parsing: Handles wildcard hostnames with IPs or hostname-to-hostname redirects
    let tokens: Vec<&str> = content_part.split_whitespace().collect();
    if tokens.is_empty() {
        return HostLine::Empty;
    }

    let first = tokens[0];
    let mut rules = Vec::new();

    if let Ok(ip) = IpAddr::from_str(first) {
        // e.g. 0.0.0.0 *.github.io ??-test.com (where hostfile might reject wildcard characters)
        for &name in &tokens[1..] {
            let lower = name.trim().to_lowercase();
            if !lower.is_empty() {
                rules.push(HostRule::Address {
                    pattern: lower,
                    ip,
                });
            }
        }
    } else if tokens.len() >= 2 {
        // Hostname-to-hostname redirect: source_pattern target_hostname (e.g. google.com bing.com)
        let pattern = first.trim().to_lowercase();
        let target_hostname = tokens[1].trim().to_lowercase();
        if !pattern.is_empty() && !target_hostname.is_empty() {
            rules.push(HostRule::Redirect {
                pattern,
                target_hostname,
            });
        }
    }

    HostLine::Entry {
        raw_text: line.to_string(),
        rules,
        inline_comment: comment_part,
    }
}

/// Parses the full contents of a hosts file into a vector of `HostRule`s.
pub fn parse_hosts_content(content: &str) -> Vec<HostRule> {
    let mut rules = Vec::new();
    for line in content.lines() {
        if let HostLine::Entry { rules: line_rules, .. } = parse_line(line) {
            rules.extend(line_rules);
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_and_comments() {
        assert_eq!(parse_line("   "), HostLine::Empty);
        assert_eq!(
            parse_line("# this is a comment"),
            HostLine::Comment("# this is a comment".to_string())
        );
    }

    #[test]
    fn test_parse_multiple_hostnames_per_line() {
        let line = "0.0.0.0 foo.com bar.org *.baz.net # trackers";
        let parsed = parse_line(line);
        if let HostLine::Entry { rules, inline_comment, .. } = parsed {
            assert_eq!(inline_comment, Some("trackers".to_string()));
            assert_eq!(rules.len(), 3);
            assert_eq!(
                rules[0],
                HostRule::Address {
                    pattern: "foo.com".to_string(),
                    ip: "0.0.0.0".parse().unwrap(),
                }
            );
            assert_eq!(
                rules[1],
                HostRule::Address {
                    pattern: "bar.org".to_string(),
                    ip: "0.0.0.0".parse().unwrap(),
                }
            );
            assert_eq!(
                rules[2],
                HostRule::Address {
                    pattern: "*.baz.net".to_string(),
                    ip: "0.0.0.0".parse().unwrap(),
                }
            );
        } else {
            panic!("Expected Entry variant");
        }
    }

    #[test]
    fn test_parse_hostname_redirect() {
        let line = "google.com bing.com # redirect search";
        let parsed = parse_line(line);
        if let HostLine::Entry { rules, inline_comment, .. } = parsed {
            assert_eq!(inline_comment, Some("redirect search".to_string()));
            assert_eq!(rules.len(), 1);
            assert_eq!(
                rules[0],
                HostRule::Redirect {
                    pattern: "google.com".to_string(),
                    target_hostname: "bing.com".to_string(),
                }
            );
        } else {
            panic!("Expected Entry variant");
        }
    }

    #[test]
    fn test_parse_ipv6() {
        let line = "::1 localhost ip6-localhost";
        let rules = parse_hosts_content(line);
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0],
            HostRule::Address {
                pattern: "localhost".to_string(),
                ip: "::1".parse().unwrap(),
            }
        );
        assert_eq!(
            rules[1],
            HostRule::Address {
                pattern: "ip6-localhost".to_string(),
                ip: "::1".parse().unwrap(),
            }
        );
    }
}
