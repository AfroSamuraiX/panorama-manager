//! Media inspection and ffmpeg-driven conversion to MP4.
//!
//! Shells out to `ffmpeg` via `std::process::Command`; no shell quoting needed.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TMP_DIR_NAME: &str = "tryx-panorama-mgr";

const SCALE_FILTER: &str = "scale=trunc(iw/2)*2:trunc(ih/2)*2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Unknown,
    Video,
    Gif,
    Image,
}

/// Lower-case file extension including the leading dot, or "" if none.
pub fn normalize_ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|os| format!(".{}", os.to_string_lossy().to_ascii_lowercase()))
        .unwrap_or_default()
}

pub fn detect_type(path: &str) -> MediaType {
    match normalize_ext(path).as_str() {
        ".mp4" | ".webm" | ".mkv" | ".avi" | ".mov" => MediaType::Video,
        ".gif" => MediaType::Gif,
        ".jpg" | ".jpeg" | ".png" | ".bmp" | ".webp" => MediaType::Image,
        _ => MediaType::Unknown,
    }
}

pub fn needs_conversion(path: &str) -> bool {
    matches!(
        normalize_ext(path).as_str(),
        ".webm" | ".mkv" | ".avi" | ".mov" | ".gif"
    )
}

pub fn get_basename(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|os| os.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn get_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|os| os.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn get_converted_name(original: &str) -> String {
    format!("{}.mp4", get_basename(original))
}

pub fn tmp_dir() -> PathBuf {
    std::env::temp_dir().join(TMP_DIR_NAME)
}

pub fn tmp_file(filename: impl AsRef<Path>) -> PathBuf {
    tmp_dir().join(filename)
}

pub fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Convert a non-MP4 video into a libx264-encoded MP4.
pub fn convert_to_mp4(input: &str, output: &str) -> bool {
    let _ = std::fs::create_dir_all(tmp_dir());

    let ok = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input,
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "23",
            "-movflags",
            "faststart",
            "-pix_fmt",
            "yuv420p",
            "-vf",
            SCALE_FILTER,
            "-an",
            output,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    ok && PathBuf::from(output).exists()
}

/// Convert a GIF into an MP4 suitable for the device. Skips libx264 flags —
/// only pixel-format normalization and even-dimension scaling are applied.
pub fn convert_gif_to_mp4(input: &str, output: &str) -> bool {
    let _ = std::fs::create_dir_all(tmp_dir());

    let ok = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input,
            "-movflags",
            "faststart",
            "-pix_fmt",
            "yuv420p",
            "-vf",
            SCALE_FILTER,
            output,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    ok && PathBuf::from(output).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ext_lowercases() {
        assert_eq!(normalize_ext("VIDEO.MP4"), ".mp4");
        assert_eq!(normalize_ext("frame.PNG"), ".png");
        assert_eq!(normalize_ext("clip.WeBm"), ".webm");
    }

    #[test]
    fn normalize_ext_handles_no_extension() {
        assert_eq!(normalize_ext("noext"), "");
        assert_eq!(normalize_ext(""), "");
    }

    #[test]
    fn detect_type_classifies_videos() {
        assert_eq!(detect_type("a.mp4"), MediaType::Video);
        assert_eq!(detect_type("a.webm"), MediaType::Video);
        assert_eq!(detect_type("a.mkv"), MediaType::Video);
        assert_eq!(detect_type("a.avi"), MediaType::Video);
        assert_eq!(detect_type("a.mov"), MediaType::Video);
    }

    #[test]
    fn detect_type_classifies_images_and_gifs() {
        assert_eq!(detect_type("a.gif"), MediaType::Gif);
        assert_eq!(detect_type("a.png"), MediaType::Image);
        assert_eq!(detect_type("a.JPG"), MediaType::Image);
        assert_eq!(detect_type("a.jpeg"), MediaType::Image);
        assert_eq!(detect_type("a.bmp"), MediaType::Image);
        assert_eq!(detect_type("a.webp"), MediaType::Image);
    }

    #[test]
    fn detect_type_returns_unknown_for_other_extensions() {
        assert_eq!(detect_type("a.txt"), MediaType::Unknown);
        assert_eq!(detect_type("noext"), MediaType::Unknown);
    }

    #[test]
    fn needs_conversion_only_for_non_mp4_videos_and_gifs() {
        assert!(needs_conversion("a.webm"));
        assert!(needs_conversion("a.mkv"));
        assert!(needs_conversion("a.avi"));
        assert!(needs_conversion("a.mov"));
        assert!(needs_conversion("a.gif"));
        assert!(!needs_conversion("a.mp4"));
        assert!(!needs_conversion("a.png"));
        assert!(!needs_conversion("a.txt"));
    }

    #[test]
    fn basename_strips_directory_and_extension() {
        assert_eq!(get_basename("/foo/bar/video.mp4"), "video");
        assert_eq!(get_basename("video.mp4"), "video");
        assert_eq!(get_basename("/foo/noext"), "noext");
    }

    #[test]
    fn filename_strips_directory_only() {
        assert_eq!(get_filename("/foo/bar/video.mp4"), "video.mp4");
        assert_eq!(get_filename("video.mp4"), "video.mp4");
    }

    #[test]
    fn converted_name_replaces_extension_with_mp4() {
        assert_eq!(get_converted_name("/foo/clip.webm"), "clip.mp4");
        assert_eq!(get_converted_name("anim.gif"), "anim.mp4");
        assert_eq!(get_converted_name("/path/to/raw.mov"), "raw.mp4");
    }

    #[test]
    fn tmp_file_joins_under_panorama_temp_dir() {
        assert_eq!(tmp_file("clip.mp4"), tmp_dir().join("clip.mp4"));
    }
}
