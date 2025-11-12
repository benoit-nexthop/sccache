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

The statistics database can be queried programmatically using the fjall library. Here's an example:

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
        let stats: TranslationUnitStats = bincode::deserialize(&value)?;
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

