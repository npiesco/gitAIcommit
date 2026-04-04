//! Git AI Commit - AI-powered commit message generator
//!
//! This library provides functionality to analyze git repositories,
//! manage Ollama instances, and generate intelligent commit messages.

pub mod cli;
pub mod commit;
pub mod config;
pub mod formatting;
pub mod git;
pub mod llm;
pub mod ollama;
pub mod utils;

pub use cli::Args;
pub use commit::{commit_message_to_repo, sanitize_commit_message};
pub use config::Config;
pub use formatting::PromptBuilder;
pub use git::GitCollector;
pub use llm::LlmManager;
pub use ollama::OllamaManager;
