use crate::error::Error;

#[test]
fn io_error_converts() {
    let io_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "port taken");
    let err: Error = io_err.into();
    assert!(matches!(err, Error::Io(_)));
    assert!(err.to_string().contains("port taken"));
}

#[test]
fn plugin_exit_error_displays_name_and_code() {
    let err = Error::PluginExit {
        name: "v2ray-plugin".into(),
        code: 1,
    };
    assert!(err.to_string().contains("v2ray-plugin"));
    assert!(err.to_string().contains("1"));
}

#[test]
fn plugin_killed_error_displays_name() {
    let err = Error::PluginKilled {
        name: "yamux".into(),
    };
    assert!(err.to_string().contains("yamux"));
}

#[test]
fn chain_error_displays_message() {
    let err = Error::Chain("port allocation failed".into());
    assert!(err.to_string().contains("port allocation failed"));
}
