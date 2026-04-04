use anyhow::Result;
use git_ai_commit::{
    cli::Args,
    commit::{commit_message_to_repo, sanitize_commit_message},
    formatting::PromptBuilder,
    git::GitCollector,
    llm::{LlmManager, LlmManagerOptions},
    utils::error::GitAiError,
};
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::load();

    let mut llm_manager = LlmManager::new(LlmManagerOptions {
        provider: args.provider.clone(),
        model: args.model.clone(),
        port: args.port,
        base_url: args.base_url.clone(),
        api_key: args.api_key.clone(),
    })?;

    // Handle --list-models flag
    if args.list_models {
        match llm_manager.list_models().await {
            Ok(listing) => {
                if listing.models.is_empty() {
                    println!(
                        "{}",
                        listing
                            .empty_hint
                            .unwrap_or_else(|| "No models found.".to_string())
                    );
                } else {
                    println!("Available models:");
                    for model in listing.models {
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

    if args.provider.trim() != "ollama" && args.model.trim().is_empty() {
        eprintln!(
            "Error: --model is required when using provider '{}'",
            args.provider
        );
        std::process::exit(1);
    }

    // Initialize components
    let git_collector = GitCollector::new(current_dir.clone());
    let prompt_builder = match &args.template {
        Some(template_path) => {
            PromptBuilder::from_template_file(template_path, args.max_files, args.max_diff_lines)?
        }
        None => PromptBuilder::with_user_context(
            args.max_files,
            args.max_diff_lines,
            args.context.clone(),
        ),
    };

    // Ensure the model is available
    println!("[CHECK] Checking if model '{}' is available...", args.model);
    llm_manager.ensure_model_available(&args.model).await?;

    // Collect initial git information
    println!("[ANALYZE] Analyzing git repository...");
    let mut git_info = git_collector.collect_all().await?;

    // If --add-unstaged flag is set, stage all unstaged changes and refresh git info
    let mut after_staging = false;
    if args.add_unstaged
        && (!git_info.status.modified_files.is_empty()
            || !git_info.status.untracked_files.is_empty()
            || !git_info.status.deleted_files.is_empty())
    {
        println!("[STAGE] Staging all unstaged changes...");
        git_collector.stage_all_unstaged().await?;

        // Refresh git info after staging
        println!("[REFRESH] Refreshing repository status...");
        git_info = git_collector.collect_all().await?;
        after_staging = true;

        if git_info.is_empty(true) {
            // true = after staging
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
    llm_manager.ensure_running().await?;

    // Generate commit message
    println!("[GENERATE] Generating commit message...");
    let prompt = prompt_builder.build(&git_info);

    if args.verbose {
        println!("[PROMPT] Generated prompt:");
        println!("{}", prompt);
        println!("==============================");
    }

    let commit_message = llm_manager.generate_commit(&prompt).await?;
    let sanitized_commit_message = sanitize_commit_message(&commit_message);

    // In dry-run mode, just show the message without committing
    if args.dry_run {
        println!("\n[DRY RUN] Generated Commit Message (not committed):");
        println!("==============================");
        println!("{}", sanitized_commit_message.trim());
        println!("==============================");
        println!("\nThis was a dry run. To actually commit, run without --dry-run");
        return Ok(());
    }

    // Display the generated commit message
    println!("\n[COMMIT] Generated Commit Message:");
    println!("==============================");
    println!("{}", sanitized_commit_message.trim());
    println!("==============================");

    // Check if we're in an interactive terminal
    let is_interactive = atty::is(atty::Stream::Stdout);

    // Skip confirmation if not in an interactive terminal or if --no-confirm is set
    if !is_interactive || args.no_confirm {
        // Auto-confirm if not interactive
        println!("[AUTO] Auto-confirmed (non-interactive terminal or --no-confirm)");
        commit_message_to_repo(&commit_message, &current_dir).await?;
    } else {
        // Interactive confirmation
        use dialoguer::Confirm;

        if Confirm::new()
            .with_prompt("Commit these changes?")
            .default(true)
            .interact()?
        {
            commit_message_to_repo(&commit_message, &current_dir).await?;
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
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .await?;

    Ok(output.status.success())
}
