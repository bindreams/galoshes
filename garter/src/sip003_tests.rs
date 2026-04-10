use crate::sip003::{parse_plugin_options, PluginEnv};

#[test]
#[serial_test::serial]
fn parse_env_all_set() {
    let vars = [
        ("SS_LOCAL_HOST", "127.0.0.1"),
        ("SS_LOCAL_PORT", "1080"),
        ("SS_REMOTE_HOST", "example.com"),
        ("SS_REMOTE_PORT", "443"),
        ("SS_PLUGIN_OPTIONS", "tls;host=example.com"),
    ];
    for (k, v) in &vars {
        std::env::set_var(k, v);
    }
    let env = PluginEnv::from_env().unwrap();
    assert_eq!(env.local_host, "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
    assert_eq!(env.local_port, 1080);
    assert_eq!(env.remote_host, "example.com");
    assert_eq!(env.remote_port, 443);
    assert_eq!(env.plugin_options.as_deref(), Some("tls;host=example.com"));
    for (k, _) in &vars {
        std::env::remove_var(k);
    }
}

#[test]
#[serial_test::serial]
fn parse_env_missing_required_var() {
    std::env::remove_var("SS_LOCAL_HOST");
    std::env::remove_var("SS_LOCAL_PORT");
    std::env::remove_var("SS_REMOTE_HOST");
    std::env::remove_var("SS_REMOTE_PORT");
    std::env::remove_var("SS_PLUGIN_OPTIONS");
    let result = PluginEnv::from_env();
    assert!(result.is_err());
}

#[test]
#[serial_test::serial]
fn parse_env_no_plugin_options() {
    std::env::set_var("SS_LOCAL_HOST", "0.0.0.0");
    std::env::set_var("SS_LOCAL_PORT", "1080");
    std::env::set_var("SS_REMOTE_HOST", "server.example.com");
    std::env::set_var("SS_REMOTE_PORT", "8388");
    std::env::remove_var("SS_PLUGIN_OPTIONS");
    let env = PluginEnv::from_env().unwrap();
    assert!(env.plugin_options.is_none());
    std::env::remove_var("SS_LOCAL_HOST");
    std::env::remove_var("SS_LOCAL_PORT");
    std::env::remove_var("SS_REMOTE_HOST");
    std::env::remove_var("SS_REMOTE_PORT");
}

#[test]
fn parse_plugin_options_basic() {
    let opts = parse_plugin_options("tls;host=example.com;mode=websocket");
    assert_eq!(
        opts,
        vec![
            ("tls".to_string(), "".to_string()),
            ("host".to_string(), "example.com".to_string()),
            ("mode".to_string(), "websocket".to_string()),
        ]
    );
}

#[test]
fn parse_plugin_options_escaped() {
    let opts = parse_plugin_options(r"path=/a\;b;key=val\\ue");
    assert_eq!(
        opts,
        vec![
            ("path".to_string(), "/a;b".to_string()),
            ("key".to_string(), r"val\ue".to_string()),
        ]
    );
}

#[test]
fn parse_plugin_options_empty() {
    let opts = parse_plugin_options("");
    assert!(opts.is_empty());
}

#[test]
#[serial_test::serial]
fn plugin_env_local_addr() {
    std::env::set_var("SS_LOCAL_HOST", "127.0.0.1");
    std::env::set_var("SS_LOCAL_PORT", "1080");
    std::env::set_var("SS_REMOTE_HOST", "example.com");
    std::env::set_var("SS_REMOTE_PORT", "443");
    std::env::remove_var("SS_PLUGIN_OPTIONS");
    let env = PluginEnv::from_env().unwrap();
    let addr = env.local_addr();
    assert_eq!(addr.ip(), "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
    assert_eq!(addr.port(), 1080);
    std::env::remove_var("SS_LOCAL_HOST");
    std::env::remove_var("SS_LOCAL_PORT");
    std::env::remove_var("SS_REMOTE_HOST");
    std::env::remove_var("SS_REMOTE_PORT");
}

#[test]
fn parse_plugin_options_escaped_equals_in_key() {
    let opts = parse_plugin_options(r"k\=ey=value");
    assert_eq!(opts, vec![("k=ey".to_string(), "value".to_string()),]);
}

#[test]
fn parse_plugin_options_equals_in_value() {
    let opts = parse_plugin_options("key=a=b");
    assert_eq!(opts, vec![("key".to_string(), "a=b".to_string()),]);
}
