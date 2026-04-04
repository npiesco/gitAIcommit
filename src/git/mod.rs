//! Git repository analysis and data collection

pub mod collector;
pub mod diff;
pub mod files;
pub mod status;

pub use collector::{GitCollector, GitInfo};
pub use diff::DiffInfo;
pub use files::FileChange;
pub use status::GitStatus;
