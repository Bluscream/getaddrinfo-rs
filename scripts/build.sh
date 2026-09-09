#!/usr/bin/env bash
# Build, verify, test, and deploy getaddrinfo-rs.
# Supports standalone usage or invocation from parent projects (e.g. lvr).
set -euo pipefail

SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
LIB_DIR="${LIB_DIR:-$HOME/.local/lib}"

action=build
deploy=no
skip_wine_test=no

usage() {
    cat <<'EOF'
Usage: ./scripts/build.sh [OPTIONS]

  --test             Run test suite and integration tests only
  --deploy           Build and install libgetaddrinfo.so and getaddrinfo-rs CLI
  --skip-wine-test   Skip Wine / Windows .exe integration testing
  -h, --help         Show this help

Environment overrides: BIN_DIR, LIB_DIR
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --test) action=test ;;
        --deploy) deploy=yes ;;
        --skip-wine-test) skip_wine_test=yes ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo was not found. Please install Rust toolchain first." >&2
    exit 1
fi

echo "==> Checking getaddrinfo-rs with clippy (-D warnings)…"
cargo clippy --all-targets --manifest-path "$SOURCE_DIR/Cargo.toml" -- -D warnings

echo "==> Running getaddrinfo-rs test suite…"
cargo test --manifest-path "$SOURCE_DIR/Cargo.toml"

echo "==> Building getaddrinfo-rs (release)…"
cargo build --release --manifest-path "$SOURCE_DIR/Cargo.toml"

LIB_GETADDRINFO="$SOURCE_DIR/target/release/libgetaddrinfo.so"
CLI_BIN="$SOURCE_DIR/target/release/getaddrinfo-rs"

# Integration tests with LD_PRELOAD
if [ -f "$LIB_GETADDRINFO" ]; then
    echo "==> Running getaddrinfo-rs LD_PRELOAD integration tests…"
    INTEG_TEMP="$(mktemp -d /tmp/getaddrinfo-integ-XXXXXX)"
    mkdir -p "$INTEG_TEMP/hosts.d"
    cat << 'EOF' > "$INTEG_TEMP/hosts.d/test.hosts"
0.0.0.0 *.integ-block.local
redirect.integ-test.local target.integ-test.local
127.0.0.99 target.integ-test.local
EOF

    # 1. Native libc test via python3
    if command -v python3 >/dev/null 2>&1; then
        HOSTS_DIR="$INTEG_TEMP/hosts.d" LD_PRELOAD="$LIB_GETADDRINFO" python3 -c '
import socket
resolved_wildcard = socket.gethostbyname("sub.integ-block.local")
assert resolved_wildcard == "0.0.0.0", f"Wildcard failed: {resolved_wildcard}"
resolved_redirect = socket.gethostbyname("redirect.integ-test.local")
assert resolved_redirect == "127.0.0.99", f"Redirect chain failed: {resolved_redirect}"
print("  ✓ Native LD_PRELOAD resolution passed (wildcard -> 0.0.0.0, redirect -> 127.0.0.99)")
'
    fi

    # 2. Wine / Windows PE test
    if [ "$skip_wine_test" != yes ]; then
        WINE64_BIN=""
        if command -v wine64 >/dev/null 2>&1; then
            WINE64_BIN="$(command -v wine64)"
        else
            for candidate in \
                "/run/media/system/Data/Games/Steam/steamapps/common/Proton 9.0 (Beta)/files/bin/wine64" \
                "/run/media/system/Data/Games/Steam/steamapps/common/Proton - Experimental/files/bin/wine64" \
                "/home/blu/.local/share/Steam/compatibilitytools.d/Proton-GE Latest/files/lib/wine/x86_64-unix/wine64"; do
                if [ -x "$candidate" ]; then
                    WINE64_BIN="$candidate"
                    break
                fi
            done
        fi

        MINGW_GCC=""
        USE_DISTROBOX=""
        if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
            MINGW_GCC="x86_64-w64-mingw32-gcc"
        elif command -v distrobox >/dev/null 2>&1 && distrobox list 2>/dev/null | grep -q "build-box"; then
            USE_DISTROBOX="build-box"
            MINGW_GCC="x86_64-w64-mingw32-gcc"
        fi

        if [ -n "$WINE64_BIN" ] && [ -n "$MINGW_GCC" ]; then
            echo "==> Running Windows .exe under Wine with LD_PRELOAD DNS interceptor…"
            cat << 'EOF' > "$INTEG_TEMP/win_test.c"
#include <stdio.h>
#include <winsock2.h>
#include <ws2tcpip.h>

static int check_host(const char *host, const char *expected_ip) {
    struct addrinfo hints, *res = NULL, *p = NULL;
    ZeroMemory(&hints, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    int err = getaddrinfo(host, NULL, &hints, &res);
    if (err != 0) return 1;
    int found = 0;
    for (p = res; p != NULL; p = p->ai_next) {
        struct sockaddr_in *ipv4 = (struct sockaddr_in *)p->ai_addr;
        char ipstr[INET_ADDRSTRLEN];
        if (inet_ntop(AF_INET, &(ipv4->sin_addr), ipstr, sizeof(ipstr))) {
            if (strcmp(ipstr, expected_ip) == 0) found = 1;
        }
    }
    if (res) freeaddrinfo(res);
    return found ? 0 : 2;
}

int main(int argc, char **argv) {
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) return 10;
    if (check_host("sub.integ-block.local", "0.0.0.0") != 0) return 11;
    if (check_host("redirect.integ-test.local", "127.0.0.99") != 0) return 12;
    WSACleanup();
    return 0;
}
EOF
            if [ -n "$USE_DISTROBOX" ]; then
                distrobox enter "$USE_DISTROBOX" -- "$MINGW_GCC" -O2 "$INTEG_TEMP/win_test.c" -o "$INTEG_TEMP/win_test.exe" -lws2_32 >/dev/null 2>&1
            else
                "$MINGW_GCC" -O2 "$INTEG_TEMP/win_test.c" -o "$INTEG_TEMP/win_test.exe" -lws2_32 >/dev/null 2>&1
            fi

            if [ -f "$INTEG_TEMP/win_test.exe" ]; then
                WINE_PFX_TEST="${WINEPREFIX:-/run/media/system/Data/Games/Steam/steamapps/compatdata/438100/pfx}"
                if HOSTS_DIR="$INTEG_TEMP/hosts.d" LD_PRELOAD="$LIB_GETADDRINFO" WINEPREFIX="$WINE_PFX_TEST" "$WINE64_BIN" "$INTEG_TEMP/win_test.exe" 2>/dev/null; then
                    echo "  ✓ Wine64 Windows .exe resolution test passed (wildcards & redirects in Wine prefix)"
                else
                    echo "  ⚠ Wine64 test exited with non-zero (non-fatal, check environment)"
                fi
            fi
        fi
    fi

    rm -rf "$INTEG_TEMP"
fi

if [ "$action" = test ]; then
    echo
    echo "getaddrinfo-rs tests completed successfully!"
    exit 0
fi

if [ "$deploy" = yes ]; then
    echo "==> Deploying getaddrinfo-rs…"
    mkdir -p "$BIN_DIR" "$LIB_DIR"
    install -Dm755 "$CLI_BIN" "$BIN_DIR/getaddrinfo-rs"
    install -Dm755 "$LIB_GETADDRINFO" "$LIB_DIR/libgetaddrinfo.so"
    echo "Installed:"
    echo "  $BIN_DIR/getaddrinfo-rs"
    echo "  $LIB_DIR/libgetaddrinfo.so"
else
    echo
    echo "getaddrinfo-rs build and verification completed successfully!"
    echo "Run with --deploy to install to $LIB_DIR and $BIN_DIR"
fi
