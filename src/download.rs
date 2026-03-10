use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

const MODEL_URL: &str = "https://huggingface.co/Sid77449/sift/resolve/main/model.gguf";

const MODEL_FILENAME: &str = "model.gguf";
const MODEL_FILENAME_LOCAL: &str = "model.gguf";
const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46]; // ASCII bytes for "GGUF"
const MIN_MODEL_SIZE: u64 = 400_000_000; // 400MB

/// Check if a file has valid GGUF magic bytes
fn has_valid_magic(path: &Path) -> bool {
    let Ok(mut f) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    if f.read_exact(&mut magic).is_err() {
        return false;
    }
    magic == GGUF_MAGIC
}

/// Check if model file exists and is valid
fn is_model_valid(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.len() >= MIN_MODEL_SIZE && has_valid_magic(path)
}

/// Ensure the model exists, searching multiple locations before downloading.
/// Search order:
///   1. `Qwen3.5-0.8B.Q4_K_M.gguf` in the current working directory
///   2. `Qwen3.5-0.8B.Q4_K_M.gguf` next to the running executable
///   3. `model.gguf` in `model_dir` (original behaviour)
/// If `force` is true, skip all local checks and re-download.
/// Returns the path to the model file.
pub fn ensure_model(model_dir: &Path, force: bool) -> Result<PathBuf> {
    if MODEL_URL.is_empty() {
        bail!("MODEL_URL is not set; cannot download model. Please provide a valid model file manually.");
    }

    if !force {
        // 1. Check current working directory
        let cwd_path = PathBuf::from(".").join(MODEL_FILENAME_LOCAL);
        if is_model_valid(&cwd_path) {
            // canonicalize may fail if the path doesn't exist yet; fall back to the raw path
            return Ok(fs::canonicalize(&cwd_path).unwrap_or(cwd_path));
        }

        // 2. Check next to the executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let exe_path = exe_dir.join(MODEL_FILENAME_LOCAL);
                if is_model_valid(&exe_path) {
                    return Ok(exe_path);
                }
            }
        }

        // 3. Check model_dir/model.gguf 
        let model_path = model_dir.join(MODEL_FILENAME);
        if is_model_valid(&model_path) {
            return Ok(model_path);
        }
    }

    fs::create_dir_all(model_dir).context("Failed to create model directory")?;
    let model_path = model_dir.join(MODEL_FILENAME);

    // If model exists but is invalid, remove it
    if model_path.exists() {
        fs::remove_file(&model_path).ok();
    }

    eprintln!("Downloading sift model (~500MB)...");

    let response = ureq::get(MODEL_URL).call()
        .context("Failed to start model download")?;

    let total_size: u64 = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(500_000_000);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {percent}% {bytes}/{total_bytes} eta {eta}")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message("Downloading");

    let tmp_path = model_dir.join("model.gguf.tmp");
    let mut file = fs::File::create(&tmp_path).context("Failed to create temp model file")?;

    let mut reader = response.into_body().into_reader();
    let mut buf = [0u8; 65536];
    let mut downloaded: u64 = 0;

    let download_result = (|| -> Result<()> {
        loop {
            let n = reader.read(&mut buf).context("Failed to read download stream")?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).context("Failed to write model file")?;
            downloaded += u64::from(n as u32); // lossless: n <= buf size (65536), always fits u32
            pb.set_position(downloaded);
        }

        pb.finish_with_message("Download complete");

        // Rename tmp to final
        fs::rename(&tmp_path, &model_path).context("Failed to rename model file")?;
        Ok(())
    })();

    if let Err(e) = download_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Verify
    if !is_model_valid(&model_path) {
        bail!("Downloaded model file is invalid (bad magic bytes or too small)");
    }

    eprintln!("Model downloaded successfully.");
    Ok(model_path)
}
