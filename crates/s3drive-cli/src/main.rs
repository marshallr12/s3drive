use clap::{Parser, Subcommand};
use s3drive_core::Config;
use std::{error::Error, fs, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "S3Drive research scaffold (mounts and S3 I/O are not implemented)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a local configuration without contacting a provider.
    ConfigCheck { config: PathBuf },
    /// Show which bucket/key a slash-separated virtual file path would use.
    Resolve { config: PathBuf, path: String },
    /// Create or reopen a local SQLite metadata database (no network access).
    InitState {
        #[arg(long)]
        directory: PathBuf,
    },
    /// Display implemented and planned components, without probing the host.
    Status,
    /// Reserved for future adapters; currently fails without mounting anything.
    Mount,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    match Cli::parse().command {
        Command::ConfigCheck { config } => {
            let config = Config::parse(&fs::read_to_string(config)?)?;
            println!(
                "Valid read-only configuration: {} source(s). No provider connection attempted.",
                config.sources.len()
            );
        }
        Command::Resolve { config, path } => {
            let config = Config::parse(&fs::read_to_string(config)?)?;
            let location = config.resolve(&path)?;
            println!(
                "alias={} bucket={} key={}",
                location.alias, location.bucket, location.key
            );
        }
        Command::InitState { directory } => {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&directory)?;
            let state = s3drive_state::State::open(&directory.join("metadata.sqlite3"))?;
            let info = state.info()?;
            println!(
                "Local state ready: schema={}, journal={}, SQLite={}. No file data or uploads created.",
                info.schema_version, info.journal_mode, info.sqlite_version
            );
        }
        Command::Status => {
            println!("Phase P0 research scaffold; host={}", std::env::consts::OS);
            println!("Implemented: config validation, namespace planning, local SQLite bootstrap.");
            println!("S3: {}", s3drive_s3::IMPLEMENTATION_STATUS);
            println!("Linux: {}", s3drive_fuse::IMPLEMENTATION_STATUS);
            println!("Windows: {}", s3drive_winfsp::IMPLEMENTATION_STATUS);
            println!("See docs/src/implementation-plan.md for the acceptance gates.");
        }
        Command::Mount => {
            #[cfg(target_os = "linux")]
            s3drive_fuse::mount()?;
            #[cfg(target_os = "windows")]
            s3drive_winfsp::mount()?;
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            return Err("only Linux and Windows adapters are planned".into());
        }
    }
    Ok(())
}
