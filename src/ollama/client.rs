use crate::utils::error::GitAiError;
use crate::ollama::OllamaClientTrait;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

/// HTTP client for communicating with Ollama API
#[derive(Clone)]
pub struct OllamaClient {
    client: Client,
    base_url: String,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    models: Vec<ModelInfo>,
}

#[async_trait]
impl OllamaClientTrait for OllamaClient {
    async fn is_running(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await.is_ok()
    }

    async fn generate(&self, model: &str, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url);
        
        let payload = json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.7,
                "top_p": 0.9,
                "max_tokens": 200
            }
        });
        
        let response = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitAiError::Ollama(format!("Failed to send request: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(GitAiError::Ollama(format!("Request failed with status {}: {}", status, text)).into());
        }
        
        let generate_response: GenerateResponse = response
            .json()
            .await
            .map_err(|e| GitAiError::Ollama(format!("Failed to parse response: {}", e)))?;
        
        Ok(generate_response.response)
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| GitAiError::Ollama(format!("Failed to get models: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(GitAiError::Ollama("Failed to fetch models".to_string()).into());
        }
        
        let models_response: ModelsResponse = response
            .json()
            .await
            .map_err(|e| GitAiError::Ollama(format!("Failed to parse models response: {}", e)))?;
            
        let models = models_response.models.into_iter()
            .map(|m| m.name)
            .collect();
            
        Ok(models)
    }
    
    async fn has_model(&self, model_name: &str) -> Result<bool> {
        let models = self.list_models().await?;
        Ok(models.iter().any(|m| m == model_name))
    }
    
    async fn get_last_model(&self) -> Result<Option<String>> {
        let models = self.list_models().await?;
        Ok(models.last().cloned())
    }
    
    async fn pull_model(&self, model_name: &str) -> Result<()> {
        let url = format!("{}/api/pull", self.base_url);
        
        let payload = json!({
            "name": model_name,
            "stream": false
        });
        
        let response = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitAiError::Ollama(format!("Failed to pull model: {}", e)))?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(GitAiError::Ollama(format!("Failed to pull model: {} - {}", status, text)).into());
        }
        
        Ok(())
    }
    
    async fn delete_model(&self, model_name: &str) -> Result<()> {
        let url = format!("{}/api/delete", self.base_url);
        
        let payload = json!({
            "name": model_name,
        });
        
        let response = self.client
            .delete(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitAiError::Ollama(e.to_string()))?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            let error_msg = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Err(GitAiError::Ollama(format!("Failed to delete model: {}", error_msg)).into())
        }
    }
}

impl OllamaClient {
    pub fn new(port: u16) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout for long operations
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: format!("http://localhost:{}", port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_ollama_connection() {
        let client = OllamaClient::new(11434);
        
        // Test if Ollama is running
        let is_running = client.is_running().await;
        println!("Ollama running: {}", is_running);
        
        if !is_running {
            // Skip test if Ollama is not running
            println!("Skipping test - Ollama not running");
            return;
        }
        
        // Test simple generation with a tiny model
        let result = client.generate("tinyllama", "Hello").await;
        match result {
            Ok(response) => {
                println!("Response: {}", response);
                assert!(!response.is_empty());
            }
            Err(e) => {
                println!("Error: {}", e);
                // Don't fail the test if model doesn't exist
            }
        }
    }
    
    #[tokio::test]
    async fn test_model_listing() {
        let client = OllamaClient::new(11434);
        
        if !client.is_running().await {
            println!("Skipping test - Ollama not running");
            return;
        }
        
        let has_model = client.has_model("tinyllama").await;
        match has_model {
            Ok(exists) => println!("TinyLlama exists: {}", exists),
            Err(e) => println!("Error checking model: {}", e),
        }
    }
}
