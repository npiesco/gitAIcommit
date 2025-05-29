use git_ai_commit::formatting::prompt::PromptBuilder;
use git_ai_commit::git::{DiffInfo, FileChange, GitInfo, GitStatus};
use git_ai_commit::git::diff::FileStat;
use git_ai_commit::git::files::ChangeType;
use std::path::PathBuf;

#[test]
fn test_prompt_builder_with_empty_git_info() {
    // Given
    let builder = PromptBuilder::new(10, 100);
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![],
            modified_files: vec![],
            untracked_files: vec![],
            deleted_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            file_stats: vec![],
        },
        file_changes: vec![],
        untracked_files: vec![],
        branch_name: "main".to_string(),
        last_commit: None,
    };
    
    // When
    let prompt = builder.build(&git_info);
    
    // Then
    assert!(prompt.contains("You are an expert software developer"));
    assert!(prompt.contains("Current branch: main"));
    assert!(!prompt.contains("File changes:"));
}

#[test]
fn test_prompt_builder_with_file_changes() {
    // Given
    let builder = PromptBuilder::new(10, 100);
    let git_info = GitInfo {
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
    
    // When
    let prompt = builder.build(&git_info);
    
    // Then
    assert!(prompt.contains("You are an expert software developer"));
    assert!(prompt.contains("Current branch: feature/test"));
    assert!(prompt.contains("Last commit: Initial commit"));
    
    // Check for staged changes section
    assert!(prompt.contains("Staged changes (will be committed):"));
    assert!(prompt.contains("src/main.rs"));
    
    // Check for unstaged changes section
    assert!(prompt.contains("Unstaged changes (will NOT be committed):"));
    assert!(prompt.contains("Cargo.toml"));
    
    // Check diff stats
    assert!(prompt.contains("2 files changed, 15 insertions(+), 3 deletions(-)"));
}

#[test]
fn test_prompt_builder_with_untracked_files() {
    // Given
    let builder = PromptBuilder::new(10, 100);
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![],
            modified_files: vec![],
            untracked_files: vec![
                PathBuf::from("new_file.txt"),
                PathBuf::from("config/local.yaml"),
            ],
            deleted_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            file_stats: vec![],
        },
        file_changes: vec![],
        untracked_files: vec![
            PathBuf::from("new_file.txt"),
            PathBuf::from("config/local.yaml"),
        ],
        branch_name: "main".to_string(),
        last_commit: Some("Previous commit".to_string()),
    };
    
    // When
    let prompt = builder.build(&git_info);
    
    // Then
    assert!(prompt.contains("You are an expert software developer"));
    assert!(prompt.contains("Current branch: main"));
    assert!(prompt.contains("Last commit: Previous commit"));
    assert!(prompt.contains("Untracked files"));
    assert!(prompt.contains("new_file.txt"));
    assert!(prompt.contains("config/local.yaml"));
    assert!(!prompt.contains("File changes"));
}

#[test]
fn test_prompt_includes_both_staged_and_unstaged_changes() {
    // Given
    let builder = PromptBuilder::new(10, 100);
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("staged.txt")],
            modified_files: vec![PathBuf::from("unstaged.txt")],
            untracked_files: vec![],
            deleted_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 2,
            insertions: 5,
            deletions: 2,
            file_stats: vec![
                FileStat {
                    filename: "staged.txt".to_string(),
                    insertions: 3,
                    deletions: 1,
                },
                FileStat {
                    filename: "unstaged.txt".to_string(),
                    insertions: 2,
                    deletions: 1,
                },
            ],
        },
        file_changes: vec![
            FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("staged.txt"),
                old_path: None,
            },
            FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("unstaged.txt"),
                old_path: None,
            },
        ],
        untracked_files: vec![],
        branch_name: "main".to_string(),
        last_commit: Some("Initial commit".to_string()),
    };
    
    // When
    let prompt = builder.build(&git_info);
    
    // Then
    // Print the actual prompt for debugging
    println!("\n=== ACTUAL PROMPT ===\n{}\n===================\n", prompt);
    
    // Should include both staged and unstaged changes in the diff
    assert!(prompt.contains("staged.txt"), "Should include staged files in diff");
    assert!(prompt.contains("unstaged.txt"), "Should include unstaged files in diff");
    
    // Check for the detailed diff statistics in the prompt
    let expected_entries = vec![
        "staged.txt: 3 insertions(+), 1 deletions(-)",
        "unstaged.txt: 2 insertions(+), 1 deletions(-)",
        "Diff summary: 2 files changed, 5 insertions(+), 2 deletions(-)",
    ];
    
    for entry in expected_entries {
        assert!(
            prompt.contains(entry),
            "Prompt should contain: {}\nFull prompt:\n{}",
            entry,
            prompt
        );
    }
}
