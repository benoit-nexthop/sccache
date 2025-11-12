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

