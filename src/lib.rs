//! Git AI Commit - AI-powered commit message generator
//! 
//! This library provides functionality to analyze git repositories,
//! manage Ollama instances, and generate intelligent commit messages.

pub mod cli;
pub mod config;
pub mod git;
pub mod ollama;
pub mod formatting;
pub mod utils;

pub use cli::Args;
pub use config::Config;
pub use git::GitCollector;
pub use ollama::OllamaManager;
pub use formatting::PromptBuilder;
