use std::time::Duration;
use tokio::process::Child;
use tokio_util::sync::CancellationToken;

/// Register OS signal handlers that cancel the given token on SIGTERM/SIGINT.
pub fn register_signal_handler(shutdown: CancellationToken) {
    tokio::spawn(async move {
        wait_for_signal().await;
        tracing::info!("shutdown signal received");
        shutdown.cancel();
    });
}

#[cfg(unix)]
async fn wait_for_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
}

#[cfg(windows)]
async fn wait_for_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to register Ctrl+C handler");
}

/// Gracefully terminate a child process.
/// Sends a termination signal, waits up to `timeout`, then force-kills.
pub async fn graceful_kill(child: &mut Child, timeout: Duration) -> crate::Result<()> {
    send_term_signal(child)?;
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(_status)) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Err(_elapsed) => {
            tracing::warn!("child did not exit within timeout, force-killing");
            child.kill().await?;
            Ok(())
        }
    }
}

#[cfg(unix)]
fn send_term_signal(child: &Child) -> crate::Result<()> {
    if let Some(pid) = child.id() {
        let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if ret == -1 {
            let err = std::io::Error::last_os_error();
            // ESRCH: process already exited — not an error
            if err.raw_os_error() != Some(libc::ESRCH) {
                return Err(err.into());
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn send_term_signal(child: &Child) -> crate::Result<()> {
    // On Windows, there is no direct equivalent of SIGTERM for arbitrary processes.
    // GenerateConsoleCtrlEvent requires CREATE_NEW_PROCESS_GROUP, which will be set up
    // in Task 6 (BinaryPlugin) when we actually spawn child processes.
    // For now, this is a no-op — graceful_kill will fall through to the timeout+force-kill path.
    let _ = child;
    Ok(())
}
