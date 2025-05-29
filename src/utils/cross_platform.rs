//! Cross-platform utilities for file system operations

use std::path::PathBuf;

/// Get the appropriate Ollama binary name for the current platform
pub fn get_ollama_binary_name() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "ollama-darwin-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "ollama-darwin-amd64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "ollama-linux-amd64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "ollama-windows-amd64.exe"
    } else {
        // Fallback - user will need system ollama
        "ollama"
    }
}

/// Get the executable name with proper extension for the platform
pub fn get_ollama_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ollama.exe"
    } else {
        "ollama"
    }
}

/// Get the temporary directory for the application
pub fn get_temp_dir() -> PathBuf {
    std::env::temp_dir().join("git-ai-commit")
}

/// Check if a path is executable
pub fn is_executable(path: &PathBuf) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let permissions = metadata.permissions();
            permissions.mode() & 0o111 != 0
        } else {
            false
        }
    }
    
    #[cfg(windows)]
    {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
    }
}

/// Create directory if it doesn't exist
pub fn ensure_dir_exists(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Get the appropriate shell command for the platform
pub fn get_shell_command() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Convert path to string safely
pub fn path_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}

/// Join paths in a cross-platform way
pub fn join_paths(base: &PathBuf, relative: &str) -> PathBuf {
    let mut result = base.clone();
    result.push(relative);
    result
}
