//! Ollama integration module for managing local AI models

use async_trait::async_trait;
use anyhow::Result;

pub mod manager;
pub mod client;
pub mod binary;
pub mod model_manager;

#[cfg(test)]
mod client_test;

#[async_trait]
pub trait OllamaClientTrait: Send + Sync {
    async fn is_running(&self) -> bool;
    async fn generate(&self, model: &str, prompt: &str) -> Result<String>;
    async fn list_models(&self) -> Result<Vec<String>>;
    async fn has_model(&self, model_name: &str) -> Result<bool>;
    async fn pull_model(&self, model_name: &str) -> Result<()>;
    
    /// Get the last available model from the list of installed models
    /// Returns None if no models are installed
    async fn get_last_model(&self) -> Result<Option<String>>;
    
    /// Delete a model from the Ollama server
    /// 
    /// # Arguments
    /// * `model_name` - The name of the model to delete (including tag, e.g., "llama2:latest")
    /// 
    /// # Returns
    /// * `Ok(())` if the model was successfully deleted
    /// * `Err` if there was an error deleting the model
    async fn delete_model(&self, model_name: &str) -> Result<()>;
}

pub use manager::OllamaManager;
pub use client::OllamaClient;
pub use binary::OllamaBinary;
pub use model_manager::ModelManager;
