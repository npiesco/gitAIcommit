use super::*;
use mockito::Server;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn test_list_models() {
    // Start a mock server
    let mut server = Server::new_async().await;
    
    // Create client pointing to our mock server
    let url = server.url();
    let port: u16 = url.split(':').nth(2).unwrap().parse().unwrap();
    
    // Create mock responses
    let mock_models = vec!["model1:latest", "model2:latest", "model3:latest"];
    let mock_response = json!({ "models": mock_models.iter().map(|m| json!({ "name": m })).collect::<Vec<_>>() });
    
    // Set up the mock for list_models
    let _m = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_response.to_string())
        .create_async()
        .await;
    
    // Create client using the public constructor
    let client = OllamaClient::new(port);
    
    // Test list_models
    let models = client.list_models().await;
    assert!(models.is_ok(), "list_models failed: {:?}", models.err());
    let models = models.unwrap();
    assert_eq!(models, mock_models);
    
    // Test has_model with full model name including tag
    let _m_has = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_response.to_string())
        .create_async()
        .await;
    
    let has_model = client.has_model("model1:latest").await;
    assert!(has_model.is_ok(), "has_model returned error: {:?}", has_model.err());
    let has_model = has_model.unwrap();
    assert!(has_model, "Expected model1:latest to exist");
    
    // Test has_model with a non-existent model
    let _m_has_not = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_response.to_string())
        .create_async()
        .await;
    
    let has_nonexistent = client.has_model("nonexistent:latest").await;
    assert!(has_nonexistent.is_ok(), "has_model returned error: {:?}", has_nonexistent.err());
    assert!(!has_nonexistent.unwrap(), "Expected nonexistent:latest to not exist");
    
    // Test get_last_model - should return the last model from the list
    let _m_last = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_response.to_string())
        .create_async()
        .await;
    
    let last_model = client.get_last_model().await;
    assert!(last_model.is_ok(), "get_last_model returned error: {:?}", last_model.err());
    assert_eq!(last_model.unwrap(), Some("model3:latest".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_model_operations() {
    // Start a mock server
    let mut server = Server::new_async().await;
    
    // Create client pointing to our mock server
    let url = server.url();
    let port: u16 = url.split(':').nth(2).unwrap().parse().unwrap();
    
    // Create client using the public constructor
    let client = OllamaClient::new(port);
    
    // Test 1: Pull model
    {
        // Mock the pull response
        let _m_pull = server
            .mock("POST", "/api/pull")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({"status": "success"}).to_string())
            .create_async()
            .await;
        
        // Mock the tags endpoint to include the newly pulled model
        let _m_tags = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "models": [{"name": "test-model:latest"}] }).to_string())
            .create_async()
            .await;
        
        // Test pull_model
        let pull_result = client.pull_model("test-model:latest").await;
        assert!(pull_result.is_ok(), "Failed to pull model: {:?}", pull_result.err());
    }
    
    // Test 2: Delete model
    {
        // Mock the delete response
        let _m_delete = server
            .mock("DELETE", "/api/delete")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({"status": "success"}).to_string())
            .create_async()
            .await;
        
        // Mock the tags endpoint to return an empty list after deletion
        let _m_tags = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "models": [] }).to_string())
            .create_async()
            .await;
        
        // Test delete_model
        let delete_result = client.delete_model("test-model:latest").await;
        assert!(delete_result.is_ok(), "Failed to delete model: {:?}", delete_result.err());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_generate() {
    // Start a mock server
    let mut server = Server::new_async().await;
    
    // Mock the /api/generate endpoint
    let mock_response = json!({
        "response": "This is a test response"
    });
    
    let _m = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_response.to_string())
        .create_async()
        .await;
    
    // Create client pointing to our mock server
    let url = server.url();
    let port: u16 = url.split(':').nth(2).unwrap().parse().unwrap();
    let client = OllamaClient::new(port);
    
    // Test generate
    let response = client.generate("test-model", "Test prompt").await.unwrap();
    assert_eq!(response, "This is a test response");
}

#[tokio::test]
async fn test_get_last_model_empty_list() {
    // Start a mock server
    let mut server = Server::new_async().await;
    
    // Mock the /api/tags endpoint with empty models list
    let mock_response = json!({ "models": [] });
    
    let _m = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_response.to_string())
        .create_async()
        .await;
    
    // Create client pointing to our mock server
    let url = server.url();
    let port: u16 = url.split(':').nth(2).unwrap().parse().unwrap();
    let client = OllamaClient::new(port);
    
    // Test get_last_model with empty list
    let last_model = client.get_last_model().await.unwrap();
    assert_eq!(last_model, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_last_model_integration() {
    let client = OllamaClient::new(11434);
    
    if !client.is_running().await {
        println!("Skipping test - Ollama not running");
        return;
    }
    
    let models = client.list_models().await.unwrap();
    if models.is_empty() {
        println!("Skipping test - No models available");
        return;
    }
    
    let last_model = models.last().unwrap().clone();
    println!("Using model: {}", last_model);
    
    // Test that the last model can be used for generation
    let result = client.generate(&last_model, "Hello").await;
    assert!(result.is_ok(), "Should be able to generate with last model");
}
