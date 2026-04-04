use git_ai_commit::ollama::{OllamaClient, OllamaClientTrait};
use git_ai_commit::OllamaManager;
use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Once;
use std::thread;

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

async fn ensure_ollama_running_for_provider_tests() -> Option<OllamaManager> {
    let client = OllamaClient::new(11434);
    if client.is_running().await {
        return None;
    }

    let mut manager =
        OllamaManager::new("tinyllama:latest".to_string(), 11434).expect("manager should build");
    manager
        .ensure_running()
        .await
        .expect("provider integration test needs a real local Ollama server");

    Some(manager)
}

fn with_openai_compatible_empty_models_server<F>(test_fn: F)
where
    F: FnOnce(String),
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind test HTTP server");
    let address = listener
        .local_addr()
        .expect("Failed to read test server address");
    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0_u8; 4096];
            let bytes_read = stream
                .read(&mut buffer)
                .expect("Failed to read test HTTP request");
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            assert!(
                request.starts_with("GET /v1/models HTTP/1.1"),
                "expected /v1/models request, got: {request}"
            );

            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: application/json\r\n",
                "Content-Length: 11\r\n",
                "Connection: close\r\n",
                "\r\n",
                "{\"data\":[]}"
            );
            stream
                .write_all(response.as_bytes())
                .expect("Failed to write test HTTP response");
        }
    });

    test_fn(format!("http://{}", address));
    handle
        .join()
        .expect("Test HTTP server thread should finish");
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
async fn test_list_models_with_explicit_ollama_provider_flag() {
    setup();

    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "git-ai-commit",
            "--",
            "--provider",
            "ollama",
            "--list-models",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed with status: {}\nStdout: {}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[serial(ollama)]
async fn test_list_models_with_openai_compatible_provider_uses_local_ollama_port() {
    setup();
    let _ollama_manager = ensure_ollama_running_for_provider_tests().await;

    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "git-ai-commit",
            "--",
            "--provider",
            "openai-compatible",
            "--port",
            "11434",
            "--list-models",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed with status: {}\nStdout: {}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_list_models_with_openai_compatible_provider_does_not_print_ollama_pull_hint() {
    setup();

    with_openai_compatible_empty_models_server(|base_url| {
        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "git-ai-commit",
                "--",
                "--provider",
                "openai-compatible",
                "--base-url",
                &base_url,
                "--list-models",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}\nStdout: {}\nStderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("No models found for provider 'openai-compatible'."),
            "expected provider-aware empty-list message, got: {stdout}"
        );
        assert!(
            !stdout.contains("ollama pull"),
            "openai-compatible empty-list output should not suggest an Ollama-only command: {stdout}"
        );
    });
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
