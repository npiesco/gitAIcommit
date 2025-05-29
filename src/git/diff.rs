use anyhow::Result;

/// Git diff statistics
#[derive(Debug, Clone)]
pub struct DiffInfo {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub file_stats: Vec<FileStat>,
}

#[derive(Debug, Clone)]
pub struct FileStat {
    pub filename: String,
    pub insertions: usize,
    pub deletions: usize,
}

impl DiffInfo {
    pub fn parse(diff_text: &str) -> Result<Self> {
        let mut files_changed = 0;
        let mut total_insertions = 0;
        let mut total_deletions = 0;
        let mut file_stats = Vec::new();
        
        for line in diff_text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 3 {
                continue;
            }
            
            let insertions = parts[0].parse::<usize>().unwrap_or(0);
            let deletions = parts[1].parse::<usize>().unwrap_or(0);
            let filename = parts[2].to_string();
            
            files_changed += 1;
            total_insertions += insertions;
            total_deletions += deletions;
            
            file_stats.push(FileStat {
                filename,
                insertions,
                deletions,
            });
        }
        
        Ok(DiffInfo {
            files_changed,
            insertions: total_insertions,
            deletions: total_deletions,
            file_stats,
        })
    }
    
    pub fn display(&self) -> String {
        if self.files_changed == 0 {
            return "  No changes in diff".to_string();
        }
        
        let mut output = format!(
            "  {} files changed, {} insertions(+), {} deletions(-)\n",
            self.files_changed, self.insertions, self.deletions
        );
        
        for stat in &self.file_stats {
            if stat.insertions > 0 || stat.deletions > 0 {
                output.push_str(&format!(
                    "    {}: +{} -{}\n",
                    stat.filename, stat.insertions, stat.deletions
                ));
            }
        }
        
        output
    }
}
