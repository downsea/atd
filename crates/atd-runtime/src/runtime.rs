//! Tokio runtime helpers.
//!
//! SP-concurrency-baseline §5.1 — `default_worker_threads()` chooses a sensible
//! default for ATD reference binaries: `min(available_parallelism, 4)`,
//! overridable via `ATD_WORKER_THREADS` env. The 4-thread cap prevents a
//! reference binary launched on a 32-core dev machine from spawning 32 idle
//! worker threads per process (adopter test suites that spin up several
//! ref-server instances would otherwise pay 100s of idle threads).

/// Returns the recommended tokio `worker_threads` count for an ATD server
/// binary. Reads `ATD_WORKER_THREADS` if set; otherwise defaults to
/// `min(available_parallelism, 4)`, falling back to `2` if the OS query
/// fails.
pub fn default_worker_threads() -> usize {
    if let Some(n) = std::env::var("ATD_WORKER_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
    {
        return n;
    }
    std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env mutations across these tests must not race. We don't pull in
    /// `serial_test`; instead we serialize via a local mutex so the suite
    /// stays dep-free and the workspace-wide nextest pool isn't poisoned.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn defaults_to_at_most_four() {
        let _g = env_lock();
        // Safety: env mutations serialized within this test module.
        unsafe { std::env::remove_var("ATD_WORKER_THREADS") };
        let n = default_worker_threads();
        assert!((1..=4).contains(&n), "default {n} outside [1,4]");
    }

    #[test]
    fn env_override_respected() {
        let _g = env_lock();
        unsafe { std::env::set_var("ATD_WORKER_THREADS", "8") };
        let n = default_worker_threads();
        unsafe { std::env::remove_var("ATD_WORKER_THREADS") };
        assert_eq!(n, 8);
    }

    #[test]
    fn env_zero_falls_back_to_default() {
        let _g = env_lock();
        unsafe { std::env::set_var("ATD_WORKER_THREADS", "0") };
        let n = default_worker_threads();
        unsafe { std::env::remove_var("ATD_WORKER_THREADS") };
        // Zero is nonsense; we fall back to the cap-4 default.
        assert!((1..=4).contains(&n), "fallback {n} outside [1,4]");
    }

    #[test]
    fn env_garbage_falls_back_to_default() {
        let _g = env_lock();
        unsafe { std::env::set_var("ATD_WORKER_THREADS", "not-a-number") };
        let n = default_worker_threads();
        unsafe { std::env::remove_var("ATD_WORKER_THREADS") };
        assert!((1..=4).contains(&n), "garbage fallback {n} outside [1,4]");
    }
}
