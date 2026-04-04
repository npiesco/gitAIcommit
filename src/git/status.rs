use anyhow::Result;
use std::path::PathBuf;

/// Git repository status information
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub staged_files: Vec<PathBuf>,
    pub modified_files: Vec<PathBuf>,
    pub untracked_files: Vec<PathBuf>,
    pub deleted_files: Vec<PathBuf>,
    pub unmerged_files: Vec<PathBuf>,
}

impl GitStatus {
    pub fn parse(status_text: &str) -> Result<Self> {
        let mut staged_files = Vec::new();
        let mut modified_files = Vec::new();
        let mut untracked_files = Vec::new();
        let mut deleted_files = Vec::new();
        let mut unmerged_files = Vec::new();

        for line in status_text.lines() {
            if line.len() < 3 {
                continue;
            }

            let index_status = line.chars().nth(0).unwrap();
            let worktree_status = line.chars().nth(1).unwrap();
            let path = Self::parse_status_path(line, index_status, worktree_status);

            // Parse staged changes (index status)
            match index_status {
                'A' | 'M' | 'R' | 'C' => staged_files.push(path.clone()),
                'D' => {
                    staged_files.push(path.clone());
                    deleted_files.push(path.clone());
                }
                'U' => unmerged_files.push(path.clone()),
                _ => {}
            }

            // Parse working tree changes
            match worktree_status {
                'M' => modified_files.push(path.clone()),
                'D' => deleted_files.push(path),
                'U' => unmerged_files.push(path),
                '?' => untracked_files.push(path),
                _ => {}
            }
        }

        unmerged_files.sort();
        unmerged_files.dedup();

        Ok(GitStatus {
            staged_files,
            modified_files,
            untracked_files,
            deleted_files,
            unmerged_files,
        })
    }

    fn parse_status_path(line: &str, index_status: char, worktree_status: char) -> PathBuf {
        let filename = &line[3..];

        if matches!(index_status, 'R' | 'C') || matches!(worktree_status, 'R' | 'C') {
            let mut rename_parts = filename.splitn(2, " -> ");
            let _old_path = rename_parts.next();
            if let Some(new_path) = rename_parts.next() {
                return PathBuf::from(new_path);
            }
        }

        PathBuf::from(filename)
    }

    pub fn display(&self) -> String {
        let mut output = String::new();

        if !self.staged_files.is_empty() {
            output.push_str(&format!(
                "  Staged files ({}): {}\n",
                self.staged_files.len(),
                self.staged_files
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if !self.modified_files.is_empty() {
            output.push_str(&format!(
                "  Modified files ({}): {}\n",
                self.modified_files.len(),
                self.modified_files
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if !self.deleted_files.is_empty() {
            output.push_str(&format!(
                "  Deleted files ({}): {}\n",
                self.deleted_files.len(),
                self.deleted_files
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if !self.untracked_files.is_empty() {
            output.push_str(&format!(
                "  Untracked files ({}): {}\n",
                self.untracked_files.len(),
                self.untracked_files
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if !self.unmerged_files.is_empty() {
            output.push_str(&format!(
                "  Unmerged files ({}): {}\n",
                self.unmerged_files.len(),
                self.unmerged_files
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if output.is_empty() {
            output.push_str("  No changes detected\n");
        }

        output
    }
}
