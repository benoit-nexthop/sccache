// Copyright 2016 Mozilla Foundation
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Translation unit statistics collection and storage

use crate::errors::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Statistics about include path contributions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeStats {
    /// Path prefix (e.g., "fboss/fsdb/tests" or "external/folly/io")
    pub path_prefix: String,
    /// Number of files included from this prefix
    pub count: usize,
    /// Total lines of preprocessed output contributed by files from this prefix
    pub lines: usize,
}

/// Statistics about a translation unit compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationUnitStats {
    /// Path to the input source file
    pub input_file: PathBuf,
    /// Size of the preprocessed translation unit in bytes
    pub preprocessed_size: usize,
    /// Number of files included in the translation unit
    pub num_includes: usize,
    /// Time taken to preprocess the file
    pub preprocess_duration: Duration,
    /// Time taken to compile the file
    pub compile_duration: Duration,
    /// Number of retry attempts for distributed compilation (0 if not distributed or no retries)
    pub dist_retry_count: u32,
    /// Whether this was a distributed compilation
    pub is_distributed: bool,
    /// Top 10 include path prefixes by frequency
    pub top_includes_by_count: Vec<IncludeStats>,
    /// Top 10 include path prefixes by size contribution
    pub top_includes_by_size: Vec<IncludeStats>,
    /// Timestamp when the compilation occurred
    pub timestamp: std::time::SystemTime,
}

#[cfg(feature = "translation-unit-stats")]
mod storage {
    use super::*;
    use fjall::{Config, Keyspace, PartitionCreateOptions};
    use std::sync::Arc;

    /// Storage backend for translation unit statistics using fjall
    pub struct TuStatsStorage {
        keyspace: Arc<Keyspace>,
        partition_name: &'static str,
    }

    impl TuStatsStorage {
        /// Create a new statistics storage at the given path
        pub fn new(path: &Path) -> Result<Self> {
            let keyspace = Config::new(path)
                .open()
                .context("Failed to open fjall keyspace for TU stats")?;

            Ok(Self {
                keyspace: Arc::new(keyspace),
                partition_name: "tu_stats",
            })
        }

        /// Record statistics for a translation unit
        pub fn record(&self, stats: &TranslationUnitStats) -> Result<()> {
            let partition = self
                .keyspace
                .open_partition(self.partition_name, PartitionCreateOptions::default())
                .context("Failed to open partition for TU stats")?;

            // Use timestamp + input file as key to allow multiple compilations of the same file
            let key = format!(
                "{:?}:{}",
                stats.timestamp,
                stats.input_file.display()
            );

            let value = serde_json::to_vec(stats)
                .context("Failed to serialize TU stats")?;

            partition
                .insert(key.as_bytes(), &value)
                .context("Failed to insert TU stats")?;

            // Flush to ensure data is persisted
            self.keyspace.persist(fjall::PersistMode::SyncAll)
                .context("Failed to persist TU stats")?;

            Ok(())
        }

        /// Get all statistics (for querying/analysis)
        pub fn get_all(&self) -> Result<Vec<TranslationUnitStats>> {
            let partition = self
                .keyspace
                .open_partition(self.partition_name, PartitionCreateOptions::default())
                .context("Failed to open partition for TU stats")?;

            let mut stats = Vec::new();
            for item in partition.iter() {
                let (_key, value) = item.context("Failed to read TU stats entry")?;
                let stat: TranslationUnitStats = serde_json::from_slice(&value)
                    .context("Failed to deserialize TU stats")?;
                stats.push(stat);
            }

            Ok(stats)
        }
    }
}

#[cfg(feature = "translation-unit-stats")]
pub use storage::TuStatsStorage;

#[cfg(not(feature = "translation-unit-stats"))]
pub struct TuStatsStorage;

#[cfg(not(feature = "translation-unit-stats"))]
impl TuStatsStorage {
    pub fn new(_path: &Path) -> Result<Self> {
        Ok(Self)
    }

    pub fn record(&self, _stats: &TranslationUnitStats) -> Result<()> {
        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<TranslationUnitStats>> {
        Ok(Vec::new())
    }
}

/// Global statistics recorder
static GLOBAL_RECORDER: Lazy<Mutex<Option<TuStatsStorage>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the global TU stats recorder
pub fn init_recorder(config: &crate::config::TranslationUnitStatsConfig) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let stats_file = if let Some(ref path) = config.stats_file {
        path.clone()
    } else {
        // Default to sccache cache dir
        crate::config::default_disk_cache_dir().join("tu_stats.db")
    };

    let storage = TuStatsStorage::new(&stats_file)?;
    let mut recorder = GLOBAL_RECORDER.lock().unwrap();
    *recorder = Some(storage);
    Ok(())
}

/// Record translation unit statistics
pub fn record_stats(stats: TranslationUnitStats) {
    if let Ok(recorder) = GLOBAL_RECORDER.lock() {
        if let Some(ref storage) = *recorder {
            if let Err(e) = storage.record(&stats) {
                warn!("Failed to record TU stats: {}", e);
            }
        }
    }
}

/// Query all translation unit statistics from the database
pub fn query_stats(stats_file: Option<&Path>) -> Result<Vec<TranslationUnitStats>> {
    let db_path = if let Some(path) = stats_file {
        path.to_path_buf()
    } else {
        crate::config::default_disk_cache_dir().join("tu_stats.db")
    };

    let storage = TuStatsStorage::new(&db_path)?;
    storage.get_all()
}

/// Export statistics to CSV format
pub fn export_to_csv(stats: &[TranslationUnitStats]) -> String {
    let mut csv = String::new();

    // Header - include top 3 by count and top 3 by size
    csv.push_str("timestamp,input_file,preprocessed_size,num_includes,preprocess_duration_ms,compile_duration_ms,dist_retry_count,is_distributed,");
    csv.push_str("top1_by_count,top1_count,top1_lines,top2_by_count,top2_count,top2_lines,top3_by_count,top3_count,top3_lines,");
    csv.push_str("top1_by_size,top1_lines,top1_count,top2_by_size,top2_lines,top2_count,top3_by_size,top3_lines,top3_count\n");

    // Data rows
    for stat in stats {
        let timestamp = stat.timestamp
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}",
            timestamp,
            stat.input_file.display(),
            stat.preprocessed_size,
            stat.num_includes,
            stat.preprocess_duration.as_millis(),
            stat.compile_duration.as_millis(),
            stat.dist_retry_count,
            if stat.is_distributed { "true" } else { "false" }
        ));

        // Add top 3 by count
        for i in 0..3 {
            if let Some(inc) = stat.top_includes_by_count.get(i) {
                csv.push_str(&format!(",{},{},{}", inc.path_prefix, inc.count, inc.lines));
            } else {
                csv.push_str(",,,");
            }
        }

        // Add top 3 by size
        for i in 0..3 {
            if let Some(inc) = stat.top_includes_by_size.get(i) {
                csv.push_str(&format!(",{},{},{}", inc.path_prefix, inc.lines, inc.count));
            } else {
                csv.push_str(",,,");
            }
        }

        csv.push('\n');
    }

    csv
}

/// Print statistics in human-readable format
pub fn print_stats(stats: &[TranslationUnitStats]) {
    if stats.is_empty() {
        println!("No translation unit statistics found.");
        return;
    }

    println!("Translation Unit Statistics ({} entries):", stats.len());
    println!();

    for (i, stat) in stats.iter().enumerate() {
        println!("Entry {}:", i + 1);
        println!("  File:              {}", stat.input_file.display());
        println!("  Preprocessed size: {} bytes", stat.preprocessed_size);
        println!("  Includes:          {}", stat.num_includes);
        println!("  Preprocess time:   {:?}", stat.preprocess_duration);
        println!("  Compile time:      {:?}", stat.compile_duration);
        println!("  Distributed:       {}", if stat.is_distributed { "yes" } else { "no" });
        if stat.dist_retry_count > 0 {
            println!("  Retry count:       {}", stat.dist_retry_count);
        }

        // Show top includes by count
        if !stat.top_includes_by_count.is_empty() {
            println!("  Top includes by count:");
            for (j, inc) in stat.top_includes_by_count.iter().enumerate().take(5) {
                println!("    {}: {} ({} files, {} lines)", j + 1, inc.path_prefix, inc.count, inc.lines);
            }
        }

        // Show top includes by size
        if !stat.top_includes_by_size.is_empty() {
            println!("  Top includes by size:");
            for (j, inc) in stat.top_includes_by_size.iter().enumerate().take(5) {
                println!("    {}: {} ({} lines, {} files)", j + 1, inc.path_prefix, inc.lines, inc.count);
            }
        }

        println!("  Timestamp:         {:?}", stat.timestamp);
        println!();
    }
}

