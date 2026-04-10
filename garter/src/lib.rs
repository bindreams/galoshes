pub mod error;
pub mod sip003;
pub mod plugin;
pub mod shutdown;
pub mod binary;
pub mod chain;

pub use error::{Error, Result};
pub use plugin::ChainPlugin;
pub use binary::BinaryPlugin;
pub use chain::ChainRunner;
pub use sip003::PluginEnv;

#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod sip003_tests;
