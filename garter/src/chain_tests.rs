use std::net::SocketAddr;

use tokio_util::sync::CancellationToken;

use crate::chain::{allocate_ports, ChainRunner};
use crate::plugin::ChainPlugin;

// Port allocation tests =====

#[test]
fn allocate_zero_ports() {
    let ports = allocate_ports(0).unwrap();
    assert!(ports.is_empty());
}

#[test]
fn allocate_one_port() {
    let ports = allocate_ports(1).unwrap();
    assert_eq!(ports.len(), 1);
    assert!(ports[0].port() > 0);
    assert_eq!(
        ports[0].ip(),
        "127.0.0.1".parse::<std::net::IpAddr>().unwrap()
    );
}

#[test]
fn allocate_multiple_ports_are_unique() {
    let ports = allocate_ports(5).unwrap();
    assert_eq!(ports.len(), 5);
    let unique: std::collections::HashSet<u16> = ports.iter().map(|a| a.port()).collect();
    assert_eq!(unique.len(), 5, "all allocated ports should be unique");
}

// ChainRunner tests =====

struct InstantPlugin {
    name: String,
}

#[async_trait::async_trait]
impl ChainPlugin for InstantPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        self: Box<Self>,
        _local: SocketAddr,
        _remote: SocketAddr,
        _shutdown: CancellationToken,
    ) -> crate::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn chain_runner_single_plugin() {
    let runner =
        ChainRunner::new().add(Box::new(InstantPlugin { name: "test".into() }));

    let env = crate::sip003::PluginEnv {
        local_host: "127.0.0.1".parse().unwrap(),
        local_port: 10000,
        remote_host: "127.0.0.1".into(),
        remote_port: 20000,
        plugin_options: None,
    };

    let result = runner.run(env).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn chain_runner_multiple_plugins() {
    let runner = ChainRunner::new()
        .add(Box::new(InstantPlugin { name: "first".into() }))
        .add(Box::new(InstantPlugin { name: "second".into() }))
        .add(Box::new(InstantPlugin { name: "third".into() }));

    let env = crate::sip003::PluginEnv {
        local_host: "127.0.0.1".parse().unwrap(),
        local_port: 10000,
        remote_host: "127.0.0.1".into(),
        remote_port: 20000,
        plugin_options: None,
    };

    let result = runner.run(env).await;
    assert!(result.is_ok());
}
