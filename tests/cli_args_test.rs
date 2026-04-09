use git_ai_commit::cli::Args;
use std::path::PathBuf;

#[test]
fn test_default_args() {
    // Test with no arguments (all defaults)
    let args = Args::try_parse_from(["git-ai-commit"]).expect("Failed to parse args");

    assert_eq!(args.provider, "ollama");
    assert!(
        !args.model.trim().is_empty(),
        "default model should resolve to a usable model name"
    );
    assert!(args.base_url.is_none());
    assert!(args.api_key.is_none());
    assert_eq!(args.max_files, 10);
    assert_eq!(args.max_diff_lines, 50);
    assert!(
        args.no_confirm,
        "no_confirm should be true by default (no confirmation needed)"
    );
    assert!(args.template.is_none());
    assert!(args.context.is_none());
    assert!(!args.pr);
    assert!(!args.push);
    assert!(!args.push_pr);
    assert!(!args.dry_run);
    assert!(!args.verbose);
    assert_eq!(args.port, 11434);
    assert_eq!(args.timeout_seconds, 60);
    assert!(!args.add_unstaged);
}

#[test]
fn test_custom_model() {
    // Test custom model
    let args =
        Args::try_parse_from(["git-ai-commit", "--model", "llama3"]).expect("Failed to parse args");
    assert_eq!(args.model, "llama3");

    // Test short form
    let args =
        Args::try_parse_from(["git-ai-commit", "-m", "mistral"]).expect("Failed to parse args");
    assert_eq!(args.model, "mistral");
}

#[test]
fn test_provider_options() {
    let args = Args::try_parse_from(["git-ai-commit", "--provider", "openai-compatible"])
        .expect("Failed to parse args");
    assert_eq!(args.provider, "openai-compatible");

    let args = Args::try_parse_from([
        "git-ai-commit",
        "--base-url",
        "http://localhost:11434",
        "--api-key",
        "sk-test",
    ])
    .expect("Failed to parse args");
    assert_eq!(args.base_url, Some("http://localhost:11434".to_string()));
    assert_eq!(args.api_key, Some("sk-test".to_string()));
}

#[test]
fn test_openai_compatible_provider_requires_explicit_model() {
    let args = Args::try_parse_from(["git-ai-commit", "--provider", "openai-compatible"])
        .expect("Failed to parse args");

    assert_eq!(args.provider, "openai-compatible");
    assert!(
        args.model.is_empty(),
        "openai-compatible provider should not inherit Ollama default-model discovery"
    );
}

#[test]
fn test_diff_options() {
    // Test max files (long form)
    let args =
        Args::try_parse_from(["git-ai-commit", "--max-files", "20"]).expect("Failed to parse args");
    assert_eq!(args.max_files, 20);

    // Test max files (short form)
    let args = Args::try_parse_from(["git-ai-commit", "-f", "25"]).expect("Failed to parse args");
    assert_eq!(args.max_files, 25);

    // Test max diff lines (long form)
    let args = Args::try_parse_from(["git-ai-commit", "--max-diff-lines", "100"])
        .expect("Failed to parse args");
    assert_eq!(args.max_diff_lines, 100);

    // Test max diff lines (short form)
    let args = Args::try_parse_from(["git-ai-commit", "-l", "150"]).expect("Failed to parse args");
    assert_eq!(args.max_diff_lines, 150);
}

#[test]
fn test_commit_options() {
    // Test no_confirm flag (default is true, meaning it will NOT ask for confirmation)
    let args = Args::try_parse_from(["git-ai-commit"]).expect("Failed to parse args");
    assert!(args.no_confirm);

    // Test confirm flag set to false (disables confirmation)
    let args = Args::try_parse_from(["git-ai-commit", "--confirm"]).expect("Failed to parse args");
    assert!(!args.no_confirm);
}

#[test]
fn test_custom_template() {
    // Test template path
    let args = Args::try_parse_from(["git-ai-commit", "--template", "/path/to/template.txt"])
        .expect("Failed to parse args");
    assert_eq!(args.template, Some(PathBuf::from("/path/to/template.txt")));
}

#[test]
fn test_context_flag() {
    let args = Args::try_parse_from([
        "git-ai-commit",
        "--context",
        "focus on cleanup before release",
    ])
    .expect("Failed to parse args");

    assert_eq!(
        args.context,
        Some("focus on cleanup before release".to_string())
    );
}

#[test]
fn test_pr_flag() {
    let args = Args::try_parse_from(["git-ai-commit", "--pr"]).expect("Failed to parse args");
    assert!(args.pr);
}

#[test]
fn test_push_flag() {
    let args = Args::try_parse_from(["git-ai-commit", "--push"]).expect("Failed to parse args");
    assert!(args.push);
}

#[test]
fn test_push_pr_flag() {
    let args = Args::try_parse_from(["git-ai-commit", "--push-pr"]).expect("Failed to parse args");
    assert!(args.push_pr);
}

#[test]
fn test_debug_options() {
    // Test dry run (long form)
    let args = Args::try_parse_from(["git-ai-commit", "--dry-run"]).expect("Failed to parse args");
    assert!(args.dry_run);

    // Test dry run (short form)
    let args = Args::try_parse_from(["git-ai-commit", "-d"]).expect("Failed to parse args");
    assert!(args.dry_run);

    // Test verbose (long form)
    let args = Args::try_parse_from(["git-ai-commit", "--verbose"]).expect("Failed to parse args");
    assert!(args.verbose);

    // Test verbose (short form)
    let args = Args::try_parse_from(["git-ai-commit", "-v"]).expect("Failed to parse args");
    assert!(args.verbose);
}

#[test]
fn test_advanced_options() {
    // Test custom port (long form)
    let args =
        Args::try_parse_from(["git-ai-commit", "--port", "12345"]).expect("Failed to parse args");
    assert_eq!(args.port, 12345);

    // Test custom port (short form)
    let args =
        Args::try_parse_from(["git-ai-commit", "-p", "54321"]).expect("Failed to parse args");
    assert_eq!(args.port, 54321);

    // Test timeout (long form)
    let args = Args::try_parse_from(["git-ai-commit", "--timeout-seconds", "120"])
        .expect("Failed to parse args");
    assert_eq!(args.timeout_seconds, 120);

    // Test timeout (short form)
    let args = Args::try_parse_from(["git-ai-commit", "-t", "180"]).expect("Failed to parse args");
    assert_eq!(args.timeout_seconds, 180);
}

#[test]
fn test_staging_options() {
    // Test add unstaged
    let args =
        Args::try_parse_from(["git-ai-commit", "--add-unstaged"]).expect("Failed to parse args");
    assert!(args.add_unstaged);
}

#[test]
fn test_combined_options() {
    // Test multiple options together
    let args = Args::try_parse_from([
        "git-ai-commit",
        "--model",
        "llama3",
        "--provider",
        "ollama",
        "--base-url",
        "http://localhost:11434",
        "--api-key",
        "unused-local-key",
        "--max-files",
        "15",
        "--max-diff-lines",
        "75",
        "--confirm", // This should set no_confirm to false
        "--template",
        "custom.tpl",
        "--context",
        "ship blocker fix",
        "--pr",
        "--push",
        "--push-pr",
        "--dry-run",
        "--verbose",
        "--port",
        "12345",
        "--timeout-seconds",
        "120",
        "--add-unstaged",
    ])
    .expect("Failed to parse args");

    assert_eq!(args.provider, "ollama");
    assert_eq!(args.model, "llama3");
    assert_eq!(args.base_url, Some("http://localhost:11434".to_string()));
    assert_eq!(args.api_key, Some("unused-local-key".to_string()));
    assert_eq!(args.max_files, 15);
    assert_eq!(args.max_diff_lines, 75);
    assert!(!args.no_confirm, "--confirm should set no_confirm to false");
    assert_eq!(args.template, Some(PathBuf::from("custom.tpl")));
    assert_eq!(args.context, Some("ship blocker fix".to_string()));
    assert!(args.pr);
    assert!(args.push);
    assert!(args.push_pr);
    assert!(args.dry_run);
    assert!(args.verbose);
    assert_eq!(args.port, 12345);
    assert_eq!(args.timeout_seconds, 120);
    assert!(args.add_unstaged);
}

#[test]
fn test_list_models_flag() {
    // Test --list-models flag
    let args =
        Args::try_parse_from(["git-ai-commit", "--list-models"]).expect("Failed to parse args");
    assert!(args.list_models);

    // Test that list_models is false by default
    let args = Args::try_parse_from(["git-ai-commit"]).expect("Failed to parse args");
    assert!(!args.list_models);
}
