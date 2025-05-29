use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default AI model to use
    #[serde(default = "default_model")]
    pub model: String,
    
    /// Maximum number of files to include in the diff analysis
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    
    /// Maximum number of diff lines to include per file
    #[serde(default = "default_max_diff_lines")]
    pub max_diff_lines: usize,
    
    /// Port for the Ollama server
    #[serde(default = "default_port")]
    pub port: u16,
    
    /// Timeout for AI generation in seconds
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

fn default_model() -> String {
    "gemma3:4b".to_string()
}

fn default_max_files() -> usize {
    10
}

fn default_max_diff_lines() -> usize {
    50
}

fn default_port() -> u16 {
    11434
}

fn default_timeout_seconds() -> u64 {
    60
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: default_model(),
            max_files: default_max_files(),
            max_diff_lines: default_max_diff_lines(),
            port: default_port(),
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

impl Config {
    /// Load configuration from the default location
    pub fn load() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("git-ai-commit");
        
        std::fs::create_dir_all(&config_dir)
            .context("Failed to create config directory")?;
            
        let config_path = config_dir.join("config.toml");
        println!("Loading config from: {}", config_path.display());
        
        let config = Self::load_from_path(&config_path);
        println!("Config loaded: {:?}", config);
        config
    }
    
    /// Load configuration from a specific path
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        // If the config file doesn't exist, return defaults
        if !path.exists() {
            println!("Config file not found at: {}", path.display());
            return Ok(Self::default());
        }
        
        println!("Reading config from: {}", path.display());
        let config_content = fs::read_to_string(path)
            .context(format!("Failed to read config file: {}", path.display()))?;
            
        println!("Config content: {}", config_content);
        let config: Self = toml::from_str(&config_content)
            .context("Failed to parse config file")?;
            
        println!("Parsed config: {:?}", config);
        Ok(config)
    }
    
    /// Save the current configuration to the default location
    pub fn save(&self) -> Result<()> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("git-ai-commit");
            
        std::fs::create_dir_all(&config_dir)
            .context("Failed to create config directory")?;
            
        let config_path = config_dir.join("config.toml");
        let config_content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
            
        fs::write(&config_path, config_content)
            .context(format!("Failed to write config file: {}", config_path.display()))?;
            
        Ok(())
    }
}
