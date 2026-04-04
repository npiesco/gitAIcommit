use git_ai_commit::formatting::prompt::PromptBuilder;
use git_ai_commit::git::diff::FileStat;
use git_ai_commit::git::files::ChangeType;
use git_ai_commit::git::{DiffInfo, FileChange, GitCollector, GitStatus};
use git_ai_commit::ollama::{OllamaClient, OllamaClientTrait};
use git_ai_commit::OllamaManager;
use serial_test::serial;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

// For now, we'll skip the mock testing and focus on the actual git operations
// as setting up proper async mocks is complex and not necessary for our current needs

async fn ensure_shared_test_ollama(model: &str) -> Option<OllamaManager> {
    let client = OllamaClient::new(11434);
    if client.is_running().await {
        return None;
    }

    let mut manager = OllamaManager::new(model.to_string(), 11434).expect("manager should build");
    manager
        .ensure_running()
        .await
        .expect("real Ollama integration test needs a shared local server");
    Some(manager)
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
