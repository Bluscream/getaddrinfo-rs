# getaddrinfo-rs

A fast, drop-in `getaddrinfo` interceptor (`LD_PRELOAD`) and extended HOSTS resolver written in Rust.

## Features

- **Standard HOSTS resolution**: Supports IPv4 and IPv6 mappings (`0.0.0.0 ads.example.com`, `127.0.0.1 localhost`).
- **Multiple hostnames per line**: `0.0.0.0 tracker1.com tracker2.com tracker3.com`.
- **Wildcards (`*` and `?`)**: Full glob support powered by `glob-match` (e.g. `0.0.0.0 *.github.io`, `127.0.0.1 ??-service.local`).
- **Hostname-to-Hostname Redirection**: Redirect hostnames transparently (e.g. `google.com bing.com`).
- **Preserves Comments**: Parsing preserves comments (`# ...`) and does not corrupt whitespace or existing entries.
- **Modular `hosts.d/` Drop-in Directories**: Automatically discovers and aggregates snippet files from:
  - System: `/etc/hosts` and `/etc/hosts.d/*`
  - Wine / Proton prefix: `<pfx>/drive_c/windows/system32/drivers/etc/hosts` and `.../hosts.d/*`
  - User configuration: `~/.config/hosts.d/*`
  - Environment overrides: `$HOSTS_FILE` and `$HOSTS_DIR`
- **Zero-restart Hot Reload**: Dynamically re-checks file timestamps (`mtime`) with a 1-second check throttle, ensuring updates take effect immediately in running processes.

## Usage

### Build
```bash
cargo build --release
```
Produces `target/release/libgetaddrinfo.so`.

### Launching an Application with `LD_PRELOAD`
```bash
LD_PRELOAD=/path/to/libgetaddrinfo.so your-application
```

### CLI Diagnostic Tool
```bash
cargo run -- resolve test.github.io
```

## License
MIT OR Apache-2.0
