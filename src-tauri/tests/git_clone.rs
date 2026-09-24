mod support;

use std::process::Command;
use std::time::Duration;

use session_relay_lib::engine::error::Error;
use session_relay_lib::engine::git_clone::{cancel, clone_url};
use support::bare_remote;

/// One test: clones share a process-wide "one clone at a time" slot.
#[test]
fn clones_into_its_own_folder_and_cleans_up_after_a_failure_or_a_cancel() {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_remote(tmp.path());
    let work = tmp.path().join("seed");
    let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&work).args(args).status().unwrap().success());
    std::fs::create_dir_all(&work).unwrap();
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(work.join("README.md"), b"app").unwrap();
    git(&["add", "-A"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "init"]);
    git(&["push", "-q", &url, "main"]);

    let dest = tmp.path().join("root").join("app");
    clone_url(&url, &dest, &mut |_| {}).unwrap();
    assert!(dest.join(".git").exists() && dest.join("README.md").exists());
    assert!(matches!(clone_url(&url, &dest, &mut |_| {}), Err(Error::DestinationExists)), "never clones over a folder");
    assert!(dest.join("README.md").exists(), "an existing folder is left alone");

    let failed = tmp.path().join("root").join("missing");
    assert!(matches!(clone_url("https://github.invalid/acme/missing.git", &failed, &mut |_| {}), Err(Error::Clone(_))));
    assert!(!failed.exists(), "no half clone left behind");

    // A server that accepts the connection and never answers: git waits until cancelled.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let hung_url = format!("https://127.0.0.1:{}/acme/hung.git", silent.local_addr().unwrap().port());
    let hung = tmp.path().join("root").join("hung");
    let running = {
        let hung = hung.clone();
        std::thread::spawn(move || clone_url(&hung_url, &hung, &mut |_| {}))
    };
    std::thread::sleep(Duration::from_millis(500));
    assert!(matches!(clone_url(&url, &tmp.path().join("root").join("second"), &mut |_| {}), Err(Error::Busy)), "one clone at a time");
    cancel();
    assert!(matches!(running.join().unwrap(), Err(Error::Cancelled)));
    assert!(!hung.exists(), "a cancelled clone is removed");
    assert!(!tmp.path().join("root").join("second").exists());
}
