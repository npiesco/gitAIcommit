use anyhow::Result;
use git_ai_commit::{
    cli::Args,
    git::GitCollector,
    ollama::{OllamaManager, OllamaClient, OllamaClientTrait},
    formatting::PromptBuilder,
    utils::error::GitAiError,
};
use std::env;
use std::path::PathBuf;
use tokio;
use atty;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::load();
    
    // Handle --list-models flag
    if args.list_models {
        let client = OllamaClient::new(args.port);
        if !client.is_running().await {
            eprintln!("Error: Ollama is not running. Please start Ollama first.");
            std::process::exit(1);
        }
        
        match client.list_models().await {
            Ok(models) => {
                if models.is_empty() {
                    println!("No models found. Install models with 'ollama pull <model>'");
                } else {
                    println!("Available models:");
                    for model in models {
                        println!("- {}", model);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to list models: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    
    // Check if we're in a git repository
    let current_dir = env::current_dir()
        .map_err(|e| GitAiError::Git(format!("Failed to get current directory: {}", e)))?;
    
    if !is_git_repository(&current_dir).await? {
        eprintln!("Error: Not a git repository");
        eprintln!("Please run this command from within a git repository.");
        std::process::exit(1);
    }
    
    println!("AI Commit Message Generator");
    println!("==============================");
    
    // Initialize components
    let git_collector = GitCollector::new(current_dir.clone());
    let mut ollama_manager = OllamaManager::new(args.model.clone(), args.port)?;
    let prompt_builder = PromptBuilder::new(args.max_files, args.max_diff_lines);
    
    // Ensure the model is available
    println!("[CHECK] Checking if model '{}' is available...", args.model);
    ollama_manager.ensure_model_available(&args.model).await?;
    
    // Collect initial git information
    println!("[ANALYZE] Analyzing git repository...");
    let mut git_info = git_collector.collect_all().await?;
    
    // If --add-unstaged flag is set, stage all unstaged changes and refresh git info
    let mut after_staging = false;
    if args.add_unstaged && (!git_info.status.modified_files.is_empty() || !git_info.status.untracked_files.is_empty()) {
        println!("[STAGE] Staging all unstaged changes...");
        git_collector.stage_all_unstaged().await?;
        
        // Refresh git info after staging
        println!("[REFRESH] Refreshing repository status...");
        git_info = git_collector.collect_all().await?;
        after_staging = true;
        
        if git_info.is_empty(true) {  // true = after staging
            println!("[INFO] No changes to commit after staging.");
            return Ok(());
        }
    }
    
    if git_info.is_empty(after_staging) {
        println!("[INFO] No changes detected in the repository.");
        println!("Please make some changes and stage them before generating a commit message.");
        return Ok(());
    }
    
    if args.dry_run {
        println!("[DRY RUN] Dry run mode - will generate commit message but not commit");
        println!("[ANALYSIS] Git Repository Analysis:");
        println!("{}", git_info.display());
    }
    
    // Start Ollama if needed
    println!("[START] Starting Ollama...");
    ollama_manager.ensure_running().await?;
    
    // Generate commit message
    println!("[GENERATE] Generating commit message...");
    let prompt = prompt_builder.build(&git_info);
    
    if args.verbose {
        println!("[PROMPT] Generated prompt:");
        println!("{}", prompt);
        println!("==============================");
    }
    
    let commit_message = ollama_manager.generate_commit(&prompt).await?;
    
    // In dry-run mode, just show the message without committing
    if args.dry_run {
        println!("\n[DRY RUN] Generated Commit Message (not committed):");
        println!("==============================");
        println!("{}", commit_message.trim());
        println!("==============================");
        println!("\nThis was a dry run. To actually commit, run without --dry-run");
        return Ok(());
    }

    // Display the generated commit message
    println!("\n[COMMIT] Generated Commit Message:");
    println!("==============================");
    println!("{}", commit_message.trim());
    println!("==============================");
    
    // Check if we're in an interactive terminal
    let is_interactive = atty::is(atty::Stream::Stdout);
    
    // Skip confirmation if not in an interactive terminal or if --no-confirm is set
    if !is_interactive || args.no_confirm {
        // Auto-confirm if not interactive
        println!("[AUTO] Auto-confirmed (non-interactive terminal or --no-confirm)");
        perform_commit(&commit_message, &current_dir).await?;
    } else {
        // Interactive confirmation
        use dialoguer::Confirm;
        
        if Confirm::new()
            .with_prompt("Commit these changes?")
            .default(true)
            .interact()?
        {
            perform_commit(&commit_message, &current_dir).await?;
        } else {
            println!("[CANCEL] Commit cancelled by user");
            return Ok(());
        }
    }
    println!("[DONE] Commit created successfully!");
    
    Ok(())
}

async fn is_git_repository(path: &PathBuf) -> Result<bool> {
    let output = tokio::process::Command::new("git")
        .args(&["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .await?;
    
    Ok(output.status.success())
}

async fn perform_commit(message: &str, repo_path: &PathBuf) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args(&["commit", "-m", message])
        .current_dir(repo_path)
        .output()
        .await?;
    
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(GitAiError::Git(format!("Git commit failed: {}", error)).into());
    }
    
    Ok(())
}
