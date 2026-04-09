use crate::git::{FileChange, GitInfo};
use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;

/// Builds optimized prompts for AI commit message generation
pub struct PromptBuilder {
    max_files: usize,
    max_diff_lines: usize,
    template: String,
    include_repo_metadata: bool,
    user_context: Option<String>,
}

impl PromptBuilder {
    pub fn new(max_files: usize, max_diff_lines: usize) -> Self {
        Self::with_template(
            Self::default_template(),
            max_files,
            max_diff_lines,
            false,
            None,
        )
    }

    pub fn with_user_context(
        max_files: usize,
        max_diff_lines: usize,
        user_context: Option<String>,
    ) -> Self {
        Self::with_template(
            Self::default_template(),
            max_files,
            max_diff_lines,
            false,
            user_context,
        )
    }

    pub fn from_template_file(
        path: impl AsRef<Path>,
        max_files: usize,
        max_diff_lines: usize,
    ) -> Result<Self> {
        let template = fs::read_to_string(path.as_ref())?;
        Self::from_template(template, max_files, max_diff_lines)
    }

    pub fn from_template(
        template: impl Into<String>,
        max_files: usize,
        max_diff_lines: usize,
    ) -> Result<Self> {
        let template = template.into();
        if !template.contains("{CONTEXT}") {
            return Err(anyhow!(
                "custom template must include the {{CONTEXT}} placeholder"
            ));
        }
        Ok(Self::with_template(
            template,
            max_files,
            max_diff_lines,
            true,
            None,
        ))
    }

    fn with_template(
        template: String,
        max_files: usize,
        max_diff_lines: usize,
        include_repo_metadata: bool,
        user_context: Option<String>,
    ) -> Self {
        Self {
            max_files,
            max_diff_lines,
            template,
            include_repo_metadata,
            user_context,
        }
    }

    /// Build a comprehensive prompt from git information
    pub fn build(&self, git_info: &GitInfo) -> String {
        let context = if self.include_repo_metadata {
            self.build_custom_template_context(git_info)
        } else {
            self.build_default_prompt_context(git_info)
        };

        // Build final prompt
        self.template.replace("{CONTEXT}", &context)
    }

    pub fn build_pr(&self, git_info: &GitInfo) -> String {
        let context = self.build_pr_prompt_context(git_info);

        if self.include_repo_metadata {
            self.template.replace("{CONTEXT}", &context)
        } else {
            Self::pr_template().replace("{CONTEXT}", &context)
        }
    }

    pub fn build_pr_from_branch_diff(
        &self,
        base_branch: &str,
        branch_diff: &str,
        recent_commit: Option<&str>,
    ) -> String {
        let mut context = String::new();
        context.push_str(&format!(
            "Branch diff summary against {}:\n{}\n",
            base_branch,
            truncate_for_prompt(branch_diff.trim(), self.max_diff_lines)
        ));

        if let Some(last_commit) = recent_commit
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            context.push_str("\n\nRecent commits:\n");
            context.push_str(&truncate_for_prompt(last_commit, self.max_diff_lines));
        }

        self.add_user_context(&mut context);

        if self.include_repo_metadata {
            self.template.replace("{CONTEXT}", &context)
        } else {
            Self::pr_template().replace("{CONTEXT}", &context)
        }
    }

    fn build_default_prompt_context(&self, git_info: &GitInfo) -> String {
        let mut context = String::new();
        self.add_default_staged_diff_context(&mut context, git_info);
        self.add_user_context(&mut context);
        context
    }

    fn build_custom_template_context(&self, git_info: &GitInfo) -> String {
        let mut context = String::new();
        let staged_changes: Vec<_> = git_info
            .file_changes
            .iter()
            .filter(|c| git_info.status.staged_files.contains(&c.file_path))
            .collect();
        let staged_stats: Vec<_> = git_info
            .diff_stat
            .file_stats
            .iter()
            .filter(|stat| {
                let stat_path = Path::new(&stat.filename);
                git_info
                    .status
                    .staged_files
                    .iter()
                    .any(|path| path.as_path() == stat_path)
            })
            .collect();

        context.push_str(&format!("Current branch: {}\n", git_info.branch_name));

        if let Some(ref last_commit) = git_info.last_commit {
            context.push_str(&format!("Last commit: {}\n", last_commit));
        }

        if !staged_changes.is_empty() {
            if !context.is_empty() {
                context.push('\n');
            }
            context.push_str("Staged changes (will be committed):\n");
            self.add_file_changes_to_context(&mut context, &staged_changes);
        }

        self.add_staged_diff_details(&mut context, &staged_stats);
        self.add_user_context(&mut context);
        context
    }

    fn build_pr_prompt_context(&self, git_info: &GitInfo) -> String {
        let mut context = String::new();
        self.add_default_staged_diff_context(&mut context, git_info);
        self.add_recent_commit_context(&mut context, git_info);
        self.add_user_context(&mut context);
        context
    }

    fn add_default_staged_diff_context(&self, context: &mut String, git_info: &GitInfo) {
        let staged_stats: Vec<_> = git_info
            .diff_stat
            .file_stats
            .iter()
            .filter(|stat| {
                let stat_path = Path::new(&stat.filename);
                git_info
                    .status
                    .staged_files
                    .iter()
                    .any(|path| path.as_path() == stat_path)
            })
            .collect();

        if !staged_stats.is_empty() {
            let staged_insertions: usize = staged_stats.iter().map(|stat| stat.insertions).sum();
            let staged_deletions: usize = staged_stats.iter().map(|stat| stat.deletions).sum();
            let mut staged_context = format!(
                "Staged diff summary: {} files changed, {} insertions(+), {} deletions(-)\n",
                staged_stats.len(),
                staged_insertions,
                staged_deletions
            );

            for stat in staged_stats {
                staged_context.push_str(&format!(
                    "{} | +{} -{}\n",
                    stat.filename, stat.insertions, stat.deletions
                ));
            }

            context.push_str(&truncate_lines_for_prompt(
                &staged_context,
                self.max_diff_lines,
            ));
        }
    }

    fn add_staged_diff_details(
        &self,
        context: &mut String,
        staged_stats: &[&crate::git::diff::FileStat],
    ) {
        if !staged_stats.is_empty() {
            let staged_insertions: usize = staged_stats.iter().map(|stat| stat.insertions).sum();
            let staged_deletions: usize = staged_stats.iter().map(|stat| stat.deletions).sum();
            context.push_str(&format!(
                "\nStaged diff summary: {} files changed, {} insertions(+), {} deletions(-)\n",
                staged_stats.len(),
                staged_insertions,
                staged_deletions
            ));

            context.push_str("\nDetailed staged changes per file:\n");
            for stat in staged_stats {
                context.push_str(&format!(
                    "  {}: {} insertions(+), {} deletions(-)\n",
                    stat.filename, stat.insertions, stat.deletions
                ));
            }
        }
    }

    fn add_user_context(&self, context: &mut String) {
        if let Some(user_context) = self.user_context.as_deref().map(str::trim) {
            if !user_context.is_empty() {
                context.push_str("\n\nRecent conversation context:\n");
                context.push_str(&truncate_for_prompt(user_context, self.max_diff_lines));
            }
        }
    }

    fn add_recent_commit_context(&self, context: &mut String, git_info: &GitInfo) {
        if let Some(last_commit) = git_info.last_commit.as_deref().map(str::trim) {
            if !last_commit.is_empty() {
                context.push_str("\n\nRecent commits:\n");
                context.push_str(&truncate_for_prompt(last_commit, self.max_diff_lines));
            }
        }
    }

    /// Helper method to add file changes to the context with proper formatting
    fn add_file_changes_to_context(&self, context: &mut String, changes: &[&FileChange]) {
        let mut staged_context = String::new();

        for (i, change) in changes.iter().enumerate() {
            if i >= self.max_files {
                staged_context.push_str(&format!("  ... and {} more files\n", changes.len() - i));
                break;
            }

            staged_context.push_str(&format!("  - {}\n", change.display()));

            // Add priority indicators
            if change.is_config_file() {
                staged_context.push_str("    [CONFIG FILE]\n");
            } else if change.is_test_file() {
                staged_context.push_str("    [TEST FILE]\n");
            }
        }

        context.push_str(&truncate_for_prompt(&staged_context, self.max_diff_lines));
    }

    fn default_template() -> String {
        r#"Generate a git commit message in plain text Lore format only. Base it on this staged diff summary:

{CONTEXT}"#
            .to_string()
    }

    fn pr_template() -> String {
        r#"Generate a pull request title and body from this conversation and diff summary. Output plain text in this format exactly:
TITLE: <title>
BODY:
<body markdown>

{CONTEXT}"#
            .to_string()
    }
}

fn truncate_for_prompt(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        let truncated = value.chars().take(limit).collect::<String>();
        format!("{}\n…[truncated]\n", truncated.trim_end())
    }
}

fn truncate_lines_for_prompt(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }

    let mut kept = String::new();
    for line in value.lines() {
        let candidate = if kept.is_empty() {
            line.to_string()
        } else {
            format!("{kept}\n{line}")
        };

        if candidate.chars().count() > limit {
            break;
        }

        kept = candidate;
    }

    if kept.is_empty() {
        truncate_for_prompt(value, limit)
    } else {
        format!("{}\n…[truncated]\n", kept.trim_end())
    }
}
