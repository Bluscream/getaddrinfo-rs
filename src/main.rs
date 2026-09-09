//! Diagnostic CLI utility for getaddrinfo-rs.

use std::env;
use getaddrinfo::discovery;
use getaddrinfo::rules::{ResolutionOutcome, RuleTable};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("getaddrinfo-rs v0.1.0");
        println!("Usage: getaddrinfo-rs <query-hostname> [--dump-rules]");
        return;
    }

    let query = &args[1];
    let dump_rules = args.iter().any(|a| a == "--dump-rules");

    let paths = discovery::discover_hosts_files();
    println!("Discovered {} hosts/snippet files:", paths.len());
    for p in &paths {
        println!("  - {}", p.display());
    }

    let table = RuleTable::load_from_paths(&paths);
    println!("\nLoaded {} rules total.", table.rules.len());

    if dump_rules {
        println!("\nRule list:");
        for r in &table.rules {
            println!("  {r:?}");
        }
    }

    println!("\nQuerying: {query}");
    match table.resolve(query) {
        ResolutionOutcome::DirectIp(ip) => {
            println!("=> Resolved to IP: {ip}");
        }
        ResolutionOutcome::Redirect(target) => {
            println!("=> Redirected to hostname: {target}");
        }
        ResolutionOutcome::PassThrough => {
            println!("=> PassThrough (not matched, using system DNS)");
        }
    }
}
