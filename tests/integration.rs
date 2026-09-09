//! Integration test for getaddrinfo-rs RuleTable and discovery logic.

use getaddrinfo::parser::parse_hosts_content;
use getaddrinfo::rules::{ResolutionOutcome, RuleTable};

#[test]
fn test_end_to_end_hosts_resolution() {
    let hosts_content = r#"
# Standard IPv4 and IPv6 mappings
127.0.0.1 localhost
::1 localhost ip6-localhost

# Wildcards
0.0.0.0 *.github.io *.metrics.vrchat.com
127.0.0.1 test-?.internal.net

# Hostname to Hostname Redirection
google.com bing.com
alias.local google.com
"#;

    let rules = parse_hosts_content(hosts_content);
    let mut table = RuleTable::default();
    table.rules.extend(rules);

    // Test standard IP resolution
    assert_eq!(
        table.resolve("localhost"),
        ResolutionOutcome::DirectIp("127.0.0.1".parse().unwrap())
    );

    // Test wildcard resolution
    assert_eq!(
        table.resolve("sub.github.io"),
        ResolutionOutcome::DirectIp("0.0.0.0".parse().unwrap())
    );
    assert_eq!(
        table.resolve("deep.sub.metrics.vrchat.com"),
        ResolutionOutcome::DirectIp("0.0.0.0".parse().unwrap())
    );
    assert_eq!(
        table.resolve("test-1.internal.net"),
        ResolutionOutcome::DirectIp("127.0.0.1".parse().unwrap())
    );

    // Test redirect chain (alias.local -> google.com -> bing.com)
    assert_eq!(
        table.resolve("alias.local"),
        ResolutionOutcome::Redirect("bing.com".to_string())
    );
    assert_eq!(
        table.resolve("google.com"),
        ResolutionOutcome::Redirect("bing.com".to_string())
    );

    // Test unmatched pass through
    assert_eq!(
        table.resolve("unmatched.domain.org"),
        ResolutionOutcome::PassThrough
    );
}
