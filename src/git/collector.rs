use crate::git::{GitStatus, DiffInfo, FileChange};
use crate::utils::error::GitAiError;
use anyhow::Result;
use std::path::PathBuf;
use tokio::process::Command;

/// Main git data collector that orchestrates all git operations
pub struct GitCollector {
    repo_path: PathBuf,
}

/// Comprehensive git repository information
#[derive(Debug, Clone)]
pub struct GitInfo {
    pub status: GitStatus,
    pub diff_stat: DiffInfo,
    pub file_changes: Vec<FileChange>,
    pub untracked_files: Vec<PathBuf>,
    pub branch_name: String,
    pub last_commit: Option<String>,
}

impl GitInfo {
    /// Check if there are no changes to commit
    /// 
    /// # Arguments
    /// * `after_staging` - If true, only checks for staged changes (used with --add-unstaged flag)
    pub fn is_empty(&self, after_staging: bool) -> bool {
        if after_staging {
            // After staging with --add-unstaged, we only care about staged files
            self.status.staged_files.is_empty()
        } else {
            // Before staging, check all possible changes
            self.status.staged_files.is_empty() && 
            self.status.modified_files.is_empty() && 
            self.status.untracked_files.is_empty() && 
            self.status.deleted_files.is_empty()
        }
    }
    
    pub fn display(&self) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("Branch: {}\n", self.branch_name));
        
        if let Some(ref last_commit) = self.last_commit {
            output.push_str(&format!("Last commit: {}\n", last_commit));
        }
        
        output.push_str(&format!("\nStatus:\n{}\n", self.status.display()));
        output.push_str(&format!("Diff stats:\n{}\n", self.diff_stat.display()));
        
        if !self.file_changes.is_empty() {
            output.push_str("\nFile changes:\n");
            for change in &self.file_changes {
                output.push_str(&format!("  {}\n", change.display()));
            }
        }
        
        if !self.untracked_files.is_empty() {
            output.push_str("\nUntracked files:\n");
            for file in &self.untracked_files {
                output.push_str(&format!("  {}\n", file.display()));
            }
        }
        
        output
    }
}

impl GitCollector {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }
    
    /// Collect all git information in parallel where possible
    pub async fn collect_all(&self) -> Result<GitInfo> {
        // Run git operations concurrently for better performance
        let status_task = self.get_status();
        let diff_task = self.get_diff_stat();
        let branch_task = self.get_branch_name();
        let last_commit_task = self.get_last_commit();
        
        let (status, diff_stat, branch_name, last_commit) = tokio::try_join!(
            status_task,
            diff_task, 
            branch_task,
            last_commit_task
        )?;
        
        // These depend on the status, so run sequentially
        let file_changes = self.get_file_changes().await?;
        let untracked_files = self.get_untracked_files().await?;
        
        Ok(GitInfo {
            status,
            diff_stat,
            file_changes,
            untracked_files,
            branch_name,
            last_commit,
        })
    }
    
    async fn get_status(&self) -> Result<GitStatus> {
        let output = Command::new("git")
            .args(&["status", "--porcelain=v1"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(GitAiError::Git(format!("Failed to get git status: {}", error)).into());
        }
        
        let status_text = String::from_utf8_lossy(&output.stdout);
        GitStatus::parse(&status_text)
    }
    
    async fn get_diff_stat(&self) -> Result<DiffInfo> {
        // Get staged changes
        let staged_output = Command::new("git")
            .args(&["diff", "--cached", "--numstat"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        // Get unstaged changes
        let unstaged_output = Command::new("git")
            .args(&["diff", "--numstat"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !staged_output.status.success() {
            let error = String::from_utf8_lossy(&staged_output.stderr);
            return Err(GitAiError::Git(format!("Failed to get staged diff stats: {}", error)).into());
        }
        
        if !unstaged_output.status.success() {
            let error = String::from_utf8_lossy(&unstaged_output.stderr);
            return Err(GitAiError::Git(format!("Failed to get unstaged diff stats: {}", error)).into());
        }
        
        // Parse both sets of changes
        let staged_diff = String::from_utf8_lossy(&staged_output.stdout);
        let unstaged_diff = String::from_utf8_lossy(&unstaged_output.stdout);
        
        // Combine the diffs
        let mut combined_diff = staged_diff.to_string();
        if !unstaged_diff.trim().is_empty() {
            combined_diff.push_str("\n");
            combined_diff.push_str(&unstaged_diff);
        }
        
        DiffInfo::parse(&combined_diff)
    }
    
    async fn get_file_changes(&self) -> Result<Vec<FileChange>> {
        // Get staged changes
        let staged_output = Command::new("git")
            .args(&["diff", "--cached", "--name-status"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        // Get unstaged changes
        let unstaged_output = Command::new("git")
            .args(&["diff", "--name-status"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !staged_output.status.success() {
            let error = String::from_utf8_lossy(&staged_output.stderr);
            return Err(GitAiError::Git(format!("Failed to get staged file changes: {}", error)).into());
        }
        
        if !unstaged_output.status.success() {
            let error = String::from_utf8_lossy(&unstaged_output.stderr);
            return Err(GitAiError::Git(format!("Failed to get unstaged file changes: {}", error)).into());
        }
        
        // Parse both sets of changes
        let staged_changes = String::from_utf8_lossy(&staged_output.stdout);
        let unstaged_changes = String::from_utf8_lossy(&unstaged_output.stdout);
        
        // Combine the changes
        let mut all_changes = Vec::new();
        
        // Add staged changes first
        if !staged_changes.trim().is_empty() {
            let mut changes = FileChange::parse_list(&staged_changes)?;
            all_changes.append(&mut changes);
        }
        
        // Add unstaged changes, avoiding duplicates
        if !unstaged_changes.trim().is_empty() {
            let changes = FileChange::parse_list(&unstaged_changes)?;
            // Only add unstaged changes for files that aren't already in the list
            for change in changes {
                if !all_changes.iter().any(|c: &FileChange| c.file_path == change.file_path) {
                    all_changes.push(change);
                }
            }
        }
        
        Ok(all_changes)
    }
    
    async fn get_untracked_files(&self) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .args(&["ls-files", "--others", "--exclude-standard"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(GitAiError::Git(format!("Failed to get untracked files: {}", error)).into());
        }
        
        let files_text = String::from_utf8_lossy(&output.stdout);
        Ok(files_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| PathBuf::from(line.trim()))
            .collect())
    }
    
    async fn get_branch_name(&self) -> Result<String> {
        let output = Command::new("git")
            .args(&["branch", "--show-current"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(GitAiError::Git(format!("Failed to get branch name: {}", error)).into());
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
    
    async fn get_last_commit(&self) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(&["log", "-1", "--pretty=%B"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !output.status.success() {
            return Ok(None);
        }
        
        let commit_msg = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if commit_msg.is_empty() {
            Ok(None)
        } else {
            Ok(Some(commit_msg))
        }
    }
    
    /// Stage all unstaged changes in the working directory
    pub async fn stage_all_unstaged(&self) -> Result<()> {
        // First, stage modified and deleted files
        let output = Command::new("git")
            .args(&["add", "--update"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(GitAiError::Git(format!("Failed to stage changes: {}", error)).into());
        }
        
        // Then, stage untracked files (but respect .gitignore)
        let output = Command::new("git")
            .args(&["add", "--all"])
            .current_dir(&self.repo_path)
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(GitAiError::Git(format!("Failed to stage untracked files: {}", error)).into());
        }
        
        Ok(())
    }
}
