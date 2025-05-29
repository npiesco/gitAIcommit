use git_ai_commit::ollama::model_manager::ModelManager;
use serial_test::serial;

#[test]
#[serial]
fn test_ensure_model_available() {
    let manager = ModelManager::new();
    
    // Ensure the model is not present initially
    let _ = manager.delete_model("tinyllama:latest");
    
    // Test ensuring the model is available
    let result = manager.ensure_model_available("tinyllama:latest");
    assert!(result.is_ok(), "Failed to ensure model is available: {:?}", result);
    
    // Verify the model is now available
    let models = manager.list_models()
        .expect("Failed to list models");
    assert!(
        models.iter().any(|m| m.name == "tinyllama:latest"), 
        "Model should be in the list after ensuring it's available. Models found: {:?}", 
        models
    );
    
    // Verify the model is now available
    let models = manager.list_models().unwrap();
    assert!(models.iter().any(|m| m.name == "tinyllama:latest"), "Model should be available after ensuring");
    
    // Clean up
    manager.delete_model("tinyllama:latest").unwrap();
}

#[test]
#[serial]
fn test_ensure_model_available_already_exists() {
    let manager = ModelManager::new();
    
    // Make sure the model exists first
    let _ = manager.ensure_model_available("tinyllama:latest");
    
    // Test ensuring the model is available when it already exists
    let result = manager.ensure_model_available("tinyllama:latest");
    assert!(
        result.is_ok(), 
        "Should handle already existing model gracefully: {:?}", 
        result
    );
    
    // Clean up
    manager.delete_model("tinyllama:latest").unwrap();
}
