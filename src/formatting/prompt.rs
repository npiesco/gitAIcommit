use crate::git::{GitInfo, FileChange};

/// Builds optimized prompts for AI commit message generation
pub struct PromptBuilder {
    max_files: usize,
    max_diff_lines: usize,
    template: String,
}

impl PromptBuilder {
    pub fn new(max_files: usize, max_diff_lines: usize) -> Self {
        let template = Self::default_template();
        
        Self {
            max_files,
            max_diff_lines,
            template,
        }
    }
    
    /// Build a comprehensive prompt from git information
    pub fn build(&self, git_info: &GitInfo) -> String {
        let mut context = String::new();
        
        // Add branch information
        context.push_str(&format!("Current branch: {}\n", git_info.branch_name));
        
        if let Some(ref last_commit) = git_info.last_commit {
            context.push_str(&format!("Last commit: {}\n", last_commit));
        }
        
        // Add file changes summary with diff line limits
        if !git_info.file_changes.is_empty() {
            // Group changes by staged/unstaged status
            let staged_changes: Vec<_> = git_info.file_changes.iter()
                .filter(|c| git_info.status.staged_files.contains(&c.file_path))
                .collect();
                
            let unstaged_changes: Vec<_> = git_info.file_changes.iter()
                .filter(|c| !git_info.status.staged_files.contains(&c.file_path))
                .collect();
            
            // Show staged changes first
            if !staged_changes.is_empty() {
                context.push_str("\nStaged changes (will be committed):\n");
                self.add_file_changes_to_context(&mut context, &staged_changes);
            }
            
            // Then show unstaged changes
            if !unstaged_changes.is_empty() {
                if !staged_changes.is_empty() {
                    context.push_str("\n");
                }
                context.push_str("Unstaged changes (will NOT be committed):\n");
                self.add_file_changes_to_context(&mut context, &unstaged_changes);
            }
        }
        
        // Add diff statistics
        if git_info.diff_stat.files_changed > 0 {
            // Combined summary
            context.push_str(&format!(
                "\nDiff summary: {} files changed, {} insertions(+), {} deletions(-)\n",
                git_info.diff_stat.files_changed,
                git_info.diff_stat.insertions,
                git_info.diff_stat.deletions
            ));
            
            // Detailed per-file statistics
            if !git_info.diff_stat.file_stats.is_empty() {
                context.push_str("\nDetailed changes per file:\n");
                for stat in &git_info.diff_stat.file_stats {
                    context.push_str(&format!(
                        "  {}: {} insertions(+), {} deletions(-)\n",
                        stat.filename, stat.insertions, stat.deletions
                    ));
                }
            }
        }
        
        // Add untracked files summary (limited)
        if !git_info.untracked_files.is_empty() {
            context.push_str(&format!("\nUntracked files ({}): ", git_info.untracked_files.len()));
            let untracked_display: Vec<_> = git_info.untracked_files
                .iter()
                .take(5)
                .map(|p| p.to_string_lossy())
                .collect();
            context.push_str(&untracked_display.join(", "));
            
            if git_info.untracked_files.len() > 5 {
                context.push_str(&format!(" and {} more", git_info.untracked_files.len() - 5));
            }
            context.push('\n');
        }
        
        // Build final prompt
        self.template.replace("{CONTEXT}", &context)
    }
    
    /// Helper method to add file changes to the context with proper formatting
    fn add_file_changes_to_context(&self, context: &mut String, changes: &[&FileChange]) {
        let mut total_diff_lines = 0;
        
        for (i, change) in changes.iter().take(self.max_files).enumerate() {
            if i >= self.max_files {
                context.push_str(&format!("  ... and {} more files\n", changes.len() - i));
                break;
            }
            
            // Estimate diff lines for this change (rough estimate based on file type)
            let estimated_lines = if change.is_config_file() { 5 } else { 20 };
            if total_diff_lines + estimated_lines > self.max_diff_lines {
                context.push_str(&format!("  ... and {} more files (diff limit reached)\n", changes.len() - i));
                break;
            }
            total_diff_lines += estimated_lines;
            
            context.push_str(&format!("  - {}\n", change.display()));
            
            // Add priority indicators
            if change.is_config_file() {
                context.push_str("    [CONFIG FILE]\n");
            } else if change.is_test_file() {
                context.push_str("    [TEST FILE]\n");
            }
        }
    }
    
    fn default_template() -> String {
        r#"You are an expert software developer creating a git commit message. 

Based on the following git repository changes, generate a concise, descriptive commit message that follows conventional commit format.

Repository Context:
{CONTEXT}

Guidelines for the commit message:
1. Use conventional commit format: type(scope): description
2. Types: feat, fix, docs, style, refactor, test, chore
3. Keep the first line under 50 characters
4. Be specific about what changed and why
5. Use imperative mood (e.g., "add" not "added")
6. Focus on the most significant changes
7. If there are breaking changes, mention them
8. For config file changes, use "chore" type
9. For test changes, use "test" type
10. Only include changes that are staged for commit in the commit message

Generate only the commit message, no additional explanation:"#.to_string()
    }
}
