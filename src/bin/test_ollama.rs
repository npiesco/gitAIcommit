
use git_ai_commit::ollama::{OllamaClient, OllamaClientTrait};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[TEST] Testing Ollama Connectivity");
    println!("--------------------------------");
    
    let client = OllamaClient::new(11434);
    
    // Step 1: Check server
    print!("Checking Ollama server... ");
    let is_running = client.is_running().await;
    if is_running {
        println!("[PASS] RUNNING");
    } else {
        println!("[FAIL] NOT RUNNING");
        println!("\n[WARN] Please start Ollama:");
        println!("  ollama serve");
        return Ok(());
    }
    
    // Step 2: Check for tiny model
    let model = "tinyllama";
    print!("Checking for model '{}'... ", model);
    let has_model = client.has_model(model).await?;
    if has_model {
        println!("[PASS] AVAILABLE");
    } else {
        println!("[FAIL] NOT FOUND");
        println!("\n[WAIT] Pulling model (this will take a few minutes)...");
        client.pull_model(model).await?;
        println!("[ OK ] Model pulled successfully");
    }
    
    // Step 3: Test generation
    println!("\n[TEST] Testing generation...");
    let prompt = "Hello! Respond with just the word 'success'";
    match client.generate(model, prompt).await {
        Ok(response) => {
            println!("[ OK ] Generation successful!");
            println!("[RESP] Response: {}", response.trim());
        }
        Err(e) => {
            eprintln!("[ERR ] Generation failed: {}", e);
            return Err(e.into());
        }
    }
    
    println!("\n[ OK ] All tests passed! Ollama is ready.");
    Ok(())
}
