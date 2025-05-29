use git_ai_commit::git::{
    GitCollector, GitStatus, FileChange, DiffInfo
};
use git_ai_commit::git::diff::FileStat;
use git_ai_commit::formatting::prompt::PromptBuilder;
use git_ai_commit::git::files::ChangeType;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

// For now, we'll skip the mock testing and focus on the actual git operations
// as setting up proper async mocks is complex and not necessary for our current needs

#[tokio::test]
async fn test_interactive_stage_unstaged() {
    // Create a temporary directory for the test repository
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();
    
    // Initialize a new git repository
    Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
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
        .current_dir(&repo_path)
        .status()
        .expect("Failed to add files");
    
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_path)
        .status()
        .expect("Failed to commit files");
    
    // Modify both files
    std::fs::write(&file1, "modified content1").expect("Failed to modify file1");
    std::fs::write(&file2, "modified content2").expect("Failed to modify file2");
    
    // Stage only file1
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(&repo_path)
        .status()
        .expect("Failed to stage file1");
    
    // Create a GitCollector for testing
    let git_collector = GitCollector::new(repo_path.to_path_buf());
    
    // Test that we detect both staged and unstaged files
    let git_info = git_collector.collect_all().await.expect("Failed to collect status");
    
    // Debug output
    println!("Staged files: {:?}", git_info.status.staged_files);
    println!("Modified files: {:?}", git_info.status.modified_files);
    
    assert!(git_info.status.staged_files.contains(&PathBuf::from("file1.txt")), 
           "file1.txt should be staged");
    assert!(git_info.status.modified_files.contains(&PathBuf::from("file2.txt")),
           "file2.txt should be modified but unstaged");
    
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
        .current_dir(&repo_path)
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
        .current_dir(&repo_path)
        .status()
        .expect("Failed to add files");
    
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_path)
        .status()
        .expect("Failed to commit files");
    
    // Modify both files
    std::fs::write(&file1, "fn main() { println!('Hello'); }").expect("Failed to modify file1");
    std::fs::write(&file2, "fn helper() { println!('Helper'); }").expect("Failed to modify file2");
    
    // Stage only file1
    Command::new("git")
        .args(["add", "file1.rs"])
        .current_dir(&repo_path)
        .status()
        .expect("Failed to stage file1");
    
    // In a real implementation, we would test the interactive staging here
    // For now, we'll just verify that we can detect both staged and unstaged changes
    let git_collector = GitCollector::new(repo_path.to_path_buf());
    let git_info = git_collector.collect_all().await.expect("Failed to collect status");
    
    assert!(git_info.status.staged_files.contains(&PathBuf::from("file1.rs")));
    assert!(git_info.status.modified_files.contains(&PathBuf::from("file2.rs")));
    
    // Clean up
    temp_dir.close().expect("Failed to clean up temp dir");
}

#[tokio::test]
async fn test_prompt_shows_staged_and_unstaged() {
    // Create test data with both staged and unstaged changes
    let git_info = git_ai_commit::git::GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("src/main.rs")],
            modified_files: vec![PathBuf::from("Cargo.toml")],
            untracked_files: vec![],
            deleted_files: vec![],
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
    
    // Verify the prompt includes both staged and unstaged sections
    assert!(prompt.contains("Staged changes (will be committed):"));
    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("Unstaged changes (will NOT be committed):"));
    assert!(prompt.contains("Cargo.toml"));
}
