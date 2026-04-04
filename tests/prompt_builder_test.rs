use git_ai_commit::formatting::prompt::PromptBuilder;
use git_ai_commit::git::diff::FileStat;
use git_ai_commit::git::files::ChangeType;
use git_ai_commit::git::{DiffInfo, FileChange, GitInfo, GitStatus};
use std::path::PathBuf;
use tempfile::tempdir;

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
            unmerged_files: vec![],
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
    assert!(prompt.contains("Generate a git commit message in plain text Lore format only."));
    assert!(!prompt.contains("Current branch: main"));
    assert!(!prompt.contains("Last commit:"));
    assert!(!prompt.contains("Unstaged changes (will NOT be committed):"));
}

#[test]
fn test_default_prompt_avoids_extra_instruction_block_and_repo_metadata_scaffolding() {
    let builder = PromptBuilder::new(10, 100);
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("src/main.rs")],
            modified_files: vec![],
            untracked_files: vec![],
            deleted_files: vec![],
            unmerged_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 1,
            insertions: 10,
            deletions: 2,
            file_stats: vec![FileStat {
                filename: "src/main.rs".to_string(),
                insertions: 10,
                deletions: 2,
            }],
        },
        file_changes: vec![FileChange {
            change_type: ChangeType::Modified,
            file_path: PathBuf::from("src/main.rs"),
            old_path: None,
        }],
        untracked_files: vec![],
        branch_name: "feature/test".to_string(),
        last_commit: Some("Initial commit".to_string()),
    };

    let prompt = builder.build(&git_info);

    assert!(
        !prompt.contains("Requirements:"),
        "default prompt should match crabclaw's tighter run_commit contract without an extra instruction block"
    );
    assert!(
        !prompt.contains("Current branch:"),
        "default prompt should not prepend branch metadata scaffolding that crabclaw's run_commit prompt does not include"
    );
    assert!(
        !prompt.contains("Last commit:"),
        "default prompt should not prepend last-commit metadata scaffolding that crabclaw's run_commit prompt does not include"
    );
}

#[test]
fn test_default_prompt_uses_diff_stat_contract_without_extra_staged_file_scaffolding() {
    let builder = PromptBuilder::new(10, 500);
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/formatting/prompt.rs"),
            ],
            modified_files: vec![],
            untracked_files: vec![],
            deleted_files: vec![],
            unmerged_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 2,
            insertions: 18,
            deletions: 4,
            file_stats: vec![
                FileStat {
                    filename: "src/main.rs".to_string(),
                    insertions: 10,
                    deletions: 2,
                },
                FileStat {
                    filename: "src/formatting/prompt.rs".to_string(),
                    insertions: 8,
                    deletions: 2,
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
                file_path: PathBuf::from("src/formatting/prompt.rs"),
                old_path: None,
            },
        ],
        untracked_files: vec![],
        branch_name: "feature/test".to_string(),
        last_commit: Some("Initial commit".to_string()),
    };

    let prompt = builder.build(&git_info);

    assert!(
        prompt.contains("Staged diff summary: 2 files changed, 18 insertions(+), 4 deletions(-)"),
        "default prompt should still carry staged diff summary context"
    );
    assert!(
        !prompt.contains("Staged changes (will be committed):"),
        "default prompt should follow crabclaw's run_commit contract and avoid a separate staged file list block"
    );
    assert!(
        !prompt.contains("Detailed staged changes per file:"),
        "default prompt should avoid extra per-file scaffolding beyond the staged diff summary"
    );
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

    // When
    let prompt = builder.build(&git_info);

    // Then
    assert!(prompt.contains("Generate a git commit message in plain text Lore format only."));
    assert!(!prompt.contains("Current branch: feature/test"));
    assert!(!prompt.contains("Last commit: Initial commit"));

    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("src/main.rs | +10 -2"));

    // Unstaged changes should not be part of the commit prompt
    assert!(!prompt.contains("Unstaged changes (will NOT be committed):"));
    assert!(!prompt.contains("Cargo.toml"));

    // Check diff stats
    assert!(
        prompt.contains("Staged diff summary: 1 files changed, 10 insertions(+), 2 deletions(-)")
    );
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
            unmerged_files: vec![],
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
    assert!(prompt.contains("Generate a git commit message in plain text Lore format only."));
    assert!(!prompt.contains("Current branch: main"));
    assert!(!prompt.contains("Last commit: Previous commit"));
    assert!(!prompt.contains("Untracked files"));
    assert!(!prompt.contains("new_file.txt"));
    assert!(!prompt.contains("config/local.yaml"));
}

#[test]
fn test_prompt_includes_only_staged_changes() {
    // Given
    let builder = PromptBuilder::new(10, 100);
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("staged.txt")],
            modified_files: vec![PathBuf::from("unstaged.txt")],
            untracked_files: vec![],
            deleted_files: vec![],
            unmerged_files: vec![],
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

    // Should include only staged changes in the diff
    assert!(
        prompt.contains("staged.txt"),
        "Should include staged files in diff"
    );
    assert!(
        !prompt.contains("unstaged.txt"),
        "Should not include unstaged files in diff"
    );

    // Check for the detailed diff statistics in the prompt
    let expected_entries = vec![
        "staged.txt | +3 -1",
        "Staged diff summary: 1 files changed, 3 insertions(+), 1 deletions(-)",
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

#[test]
fn test_prompt_builder_uses_custom_template_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let template_path = temp_dir.path().join("custom-template.txt");
    std::fs::write(&template_path, "CUSTOM HEADER\n{CONTEXT}\nCUSTOM FOOTER")
        .expect("failed to write template file");

    let builder = PromptBuilder::from_template_file(&template_path, 10, 100)
        .expect("custom template should load");
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![],
            modified_files: vec![],
            untracked_files: vec![],
            deleted_files: vec![],
            unmerged_files: vec![],
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

    let prompt = builder.build(&git_info);

    assert!(prompt.contains("CUSTOM HEADER"));
    assert!(prompt.contains("Current branch: main"));
    assert!(prompt.contains("CUSTOM FOOTER"));
    assert!(
        !prompt.contains("You are an expert software developer"),
        "default template should not leak into a custom template render"
    );
}
