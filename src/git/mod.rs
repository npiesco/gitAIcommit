//! Git repository analysis and data collection

pub mod collector;
pub mod diff;
pub mod files;
pub mod repository;
pub mod status;

pub use collector::{GitCollector, GitInfo};
pub use diff::DiffInfo;
pub use files::FileChange;
pub use repository::{
    branch_diff_stat, current_branch, detect_default_branch, ensure_push_branch,
    preview_push_branch, push_current_branch,
};
pub use status::GitStatus;
