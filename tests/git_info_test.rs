use git_ai_commit::git::{GitInfo, GitStatus, DiffInfo, FileChange};
use git_ai_commit::git::files::ChangeType;
use std::path::PathBuf;

#[test]
fn test_git_info_is_empty_with_no_changes() {
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
    
    assert!(git_info.is_empty(false), "Should be empty with no changes");
    assert!(git_info.is_empty(true), "Should be empty after staging with no changes");
}

#[test]
fn test_git_info_is_empty_after_staging() {
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("staged.txt")],
            modified_files: vec![],
            untracked_files: vec![],
            deleted_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 1,
            insertions: 5,
            deletions: 2,
            file_stats: vec![],
        },
        file_changes: vec![FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("staged.txt"),
                old_path: None,
            }],
        untracked_files: vec![],
        branch_name: "main".to_string(),
        last_commit: None,
    };
    
    assert!(!git_info.is_empty(false), "Should not be empty with staged changes");
    assert!(!git_info.is_empty(true), "Should not be empty after staging with staged changes");
}

#[test]
fn test_git_info_with_unstaged_changes() {
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![],
            modified_files: vec![PathBuf::from("modified.txt")],
            untracked_files: vec![PathBuf::from("new.txt")],
            deleted_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 2,
            insertions: 10,
            deletions: 3,
            file_stats: vec![],
        },
        file_changes: vec![
            FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("modified.txt"),
                old_path: None,
            },
            FileChange {
                change_type: ChangeType::Added,
                file_path: PathBuf::from("new.txt"),
                old_path: None,
            },
        ],
        untracked_files: vec![PathBuf::from("new.txt")],
        branch_name: "main".to_string(),
        last_commit: None,
    };
    
    assert!(!git_info.is_empty(false), "Should not be empty with unstaged changes");
    assert!(git_info.is_empty(true), "Should be empty after staging with --add-unstaged");
}

#[test]
fn test_git_info_with_mixed_changes() {
    let git_info = GitInfo {
        status: GitStatus {
            staged_files: vec![PathBuf::from("staged.txt")],
            modified_files: vec![PathBuf::from("modified.txt")],
            untracked_files: vec![PathBuf::from("new.txt")],
            deleted_files: vec![],
        },
        diff_stat: DiffInfo {
            files_changed: 3,
            insertions: 15,
            deletions: 5,
            file_stats: vec![],
        },
        file_changes: vec![
            FileChange {
                change_type: ChangeType::Added,
                file_path: PathBuf::from("staged.txt"),
                old_path: None,
            },
            FileChange {
                change_type: ChangeType::Modified,
                file_path: PathBuf::from("modified.txt"),
                old_path: None,
            },
            FileChange {
                change_type: ChangeType::Added,
                file_path: PathBuf::from("new.txt"),
                old_path: None,
            },
        ],
        untracked_files: vec![PathBuf::from("new.txt")],
        branch_name: "main".to_string(),
        last_commit: None,
    };
    
    assert!(!git_info.is_empty(false), "Should not be empty with mixed changes");
    assert!(!git_info.is_empty(true), "Should not be empty after staging with mixed changes");
}
