use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

pub struct ModelManager;

impl ModelManager {
    pub fn new() -> Self {
        ModelManager
    }

    /// List all available models
    pub fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let output = Command::new("ollama")
            .arg("list")
            .output()
            .context("Failed to execute ollama list command")?;

        if !output.status.success() {
            anyhow::bail!("Failed to list models: {}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8(output.stdout).context("Invalid UTF-8 in command output")?;
        
        // Parse the output line by line, skipping the header
        let mut models = Vec::new();
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                models.push(ModelInfo {
                    name: parts[0].to_string(),
                    size: parts[1].parse().unwrap_or(0),
                    modified_at: parts[2..].join(" "),
                });
            }
        }

        Ok(models)
    }

    /// Check if a specific model is available
    pub fn has_model(&self, model_name: &str) -> Result<bool> {
        let models = self.list_models()?;
        Ok(models.iter().any(|m| m.name == model_name))
    }

    /// Pull a model
    pub fn pull_model(&self, model_name: &str) -> Result<()> {
        let status = Command::new("ollama")
            .args(["pull", model_name])
            .status()
            .context("Failed to execute ollama pull command")?;

        if !status.success() {
            anyhow::bail!("Failed to pull model: {}", model_name);
        }

        Ok(())
    }

    /// Delete a model
    pub fn delete_model(&self, model_name: &str) -> Result<()> {
        // First check if the model exists to avoid error messages
        if !self.has_model(model_name)? {
            return Ok(());
        }

        let status = Command::new("ollama")
            .args(["rm", model_name])
            .status()
            .context("Failed to execute ollama rm command")?;

        if !status.success() {
            anyhow::bail!("Failed to delete model: {}", model_name);
        }

        Ok(())
    }

    /// Ensure a model is available, pulling it if necessary
    pub fn ensure_model_available(&self, model_name: &str) -> Result<()> {
        if !self.has_model(model_name)? {
            println!("Model '{}' not found. Downloading...", model_name);
            self.pull_model(model_name)?;
            println!("Successfully downloaded model '{}'", model_name);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_list_models() {
        let manager = ModelManager::new();
        let result = manager.list_models();
        assert!(result.is_ok(), "Failed to list models");
    }

    #[test]
    #[serial]
    fn test_has_model() {
        let manager = ModelManager::new();
        let _ = manager.ensure_model_available("tinyllama:latest");
        
        let result = manager.has_model("tinyllama:latest");
        assert!(result.is_ok(), "Failed to check model existence");
        
        // Clean up
        let _ = manager.delete_model("tinyllama:latest");
    }
}
