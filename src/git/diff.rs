use anyhow::Result;
use std::collections::BTreeMap;

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
        let mut file_totals: BTreeMap<String, (usize, usize)> = BTreeMap::new();

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

            let entry = file_totals.entry(filename).or_default();
            entry.0 += insertions;
            entry.1 += deletions;
        }

        let file_stats: Vec<FileStat> = file_totals
            .into_iter()
            .map(|(filename, (insertions, deletions))| FileStat {
                filename,
                insertions,
                deletions,
            })
            .collect();

        let files_changed = file_stats.len();
        let total_insertions = file_stats.iter().map(|stat| stat.insertions).sum();
        let total_deletions = file_stats.iter().map(|stat| stat.deletions).sum();

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
