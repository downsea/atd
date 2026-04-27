//! Spawns atd-ref-server and runs the full conformance suite
//! against it. If the reference server drifts from the spec, this
//! test fails on the next PR's `cargo test --workspace`.

use atd_conformance::runner::Outcome;
use atd_conformance::{Opts, run_conformance};
use atd_sdk::Endpoint;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_ref_server_passes_conformance_suite() {
    let sock_dir = tempfile::tempdir().expect("create tempdir");
    let sock = sock_dir.path().join("conformance.sock");

    let bin = ref_server_bin();
    let mut child = spawn_server(&bin, &sock);

    if let Err(e) = wait_for_socket(&sock, Duration::from_secs(5)).await {
        let _ = child.kill();
        let _ = child.wait();
        panic!("server socket did not appear: {}", e);
    }

    let opts = Opts {
        target: Endpoint::unix(&sock),
        filter: None,
        categories: Vec::new(),
        stop_on_first_fail: false,
        fixtures_root: fixtures_root(),
    };

    let report = run_conformance(opts).await;

    // Clean shutdown
    let _ = child.kill();
    let _ = child.wait();

    if report.failed > 0 {
        let failures: Vec<String> = report
            .cases
            .iter()
            .filter_map(|c| match &c.outcome {
                Outcome::Fail { reason } => Some(format!(
                    "  [{}] {}: {}",
                    c.category.as_str(),
                    c.name,
                    reason
                )),
                _ => None,
            })
            .collect();
        panic!(
            "{}/{} conformance case(s) failed:\n{}\n\n\
             (total: {} passed, {} failed, {} skipped)",
            report.failed,
            report.total,
            failures.join("\n"),
            report.passed,
            report.failed,
            report.skipped
        );
    }

    assert!(
        report.total >= 28 && report.total <= 40,
        "expected ~28-40 cases, got {} (design spec §4.7; +3 in SP-tool-visibility-hidden)",
        report.total
    );
    assert_eq!(report.failed, 0, "all cases must pass");
    assert!(
        report.passed >= 28,
        "expected at least 28 passing cases, got {}",
        report.passed
    );
}

/// Locate the `atd-ref-server` binary built by Cargo for this test.
///
/// Cargo's stable `CARGO_BIN_EXE_<name>` only exposes binaries from the
/// *same* package as the test, so we can't use `env!` to find the
/// ref-server binary (it lives in `atd-ref-server`, a dev-dep).
///
/// Instead we derive the target directory from the current test
/// executable's own path: Cargo places integration tests in
/// `<target>/<profile>/deps/<name>-<hash>`, and binary dev-deps in
/// `<target>/<profile>/<name>`. So `current_exe().parent().parent()`
/// is the profile dir.
fn ref_server_bin() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let profile_dir = exe
        .parent() // .../deps/
        .and_then(Path::parent) // .../<profile>/
        .expect("test exe should live in <target>/<profile>/deps/")
        .to_path_buf();
    let bin = profile_dir.join("atd-ref-server");
    assert!(
        bin.exists(),
        "atd-ref-server binary not found at {}. \
         Cargo should have built it as part of the atd-ref-server \
         dev-dependency. Try: `cargo build -p atd-ref-server`.",
        bin.display()
    );
    bin
}

fn spawn_server(bin: &Path, sock: &Path) -> Child {
    Command::new(bin)
        .arg("--sock")
        .arg(sock)
        .arg("--grant-capability")
        .arg("read")
        .arg("--grant-capability")
        .arg("write")
        .arg("--grant-capability")
        .arg("exec")
        .arg("--enable-conformance-tool")
        // Suppress the server's startup log so the test output isn't
        // polluted. On a failure, panic! will include the conformance
        // failures themselves.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn atd-ref-server binary")
}

async fn wait_for_socket(path: &Path, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "socket {:?} did not appear within {:?}",
        path, timeout
    ))
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
