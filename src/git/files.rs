use crate::utils::error::GitAiError;
use anyhow::Result;
use std::path::PathBuf;

/// Represents a file change in git
#[derive(Debug, Clone)]
pub struct FileChange {
    pub change_type: ChangeType,
    pub file_path: PathBuf,
    pub old_path: Option<PathBuf>, // For renames/copies
}

#[derive(Debug, Clone)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
}

impl FileChange {
    pub fn parse_list(changes_text: &str) -> Result<Vec<FileChange>> {
        let mut changes = Vec::new();
        
        for line in changes_text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            let change = Self::parse_line(line)?;
            changes.push(change);
        }
        
        Ok(changes)
    }
    
    fn parse_line(line: &str) -> Result<FileChange> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            return Err(GitAiError::Git(format!("Invalid git status line: {}", line)).into());
        }
        
        let status = parts[0];
        let change_type = match status.chars().next().unwrap() {
            'A' => ChangeType::Added,
            'M' => ChangeType::Modified,
            'D' => ChangeType::Deleted,
            'R' => ChangeType::Renamed,
            'C' => ChangeType::Copied,
            'U' => ChangeType::Unmerged,
            _ => return Err(GitAiError::Git(format!("Unknown git status: {}", status)).into()),
        };
        
        let (file_path, old_path) = match change_type {
            ChangeType::Renamed | ChangeType::Copied => {
                if parts.len() < 3 {
                    return Err(GitAiError::Git(format!("Invalid rename/copy line: {}", line)).into());
                }
                (PathBuf::from(parts[2]), Some(PathBuf::from(parts[1])))
            }
            _ => {
                if parts.len() < 2 {
                    return Err(GitAiError::Git(format!("Invalid status line: {}", line)).into());
                }
                (PathBuf::from(parts[1]), None)
            }
        };
        
        Ok(FileChange {
            change_type,
            file_path,
            old_path,
        })
    }
    
    pub fn display(&self) -> String {
        match &self.change_type {
            ChangeType::Added => format!("A  {}", self.file_path.display()),
            ChangeType::Modified => format!("M  {}", self.file_path.display()),
            ChangeType::Deleted => format!("D  {}", self.file_path.display()),
            ChangeType::Renamed => {
                if let Some(ref old_path) = self.old_path {
                    format!("R  {} -> {}", old_path.display(), self.file_path.display())
                } else {
                    format!("R  {}", self.file_path.display())
                }
            }
            ChangeType::Copied => {
                if let Some(ref old_path) = self.old_path {
                    format!("C  {} -> {}", old_path.display(), self.file_path.display())
                } else {
                    format!("C  {}", self.file_path.display())
                }
            }
            ChangeType::Unmerged => format!("U  {}", self.file_path.display()),
        }
    }
    
    pub fn is_test_file(&self) -> bool {
        let path_str = self.file_path.to_string_lossy().to_lowercase();
        path_str.contains("test") || 
        path_str.contains("spec") ||
        path_str.ends_with(".test.js") ||
        path_str.ends_with(".test.ts") ||
        path_str.ends_with(".test.tsx") ||
        path_str.ends_with(".spec.js") ||
        path_str.ends_with(".spec.ts") ||
        path_str.ends_with(".spec.tsx")
    }
    
    pub fn is_config_file(&self) -> bool {
        let path_str = self.file_path.to_string_lossy().to_lowercase();
        let config_files = [
            "package.json", "cargo.toml", "pyproject.toml", "requirements.txt",
            "dockerfile", "docker-compose.yml", "makefile", ".gitignore",
            "readme.md", "license", "changelog.md"
        ];
        
        config_files.iter().any(|&config| path_str.ends_with(config))
    }
}
