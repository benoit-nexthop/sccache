# Translation Unit Statistics

This feature allows sccache to collect detailed statistics about the translation units it compiles.

## What is Collected

For each compilation, sccache records:

- **Input file**: The path to the source file being compiled
- **Preprocessed size**: The size of the translation unit after preprocessing (in bytes)
- **Number of includes**: The count of files that were `#include`d during preprocessing
- **Preprocessing time**: How long it took to preprocess the file
- **Compilation time**: How long it took to compile the file
- **Distributed compilation**: Whether the compilation was distributed to a remote server
- **Retry count**: For distributed compilations with `retry_on_busy` enabled, how many retry attempts were needed
- **Timestamp**: When the compilation occurred

## Configuration

To enable translation unit statistics collection, add the following to your sccache configuration file:

```toml
[translation_unit_stats]
enabled = true
stats_file = "/path/to/tu_stats.db"  # Optional, defaults to sccache cache dir
```

## Building with TU Stats Support

This feature requires building sccache with the `translation-unit-stats` feature flag:

```bash
cargo build --release --features translation-unit-stats
```

## Storage Backend

The statistics are stored using [fjall](https://github.com/fjall-rs/fjall), a log-structured, embeddable key-value storage engine written in Rust. Fjall was chosen because it:

- Handles concurrent writes safely
- Is embeddable (no separate database server needed)
- Has good performance characteristics for write-heavy workloads
- Is written in pure Rust

## Querying the Statistics

### Using the Built-in Command

The easiest way to query statistics is using the built-in `--tu-stats` command:

```bash
# Show statistics in human-readable format
sccache --tu-stats

# Export statistics to CSV format
sccache --tu-stats --tu-stats-csv > stats.csv

# Query a specific database file
sccache --tu-stats --tu-stats-file /path/to/tu_stats.db

# Export specific database to CSV
sccache --tu-stats --tu-stats-csv --tu-stats-file /path/to/tu_stats.db > stats.csv
```

The CSV format includes the following columns:
- `timestamp` - Unix timestamp when the compilation occurred
- `input_file` - Path to the source file
- `preprocessed_size` - Size of the preprocessed output in bytes
- `num_includes` - Number of files included
- `preprocess_duration_ms` - Preprocessing time in milliseconds
- `compile_duration_ms` - Compilation time in milliseconds
- `dist_retry_count` - Number of retry attempts for distributed compilation
- `is_distributed` - Whether the compilation was distributed (true/false)
- `top1_by_count`, `top1_count`, `top1_lines` - Top include path prefix by frequency (path, file count, line count)
- `top2_by_count`, `top2_count`, `top2_lines` - Second most frequent include path prefix
- `top3_by_count`, `top3_count`, `top3_lines` - Third most frequent include path prefix
- `top1_by_size`, `top1_lines`, `top1_count` - Top include path prefix by size contribution (path, line count, file count)
- `top2_by_size`, `top2_lines`, `top2_count` - Second largest include path prefix by size
- `top3_by_size`, `top3_lines`, `top3_count` - Third largest include path prefix by size

### Programmatic Access

You can also query the statistics programmatically using the fjall library:

```rust
use fjall::{Config, Keyspace};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Debug, Serialize, Deserialize)]
struct TranslationUnitStats {
    input_file: PathBuf,
    preprocessed_size: usize,
    num_includes: usize,
    preprocess_duration: Duration,
    compile_duration: Duration,
    dist_retry_count: u32,
    is_distributed: bool,
    timestamp: SystemTime,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let keyspace = Config::new("/path/to/tu_stats.db").open()?;
    let partition = keyspace.open_partition("tu_stats", Default::default())?;

    // Iterate over all statistics
    for item in partition.iter() {
        let (key, value) = item?;
        let stats: TranslationUnitStats = serde_json::from_slice(&value)?;
        println!("{:?}: {} bytes, {} includes, {:?} preprocess, {:?} compile",
            stats.input_file,
            stats.preprocessed_size,
            stats.num_includes,
            stats.preprocess_duration,
            stats.compile_duration
        );
    }

    Ok(())
}
```

Note: The statistics are stored as JSON (using `serde_json`), not bincode.

## Understanding Include Statistics

The statistics include detailed information about which parts of your codebase contribute most to translation unit size:

### Top Includes by Count
Shows which path prefixes have the most individual files included. For example, if you see:
- `fboss/fsdb/tests` - 200 files
- `external/folly/io` - 150 files

This means 200 different header files from `fboss/fsdb/tests/` were included in the translation unit.

### Top Includes by Size
Shows which path prefixes contribute the most lines to the preprocessed output. For example:
- `external/folly/io` - 50,000 lines
- `fboss/fsdb/tests` - 30,000 lines

This means headers from `external/folly/io/` contributed 50,000 lines of preprocessed code, even if there were fewer individual files.

**Key insight:** A path prefix with high line count but low file count indicates large, heavyweight headers. A path prefix with high file count but low line count indicates many small headers.

## Analyzing Statistics with Standard Tools

Once you've exported the statistics to CSV, you can analyze them using standard Linux tools:

```bash
# Export to CSV
sccache --tu-stats --tu-stats-csv > stats.csv

# Find the 10 largest preprocessed files
sort -t, -k3 -nr stats.csv | head -11

# Find files with the most includes
sort -t, -k4 -nr stats.csv | head -11

# Find the slowest compilations
sort -t, -k6 -nr stats.csv | head -11

# Count distributed vs local compilations
echo "Distributed: $(grep -c ',true$' stats.csv)"
echo "Local: $(grep -c ',false$' stats.csv)"

# Average preprocessing time (requires awk)
awk -F, 'NR>1 {sum+=$5; count++} END {print "Average preprocess time:", sum/count, "ms"}' stats.csv

# Average compilation time
awk -F, 'NR>1 {sum+=$6; count++} END {print "Average compile time:", sum/count, "ms"}' stats.csv

# Find most common heavy hitters (top include paths by size)
cut -d, -f19 stats.csv | tail -n +2 | sort | uniq -c | sort -rn | head -10

# Find most common include path prefixes (by count)
cut -d, -f10 stats.csv | tail -n +2 | sort | uniq -c | sort -rn | head -10
```

You can also import the CSV into spreadsheet applications (Excel, LibreOffice Calc, Google Sheets) or data analysis tools (Python pandas, R, etc.) for more sophisticated analysis.

## Use Cases

This feature is useful for:

- **Build performance analysis**: Identify which files take the longest to compile
- **Include dependency analysis**: Find files with excessive `#include` directives
- **Distributed compilation monitoring**: Track retry rates and distributed vs local compilation
- **Build optimization**: Identify opportunities to reduce preprocessing overhead
- **Capacity planning**: Understand compilation workload characteristics

## Performance Impact

When enabled, the feature adds minimal overhead:

- Statistics collection happens during normal compilation flow
- Database writes are asynchronous and don't block compilation
- The fjall storage engine is optimized for write-heavy workloads

When disabled (the default), there is zero overhead as the code is conditionally compiled out.

