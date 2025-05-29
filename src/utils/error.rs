use thiserror::Error;

/// Custom error types for the git-ai-commit application
#[derive(Error, Debug)]
pub enum GitAiError {
    #[error("Git operation failed: {0}")]
    Git(String),
    
    #[error("Ollama operation failed: {0}")]
    Ollama(String),
    
    #[error("File system operation failed: {0}")]
    FileSystem(String),
    
    #[error("Network operation failed: {0}")]
    Network(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Parsing error: {0}")]
    Parse(String),
    
    #[error("Timeout error: {0}")]
    Timeout(String),
    
    #[error("Platform not supported: {0}")]
    UnsupportedPlatform(String),
}

impl GitAiError {
    pub fn git(msg: impl Into<String>) -> Self {
        Self::Git(msg.into())
    }
    
    pub fn ollama(msg: impl Into<String>) -> Self {
        Self::Ollama(msg.into())
    }
    
    pub fn filesystem(msg: impl Into<String>) -> Self {
        Self::FileSystem(msg.into())
    }
    
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }
    
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
    
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }
    
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }
    
    pub fn unsupported_platform(msg: impl Into<String>) -> Self {
        Self::UnsupportedPlatform(msg.into())
    }
}
