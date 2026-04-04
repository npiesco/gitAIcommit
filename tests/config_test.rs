use git_ai_commit::config::Config;
use std::fs;

#[test]
fn test_load_config_defaults() {
    // Create a temporary directory for the test
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Test loading non-existent config should return defaults
    let config = Config::load_from_path(&config_path).unwrap();
    assert_eq!(config.provider, "ollama");
    assert_eq!(config.model, "gemma4:latest");
    assert_eq!(config.base_url, None);
    assert_eq!(config.api_key, None);
    assert_eq!(config.max_files, 10);
    assert_eq!(config.max_diff_lines, 50);
    assert_eq!(config.port, 11434);
    assert_eq!(config.timeout_seconds, 60);
}

#[test]
fn test_load_custom_config() {
    // Create a temporary config file
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
        provider = "openai-compatible"
        model = "gemma4:1b"
        base_url = "https://api.openai.com"
        api_key = "sk-test"
        max_files = 20
        max_diff_lines = 100
        port = 12345
        timeout_seconds = 120
    "#;

    std::fs::write(&config_path, config_content).unwrap();

    // Test loading the config
    let config = Config::load_from_path(&config_path).unwrap();
    assert_eq!(config.provider, "openai-compatible");
    assert_eq!(config.model, "gemma4:1b");
    assert_eq!(config.base_url, Some("https://api.openai.com".to_string()));
    assert_eq!(config.api_key, Some("sk-test".to_string()));
    assert_eq!(config.max_files, 20);
    assert_eq!(config.max_diff_lines, 100);
    assert_eq!(config.port, 12345);
    assert_eq!(config.timeout_seconds, 120);
}

#[test]
fn test_load_custom_config_llama3() {
    // Create a temporary config file
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
        model = "llama3"
        max_files = 20
        max_diff_lines = 100
        port = 12345
        timeout_seconds = 120
    "#;

    std::fs::write(&config_path, config_content).unwrap();

    // Test loading the config
    let config = Config::load_from_path(&config_path).unwrap();
    assert_eq!(config.model, "llama3");
    assert_eq!(config.max_files, 20);
    assert_eq!(config.max_diff_lines, 100);
    assert_eq!(config.port, 12345);
    assert_eq!(config.timeout_seconds, 120);
}

#[test]
fn test_partial_config() {
    // Test with a partial config (only some fields specified)
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("partial_config.toml");

    let config_content = r#"
        model = "mistral"
        port = 54321
    "#;

    std::fs::write(&config_path, config_content).unwrap();

    let config = Config::load_from_path(&config_path).unwrap();
    assert_eq!(config.model, "mistral");
    assert_eq!(config.port, 54321);
    // Other fields should have default values
    assert_eq!(config.max_files, 10);
    assert_eq!(config.max_diff_lines, 50);
    assert_eq!(config.timeout_seconds, 60);
}

#[test]
fn test_save_and_load_config() {
    // Test saving and then loading a config
    let _temp_dir = tempfile::tempdir().unwrap();

    let config = Config {
        provider: "ollama".to_string(),
        model: "custom-model".to_string(),
        base_url: Some("http://localhost:11434".to_string()),
        api_key: None,
        max_files: 42,
        ..Config::default()
    };

    // Save the config
    config.save().unwrap();

    // Load it back
    let loaded_config = Config::load().unwrap();

    // The saved config should match what we set
    assert_eq!(loaded_config.provider, "ollama");
    assert_eq!(loaded_config.model, "custom-model");
    assert_eq!(
        loaded_config.base_url,
        Some("http://localhost:11434".to_string())
    );
    assert_eq!(loaded_config.max_files, 42);

    // Clean up the config file
    let config_dir = dirs::config_dir().unwrap().join("git-ai-commit");
    let saved_config_path = config_dir.join("config.toml");
    if saved_config_path.exists() {
        fs::remove_file(saved_config_path).unwrap();
    }
}
