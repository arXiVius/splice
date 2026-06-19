use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::fs;
use crate::ffmpeg;

pub fn handle_trim(
    input: PathBuf,
    start: String,
    end: String,
    output: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    // Generate fallback filename if not specified (e.g., input_trimmed.mp4)
    let out_file = output.unwrap_or_else(|| {
        let mut out = input.clone();
        if let Some(stem) = input.file_stem() {
            let mut new_name = stem.to_os_string();
            new_name.push("_trimmed");
            out.set_file_name(new_name);
            if let Some(ext) = input.extension() {
                out.set_extension(ext);
            }
        }
        out
    });

    let args = vec![
        "-y",
        "-ss", &start,
        "-to", &end,
        "-i", input.to_str().context("Invalid input path")?,
        "-c", "copy", // Lossless cut (avoids re-encoding)
        out_file.to_str().context("Invalid output path")?,
    ];

    if dry_run {
        println!("{}", console::style("Dry Run: ffmpeg command syntax:").yellow().bold());
        println!("ffmpeg {}", args.join(" "));
        return Ok(());
    }

    println!("🎬 Trimming video: {} -> {}", input.display(), out_file.display());

    // Calculate the target clip duration dynamically based on parsed timestamps
    let start_sec = parse_time_to_seconds(&start).unwrap_or(0);
    let mut target_duration = if let Some(end_sec) = parse_time_to_seconds(&end) {
        if end_sec > start_sec {
            end_sec - start_sec
        } else {
            0
        }
    } else {
        0
    };

    if target_duration == 0 {
        // Fallback to reading the full file duration if parsing fails
        target_duration = ffmpeg::get_duration_seconds(&input).unwrap_or(60);
    }

    ffmpeg::run_ffmpeg_with_progress(args, target_duration, "Processing")?;
    print_size_comparison(&input, &out_file)?;

    Ok(())
}

pub fn handle_gif(
    input: PathBuf,
    fps: u32,
    width: u32,
    output: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let out_file = output.unwrap_or_else(|| {
        let mut out = input.clone();
        out.set_extension("gif");
        out
    });

    // Two-pass palette formulation for clean GIF scaling (prevents dynamic color banding)
    let filter_complex = format!(
        "fps={},scale={}:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
        fps, width
    );

    let args = vec![
        "-y",
        "-i", input.to_str().context("Invalid input path")?,
        "-vf", &filter_complex,
        out_file.to_str().context("Invalid output path")?,
    ];

    if dry_run {
        println!("{}", console::style("Dry Run: ffmpeg command syntax:").yellow().bold());
        println!("ffmpeg {}", args.join(" "));
        return Ok(());
    }

    println!("🎨 Creating high-quality GIF: {} -> {}", input.display(), out_file.display());

    let source_duration = ffmpeg::get_duration_seconds(&input).unwrap_or(60);

    ffmpeg::run_ffmpeg_with_progress(args, source_duration, "Generating GIF")?;
    print_size_comparison(&input, &out_file)?;

    Ok(())
}

pub fn handle_compress(
    input: PathBuf,
    target: String,
    output: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let out_file = output.unwrap_or_else(|| {
        let mut out = input.clone();
        if let Some(stem) = input.file_stem() {
            let mut new_name = stem.to_os_string();
            new_name.push("_compressed");
            out.set_file_name(new_name);
            if let Some(ext) = input.extension() {
                out.set_extension(ext);
            }
        }
        out
    });

    // Sanitize and parse target size (e.g., "25MB", "10mb" -> raw bytes)
    let target_clean = target
        .to_uppercase()
        .replace("MB", "")
        .replace("M", "")
        .trim()
        .to_string();
    let target_mb = target_clean
        .parse::<f64>()
        .context("Invalid target size format. Use format like '25MB' or '10MB'.")?;
    let target_bytes = target_mb * 1024.0 * 1024.0;

    // Read total duration in seconds to calculate the matching bitrates
    let duration_secs = ffmpeg::get_duration_seconds(&input)?;
    if duration_secs == 0 {
        return Err(anyhow::anyhow!("Could not determine video duration."));
    }

    // Formula: Total Bitrate (bits/sec) = (Target Bytes * 8) / Duration
    let total_bitrate_bps = (target_bytes * 8.0) / duration_secs as f64;
    
    // Allocate standard 128 kbps for the audio stream
    let audio_bitrate_bps = 128.0 * 1024.0;
    let video_bitrate_bps = total_bitrate_bps - audio_bitrate_bps;

    if video_bitrate_bps <= 0.0 {
        return Err(anyhow::anyhow!("The target size is too small for this video's length."));
    }

    let video_bitrate_kbps = (video_bitrate_bps / 1024.0).round() as u64;

    // String references for command parsing
    let video_bitrate_str = format!("{}k", video_bitrate_kbps);

    // Compress video using H.264 (AVC) and AAC audio
    let args = vec![
        "-y",
        "-i", input.to_str().context("Invalid input path")?,
        "-b:v", &video_bitrate_str,
        "-b:a", "128k",
        "-vcodec", "libx264",
        "-acodec", "aac",
        out_file.to_str().context("Invalid output path")?,
    ];

    if dry_run {
        println!("{}", console::style("Dry Run: ffmpeg command syntax:").yellow().bold());
        println!("ffmpeg {}", args.join(" "));
        return Ok(());
    }

    println!(
        "📉 Compressing video: {} -> {} (Target: {})",
        input.display(),
        out_file.display(),
        target
    );

    ffmpeg::run_ffmpeg_with_progress(args, duration_secs, "Compressing")?;
    print_size_comparison(&input, &out_file)?;

    Ok(())
}

pub fn handle_extract_audio(
    input: PathBuf,
    format: String,
    output: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let ext = format.trim().to_lowercase();
    let out_file = output.unwrap_or_else(|| {
        let mut out = input.clone();
        out.set_extension(&ext);
        out
    });

    // -vn instructs ffmpeg to disable the video stream completely
    let args = vec![
        "-y",
        "-i", input.to_str().context("Invalid input path")?,
        "-vn",
        out_file.to_str().context("Invalid output path")?,
    ];

    if dry_run {
        println!("{}", console::style("Dry Run: ffmpeg command syntax:").yellow().bold());
        println!("ffmpeg {}", args.join(" "));
        return Ok(());
    }

    println!(
        "🎵 Extracting audio stream ({}): {} -> {}",
        ext.to_uppercase(),
        input.display(),
        out_file.display()
    );

    let source_duration = ffmpeg::get_duration_seconds(&input).unwrap_or(60);

    ffmpeg::run_ffmpeg_with_progress(args, source_duration, "Extracting Audio")?;
    print_size_comparison(&input, &out_file)?;

    Ok(())
}

/// Formats and prints a comparison table showing before/after sizing metrics.
fn print_size_comparison(input_path: &Path, output_path: &Path) -> Result<()> {
    let input_metadata = fs::metadata(input_path)
        .context("Failed to read input file metadata")?;
    let output_metadata = fs::metadata(output_path)
        .context("Failed to read output file metadata")?;

    let input_bytes = input_metadata.len();
    let output_bytes = output_metadata.len();

    let input_mb = input_bytes as f64 / 1_048_576.0;
    let output_mb = output_bytes as f64 / 1_048_576.0;

    let pct_change = if input_bytes > 0 {
        ((output_bytes as f64 - input_bytes as f64) / input_bytes as f64) * 100.0
    } else {
        0.0
    };

    println!("\n✨ Processed successfully!");
    println!(
        "┌───────────────────────┬─────────────┬─────────────┬──────────────┐"
    );
    println!(
        "│ {:<21} │ {:<11} │ {:<11} │ {:<12} │",
        "Metric", "Original", "Spliced", "Change"
    );
    println!(
        "├───────────────────────┼─────────────┼─────────────┼──────────────┤"
    );
    
    let change_str = if pct_change < 0.0 {
        format!("{:.1}% 📉", pct_change)
    } else {
        format!("+{:.1}% 📈", pct_change)
    };

    println!(
        "│ {:<21} │ {:>8.2} MB │ {:>8.2} MB │ {:<12} │",
        "File Size", input_mb, output_mb, change_str
    );
    println!(
        "└───────────────────────┴─────────────┴─────────────┴──────────────┘"
    );
    println!("Saved to: {}\n", console::style(output_path.display()).cyan().bold());

    Ok(())
}

/// Parses human-readable timestamps into total seconds.
/// Supports "SS", "MM:SS", and "HH:MM:SS" formats.
fn parse_time_to_seconds(t: &str) -> Option<u64> {
    // Try to parse as a direct integer first (e.g., "120")
    if let Ok(secs) = t.parse::<u64>() {
        return Some(secs);
    }

    // Split by colon (e.g., "01:30" or "02:15:30")
    let parts: Vec<&str> = t.split(':').collect();
    match parts.len() {
        // MM:SS format
        2 => {
            let mins = parts[0].parse::<u64>().ok()?;
            let secs = parts[1].parse::<u64>().ok()?;
            Some((mins * 60) + secs)
        }
        // HH:MM:SS format
        3 => {
            let hours = parts[0].parse::<u64>().ok()?;
            let mins = parts[1].parse::<u64>().ok()?;
            let secs = parts[2].parse::<u64>().ok()?;
            Some((hours * 3600) + (mins * 60) + secs)
        }
        _ => None,
    }
}

use dialoguer::{theme::ColorfulTheme, Input as DialogInput, Select};

// Add this function to the bottom of src/commands.rs:

/// Guides the user through a TUI wizard when splice is run without arguments.
pub fn handle_interactive(dry_run: bool) -> Result<()> {
    println!("{}", console::style("⚡ Welcome to Splice Interactive Wizard ⚡").cyan().bold());

    let actions = vec![
        "Trim Video",
        "Convert to GIF",
        "Compress Video",
        "Extract Audio",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What action would you like to perform?")
        .items(&actions)
        .default(0)
        .interact()?;

    // Every command needs an input file
    let input_str: String = DialogInput::with_theme(&ColorfulTheme::default())
        .with_prompt("Path to input video file")
        .validate_with(|input: &String| -> Result<(), &str> {
            let path = Path::new(input);
            if path.exists() && path.is_file() {
                Ok(())
            } else {
                Err("File does not exist or is not a valid file path. Please try again.")
            }
        })
        .interact_text()?;

    let input_path = PathBuf::from(input_str);

    match selection {
        // Trim Video
        0 => {
            let start: String = DialogInput::with_theme(&ColorfulTheme::default())
                .with_prompt("Start timestamp")
                .default("00:00".to_string())
                .interact_text()?;

            let end: String = DialogInput::with_theme(&ColorfulTheme::default())
                .with_prompt("End timestamp")
                .default("00:10".to_string())
                .interact_text()?;

            handle_trim(input_path, start, end, None, dry_run)?;
        }
        // Convert to GIF
        1 => {
            let fps: u32 = DialogInput::with_theme(&ColorfulTheme::default())
                .with_prompt("Target FPS (Frames Per Second)")
                .default(15)
                .interact_text()?;

            let width: u32 = DialogInput::with_theme(&ColorfulTheme::default())
                .with_prompt("Target Width (aspect ratio maintained)")
                .default(480)
                .interact_text()?;

            handle_gif(input_path, fps, width, None, dry_run)?;
        }
        // Compress Video
        2 => {
            let target: String = DialogInput::with_theme(&ColorfulTheme::default())
                .with_prompt("Target file size")
                .default("25MB".to_string())
                .interact_text()?;

            handle_compress(input_path, target, None, dry_run)?;
        }
        // Extract Audio
        3 => {
            let format: String = DialogInput::with_theme(&ColorfulTheme::default())
                .with_prompt("Audio format")
                .default("mp3".to_string())
                .interact_text()?;

            handle_extract_audio(input_path, format, None, dry_run)?;
        }
        _ => unreachable!(),
    }

    Ok(())
}