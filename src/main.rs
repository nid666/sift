mod prefilter;
mod model;
mod download;
mod tui;
mod clipboard;

use std::io::{self, IsTerminal, Read};
use anyhow::{Context, Result};
use clap::Parser;

/// sift | Strip noise from error output
#[derive(Parser)]
#[command(name = "sift", version, about = "Strip noise from error output. Powered by a local LLM.")]
struct Cli {
    /// Output a search query instead of cleaned error
    #[arg(short, long)]
    search: bool,

    /// Output both cleaned error and search query
    #[arg(short, long)]
    verbose: bool,

    /// Skip heuristic pre-filter, send raw input to model
    #[arg(short, long)]
    raw: bool,

    /// Only run heuristic pre-filter, no model inference
    #[arg(short, long)]
    no_model: bool,

    /// Don't copy output to clipboard
    #[arg(long)]
    no_copy: bool,

    /// Force re-download of the model
    #[arg(long)]
    download: bool,
}

fn get_model_dir() -> std::path::PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("sift")
    } else {
        // Fallback to home directory
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".sift")
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // If --download flag force re-download and exit
    if cli.download {
        let model_dir = get_model_dir();
        download::ensure_model(&model_dir, true)?;
        eprintln!("Model downloaded to {}", model_dir.display());
        return Ok(());
    }

    let stdin = io::stdin();

    // Check if stdin is a TTY (no piped input)
    if stdin.is_terminal() {
        tui::show_help_tui()?;
        return Ok(());
    }

    // Read all of stdin
    let mut input = String::new();
    stdin.lock().read_to_string(&mut input)
        .context("Failed to read stdin")?;

    if input.trim().is_empty() {
        anyhow::bail!("No input received. Pipe error output to sift:\n  command 2>&1 | sift");
    }

    // Stage 1: Pre-filter (unless --raw)
    let filtered = if cli.raw {
        input
    } else {
        prefilter::prefilter(&input)
    };

    // If --no-model, just output the pre-filtered text
    if cli.no_model {
        println!("{filtered}");
        if !cli.no_copy {
            if clipboard::copy_to_clipboard(&filtered) {
                eprintln!("Copied to clipboard");
            }
        }
        return Ok(());
    }

    // Ensure model exists
    let model_dir = get_model_dir();
    let model_path = download::ensure_model(&model_dir, false)?;

    // Load model
    let spinner = tui::show_loading_spinner("Loading model...");
    let sift_model = model::SiftModel::load(&model_path)?;
    spinner.stop();

    let final_output = if cli.verbose {
        // Run both clean and search
        let spinner = tui::show_loading_spinner("Analyzing error...");
        let clean = sift_model.infer(model::CLEAN_SYSTEM_PROMPT, &filtered)?;
        spinner.stop();

        let spinner = tui::show_loading_spinner("Generating search query...");
        let search = sift_model.infer(model::SEARCH_SYSTEM_PROMPT, &filtered)?;
        spinner.stop();

        let output = format!("--- Clean Error ---\n{clean}\n\n--- Search Query ---\n{search}");
        println!("{output}");
        output
    } else if cli.search {
        let spinner = tui::show_loading_spinner("Generating search query...");
        let search = sift_model.infer(model::SEARCH_SYSTEM_PROMPT, &filtered)?;
        spinner.stop();

        println!("{search}");
        search
    } else {
        let spinner = tui::show_loading_spinner("Analyzing error...");
        let clean = sift_model.infer(model::CLEAN_SYSTEM_PROMPT, &filtered)?;
        spinner.stop();

        println!("{clean}");
        clean
    };

    // Stage 3: Clipboard copy
    if !cli.no_copy {
        if clipboard::copy_to_clipboard(&final_output) {
            eprintln!("Copied to clipboard");
        }
    }

    Ok(())
}
