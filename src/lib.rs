//! `getaddrinfo-rs` - libc `getaddrinfo` dynamic interceptor and extended HOSTS resolver.

use std::ffi::{CStr, CString};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub mod discovery;
pub mod parser;
pub mod rules;

use rules::{ResolutionOutcome, RuleTable};

static RULE_TABLE: RwLock<Option<RuleTable>> = RwLock::new(None);
static LAST_CHECK: AtomicU64 = AtomicU64::new(0);

// Global start instant to compute elapsed seconds lock-free
static START_INSTANT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn get_elapsed_secs() -> u64 {
    let start = START_INSTANT.get_or_init(Instant::now);
    start.elapsed().as_secs()
}

/// Checks if rules need to be reloaded (throttled to at most once per second).
pub fn check_reload_rules() {
    let now_secs = get_elapsed_secs();
    let last = LAST_CHECK.load(Ordering::Relaxed);
    if now_secs == last && last != 0 {
        return;
    }
    LAST_CHECK.store(now_secs, Ordering::Relaxed);

    let paths = discovery::discover_hosts_files();

    // Check if reload is needed
    let needs_reload = {
        match RULE_TABLE.read() {
            Ok(guard) => match &*guard {
                Some(table) => table.has_changed(&paths),
                None => true,
            },
            Err(_) => true,
        }
    };

    if needs_reload {
        let new_table = RuleTable::load_from_paths(&paths);
        if let Ok(mut guard) = RULE_TABLE.write() {
            *guard = Some(new_table);
        }
    }
}

/// Resolves a hostname query against the global rule table.
pub fn resolve_query(hostname: &str) -> ResolutionOutcome {
    check_reload_rules();
    if let Ok(guard) = RULE_TABLE.read()
        && let Some(table) = &*guard
    {
        return table.resolve(hostname);
    }
    ResolutionOutcome::PassThrough
}

// -----------------------------------------------------------------------------
// libc getaddrinfo hook
// -----------------------------------------------------------------------------

type GetaddrinfoFn = unsafe extern "C" fn(
    node: *const libc::c_char,
    service: *const libc::c_char,
    hints: *const libc::addrinfo,
    res: *mut *mut libc::addrinfo,
) -> libc::c_int;

static REAL_GETADDRINFO: std::sync::OnceLock<Option<GetaddrinfoFn>> = std::sync::OnceLock::new();

fn get_real_getaddrinfo() -> Option<GetaddrinfoFn> {
    *REAL_GETADDRINFO.get_or_init(|| unsafe {
        let sym = libc::dlsym(libc::RTLD_NEXT, c"getaddrinfo".as_ptr());
        if sym.is_null() {
            None
        } else {
            Some(std::mem::transmute::<*mut libc::c_void, GetaddrinfoFn>(sym))
        }
    })
}

/// Dynamic hook for libc `getaddrinfo`.
///
/// # Safety
/// Conforms to POSIX `getaddrinfo` C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getaddrinfo(
    node: *const libc::c_char,
    service: *const libc::c_char,
    hints: *const libc::addrinfo,
    res: *mut *mut libc::addrinfo,
) -> libc::c_int {
    let Some(real_fn) = get_real_getaddrinfo() else {
        return libc::EAI_SYSTEM;
    };

    if node.is_null() {
        return unsafe { real_fn(node, service, hints, res) };
    }

    let Ok(hostname_str) = (unsafe { CStr::from_ptr(node) }).to_str() else {
        return unsafe { real_fn(node, service, hints, res) };
    };

    match resolve_query(hostname_str) {
        ResolutionOutcome::DirectIp(ip) => {
            let ip_str = CString::new(ip.to_string()).unwrap_or_default();
            unsafe { real_fn(ip_str.as_ptr(), service, hints, res) }
        }
        ResolutionOutcome::Redirect(target) => {
            let target_str = CString::new(target).unwrap_or_default();
            unsafe { real_fn(target_str.as_ptr(), service, hints, res) }
        }
        ResolutionOutcome::PassThrough => unsafe { real_fn(node, service, hints, res) },
    }
}
