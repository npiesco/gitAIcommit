//! Git repository analysis and data collection

pub mod collector;
pub mod status;
pub mod diff;
pub mod files;

pub use collector::{GitCollector, GitInfo};
pub use status::GitStatus;
pub use diff::DiffInfo;
pub use files::FileChange;
