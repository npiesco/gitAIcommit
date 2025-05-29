use crate::ollama::{OllamaClient, OllamaBinary, OllamaClientTrait};
use crate::utils::error::GitAiError;
use anyhow::Result;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};

/// Manages Ollama binary lifecycle and AI generation
pub struct OllamaManager {
    binary: OllamaBinary,
    client: Arc<dyn OllamaClientTrait + Send + Sync>,
    model: String,
    process: Option<Child>,
    port: u16,
}

impl OllamaManager {
    pub fn new(model: String, port: u16) -> Result<Self> {
        let binary = OllamaBinary::new()?;
        let client: Arc<dyn OllamaClientTrait + Send + Sync> = Arc::new(OllamaClient::new(port));
        
        Ok(Self {
            binary,
            client,
            model,
            process: None,
            port,
        })
    }
    
    /// Ensure Ollama is running and ready to accept requests
    pub async fn ensure_running(&mut self) -> Result<()> {
        // Check if Ollama is already running
        if self.client.is_running().await {
            return Ok(());
        }
        
        // Extract and start Ollama binary
        let binary_path = self.binary.ensure_extracted().await?;
        self.start_ollama_server(&binary_path).await?;
        
        // Wait for server to be ready
        self.wait_for_server().await?;
        
        // If no model is specified, try to use the last available model
        if self.model.is_empty() {
            if let Ok(models) = self.client.list_models().await {
                if let Some(last_model) = models.last() {
                    println!("[INFO] No model specified, using last available model: {}", last_model);
                    self.model = last_model.clone();
                }
            }
        }
        
        if self.model.is_empty() {
            return Err(GitAiError::Ollama("No Ollama models found. Please install a model first with 'ollama pull <model>'".to_string()).into());
        }
        
        // Ensure default model is available
        self.ensure_default_model_available().await?;
        
        Ok(())
    }
    
    /// Generate a commit message using the AI model
    pub async fn generate_commit(&self, prompt: &str) -> Result<String> {
        self.client
            .generate(&self.model, prompt)
            .await
            .map_err(|e| GitAiError::Ollama(format!("Failed to generate commit message: {}", e)).into())
    }
    
    async fn start_ollama_server(&mut self, binary_path: &PathBuf) -> Result<()> {
        let mut cmd = Command::new(binary_path);
        cmd.arg("serve")
           .env("OLLAMA_HOST", format!("0.0.0.0:{}", self.port))
           .stdin(Stdio::null())
           .stdout(Stdio::null())
           .stderr(Stdio::null());
        
        let child = cmd.spawn()
            .map_err(|e| GitAiError::Ollama(format!("Failed to start Ollama: {}", e)))?;
        
        self.process = Some(child);
        Ok(())
    }
    
    async fn wait_for_server(&self) -> Result<()> {
        let max_attempts = 30;
        let delay = std::time::Duration::from_secs(1);
        
        for _ in 0..max_attempts {
            if self.client.is_running().await {
                return Ok(());
            }
            tokio::time::sleep(delay).await;
        }
        
        Err(GitAiError::Ollama("Timed out waiting for Ollama server to start".to_string()).into())
    }
    
    /// Ensure the specified model is available, downloading it if necessary
    pub async fn ensure_model_available(&self, model_name: &str) -> Result<()> {
        if !self.client.has_model(model_name).await? {
            println!("[DOWN] Model '{}' not found. Downloading...", model_name);
            self.client.pull_model(model_name).await?;
            println!("[ OK ] Successfully downloaded model '{}'", model_name);
        }
        Ok(())
    }
    
    /// Ensure the default model is available
    pub async fn ensure_default_model_available(&self) -> Result<()> {
        self.ensure_model_available(&self.model).await
    }
}

impl Drop for OllamaManager {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            // Attempt to gracefully terminate the process
            let _ = process.start_kill();
        }
    }
}
