use git_ai_commit::formatting::prompt::PromptBuilder;
use git_ai_commit::git::diff::FileStat;
use git_ai_commit::git::files::ChangeType;
use git_ai_commit::git::{detect_default_branch, DiffInfo, FileChange, GitCollector, GitStatus};
use git_ai_commit::ollama::{OllamaClient, OllamaClientTrait};
use git_ai_commit::OllamaManager;
use serial_test::serial;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

// For now, we'll skip the mock testing and focus on the actual git operations
// as setting up proper async mocks is complex and not necessary for our current needs

async fn ensure_shared_test_ollama(model: &str) -> Option<OllamaManager> {
    let client = OllamaClient::new(11434);
    if client.is_running().await {
        if !client
            .has_model(model)
            .await
            .expect("should check shared test model availability")
        {
            client
                .pull_model(model)
                .await
                .expect("should pull shared test model");
        }
        return None;
    }

    let mut manager = OllamaManager::new(model.to_string(), 11434).expect("manager should build");
    manager
        .ensure_running()
        .await
        .expect("real Ollama integration test needs a shared local server");
    manager
        .ensure_model_available(model)
        .await
        .expect("real Ollama integration test needs the requested model");
    Some(manager)
}

struct DisposableGithubRepo {
    full_name: String,
}

impl DisposableGithubRepo {
    fn create(prefix: &str) -> Self {
        let owner = github_login();
        let repo_name = format!("{prefix}-{}-{}", std::process::id(), unix_timestamp());
        let full_name = format!("{owner}/{repo_name}");

        let output = Command::new("gh")
            .args([
                "repo",
                "create",
                &full_name,
                "--private",
                "--add-readme",
                "--disable-issues",
                "--disable-wiki",
            ])
            .output()
            .expect("Failed to create disposable GitHub repo");

        assert!(
            output.status.success(),
            "expected gh repo create to succeed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        Self { full_name }
    }
}

impl Drop for DisposableGithubRepo {
    fn drop(&mut self) {
        let _ = Command::new("gh")
            .args(["repo", "delete", &self.full_name, "--yes"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn github_login() -> String {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .expect("Failed to read GitHub login");

    assert!(
        output.status.success(),
        "expected gh api user to succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn unix_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis()
}

fn parse_json_url(raw: &str) -> Option<String> {
    let marker = "\"url\":";
    let start = raw.find(marker)? + marker.len();
    let rest = raw[start..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_output_field(raw: &str, prefix: &str) -> Option<String> {
    raw.lines()
        .find_map(|line| line.trim().strip_prefix(prefix).map(str::trim))
        .map(str::to_string)
}

fn init_named_repo(name: &str) -> tempfile::TempDir {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user name");

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user email");

    std::fs::write(
        repo_path.join("src.txt"),
        format!("base\n{name}\ninitial line\n"),
    )
    .expect("Failed to create test file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create initial commit");

    std::fs::write(
        repo_path.join("src.txt"),
        format!("base\n{name}\nupdated implementation line\nextra detail\n"),
    )
    .expect("Failed to modify test file");

    Command::new("git")
        .args(["add", "src.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage modified file");

    temp_dir
}

fn run_real_ollama_dry_run(repo_path: &std::path::Path, model: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_git-ai-commit"))
        .args(["--provider", "ollama", "--model", model, "--dry-run"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit against real Ollama")
}

#[tokio::test]
async fn test_interactive_stage_unstaged() {
    // Create a temporary directory for the test repository
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    // Initialize a new git repository
    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    // Create and commit initial files
    let file1 = repo_path.join("file1.txt");
    let file2 = repo_path.join("file2.txt");

    // Create initial content
    std::fs::write(&file1, "content1").expect("Failed to create file1");
    std::fs::write(&file2, "content2").expect("Failed to create file2");

    // Add and commit both files
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit files");

    // Modify both files
    std::fs::write(&file1, "modified content1").expect("Failed to modify file1");
    std::fs::write(&file2, "modified content2").expect("Failed to modify file2");

    // Stage only file1
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage file1");

    // Create a GitCollector for testing
    let git_collector = GitCollector::new(repo_path.to_path_buf());

    // Test that we detect both staged and unstaged files
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect status");

    // Debug output
    println!("Staged files: {:?}", git_info.status.staged_files);
    println!("Modified files: {:?}", git_info.status.modified_files);

    assert!(
        git_info
            .status
            .staged_files
            .contains(&PathBuf::from("file1.txt")),
        "file1.txt should be staged"
    );
    assert!(
        git_info
            .status
            .modified_files
            .contains(&PathBuf::from("file2.txt")),
        "file2.txt should be modified but unstaged"
    );

    // Clean up
    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[ignore = "requires a real local Ollama daemon and large local models"]
#[serial(ollama)]
async fn test_ollama_gemma4_and_qwen3_coder_generate_commit_messages() {
    let _ollama_manager = ensure_shared_test_ollama("gemma4:latest").await;
    let client = OllamaClient::new(11434);
    if !client
        .has_model("qwen3-coder:latest")
        .await
        .expect("should check qwen3-coder availability")
    {
        client
            .pull_model("qwen3-coder:latest")
            .await
            .expect("should pull qwen3-coder for the real integration test");
    }

    for model in ["gemma4:latest", "qwen3-coder:latest"] {
        let temp_dir = init_named_repo(model);
        let output = run_real_ollama_dry_run(temp_dir.path(), model);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "real ollama dry-run should succeed for model {model}\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stdout.contains("[DRY RUN] Generated Commit Message (not committed):"),
            "expected generated commit message heading for model {model}, got stdout: {stdout}"
        );
        assert!(
            stdout.lines().any(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty()
                    && !trimmed.starts_with('[')
                    && !trimmed.starts_with('=')
                    && !trimmed.starts_with("AI Commit Message Generator")
            }),
            "expected non-empty commit message content for model {model}, got stdout: {stdout}"
        );
    }
}

#[tokio::test]
async fn test_interactive_stage_and_regenerate() {
    // Setup test repository
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    // Initialize git
    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    // Create and commit initial files
    let file1 = repo_path.join("file1.rs");
    let file2 = repo_path.join("file2.rs");

    // Create initial content
    std::fs::write(&file1, "fn main() {}").expect("Failed to create file1");
    std::fs::write(&file2, "fn helper() {}").expect("Failed to create file2");

    // Add and commit both files
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit files");

    // Modify both files
    std::fs::write(&file1, "fn main() { println!('Hello'); }").expect("Failed to modify file1");
    std::fs::write(&file2, "fn helper() { println!('Helper'); }").expect("Failed to modify file2");

    // Stage only file1
    Command::new("git")
        .args(["add", "file1.rs"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage file1");

    // In a real implementation, we would test the interactive staging here
    // For now, we'll just verify that we can detect both staged and unstaged changes
    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect status");

    assert!(git_info
        .status
        .staged_files
        .contains(&PathBuf::from("file1.rs")));
    assert!(git_info
        .status
        .modified_files
        .contains(&PathBuf::from("file2.rs")));

    // Clean up
    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_prompt_prefers_staged_changes_only() {
    // Create test data with both staged and unstaged changes
    let git_info = git_ai_commit::git::GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("src/main.rs")],
            modified_files: vec![PathBuf::from("Cargo.toml")],
            untracked_files: vec![],
            deleted_files: vec![],
            unmerged_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 2,
            insertions: 15,
            deletions: 3,
            file_stats: vec![
                FileStat {
                    filename: "src/main.rs".to_string(),
                    insertions: 10,
                    deletions: 2,
                },
                FileStat {
                    filename: "Cargo.toml".to_string(),
                    insertions: 5,
                    deletions: 1,
                },
            ],
        },
        file_changes: vec![
            FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("src/main.rs"),
                old_path: None,
            },
            FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("Cargo.toml"),
                old_path: None,
            },
        ],
        untracked_files: vec![],
        branch_name: "feature/test".to_string(),
        last_commit: Some("Initial commit".to_string()),
    };

    let builder = PromptBuilder::new(10, 100);
    let prompt = builder.build(&git_info);

    // Verify the prompt uses only staged commit context
    assert!(prompt.contains("Generate a git commit message in plain text Lore format only."));
    assert!(prompt.contains("src/main.rs"));
    assert!(
        prompt.contains("Staged diff summary: 1 files changed, 10 insertions(+), 2 deletions(-)")
    );
    assert!(!prompt.contains("Unstaged changes (will NOT be committed):"));
    assert!(!prompt.contains("Cargo.toml"));
}

#[tokio::test]
async fn test_collect_all_reports_staged_rename_with_old_and_new_paths() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let old_file = repo_path.join("old_name.txt");
    std::fs::write(&old_file, "content").expect("Failed to create original file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit initial file");

    let new_file = repo_path.join("new_name.txt");
    std::fs::rename(&old_file, &new_file).expect("Failed to rename file");

    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage rename");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    assert!(
        git_info
            .status
            .staged_files
            .contains(&PathBuf::from("new_name.txt")),
        "staged files should contain the renamed destination path"
    );

    let rename = git_info
        .file_changes
        .iter()
        .find(|change| change.file_path == std::path::Path::new("new_name.txt"))
        .expect("expected staged rename in file changes");

    assert!(matches!(rename.change_type, ChangeType::Renamed));
    assert_eq!(
        rename.old_path.as_deref(),
        Some(std::path::Path::new("old_name.txt"))
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_stage_all_unstaged_stages_modified_deleted_and_untracked_files() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let tracked_file = repo_path.join("tracked.txt");
    let deleted_file = repo_path.join("deleted.txt");
    let ignored_file = repo_path.join("ignored.log");
    let gitignore = repo_path.join(".gitignore");

    std::fs::write(&tracked_file, "original tracked").expect("Failed to create tracked file");
    std::fs::write(&deleted_file, "original deleted").expect("Failed to create deleted file");
    std::fs::write(&gitignore, "*.log\n").expect("Failed to create .gitignore");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit initial files");

    std::fs::write(&tracked_file, "updated tracked").expect("Failed to modify tracked file");
    std::fs::remove_file(&deleted_file).expect("Failed to delete tracked file");
    std::fs::write(repo_path.join("new_file.txt"), "brand new").expect("Failed to add new file");
    std::fs::write(&ignored_file, "ignore me").expect("Failed to add ignored file");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    git_collector
        .stage_all_unstaged()
        .await
        .expect("Failed to stage all unstaged changes");

    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info after staging");

    assert!(
        git_info
            .status
            .staged_files
            .contains(&PathBuf::from("tracked.txt")),
        "modified tracked file should be staged"
    );
    assert!(
        git_info
            .status
            .staged_files
            .contains(&PathBuf::from("deleted.txt")),
        "deleted tracked file should be staged"
    );
    assert!(
        git_info
            .status
            .staged_files
            .contains(&PathBuf::from("new_file.txt")),
        "untracked file should be staged"
    );
    assert!(
        !git_info
            .status
            .staged_files
            .contains(&PathBuf::from("ignored.log")),
        "ignored files should not be staged"
    );
    assert!(
        !git_info
            .untracked_files
            .contains(&PathBuf::from("ignored.log")),
        "ignored files should stay excluded from untracked reporting"
    );
    assert!(
        git_info.status.modified_files.is_empty(),
        "no modified files should remain unstaged"
    );
    assert!(
        git_info.untracked_files.is_empty(),
        "no untracked files should remain except ignored files"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_collect_all_treats_merge_conflicts_as_non_empty_changes() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let shared_file = repo_path.join("conflict.txt");
    std::fs::write(&shared_file, "base\n").expect("Failed to create base file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add base file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit base file");

    Command::new("git")
        .args(["checkout", "-b", "feature/conflict"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create feature branch");

    std::fs::write(&shared_file, "feature change\n").expect("Failed to update feature branch file");
    Command::new("git")
        .args(["commit", "-am", "Feature change"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit feature branch change");

    Command::new("git")
        .args(["checkout", "master"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to checkout master");

    std::fs::write(&shared_file, "main change\n").expect("Failed to update master branch file");
    Command::new("git")
        .args(["commit", "-am", "Main change"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit master branch change");

    let merge_status = Command::new("git")
        .args(["merge", "feature/conflict"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to run merge");
    assert!(!merge_status.success(), "merge should produce a conflict");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info for conflicted repo");

    assert!(
        !git_info.is_empty(false),
        "merge conflicts should count as repository changes"
    );
    assert!(
        git_info
            .file_changes
            .iter()
            .any(|change| matches!(change.change_type, ChangeType::Unmerged)),
        "merge conflicts should be reported as unmerged file changes"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_collect_all_deduplicates_diff_stats_for_staged_and_unstaged_same_file() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let tracked_file = repo_path.join("tracked.txt");
    std::fs::write(&tracked_file, "base\n").expect("Failed to create tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add base file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit base file");

    std::fs::write(&tracked_file, "staged change\n").expect("Failed to write staged version");
    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    std::fs::write(&tracked_file, "staged change\nunstaged change\n")
        .expect("Failed to write unstaged follow-up change");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    assert!(
        git_info
            .status
            .staged_files
            .contains(&PathBuf::from("tracked.txt")),
        "tracked.txt should still be staged"
    );
    assert!(
        git_info
            .status
            .modified_files
            .contains(&PathBuf::from("tracked.txt")),
        "tracked.txt should also be modified in the working tree"
    );
    assert_eq!(
        git_info.diff_stat.files_changed, 1,
        "diff stats should count the file once even when it has both staged and unstaged edits"
    );
    assert_eq!(
        git_info.diff_stat.file_stats.len(),
        1,
        "diff stats should contain one aggregated entry for the file"
    );
    assert_eq!(
        git_info.diff_stat.file_stats[0].filename, "tracked.txt",
        "aggregated diff stats should still point at the changed file"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_git_info_display_lists_untracked_files_once() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to create tracked file");
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(repo_path.join("new_file.txt"), "new\n")
        .expect("Failed to create untracked file");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    let display = git_info.display();
    let untracked_header_count = display.matches("Untracked files").count();
    let untracked_path_count = display.matches("new_file.txt").count();

    assert_eq!(
        untracked_header_count, 1,
        "git info display should only render one untracked files section"
    );
    assert_eq!(
        untracked_path_count, 1,
        "git info display should only list the untracked file once"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_add_unstaged_flag_stages_deleted_files_in_main_flow() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let deleted_file = repo_path.join("deleted.txt");
    std::fs::write(&deleted_file, "tracked\n").expect("Failed to create tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::remove_file(&deleted_file).expect("Failed to delete tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let run_status = Command::new(binary)
        .args([
            "--add-unstaged",
            "--dry-run",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .status()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        run_status.success(),
        "git-ai-commit binary should succeed for deleted-only dry-run analysis"
    );

    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to read repository status after running git-ai-commit");
    assert!(
        status_output.status.success(),
        "git status should succeed after running git-ai-commit"
    );

    let porcelain = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        porcelain.contains("D  deleted.txt"),
        "--add-unstaged should stage deleted files in the main flow"
    );
    assert!(
        !porcelain.contains(" D deleted.txt"),
        "deleted file should not remain only unstaged after --add-unstaged"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_collect_all_reports_staged_copy_with_old_and_new_paths() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let original_file = repo_path.join("original.txt");
    std::fs::write(&original_file, "copy me\n").expect("Failed to create original file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit initial file");

    let copied_file = repo_path.join("copied.txt");
    std::fs::copy(&original_file, &copied_file).expect("Failed to copy file");

    Command::new("git")
        .args(["add", "copied.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage copied file");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    let copied_change = git_info
        .file_changes
        .iter()
        .find(|change| change.file_path == std::path::Path::new("copied.txt"))
        .expect("expected staged copy in file changes");

    assert!(
        matches!(copied_change.change_type, ChangeType::Copied),
        "staged copies should be reported as copied changes"
    );
    assert_eq!(
        copied_change.old_path.as_deref(),
        Some(std::path::Path::new("original.txt")),
        "copied changes should preserve the original source path"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_context_flag_is_included_in_default_prompt_via_real_cli_flow() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(repo_path.join("tracked.txt"), "base\ncontextful change\n")
        .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--dry-run",
            "--verbose",
            "--context",
            "focus on cleanup before release",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "git-ai-commit should accept --context and succeed in dry-run mode"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Recent conversation context:\nfocus on cleanup before release"),
        "verbose prompt output should include the explicit user context lane"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_openai_compatible_provider_generates_commit_message_via_local_ollama() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(
        repo_path.join("tracked.txt"),
        "base\nopenai-compatible change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "openai-compatible provider should succeed in dry-run mode against local Ollama\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[DRY RUN] Generated Commit Message"),
        "dry-run output should include a generated commit message section"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_openai_compatible_provider_fails_early_when_model_is_missing() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(
        repo_path.join("tracked.txt"),
        "base\nmissing model change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "definitely-not-installed-provider-readiness-test:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        !output.status.success(),
        "openai-compatible provider should fail fast when the selected model is missing"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "Model 'definitely-not-installed-provider-readiness-test:latest' is not available for provider 'openai-compatible'"
        ),
        "expected provider-aware readiness failure, got stderr: {stderr}"
    );
    assert!(
        !stderr.contains("generation endpoint returned status"),
        "missing-model failure should happen before generation, got stderr: {stderr}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_openai_compatible_provider_does_not_print_ollama_startup_banner() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(
        repo_path.join("tracked.txt"),
        "base\nprovider banner change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "openai-compatible provider should succeed in dry-run mode\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[START] Starting Ollama..."),
        "openai-compatible provider should not emit an Ollama-specific startup banner\nstdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_openai_compatible_provider_does_not_print_ollama_model_check_banner() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(
        repo_path.join("tracked.txt"),
        "base\nprovider check banner change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "openai-compatible provider should succeed in dry-run mode\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[CHECK] Checking if model 'tinyllama:latest' is available..."),
        "openai-compatible provider should not emit an Ollama-shaped model-check banner\nstdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_openai_compatible_provider_does_not_print_analysis_banner() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(
        repo_path.join("tracked.txt"),
        "base\nprovider analysis banner change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "openai-compatible provider should succeed in dry-run mode\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[ANALYZE] Analyzing git repository..."),
        "openai-compatible provider should not emit an Ollama-shaped analysis banner\nstdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_conflicted_repo_is_not_empty_after_staging_check() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let conflicted_file = repo_path.join("conflict-after-staging.txt");
    std::fs::write(&conflicted_file, "base\n").expect("Failed to write initial file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create initial commit");

    Command::new("git")
        .args(["checkout", "-b", "feature/conflict-after-staging"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create feature branch");

    std::fs::write(&conflicted_file, "feature branch change\n")
        .expect("Failed to update file on feature branch");

    Command::new("git")
        .args(["commit", "-am", "Feature branch change"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit feature branch change");

    Command::new("git")
        .args(["checkout", "master"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to switch back to master");

    std::fs::write(&conflicted_file, "main branch change\n")
        .expect("Failed to update file on main branch");

    Command::new("git")
        .args(["commit", "-am", "Main branch change"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit main branch change");

    let merge_status = Command::new("git")
        .args(["merge", "feature/conflict-after-staging"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to merge feature branch");
    assert!(
        !merge_status.success(),
        "merge should create a conflict for this test"
    );

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    assert!(
        !git_info.is_empty(true),
        "conflicted repositories must not be treated as empty after the after-staging check"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_prompt_uses_staged_diff_summary_and_excludes_unstaged_details() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let staged_file = repo_path.join("staged.rs");
    let unstaged_file = repo_path.join("unstaged.rs");
    std::fs::write(&staged_file, "fn staged() {}\n").expect("Failed to write staged file");
    std::fs::write(&unstaged_file, "fn unstaged() {}\n").expect("Failed to write unstaged file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create initial commit");

    std::fs::write(&staged_file, "fn staged() { println!(\"staged\"); }\n")
        .expect("Failed to modify staged file");
    std::fs::write(
        &unstaged_file,
        "fn unstaged() { println!(\"unstaged\"); }\n",
    )
    .expect("Failed to modify unstaged file");

    Command::new("git")
        .args(["add", "staged.rs"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage staged.rs");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    let prompt = PromptBuilder::new(10, 100).build(&git_info);

    assert!(
        prompt.contains("Generate a git commit message in plain text Lore format only."),
        "prompt should use the tighter crabclaw-style commit contract"
    );
    assert!(
        prompt.contains("staged.rs"),
        "prompt should include staged diff context"
    );
    assert!(
        !prompt.contains("unstaged.rs"),
        "prompt should not include unstaged file details in the commit-generation context"
    );
    assert!(
        !prompt.contains("Unstaged changes (will NOT be committed):"),
        "prompt should avoid presenting unstaged details as part of the commit prompt"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_prompt_marks_truncated_staged_context_when_limits_are_exceeded() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    for name in ["one.rs", "two.rs", "three.rs"] {
        std::fs::write(
            repo_path.join(name),
            format!("fn {}() {{}}\n", name.replace('.', "_")),
        )
        .expect("Failed to write initial file");
    }

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create initial commit");

    for name in ["one.rs", "two.rs", "three.rs"] {
        std::fs::write(
            repo_path.join(name),
            format!(
                "fn {}() {{ println!(\"{}\"); }}\n",
                name.replace('.', "_"),
                name
            ),
        )
        .expect("Failed to modify staged file");
    }

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage changed files");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    let prompt = PromptBuilder::new(10, 25).build(&git_info);

    assert!(
        prompt.contains("…[truncated]"),
        "prompt should explicitly mark when staged commit context is truncated"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_prompt_truncates_single_oversized_staged_entry_by_character_budget() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    let long_filename = format!("{}-oversized.rs", "a".repeat(180));
    let long_file_path = repo_path.join(&long_filename);

    std::fs::write(&long_file_path, "fn initial() {}\n").expect("Failed to write initial file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add initial file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to create initial commit");

    std::fs::write(
        &long_file_path,
        "fn changed() {\n    println!(\"oversized\");\n}\n",
    )
    .expect("Failed to modify oversized staged file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage oversized file");

    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector
        .collect_all()
        .await
        .expect("Failed to collect git info");

    let prompt = PromptBuilder::new(10, 30).build(&git_info);

    assert!(
        prompt.contains("…[truncated]"),
        "prompt should use deterministic character-budget truncation for oversized staged entries"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_pr_dry_run_generates_pr_title_and_body_from_repo_context() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(repo_path.join("tracked.txt"), "base\npr dry run change\n")
        .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--pr",
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "focus on the user-facing behavior change",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "PR dry-run should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[DRY RUN] Generated Pull Request Draft"),
        "expected PR dry-run output heading, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("TITLE:"),
        "expected generated PR title marker, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BODY:"),
        "expected generated PR body marker, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_pr_generation_falls_back_to_draft_when_gh_create_fails() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to initialize git repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to configure git user.email");

    std::fs::write(repo_path.join("tracked.txt"), "base\n").expect("Failed to write tracked file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .status()
        .expect("Failed to add tracked file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to commit tracked file");

    std::fs::write(
        repo_path.join("tracked.txt"),
        "base\npr create fallback change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(repo_path)
        .status()
        .expect("Failed to stage tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "focus on the user-facing behavior change",
        ])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "PR generation should fall back to a draft when gh creation fails\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[PULL REQUEST] Falling back to generated draft"),
        "expected gh failure fallback heading, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("TITLE:"),
        "expected generated PR title marker, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BODY:"),
        "expected generated PR body marker, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
#[ignore = "requires live GitHub repo access"]
async fn test_push_pr_with_real_github_repo_creates_then_reuses_pull_request() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let repo = DisposableGithubRepo::create("git-ai-commit-pr-flow");
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let clone_repo = temp_dir.path().join("clone");

    let clone_output = Command::new("gh")
        .args([
            "repo",
            "clone",
            &repo.full_name,
            clone_repo.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("Failed to clone disposable GitHub repo");

    assert!(
        clone_output.status.success(),
        "expected gh repo clone to succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&clone_output.stdout),
        String::from_utf8_lossy(&clone_output.stderr)
    );

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    let readme_path = clone_repo.join("README.md");
    let readme = std::fs::read_to_string(&readme_path).expect("Failed to read README");
    std::fs::write(
        &readme_path,
        format!("{readme}\nReal GitHub integration test change\n"),
    )
    .expect("Failed to update README");

    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage README change");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let create_output = Command::new(binary)
        .args([
            "--push-pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "github integration lifecycle",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit create flow");

    assert!(
        create_output.status.success(),
        "expected real GitHub create flow to succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&create_output.stdout),
        String::from_utf8_lossy(&create_output.stderr)
    );

    let create_stdout = String::from_utf8_lossy(&create_output.stdout);
    assert!(
        create_stdout.contains("[PULL REQUEST] Created pull request:"),
        "expected real GitHub flow to create a PR, got stdout: {create_stdout}"
    );

    let created_url = parse_output_field(&create_stdout, "URL:")
        .expect("expected created pull request URL in command output");
    assert!(
        created_url.contains(&repo.full_name) && created_url.contains("/pull/"),
        "expected created URL to point at the disposable repo PR, got: {created_url}"
    );

    let view_output = Command::new("gh")
        .args(["pr", "view", "--json", "url"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to view created pull request");

    assert!(
        view_output.status.success(),
        "expected gh pr view to succeed after PR creation\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&view_output.stdout),
        String::from_utf8_lossy(&view_output.stderr)
    );

    let live_url = parse_json_url(&String::from_utf8_lossy(&view_output.stdout))
        .expect("expected URL in gh pr view json");
    assert_eq!(
        live_url, created_url,
        "expected created PR URL to match gh pr view URL"
    );

    let existing_output = Command::new(binary)
        .args([
            "--pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "github integration lifecycle",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit existing PR flow");

    assert!(
        existing_output.status.success(),
        "expected real GitHub existing PR flow to succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&existing_output.stdout),
        String::from_utf8_lossy(&existing_output.stderr)
    );

    let existing_stdout = String::from_utf8_lossy(&existing_output.stdout);
    assert!(
        existing_stdout.contains("[PULL REQUEST] Existing pull request:"),
        "expected second PR flow to surface existing PR behavior, got stdout: {existing_stdout}"
    );
    assert!(
        existing_stdout.contains(&live_url),
        "expected second PR flow to print the existing PR URL, got stdout: {existing_stdout}"
    );
}

#[tokio::test]
#[serial(ollama)]
async fn test_pr_dry_run_with_clean_worktree_uses_existing_branch_commits() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    Command::new("git")
        .args(["switch", "-c", "feature/existing-branch-pr-dry-run"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to create feature branch in clone");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\nexisting branch pr dry run change\n",
    )
    .expect("Failed to modify tracked file in clone");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage tracked file in clone");

    Command::new("git")
        .args(["commit", "-m", "feat: prepare existing branch pr dry run"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to commit feature branch change");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--pr",
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "ready for review",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "plain pr dry-run should succeed on a clean branch that is ahead of default\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[DRY RUN] Generated Pull Request Draft"),
        "expected plain pr dry-run to print the PR draft heading, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BASE: main"),
        "expected plain pr dry-run to use the detected default branch as the PR base, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("TITLE:"),
        "expected plain pr dry-run to print a PR title, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BODY:"),
        "expected plain pr dry-run to print a PR body, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_detect_default_branch_prefers_origin_head_from_real_remote() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["switch", "-c", "feature/default-branch-test"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to create feature branch in clone");

    let detected = detect_default_branch(&clone_repo)
        .await
        .expect("Failed to detect default branch");

    assert_eq!(
        detected, "main",
        "default branch detection should prefer origin/HEAD from a real remote clone"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_pr_fallback_surfaces_detected_default_base_branch() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    Command::new("git")
        .args(["switch", "-c", "feature/pr-base-test"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to create feature branch in clone");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\npr base branch fallback change\n",
    )
    .expect("Failed to modify tracked file in clone");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage clone tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "focus on the base branch selection",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "PR generation should surface detected base branch on fallback\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[PULL REQUEST] Falling back to generated draft"),
        "expected gh failure fallback heading, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BASE: main"),
        "expected detected default branch to be surfaced in PR output, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_flag_pushes_committed_change_to_real_remote_feature_branch() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    let branch_name = "feature/push-flow-test";
    Command::new("git")
        .args(["switch", "-c", branch_name])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to create feature branch in clone");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\npush flow change from integration test\n",
    )
    .expect("Failed to modify tracked file in clone");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage clone tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "focus on the release push flow",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "commit+push flow should succeed on a real feature branch\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let local_head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read local HEAD");
    assert!(
        local_head.status.success(),
        "expected local HEAD lookup to succeed"
    );
    let local_head = String::from_utf8_lossy(&local_head.stdout)
        .trim()
        .to_string();
    assert!(
        !local_head.is_empty(),
        "expected local branch to have a commit after running git-ai-commit"
    );

    let remote_head = Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{branch_name}")])
        .current_dir(&bare_remote)
        .output()
        .expect("Failed to read remote feature branch");
    assert!(
        remote_head.status.success(),
        "expected push to create remote feature branch\nstderr: {}",
        String::from_utf8_lossy(&remote_head.stderr)
    );
    let remote_head = String::from_utf8_lossy(&remote_head.stdout)
        .trim()
        .to_string();
    assert_eq!(
        remote_head, local_head,
        "expected pushed remote branch HEAD to match local HEAD"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_flag_from_default_branch_creates_and_pushes_slugified_feature_branch() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    let starting_branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read starting branch");
    assert_eq!(
        String::from_utf8_lossy(&starting_branch.stdout).trim(),
        "main",
        "clone should start on the default branch for this test"
    );

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\npush flow change from default branch\n",
    )
    .expect("Failed to modify tracked file in clone");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage clone tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "ready for review",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "commit+push flow from default branch should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let branch_name_output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read resulting branch");
    assert!(
        branch_name_output.status.success(),
        "expected resulting branch lookup to succeed"
    );
    let branch_name = String::from_utf8_lossy(&branch_name_output.stdout)
        .trim()
        .to_string();
    assert_ne!(
        branch_name, "main",
        "expected flow to switch away from the default branch before pushing"
    );
    assert!(
        branch_name.ends_with("/ready-for-review") || branch_name == "ready-for-review",
        "expected slugified branch name derived from context, got {branch_name}"
    );

    let remote_head = Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{branch_name}")])
        .current_dir(&bare_remote)
        .output()
        .expect("Failed to read remote slugified branch");
    assert!(
        remote_head.status.success(),
        "expected push to create the slugified remote branch\nstderr: {}",
        String::from_utf8_lossy(&remote_head.stderr)
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_pr_from_default_branch_commits_pushes_and_falls_back_to_pr_draft() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\ncombined push pr flow change\n",
    )
    .expect("Failed to modify tracked file in clone");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage clone tracked file");

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push-pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "ready for review",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "commit+push+pr flow should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let branch_name_output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read resulting branch");
    let branch_name = String::from_utf8_lossy(&branch_name_output.stdout)
        .trim()
        .to_string();
    assert_ne!(
        branch_name, "main",
        "expected combined flow to switch away from the default branch"
    );

    let local_head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read local HEAD");
    let local_head = String::from_utf8_lossy(&local_head.stdout)
        .trim()
        .to_string();
    assert!(
        !local_head.is_empty(),
        "expected combined flow to create a real commit"
    );

    let remote_head = Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{branch_name}")])
        .current_dir(&bare_remote)
        .output()
        .expect("Failed to read remote branch");
    assert!(
        remote_head.status.success(),
        "expected combined flow to push the new branch\nstderr: {}",
        String::from_utf8_lossy(&remote_head.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&remote_head.stdout).trim(),
        local_head,
        "expected pushed remote branch HEAD to match local HEAD"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[DONE] Commit created successfully!"),
        "expected combined flow to create a commit, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PUSH] Pushed branch:"),
        "expected combined flow to push the branch, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PULL REQUEST] Falling back to generated draft"),
        "expected combined flow to attempt PR creation and fall back to a draft, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("TITLE:"),
        "expected combined flow to print the generated PR title, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BODY:"),
        "expected combined flow to print the generated PR body, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_pr_with_clean_worktree_uses_existing_branch_commits() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    Command::new("git")
        .args(["switch", "-c", "feature/existing-branch-pr"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to create feature branch");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\nexisting branch commit for push-pr\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage tracked file");

    Command::new("git")
        .args(["commit", "-m", "feat: prepare existing branch for pr"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to commit feature branch change");

    let status_output = Command::new("git")
        .args(["status", "--short"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read clean worktree status");
    assert_eq!(
        String::from_utf8_lossy(&status_output.stdout).trim(),
        "",
        "test setup should leave the worktree clean"
    );

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push-pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "ready for review",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "clean-worktree push-pr flow should succeed when the branch is already ahead\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[INFO] No changes detected in the repository."),
        "expected combined flow to continue from existing branch commits instead of bailing out, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PUSH] Pushed branch:"),
        "expected combined flow to push the existing feature branch, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PULL REQUEST] Falling back to generated draft"),
        "expected combined flow to attempt PR creation from existing branch commits, got stdout: {stdout}"
    );

    let remote_head = Command::new("git")
        .args(["rev-parse", "refs/heads/feature/existing-branch-pr"])
        .current_dir(&bare_remote)
        .output()
        .expect("Failed to read remote feature branch");
    assert!(
        remote_head.status.success(),
        "expected combined flow to push the existing feature branch\nstderr: {}",
        String::from_utf8_lossy(&remote_head.stderr)
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_pr_with_no_branch_changes_reports_combined_skip_reason() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    Command::new("git")
        .args(["switch", "-c", "feature/no-branch-changes"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to create feature branch");

    let status_output = Command::new("git")
        .args(["status", "--short"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read clean worktree status");
    assert_eq!(
        String::from_utf8_lossy(&status_output.stdout).trim(),
        "",
        "test setup should leave the worktree clean"
    );

    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push-pr",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            "ready for review",
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "clean-worktree push-pr flow should skip cleanly when the branch is not ahead\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[INFO] No branch changes to push or open as a pull request."),
        "expected combined-flow-specific skip reason, got stdout: {stdout}"
    );
    assert!(
        !stdout.contains("[INFO] No changes detected in the repository."),
        "expected combined flow to avoid the generic empty-repository message, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_pr_dry_run_previews_slugified_branch_without_switching() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\npush-pr dry-run branch preview change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage tracked file");

    let context = "ready for review";
    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push-pr",
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            context,
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "push-pr dry-run should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let current_branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read current branch after dry-run");
    assert_eq!(
        String::from_utf8_lossy(&current_branch.stdout).trim(),
        "main",
        "dry-run should not switch branches"
    );

    let owner = std::env::var("SAFEUSER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("USER")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
    let expected_branch = match owner {
        Some(owner) => format!("{owner}/ready-for-review"),
        None => "ready-for-review".to_string(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("BRANCH: {expected_branch}")),
        "expected dry-run combined flow to preview the slugified feature branch, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[DRY RUN] Push preview:"),
        "expected dry-run combined flow to print the push preview heading, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BRANCH ACTION: switched"),
        "expected dry-run combined flow to report that it would switch branches from main, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[DRY RUN] Generated Pull Request Draft"),
        "expected PR dry-run output heading, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
#[serial(ollama)]
async fn test_push_dry_run_previews_slugified_branch_without_switching() {
    let _ollama_manager = ensure_shared_test_ollama("tinyllama:latest").await;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let bare_remote = temp_dir.path().join("origin.git");
    let seed_repo = temp_dir.path().join("seed");
    let clone_repo = temp_dir.path().join("clone");

    Command::new("git")
        .args(["init", "--bare", bare_remote.to_string_lossy().as_ref()])
        .status()
        .expect("Failed to initialize bare remote");

    Command::new("git")
        .args([
            "init",
            "--initial-branch=main",
            seed_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to initialize seed repo");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to configure seed git user.email");

    std::fs::write(seed_repo.join("tracked.txt"), "base\n").expect("Failed to write seed file");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed file");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to commit seed file");

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            bare_remote.to_string_lossy().as_ref(),
        ])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to add seed remote");

    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&seed_repo)
        .status()
        .expect("Failed to push main branch to remote");

    Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare_remote)
        .status()
        .expect("Failed to point bare remote HEAD at main");

    Command::new("git")
        .args([
            "clone",
            bare_remote.to_string_lossy().as_ref(),
            clone_repo.to_string_lossy().as_ref(),
        ])
        .status()
        .expect("Failed to clone bare remote");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.name");

    Command::new("git")
        .args(["config", "user.email", "tests@git-ai-commit.local"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to configure clone git user.email");

    std::fs::write(
        clone_repo.join("tracked.txt"),
        "base\npush dry-run branch preview change\n",
    )
    .expect("Failed to modify tracked file");

    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&clone_repo)
        .status()
        .expect("Failed to stage tracked file");

    let context = "ready for review";
    let binary = env!("CARGO_BIN_EXE_git-ai-commit");
    let output = Command::new(binary)
        .args([
            "--push",
            "--dry-run",
            "--provider",
            "openai-compatible",
            "--model",
            "tinyllama:latest",
            "--port",
            "11434",
            "--context",
            context,
        ])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to run git-ai-commit binary");

    assert!(
        output.status.success(),
        "push dry-run should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let current_branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&clone_repo)
        .output()
        .expect("Failed to read current branch after dry-run");
    assert_eq!(
        String::from_utf8_lossy(&current_branch.stdout).trim(),
        "main",
        "dry-run should not switch branches"
    );

    let owner = std::env::var("SAFEUSER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("USER")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
    let expected_branch = match owner {
        Some(owner) => format!("{owner}/ready-for-review"),
        None => "ready-for-review".to_string(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("BRANCH: {expected_branch}")),
        "expected push dry-run to preview the slugified feature branch, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("[DRY RUN] Push preview:"),
        "expected push dry-run preview heading, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("BRANCH ACTION: switched"),
        "expected push dry-run to preview that it would switch off the default branch, got stdout: {stdout}"
    );

    temp_dir.close().expect("Failed to clean up temp dir");
}
