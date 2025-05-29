use crate::utils::{cross_platform, error::GitAiError};
use anyhow::Result;
use include_dir::{include_dir, Dir};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

static ASSETS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

/// Manages embedded Ollama binary extraction and execution
pub struct OllamaBinary {
    temp_dir: Option<PathBuf>,
    binary_path: Option<PathBuf>,
}

impl OllamaBinary {
    pub fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: None,
            binary_path: None,
        })
    }
    
    /// Extract the appropriate Ollama binary for the current platform
    pub async fn ensure_extracted(&mut self) -> Result<PathBuf> {
        if let Some(ref path) = self.binary_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }
        
        // Try to find system Ollama first
        if let Ok(system_path) = which::which("ollama") {
            self.binary_path = Some(system_path.clone());
            return Ok(system_path);
        }
        
        // Extract embedded binary
        let binary_name = cross_platform::get_ollama_binary_name();
        let binary_file = ASSETS_DIR
            .get_file(binary_name)
            .ok_or_else(|| GitAiError::Ollama(format!("Ollama binary not found for platform: {}", binary_name)))?;
        
        // Create temporary directory
        let temp_dir = tempdir()
            .map_err(|e| GitAiError::Ollama(format!("Failed to create temp directory: {}", e)))?;
        
        let temp_path = temp_dir.path().to_path_buf();
        let binary_path = temp_path.join(cross_platform::get_ollama_executable_name());
        
        // Write binary to temp file
        fs::write(&binary_path, binary_file.contents())
            .map_err(|e| GitAiError::Ollama(format!("Failed to write binary: {}", e)))?;
        
        // Make executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary_path, perms)?;
        }
        
        self.temp_dir = Some(temp_path);
        self.binary_path = Some(binary_path.clone());
        
        Ok(binary_path)
    }
}

impl Drop for OllamaBinary {
    fn drop(&mut self) {
        // Cleanup is handled automatically by tempfile
    }
}
