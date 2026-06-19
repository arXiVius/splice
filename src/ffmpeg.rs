use anyhow::{anyhow, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

/// Runs an FFmpeg command and shows a progress bar.
pub fn run_ffmpeg_with_progress(
    args: Vec<&str>,
    total_duration_secs: u64,
    message: &str,
) -> Result<()> {
    let pb = ProgressBar::new(total_duration_secs);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {percent}% (ETA: {eta})")?
            .progress_chars("#>-")
    );
    pb.set_message(message.to_string());

    // We pipe ffmpeg's internal progress status directly into stdout (pipe:1).
    // Stderr is set to Stdio::null() so standard ffmpeg logs do not mess up our CLI UI.
    let mut child = Command::new("ffmpeg")
        .args(&args)
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostats")
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) 
        .spawn()?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("out_time_us=") {
            if let Some(val_str) = line.split('=').nth(1) {
                if let Ok(us) = val_str.trim().parse::<u64>() {
                    let secs = us / 1_000_000;
                    if secs <= total_duration_secs {
                        pb.set_position(secs);
                    } else {
                        pb.set_position(total_duration_secs);
                    }
                }
            }
        } else if line == "progress=end" {
            pb.finish_with_message("Done!");
        }
    }

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("FFmpeg exited with an error status."));
    }

    Ok(())
}

/// Uses ffprobe to query the input file's actual duration in seconds.
pub fn get_duration_seconds(path: &Path) -> Result<u64> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to read file duration with ffprobe."));
    }

    let duration_str = String::from_utf8_lossy(&output.stdout);
    let duration_secs = duration_str.trim().parse::<f64>()?.round() as u64;

    Ok(duration_secs)
}