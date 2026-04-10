use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use crate::plugin::ChainPlugin;
use crate::shutdown;

// ChildGuard =====

/// RAII guard that kills a child process on drop.
/// Ensures cleanup even during panic unwind.
struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn inner_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child already taken")
    }

    /// Take the child out, disabling the kill-on-drop behavior.
    /// Used after graceful shutdown has already handled cleanup.
    fn take(&mut self) -> Child {
        self.child.take().expect("child already taken")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(ref child) = self.child {
            if let Some(id) = child.id() {
                #[cfg(unix)]
                unsafe {
                    libc::kill(id as libc::pid_t, libc::SIGKILL);
                }
                #[cfg(windows)]
                {
                    use windows::Win32::Foundation::CloseHandle;
                    use windows::Win32::System::Threading::{
                        OpenProcess, TerminateProcess, PROCESS_TERMINATE,
                    };
                    unsafe {
                        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, id) {
                            let _ = TerminateProcess(handle, 1);
                            let _ = CloseHandle(handle);
                        }
                    }
                }
            }
        }
    }
}

/// A plugin backed by an external SIP003u binary.
pub struct BinaryPlugin {
    path: PathBuf,
    options: Option<String>,
    name: String,
}

impl BinaryPlugin {
    pub fn new(path: impl Into<PathBuf>, options: Option<&str>) -> Self {
        let path = path.into();
        let name = extract_name(&path);
        Self {
            path,
            options: options.map(String::from),
            name,
        }
    }
}

fn extract_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[async_trait::async_trait]
impl ChainPlugin for BinaryPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        self: Box<Self>,
        local: SocketAddr,
        remote: SocketAddr,
        shutdown: CancellationToken,
    ) -> crate::Result<()> {
        let mut cmd = Command::new(&self.path);
        cmd.env("SS_LOCAL_HOST", local.ip().to_string());
        cmd.env("SS_LOCAL_PORT", local.port().to_string());
        cmd.env("SS_REMOTE_HOST", remote.ip().to_string());
        cmd.env("SS_REMOTE_PORT", remote.port().to_string());
        if let Some(ref opts) = self.options {
            cmd.env("SS_PLUGIN_OPTIONS", opts);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            crate::Error::Chain(format!("failed to spawn '{}': {e}", self.path.display()))
        })?;
        let mut guard = ChildGuard::new(child);

        // Capture stdout
        let stdout = guard.inner_mut().stdout.take().expect("stdout was piped");
        let plugin_name = self.name.clone();
        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => { tracing::info!(plugin = %plugin_name, "{line}"); }
                    Ok(None) => break, // EOF
                    Err(e) => {
                        tracing::debug!(plugin = %plugin_name, "log reader error: {e}");
                        break;
                    }
                }
            }
        });

        // Capture stderr
        let stderr = guard.inner_mut().stderr.take().expect("stderr was piped");
        let plugin_name = self.name.clone();
        let stderr_task = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => { tracing::warn!(plugin = %plugin_name, "{line}"); }
                    Ok(None) => break, // EOF
                    Err(e) => {
                        tracing::debug!(plugin = %plugin_name, "log reader error: {e}");
                        break;
                    }
                }
            }
        });

        // Wait for child exit or shutdown signal
        let drain_timeout = std::time::Duration::from_secs(5);
        tokio::select! {
            status = guard.inner_mut().wait() => {
                // Child exited on its own; take it out of the guard so Drop
                // doesn't try to kill an already-exited process.
                let _ = guard.take();
                let status = status?;
                // Drain remaining log lines (tasks will EOF when child's pipes close)
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    async { let _ = tokio::join!(stdout_task, stderr_task); }
                ).await;
                if status.success() {
                    Ok(())
                } else {
                    match status.code() {
                        Some(code) => Err(crate::Error::PluginExit {
                            name: self.name.clone(),
                            code,
                        }),
                        None => Err(crate::Error::PluginKilled {
                            name: self.name.clone(),
                        }),
                    }
                }
            }
            _ = shutdown.cancelled() => {
                tracing::info!(plugin = %self.name, "shutting down");
                shutdown::graceful_kill(guard.inner_mut(), drain_timeout).await?;
                // Graceful shutdown handled cleanup; take the child out so
                // Drop doesn't double-kill.
                let _ = guard.take();
                // Drain remaining log lines (tasks will EOF when child's pipes close)
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    async { let _ = tokio::join!(stdout_task, stderr_task); }
                ).await;
                Ok(())
            }
        }
    }
}
