use std::net::SocketAddr;
use std::time::Duration;

use contracts::debug_requires;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::plugin::ChainPlugin;
use crate::shutdown;

const MAX_PORT_RETRIES: usize = 3;

/// Allocate `count` unique ephemeral ports on localhost.
pub fn allocate_ports(count: usize) -> crate::Result<Vec<SocketAddr>> {
    let mut ports = Vec::with_capacity(count);
    for _ in 0..count {
        let addr = allocate_one_port()?;
        ports.push(addr);
    }
    Ok(ports)
}

fn allocate_one_port() -> crate::Result<SocketAddr> {
    for attempt in 0..MAX_PORT_RETRIES {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        drop(listener);

        match std::net::TcpListener::bind(addr) {
            Ok(l) => {
                drop(l);
                return Ok(addr);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::debug!(attempt, port = addr.port(), "port was taken, retrying");
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(crate::Error::Chain(
        "failed to allocate a free port after retries".into(),
    ))
}

/// Orchestrates a chain of SIP003u plugins.
pub struct ChainRunner {
    plugins: Vec<Box<dyn ChainPlugin>>,
    drain_timeout: Duration,
}

impl ChainRunner {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            drain_timeout: Duration::from_secs(5),
        }
    }

    /// Add a plugin to the end of the chain.
    #[debug_requires(self.plugins.len() <= 100, "chain is unreasonably long")]
    pub fn add(mut self, plugin: Box<dyn ChainPlugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Set the drain timeout for graceful shutdown.
    pub fn drain_timeout(mut self, timeout: Duration) -> Self {
        self.drain_timeout = timeout;
        self
    }

    /// Run the full chain. Blocks until all plugins exit or shutdown is requested.
    #[debug_requires(!self.plugins.is_empty(), "chain must have at least one plugin")]
    pub async fn run(self, env: crate::sip003::PluginEnv) -> crate::Result<()> {
        let n = self.plugins.len();

        // Resolve remote address
        let remote_addr: SocketAddr =
            tokio::net::lookup_host(format!("{}:{}", env.remote_host, env.remote_port))
                .await?
                .next()
                .ok_or_else(|| {
                    crate::Error::Chain(format!(
                        "failed to resolve {}:{}",
                        env.remote_host, env.remote_port
                    ))
                })?;

        // Build address chain: [local, intermediate..., remote]
        let intermediate = allocate_ports(n.saturating_sub(1))?;
        let mut addrs = Vec::with_capacity(n + 1);
        addrs.push(env.local_addr());
        addrs.extend(intermediate);
        addrs.push(remote_addr);

        // Shared shutdown token
        let shutdown = CancellationToken::new();
        shutdown::register_signal_handler(shutdown.clone());

        // Spawn all plugins
        let mut handles = Vec::with_capacity(n);
        for (i, plugin) in self.plugins.into_iter().enumerate() {
            let local = addrs[i];
            let remote = addrs[i + 1];
            let token = shutdown.child_token();
            let plugin_name = plugin.name().to_string();

            let span = tracing::info_span!("plugin", name = %plugin_name, position = i);
            let handle =
                tokio::spawn(async move { plugin.run(local, remote, token).await }.instrument(span));
            handles.push((plugin_name, handle));
        }

        // Wait for plugins to exit. Any exit (clean or error) in a multi-plugin
        // chain means data can no longer flow, so trigger shutdown for all others.
        let mut set = tokio::task::JoinSet::new();
        for (name, handle) in handles {
            set.spawn(async move { (name, handle.await) });
        }

        let mut first_error: Option<crate::Error> = None;
        while let Some(result) = set.join_next().await {
            match result {
                Ok((name, Ok(Ok(())))) => {
                    tracing::info!(plugin = %name, "exited cleanly");
                    shutdown.cancel();
                }
                Ok((name, Ok(Err(e)))) => {
                    tracing::error!(plugin = %name, error = %e, "exited with error");
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    shutdown.cancel();
                }
                Ok((name, Err(join_err))) => {
                    tracing::error!(plugin = %name, error = %join_err, "task panicked");
                    if first_error.is_none() {
                        first_error = Some(crate::Error::Chain(format!(
                            "plugin '{name}' panicked: {join_err}"
                        )));
                    }
                    shutdown.cancel();
                }
                Err(join_err) => {
                    tracing::error!(error = %join_err, "plugin task failed to join");
                    shutdown.cancel();
                }
            }
        }

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
