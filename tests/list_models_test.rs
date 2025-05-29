use git_ai_commit::ollama::{OllamaClient, OllamaClientTrait};
use std::process::Command;
use std::sync::Once;

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| {
        // Ensure Ollama is running before tests
        let status = Command::new("ollama")
            .args(["--help"])
            .status()
            .expect("Failed to check if Ollama is installed");
        
        if !status.success() {
            panic!("Ollama is not installed or not in PATH");
        }
    });
}

#[tokio::test]
async fn test_list_models_integration() {
    setup();
    
    // Test the --list-models flag
    let output = Command::new("cargo")
        .args(["run", "--bin", "git-ai-commit", "--", "--list-models"])
        .output()
        .expect("Failed to execute command");
    
    // Check that the command executed successfully
    assert!(
        output.status.success(),
        "Command failed with status: {}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Check that the output looks reasonable
    if stdout.contains("No models found") {
        // This is fine if no models are installed
        return;
    }
    
    // Otherwise, check that the output contains a list of models
    assert!(
        stdout.contains("Available models:") && stdout.split('\n').count() > 1,
        "Unexpected output format: {} ",
        stdout
    );
}

#[tokio::test]
async fn test_list_models_client() {
    setup();
    
    let client = OllamaClient::new(11434);
    
    // Skip test if Ollama is not running
    if !client.is_running().await {
        eprintln!("Skipping test - Ollama is not running");
        return;
    }
    
    // Test the list_models method directly
    let models = client.list_models().await;
    assert!(models.is_ok(), "Failed to list models: {:?}", models.err());
    
    // The test passes whether or not there are models installed,
    // as long as the API call succeeds
}
