mod ffmpeg;
mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use commands::{handle_trim, handle_gif, handle_compress, handle_extract_audio, handle_interactive};

#[derive(Parser)]
#[command(name = "splice")]
#[command(about = "A sane, beautiful wrapper around ffmpeg", long_about = None)]
struct Cli {
    /// Show the exact ffmpeg command that would run without executing it
    #[arg(short, long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Option<Commands>, // Wrapped in Option to make subcommands optional
}

#[derive(Subcommand)]
enum Commands {
    /// Trim a video file from a start time to an end time
    Trim {
        /// Path to the input video
        input: PathBuf,
        /// Start time (e.g., 00:10, 1:30, or 90)
        start: String,
        /// End time or duration (e.g., 00:25, or 120)
        end: String,
        /// Output path (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Convert a video clip to a high-quality GIF
    Gif {
        input: PathBuf,
        /// Frames per second
        #[arg(long, default_value_t = 15)]
        fps: u32,
        /// Target width (maintains aspect ratio)
        #[arg(long, default_value_t = 480)]
        width: u32,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compress a video to a target file size (e.g., 25MB)
    Compress {
        input: PathBuf,
        /// Target file size (e.g., "25MB" or "10MB")
        #[arg(long)]
        target: String,
        /// Output path (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Extract audio from a video file
    Audio {
        input: PathBuf,
        /// Audio format (mp3, wav, or m4a)
        #[arg(short, long, default_value = "mp3")]
        format: String,
        /// Output path (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    if !is_ffmpeg_installed() {
        eprintln!(
            "{}",
            console::style("Error: 'ffmpeg' binary not found in your PATH.")
                .red()
                .bold()
        );
        eprintln!("Please make sure ffmpeg is installed to use splice.");
        std::process::exit(1);
    }

    let result = match cli.command {
        Some(Commands::Trim { input, start, end, output }) => {
            handle_trim(input, start, end, output, cli.dry_run)
        }
        Some(Commands::Gif { input, fps, width, output }) => {
            handle_gif(input, fps, width, output, cli.dry_run)
        }
        Some(Commands::Compress { input, target, output }) => {
            handle_compress(input, target, output, cli.dry_run)
        }
        Some(Commands::Audio { input, format, output }) => {
            handle_extract_audio(input, format, output, cli.dry_run)
        }
        None => {
            // Trigger interactive CLI mode when no arguments are passed
            handle_interactive(cli.dry_run)
        }
    };

    if let Err(e) = result {
        eprintln!("{} {}", console::style("Error:").red().bold(), e);
        std::process::exit(1);
    }
}

fn is_ffmpeg_installed() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}