use anyhow::{bail, Result};
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use std::str::FromStr;

use crate::ollama::{OllamaClient, OllamaClientTrait, OllamaManager};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    Ollama,
    OpenAiCompatible,
}

impl FromStr for ProviderKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai-compatible" | "openai_compatible" | "openai" => Ok(Self::OpenAiCompatible),
            other => bail!("Unsupported provider '{}'", other),
        }
    }
}

pub struct LlmManagerOptions {
    pub provider: String,
    pub model: String,
    pub port: u16,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

pub struct LlmManager {
    backend: Backend,
}

pub struct ModelListing {
    pub models: Vec<String>,
    pub empty_hint: Option<String>,
}

pub struct StartupStatus {
    pub message: Option<String>,
}

pub struct ReadinessStatus {
    pub message: Option<String>,
}

pub struct AnalysisStatus {
    pub message: Option<String>,
}

enum Backend {
    Ollama { manager: OllamaManager, port: u16 },
    OpenAiCompatible(OpenAiCompatibleManager),
}

impl LlmManager {
    pub fn new(options: LlmManagerOptions) -> Result<Self> {
        let provider = ProviderKind::from_str(&options.provider)?;

        let backend = match provider {
            ProviderKind::Ollama => Backend::Ollama {
                manager: OllamaManager::new(options.model, options.port)?,
                port: options.port,
            },
            ProviderKind::OpenAiCompatible => {
                Backend::OpenAiCompatible(OpenAiCompatibleManager::new(
                    options.model,
                    options
                        .base_url
                        .unwrap_or_else(|| format!("http://localhost:{}", options.port)),
                    options.api_key,
                ))
            }
        };

        Ok(Self { backend })
    }

    pub async fn ensure_running(&mut self) -> Result<()> {
        match &mut self.backend {
            Backend::Ollama { manager, .. } => manager.ensure_running().await,
            Backend::OpenAiCompatible(_) => Ok(()),
        }
    }

    pub fn startup_status(&self) -> StartupStatus {
        match &self.backend {
            Backend::Ollama { .. } => StartupStatus {
                message: Some("[START] Starting Ollama...".to_string()),
            },
            Backend::OpenAiCompatible(_) => StartupStatus { message: None },
        }
    }

    pub fn readiness_status(&self, model_name: &str) -> ReadinessStatus {
        match &self.backend {
            Backend::Ollama { .. } => ReadinessStatus {
                message: Some(format!(
                    "[CHECK] Checking if model '{}' is available...",
                    model_name
                )),
            },
            Backend::OpenAiCompatible(_) => ReadinessStatus { message: None },
        }
    }

    pub fn analysis_status(&self) -> AnalysisStatus {
        match &self.backend {
            Backend::Ollama { .. } => AnalysisStatus {
                message: Some("[ANALYZE] Analyzing git repository...".to_string()),
            },
            Backend::OpenAiCompatible(_) => AnalysisStatus { message: None },
        }
    }

    pub async fn ensure_model_available(&self, model_name: &str) -> Result<()> {
        match &self.backend {
            Backend::Ollama { manager, .. } => manager.ensure_model_available(model_name).await,
            Backend::OpenAiCompatible(manager) => manager.ensure_model_available(model_name).await,
        }
    }

    pub async fn generate_commit(&self, prompt: &str) -> Result<String> {
        match &self.backend {
            Backend::Ollama { manager, .. } => manager.generate_commit(prompt).await,
            Backend::OpenAiCompatible(manager) => manager.generate_commit(prompt).await,
        }
    }

    pub async fn list_models(&self) -> Result<ModelListing> {
        match &self.backend {
            Backend::Ollama { port, .. } => {
                let client = OllamaClient::new(*port);
                if !client.is_running().await {
                    bail!("Ollama is not running. Please start Ollama first.");
                }
                Ok(ModelListing {
                    models: client.list_models().await?,
                    empty_hint: Some(
                        "No models found. Install models with 'ollama pull <model>'".to_string(),
                    ),
                })
            }
            Backend::OpenAiCompatible(manager) => manager.list_models().await,
        }
    }
}

struct OpenAiCompatibleManager {
    client: Client,
    model: String,
    base_url: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleManager {
    fn new(model: String, base_url: String, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            model,
            base_url,
            api_key,
        }
    }

    async fn list_models(&self) -> Result<ModelListing> {
        let models = self.fetch_model_ids().await?;
        Ok(ModelListing {
            models,
            empty_hint: Some("No models found for provider 'openai-compatible'.".to_string()),
        })
    }

    async fn ensure_model_available(&self, model_name: &str) -> Result<()> {
        let models = self.fetch_model_ids().await?;
        if models.iter().any(|model| model == model_name) {
            return Ok(());
        }

        bail!(
            "Model '{}' is not available for provider 'openai-compatible'",
            model_name
        );
    }

    async fn fetch_model_ids(&self) -> Result<Vec<String>> {
        let request = self.authorized(self.client.get(self.url("/v1/models")));
        let response = request.send().await?;

        if !response.status().is_success() {
            bail!(
                "OpenAI-compatible models endpoint returned status {}",
                response.status()
            );
        }

        let body: OpenAiModelsResponse = response.json().await?;
        Ok(body.data.into_iter().map(|model| model.id).collect())
    }

    async fn generate_commit(&self, prompt: &str) -> Result<String> {
        let request = OpenAiChatRequest {
            model: self.model.clone(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let response = self
            .authorized(self.client.post(self.url("/v1/chat/completions")))
            .json(&request)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND && self.looks_like_local_ollama() {
            return self.generate_via_ollama_native(prompt).await;
        }

        if !response.status().is_success() {
            bail!(
                "OpenAI-compatible generation endpoint returned status {}",
                response.status()
            );
        }

        let body: OpenAiChatResponse = response.json().await?;
        let content = body
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .unwrap_or_default();

        if content.trim().is_empty() {
            if self.looks_like_local_ollama() {
                return self.generate_via_ollama_native(prompt).await;
            }
            bail!("OpenAI-compatible provider returned an empty response");
        }

        Ok(content)
    }

    async fn generate_via_ollama_native(&self, prompt: &str) -> Result<String> {
        let chat_response = self
            .client
            .post(format!("{}/api/chat", self.ollama_native_base_url()))
            .json(&OllamaChatRequest {
                model: self.model.clone(),
                messages: vec![OllamaChatMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                }],
                stream: false,
            })
            .send()
            .await?;

        if chat_response.status().is_success() {
            let body: OllamaChatResponse = chat_response.json().await?;
            let content = body.message.content.trim().to_string();
            if content.is_empty() {
                bail!("Ollama native chat endpoint returned an empty response");
            }

            return Ok(content);
        }

        if chat_response.status() != reqwest::StatusCode::NOT_FOUND {
            bail!(
                "Ollama native chat endpoint returned status {}",
                chat_response.status()
            );
        }

        let generate_response = self
            .client
            .post(format!("{}/api/generate", self.ollama_native_base_url()))
            .json(&OllamaGenerateRequest {
                model: self.model.clone(),
                prompt: prompt.to_string(),
                stream: false,
            })
            .send()
            .await?;

        if !generate_response.status().is_success() {
            bail!(
                "Ollama native generation endpoint returned status {}",
                generate_response.status()
            );
        }

        let body: OllamaGenerateResponse = generate_response.json().await?;
        let content = body.response.trim().to_string();
        if content.is_empty() {
            bail!("Ollama native generation endpoint returned an empty response");
        }

        Ok(content)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    fn looks_like_local_ollama(&self) -> bool {
        self.base_url.contains("localhost") || self.base_url.contains("127.0.0.1")
    }

    fn ollama_native_base_url(&self) -> String {
        self.base_url
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .to_string()
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(api_key) = &self.api_key {
            if !api_key.trim().is_empty() {
                return request.bearer_auth(api_key.trim());
            }
        }

        request
    }
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Deserialize)]
struct OpenAiModel {
    id: String,
}

#[derive(serde::Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
}

#[derive(serde::Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatMessage,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}
