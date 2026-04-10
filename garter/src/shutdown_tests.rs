use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::shutdown;

#[tokio::test]
async fn cancel_token_on_shutdown_signal() {
    let token = CancellationToken::new();
    let child_token = token.child_token();
    token.cancel();
    assert!(child_token.is_cancelled());
}

#[tokio::test]
async fn graceful_kill_terminates_child() {
    #[cfg(unix)]
    let mut child = Command::new("sleep").arg("60").spawn().unwrap();
    #[cfg(windows)]
    let mut child = Command::new("timeout")
        .args(["/t", "60", "/nobreak"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let id = child.id().expect("child should have an id");
    assert!(id > 0);
    shutdown::graceful_kill(&mut child, Duration::from_secs(2))
        .await
        .unwrap();
    let status = child.try_wait().unwrap();
    assert!(status.is_some(), "child should have exited after graceful_kill");
}

#[tokio::test]
async fn graceful_kill_force_kills_after_timeout() {
    #[cfg(unix)]
    let mut child = Command::new("sh")
        .args(["-c", "trap '' TERM; sleep 60"])
        .spawn()
        .unwrap();
    #[cfg(windows)]
    let mut child = Command::new("timeout")
        .args(["/t", "60", "/nobreak"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    shutdown::graceful_kill(&mut child, Duration::from_millis(100))
        .await
        .unwrap();
    let status = child.try_wait().unwrap();
    assert!(status.is_some(), "child should have been force-killed");
}
