use clap::{Parser, Subcommand};
use s3drive_core::Config;
use s3drive_s3::S3Reader;
use std::{error::Error, fs, io::Write, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "S3Drive read-only S3 tools and filesystem mounts")]
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
    /// List one bounded page under a source; pass the returned token unchanged.
    List {
        config: PathBuf,
        source: String,
        #[arg(default_value = "")]
        directory: String,
        #[arg(long, default_value_t = 1000)]
        max_keys: u16,
        #[arg(long)]
        token: Option<String>,
    },
    /// Inspect a remote file's metadata without downloading its contents.
    Stat { config: PathBuf, path: String },
    /// Write a revision-pinned range of a remote file to standard output.
    Read {
        config: PathBuf,
        path: String,
        #[arg(long, default_value_t = 0)]
        offset: u64,
        #[arg(long, default_value_t = 65536)]
        length: usize,
    },
    /// Mount a read-only drive in the foreground (Linux FUSE or Windows WinFsp).
    Mount {
        config: PathBuf,
        mountpoint: PathBuf,
        /// Unmount when the supervising process closes standard input (Windows).
        #[cfg(windows)]
        #[arg(long)]
        unmount_on_stdin_close: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
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
            println!(
                "Read-only transport and Linux FUSE adapter; host={}",
                std::env::consts::OS
            );
            println!("Implemented: config validation, namespace planning, local SQLite bootstrap.");
            println!("S3: {}", s3drive_s3::IMPLEMENTATION_STATUS);
            println!("Linux: {}", s3drive_fuse::IMPLEMENTATION_STATUS);
            println!("Windows: {}", s3drive_winfsp::IMPLEMENTATION_STATUS);
            println!("See docs/src/implementation-plan.md for the acceptance gates.");
        }
        Command::List {
            config,
            source,
            directory,
            max_keys,
            token,
        } => {
            let config = Config::parse(&fs::read_to_string(config)?)?;
            let source = config
                .sources
                .iter()
                .find(|entry| entry.alias == source)
                .ok_or("unknown source alias")?;
            let reader = S3Reader::from_source(source).await?;
            let page = reader
                .list_page(&directory, token.as_deref(), max_keys)
                .await?;
            println!(
                "configuration-fingerprint={}",
                reader.configuration_fingerprint()
            );
            for directory in page.directories {
                println!("directory\t{directory:?}");
            }
            for object in page.objects {
                println!("object\t{}\t{:?}", object.size, object.key);
            }
            if let Some(token) = page.next_token {
                println!("next-token={token:?}");
            }
        }
        Command::Stat { config, path } => {
            let (reader, key) = reader_for_path(config, &path).await?;
            println!("{:#?}", reader.head(&key).await?);
        }
        Command::Read {
            config,
            path,
            offset,
            length,
        } => {
            let (reader, key) = reader_for_path(config, &path).await?;
            let handle = reader.open(&key).await?;
            let bytes = handle.read_range(offset, length).await?;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        Command::Mount {
            config,
            mountpoint,
            #[cfg(windows)]
            unmount_on_stdin_close,
        } => {
            #[cfg(target_os = "linux")]
            {
                let config = Config::parse(&fs::read_to_string(config)?)?;
                let filesystem = s3drive_fs::ReadOnlyFs::new(&config).await?;
                s3drive_fuse::run(filesystem, &mountpoint).await?;
            }
            #[cfg(target_os = "windows")]
            {
                let config = Config::parse(&fs::read_to_string(config)?)?;
                let filesystem = s3drive_fs::ReadOnlyFs::new(&config).await?;
                s3drive_winfsp::run(filesystem, &mountpoint, unmount_on_stdin_close).await?;
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                let _ = (config, mountpoint);
                return Err("only Linux and Windows adapters are planned".into());
            }
        }
    }
    Ok(())
}

async fn reader_for_path(
    config: PathBuf,
    path: &str,
) -> Result<(S3Reader, String), Box<dyn Error + Send + Sync>> {
    let config = Config::parse(&fs::read_to_string(config)?)?;
    let location = config.resolve(path)?;
    let source = config
        .sources
        .iter()
        .find(|source| source.alias == location.alias)
        .ok_or("unknown source alias")?;
    let key = location
        .key
        .strip_prefix(&source.prefix)
        .ok_or("key outside source prefix")?
        .to_owned();
    Ok((S3Reader::from_source(source).await?, key))
}
