use crate::error::CoreError;
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Upper bound on frames extracted from one video, so a long video with many cuts
/// can't blow up the per-video cost.
pub const MAX_FRAMES_PER_VIDEO: usize = 40;

const SCENE_THRESHOLD: &str = "0.3";
const MAX_FRAME_WIDTH: u32 = 720;

/// Extracts the first frame plus every scene-change frame (capped at `max_frames`) as PNG
/// bytes with their offset in milliseconds. Frames are written to a fresh temp dir that is
/// removed on return; ffmpeg is killed if it exceeds `timeout_secs`.
pub async fn extract_scene_frames(
    ffmpeg_bin: &str,
    video_path: &Path,
    timeout_secs: u64,
    max_frames: usize,
) -> Result<Vec<(u64, Vec<u8>)>> {
    let mut header = [0u8; 16];
    let read = tokio::fs::File::open(video_path)
        .await
        .with_context(|| format!("opening {}", video_path.display()))?
        .read(&mut header)
        .await?;
    if !crate::media::is_video(&header[..read]) {
        return Err(CoreError::InvalidMedia("not a supported video container".into()).into());
    }

    let out_dir = tempfile::tempdir()?;
    // Frame 0 is always selected, otherwise a video without cuts would yield nothing.
    let filter = format!("select='eq(n,0)+gt(scene,{SCENE_THRESHOLD})',scale='min({MAX_FRAME_WIDTH},iw)':-1,showinfo");
    let output = Command::new(ffmpeg_bin)
        .arg("-nostdin")
        .arg("-hide_banner")
        .args(["-loglevel", "info"])
        .arg("-i")
        .arg(video_path)
        .args(["-map", "0:v:0", "-an", "-sn", "-dn"])
        .args(["-vf", &filter])
        .args(["-fps_mode", "vfr"])
        .args(["-frames:v", &max_frames.to_string()])
        .arg(out_dir.path().join("frame_%04d.png"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let output = match timeout(Duration::from_secs(timeout_secs), output).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => bail!("ffmpeg spawn failed ({ffmpeg_bin}): {e}"),
        Err(_) => bail!("ffmpeg timed out after {timeout_secs}s"),
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
        return Err(CoreError::InvalidMedia(format!("ffmpeg exited with {}: {tail}", output.status)).into());
    }

    // showinfo logs one line per frame that passed `select`, in output order.
    let offsets = parse_showinfo_offsets(&stderr);

    let mut frames = Vec::new();
    let mut entries = tokio::fs::read_dir(out_dir.path()).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let index = name
            .to_str()
            .and_then(|n| n.strip_prefix("frame_"))
            .and_then(|n| n.strip_suffix(".png"))
            .and_then(|n| n.parse::<usize>().ok());
        if let Some(index) = index {
            frames.push((index, entry.path()));
        }
    }
    frames.sort_unstable_by_key(|(index, _)| *index);

    let mut result = Vec::with_capacity(frames.len());
    for (index, path) in frames.into_iter().take(max_frames) {
        let offset_ms = offsets.get(index.saturating_sub(1)).copied().unwrap_or(0);
        result.push((offset_ms, tokio::fs::read(&path).await?));
    }
    Ok(result)
}

fn parse_showinfo_offsets(stderr: &str) -> Vec<u64> {
    stderr
        .lines()
        .filter(|line| line.contains("Parsed_showinfo"))
        .filter_map(|line| {
            let rest = &line[line.find("pts_time:")? + "pts_time:".len()..];
            let value = rest.split_whitespace().next()?;
            let seconds: f64 = value.parse().ok()?;
            Some((seconds.max(0.0) * 1000.0).round() as u64)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_showinfo_lines() {
        let stderr = "\
Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'in.mp4':
[Parsed_showinfo_2 @ 0x5581] n:   0 pts:      0 pts_time:0       duration:512 fmt:yuv420p
[Parsed_showinfo_2 @ 0x5581] color_range:tv color_space:bt709
[Parsed_showinfo_2 @ 0x5581] n:   1 pts: 123904 pts_time:8.06667 duration:512 fmt:yuv420p
[Parsed_showinfo_2 @ 0x5581] n:   2 pts: 200000 pts_time:13.0205 duration:512
";
        assert_eq!(parse_showinfo_offsets(stderr), vec![0, 8067, 13021]);
    }

    #[tokio::test]
    async fn rejects_non_video_before_running_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.mp4");
        tokio::fs::write(&path, b"#!/bin/sh\nrm -rf /\n").await.unwrap();
        let err = extract_scene_frames("definitely-not-ffmpeg", &path, 5, 4).await.unwrap_err();
        assert!(crate::error::is_invalid_media(&err));
    }
}
