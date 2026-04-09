use anyhow::Result;
use git_ai_commit::{
    cli::Args,
    commit::{commit_message_to_repo, sanitize_commit_message},
    formatting::PromptBuilder,
    git::{
        branch_diff_stat, current_branch, detect_default_branch, ensure_push_branch,
        preview_push_branch, push_current_branch, GitCollector,
    },
    llm::{LlmManager, LlmManagerOptions},
    pr::{create_pull_request_via_gh, PullRequestCreateResult, PullRequestDraft},
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
    if let Some(message) = llm_manager.readiness_status(&args.model).message {
        println!("{}", message);
    }
    llm_manager.ensure_model_available(&args.model).await?;

    // Collect initial git information
    if let Some(message) = llm_manager.analysis_status().message {
        println!("{}", message);
    }
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

    let branch_pr_context = if (args.push_pr || args.pr) && git_info.is_empty(after_staging) {
        let default_branch = detect_default_branch(&current_dir).await?;
        let branch_diff = branch_diff_stat(&current_dir, &default_branch).await?;
        if branch_diff.trim().is_empty() {
            None
        } else {
            Some((default_branch, branch_diff))
        }
    } else {
        None
    };

    if git_info.is_empty(after_staging) && branch_pr_context.is_none() {
        if args.push_pr {
            println!("[INFO] No branch changes to push or open as a pull request.");
            return Ok(());
        }

        println!("[INFO] No changes detected in the repository.");
        println!("Please make some changes and stage them before generating a commit message.");
        return Ok(());
    }

    if args.dry_run {
        println!("[DRY RUN] Dry run mode - will generate commit message but not commit");
        println!("[ANALYSIS] Git Repository Analysis:");
        println!("{}", git_info.display());
    }

    // Start the selected provider backend if needed
    if let Some(message) = llm_manager.startup_status().message {
        println!("{}", message);
    }
    llm_manager.ensure_running().await?;

    if args.push_pr {
        let mut pr_context_branch = current_branch(&current_dir).await?;

        if branch_pr_context.is_none() {
            let starting_branch = current_branch(&current_dir).await?;
            println!("[GENERATE] Generating commit message...");
            let commit_prompt = prompt_builder.build(&git_info);

            if args.verbose {
                println!("[PROMPT] Generated commit prompt:");
                println!("{}", commit_prompt);
                println!("==============================");
            }

            let (commit_output, sanitized_commit_message) =
                generate_sanitized_commit_message(&llm_manager, &commit_prompt).await?;

            if args.dry_run {
                let push_hint = args
                    .context
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(sanitized_commit_message.trim());
                pr_context_branch = preview_push_branch(&current_dir, push_hint).await?;

                println!("\n[DRY RUN] Push preview:");
                println!("==============================");
                if starting_branch == pr_context_branch {
                    println!("BRANCH ACTION: ready");
                } else {
                    println!("BRANCH ACTION: switched");
                }
                println!("BRANCH: {}", pr_context_branch);
                println!("REMOTE: origin");
                println!("==============================");

                println!("\n[DRY RUN] Generated Commit Message (not committed):");
                println!("==============================");
                println!("{}", sanitized_commit_message.trim());
                println!("==============================");
            } else {
                println!("\n[COMMIT] Generated Commit Message:");
                println!("==============================");
                println!("{}", sanitized_commit_message.trim());
                println!("==============================");

                let push_hint = args
                    .context
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(sanitized_commit_message.trim());
                let starting_branch = current_branch(&current_dir).await?;
                let target_branch = ensure_push_branch(&current_dir, push_hint).await?;

                let is_interactive = atty::is(atty::Stream::Stdout);

                if !is_interactive || args.no_confirm {
                    println!("[AUTO] Auto-confirmed (non-interactive terminal or --no-confirm)");
                    commit_message_to_repo(&commit_output, &current_dir).await?;
                } else {
                    use dialoguer::Confirm;

                    if Confirm::new()
                        .with_prompt("Commit these changes?")
                        .default(true)
                        .interact()?
                    {
                        commit_message_to_repo(&commit_output, &current_dir).await?;
                    } else {
                        println!("[CANCEL] Commit cancelled by user");
                        return Ok(());
                    }
                }
                println!("[DONE] Commit created successfully!");

                println!("[PUSH] Pushing current branch to origin...");
                let pushed_branch = push_current_branch(&current_dir).await?;
                pr_context_branch = pushed_branch.clone();
                println!("[PUSH] Pushed branch:");
                println!("==============================");
                if starting_branch == target_branch {
                    println!("BRANCH ACTION: ready");
                } else {
                    println!("BRANCH ACTION: switched");
                }
                println!("BRANCH: {}", pushed_branch);
                println!("REMOTE: origin");
                println!("==============================");
            }
        } else if !args.dry_run {
            println!("[PUSH] Pushing current branch to origin...");
            let pushed_branch = push_current_branch(&current_dir).await?;
            pr_context_branch = pushed_branch.clone();
            println!("[PUSH] Pushed branch:");
            println!("==============================");
            println!("BRANCH ACTION: ready");
            println!("BRANCH: {}", pushed_branch);
            println!("REMOTE: origin");
            println!("==============================");
        }

        let (default_branch, pr_prompt) =
            if let Some((default_branch, branch_diff)) = branch_pr_context.as_ref() {
                (
                    default_branch.clone(),
                    prompt_builder.build_pr_from_branch_diff(
                        default_branch,
                        branch_diff,
                        git_info.last_commit.as_deref(),
                    ),
                )
            } else {
                (
                    detect_default_branch(&current_dir).await?,
                    prompt_builder.build_pr(&git_info),
                )
            };

        println!("[GENERATE] Generating pull request draft...");

        if args.verbose {
            println!("[PROMPT] Generated PR prompt:");
            println!("{}", pr_prompt);
            println!("==============================");
        }

        let pr_draft = generate_pull_request_draft(&llm_manager, &pr_prompt).await?;

        if args.dry_run {
            println!("\n[DRY RUN] Generated Pull Request Draft:");
            println!("==============================");
            println!("BRANCH: {}", pr_context_branch);
            println!("BASE: {}", default_branch);
            println!("{}", pr_draft.render());
            println!("==============================");
            println!("\nThis was a dry run. To actually commit, push, and create the PR, run without --dry-run");
            return Ok(());
        }

        match create_pull_request_via_gh(&pr_draft, &default_branch, &current_dir) {
            Ok(PullRequestCreateResult::Created { url }) => {
                println!("\n[PULL REQUEST] Created pull request:");
                println!("==============================");
                println!("BRANCH: {}", pr_context_branch);
                println!("BASE: {}", default_branch);
                println!("TITLE: {}", pr_draft.title);
                if !url.trim().is_empty() {
                    println!("URL: {}", url.trim());
                }
                println!("==============================");
            }
            Ok(PullRequestCreateResult::Existing { url }) => {
                println!("\n[PULL REQUEST] Existing pull request:");
                println!("==============================");
                println!("BRANCH: {}", pr_context_branch);
                println!("BASE: {}", default_branch);
                println!("TITLE: {}", pr_draft.title);
                if !url.trim().is_empty() {
                    println!("URL: {}", url.trim());
                }
                println!("==============================");
            }
            Err(error) => {
                println!("\n[PULL REQUEST] Falling back to generated draft:");
                println!("==============================");
                println!("BRANCH: {}", pr_context_branch);
                println!("BASE: {}", default_branch);
                println!("{}", pr_draft.render());
                println!("==============================");
                println!("\n[INFO] gh pr create failed: {}", error);
            }
        }
        return Ok(());
    }

    // Generate commit message or PR draft
    if args.pr {
        println!("[GENERATE] Generating pull request draft...");
    } else {
        println!("[GENERATE] Generating commit message...");
    }
    let prompt = if args.pr {
        prompt_builder.build_pr(&git_info)
    } else {
        prompt_builder.build(&git_info)
    };

    if args.verbose {
        println!("[PROMPT] Generated prompt:");
        println!("{}", prompt);
        println!("==============================");
    }

    if args.pr {
        let (default_branch, pr_prompt) =
            if let Some((default_branch, branch_diff)) = branch_pr_context.as_ref() {
                (
                    default_branch.clone(),
                    prompt_builder.build_pr_from_branch_diff(
                        default_branch,
                        branch_diff,
                        git_info.last_commit.as_deref(),
                    ),
                )
            } else {
                (detect_default_branch(&current_dir).await?, prompt)
            };

        let pr_draft = generate_pull_request_draft(&llm_manager, &pr_prompt).await?;

        if args.dry_run {
            println!("\n[DRY RUN] Generated Pull Request Draft:");
            println!("==============================");
            println!("BASE: {}", default_branch);
            println!("{}", pr_draft.render());
            println!("==============================");
            println!("\nThis was a dry run. To actually create the PR, run the PR flow without --dry-run once PR creation is enabled.");
            return Ok(());
        }

        match create_pull_request_via_gh(&pr_draft, &default_branch, &current_dir) {
            Ok(PullRequestCreateResult::Created { url }) => {
                println!("\n[PULL REQUEST] Created pull request:");
                println!("==============================");
                println!("BASE: {}", default_branch);
                println!("TITLE: {}", pr_draft.title);
                if !url.trim().is_empty() {
                    println!("URL: {}", url.trim());
                }
                println!("==============================");
            }
            Ok(PullRequestCreateResult::Existing { url }) => {
                println!("\n[PULL REQUEST] Existing pull request:");
                println!("==============================");
                println!("BASE: {}", default_branch);
                println!("TITLE: {}", pr_draft.title);
                if !url.trim().is_empty() {
                    println!("URL: {}", url.trim());
                }
                println!("==============================");
            }
            Err(error) => {
                println!("\n[PULL REQUEST] Falling back to generated draft:");
                println!("==============================");
                println!("BASE: {}", default_branch);
                println!("{}", pr_draft.render());
                println!("==============================");
                println!("\n[INFO] gh pr create failed: {}", error);
            }
        }
        return Ok(());
    }

    let (model_output, sanitized_commit_message) =
        generate_sanitized_commit_message(&llm_manager, &prompt).await?;

    // In dry-run mode, just show the message without committing
    if args.dry_run {
        if args.push {
            let starting_branch = current_branch(&current_dir).await?;
            let push_hint = args
                .context
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(sanitized_commit_message.trim());
            let preview_branch = preview_push_branch(&current_dir, push_hint).await?;

            println!("\n[DRY RUN] Push preview:");
            println!("==============================");
            if starting_branch == preview_branch {
                println!("BRANCH ACTION: ready");
            } else {
                println!("BRANCH ACTION: switched");
            }
            println!("BRANCH: {}", preview_branch);
            println!("REMOTE: origin");
            println!("==============================");
        }

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

    let push_plan = if args.push {
        let push_hint = args
            .context
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(sanitized_commit_message.trim());
        let starting_branch = current_branch(&current_dir).await?;
        let target_branch = ensure_push_branch(&current_dir, push_hint).await?;
        Some((starting_branch, target_branch))
    } else {
        None
    };

    // Skip confirmation if not in an interactive terminal or if --no-confirm is set
    if !is_interactive || args.no_confirm {
        // Auto-confirm if not interactive
        println!("[AUTO] Auto-confirmed (non-interactive terminal or --no-confirm)");
        commit_message_to_repo(&model_output, &current_dir).await?;
    } else {
        // Interactive confirmation
        use dialoguer::Confirm;

        if Confirm::new()
            .with_prompt("Commit these changes?")
            .default(true)
            .interact()?
        {
            commit_message_to_repo(&model_output, &current_dir).await?;
        } else {
            println!("[CANCEL] Commit cancelled by user");
            return Ok(());
        }
    }
    println!("[DONE] Commit created successfully!");

    if let Some((starting_branch, target_branch)) = push_plan {
        println!("[PUSH] Pushing current branch to origin...");
        let pushed_branch = push_current_branch(&current_dir).await?;
        println!("[PUSH] Pushed branch:");
        println!("==============================");
        if starting_branch == target_branch {
            println!("BRANCH ACTION: ready");
        } else {
            println!("BRANCH ACTION: switched");
        }
        println!("BRANCH: {}", pushed_branch);
        println!("REMOTE: origin");
        println!("==============================");
    }

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

async fn generate_sanitized_commit_message(
    llm_manager: &LlmManager,
    prompt: &str,
) -> Result<(String, String)> {
    let first_output = llm_manager.generate_commit(prompt).await?;
    let first_sanitized = sanitize_commit_message(&first_output);
    if !first_sanitized.trim().is_empty() {
        return Ok((first_output, first_sanitized));
    }

    println!("[RETRY] Empty commit message after sanitization, retrying generation...");
    let second_output = llm_manager.generate_commit(prompt).await?;
    let second_sanitized = sanitize_commit_message(&second_output);
    Ok((second_output, second_sanitized))
}

async fn generate_pull_request_draft(
    llm_manager: &LlmManager,
    prompt: &str,
) -> Result<PullRequestDraft> {
    let first_output = llm_manager.generate_commit(prompt).await?;
    if let Ok(draft) = PullRequestDraft::parse_or_normalize(&first_output) {
        return Ok(draft);
    }

    println!("[RETRY] Invalid pull request draft response, retrying generation...");
    let second_output = llm_manager.generate_commit(prompt).await?;
    PullRequestDraft::parse_or_normalize(&second_output)
}
