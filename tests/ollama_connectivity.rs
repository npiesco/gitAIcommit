use git_ai_commit::ollama::{OllamaClient, OllamaClientTrait};

#[tokio::test]
async fn test_basic_ollama_connectivity() {
    let client = OllamaClient::new(11434);

    println!("Testing Ollama connectivity...");

    // Test 1: Check if Ollama is running
    let is_running = client.is_running().await;
    println!(
        "✓ Ollama server check: {}",
        if is_running { "RUNNING" } else { "NOT RUNNING" }
    );

    if !is_running {
        println!("ℹ  Start Ollama manually to continue tests");
        println!("ℹ  Run: ollama serve");
        return;
    }

    // Test 2: Try to pull a tiny model if it doesn't exist
    let model_name = "tinyllama";
    let has_model = client.has_model(model_name).await.unwrap_or(false);
    println!("✓ Model '{}' available: {}", model_name, has_model);

    if !has_model {
        println!(
            "ℹ  Pulling model '{}' (this may take a while)...",
            model_name
        );
        match client.pull_model(model_name).await {
            Ok(_) => println!("✓ Model pulled successfully"),
            Err(e) => {
                println!("✗ Failed to pull model: {}", e);
                return;
            }
        }
    }

    // Test 3: Simple generation test
    println!("ℹ  Testing generation...");
    match client.generate(model_name, "Say hello in one word").await {
        Ok(response) => {
            println!("✓ Generation successful");
            println!("  Response: {}", response.trim());
            assert!(!response.trim().is_empty());
        }
        Err(e) => {
            println!("✗ Generation failed: {}", e);
            panic!("Generation test failed");
        }
    }

    println!("🎉 All Ollama connectivity tests passed!");
}
