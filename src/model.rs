use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
#[allow(deprecated)]
use llama_cpp_2::model::{LlamaModel, AddBos, Special, params::LlamaModelParams};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::sampling::LlamaSampler;

/// On Windows, converts a path to its short (8.3) form via `GetShortPathNameW`
/// so that the ASCII-only short path can be safely passed to MSVCRT `fopen()`.
/// Falls back to the original path if the conversion fails.
/// On non-Windows platforms, returns the path unchanged.
#[cfg(target_os = "windows")]
fn to_short_path(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::ffi::OsString;
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    // Encode the path as a null-terminated wide string.
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0u16);

    // First call: get the required buffer size.
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return path.to_path_buf();
    }

    // Second call: fill the buffer.
    let mut buf: Vec<u16> = vec![0u16; needed as usize];
    let written = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed) };
    if written == 0 || written >= needed {
        return path.to_path_buf();
    }

    PathBuf::from(OsString::from_wide(&buf[..written as usize]))
}

#[cfg(not(target_os = "windows"))]
fn to_short_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

pub const CLEAN_SYSTEM_PROMPT: &str = r#"You clean raw error output so an LLM debugging agent can diagnose the root cause. Given a raw error, return ONLY the cleaned version.
REMOVED:
- Timestamps, dates, uptime counters
- UUIDs, request IDs, correlation IDs, trace IDs, span IDs
- Absolute file paths (keep just filename + line number)
- IP addresses, hostnames, port numbers (UNLESS the error is about connectivity/DNS/TLS)
- Subscription IDs, account IDs, project IDs, tenant IDs
- Container IDs, pod name suffixes (keep the deployment/service name)
- User-specific resource names (replace with <resource_name>, <bucket_name>, <db_name>, etc.)
- Redundant/repeated lines (but note the count, e.g., "... repeated 47 times")
- Framework-internal stack frames deep in the call stack (keep top 3-5 user code frames + the deepest frame that THROWS the error)
- ANSI color codes, spinner characters, progress bar artifacts
- Auth tokens, keys, passwords, connection strings (replace with <redacted>)
KEPT:
- Error codes and error types (e.g., E0382, ORA-1234, ENOENT, SQLSTATE)
- The full error message text exactly as written
- The framework/tool name and version if mentioned
- Relevant stack frames showing user code (filename:line_number + function name)
- The DEEPEST causal frame (the frame that actually threw/originated the error)
- "Caused by" / "caused by" / chained exception chains IN FULL
- Configuration keys and values that caused the error
- HTTP status codes AND the response body/message if present
- Environment/runtime info (language version, OS, platform) if mentioned
- State or phase info (e.g., "during migration", "at startup", "while compiling")
- Variable names, types, and values mentioned in the error
- Expected vs. actual values (e.g., "expected string, got null")
- Permission/role names in auth errors
- Package/module/crate names and versions in dependency errors
- Exit codes and signal names
STRUCTURE:
- If there are chained/nested errors ("Caused by"), preserve the full chain in order
- If there are multiple distinct errors, separate them with a blank line
- Preserve the hierarchy: primary error first, then causes, then relevant stack frames
- If a repeated message was collapsed, append: [repeated N times]
Return ONLY the cleaned error text. No explanations, no markdown formatting, no prefixes, no commentary."#;

pub const SEARCH_SYSTEM_PROMPT: &str = "You are sift in search mode. Given raw error output, return ONLY a short search query (5-15 words) optimized for Google/StackOverflow. Just keywords, no quotes, no operators. Nothing else.";

/// Wraps a `llama-cpp-2` [`LlamaModel`] (C FFI). Not `Send` must not be
/// shared or moved across threads. All inference must happen on the thread
/// that called [`SiftModel::load`].
pub struct SiftModel {
    model: LlamaModel,
    backend: LlamaBackend,
}

impl SiftModel {
    pub fn load(model_path: &Path) -> Result<Self> {
        let mut backend = LlamaBackend::init()
            .context("Failed to initialize llama backend")?;

        // Suppress llama.cpp's noisy log output via the library-level API.
        // This is platform-agnostic
        backend.void_logs();

        let model_params = LlamaModelParams::default();
        let short_path = to_short_path(model_path);
        let model = LlamaModel::load_from_file(&backend, &short_path, &model_params)
            .context("Failed to load GGUF model")?;

        Ok(Self { model, backend })
    }

    pub fn infer(&self, system_prompt: &str, user_input: &str) -> Result<String> {
        let n_threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).max(1) as i32)
            .unwrap_or(4);

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZero::new(2048))
            .with_n_threads(n_threads)
            .with_n_threads_batch(n_threads);

        let mut ctx = self.model.new_context(&self.backend, ctx_params)
            .context("Failed to create llama context")?;

        // Build the prompt using Qwen chat template
        let prompt = format!(
            "<|im_start|>system\n{system_prompt}<|im_end|>\n<|im_start|>user\n{user_input}<|im_end|>\n<|im_start|>assistant\n"
        );

        // Tokenize
        let mut tokens = self.model.str_to_token(&prompt, AddBos::Always)
            .context("Failed to tokenize prompt")?;

        let max_tokens = 512;

        // Guard against context window overflow: truncate prompt tokens to
        // leave at least max_tokens slots available for generation.
        if tokens.len() + max_tokens > 2048 {
            tokens.truncate(2048 - max_tokens);
        }

        // Create batch and add tokens
        let mut batch = LlamaBatch::new(2048, 1);
        for (i, &token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(token, i as i32, &[0], is_last)
                .context("Failed to add token to batch")?;
        }

        // Decode the prompt
        ctx.decode(&mut batch).context("Failed to decode prompt batch")?;

        // Set up sampler with temperature and penalties
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.1),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::penalties(64, 1.1, 0.0, 0.0),
            LlamaSampler::dist(42),
        ]);

        // Generate tokens
        let mut output = String::new();
        let mut n_decoded = tokens.len() as i32;

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, -1);

            // Check for EOS
            if self.model.is_eog_token(token) {
                break;
            }

            #[allow(deprecated)]
            // All tag bytes are ASCII so str::find boundaries are always char-aligned.
            let piece = self.model.token_to_str(token, Special::Tokenize)
                .unwrap_or_else(|e| {
                    eprintln!("[sift] token decode error: {e}");
                    String::new()
                });

            // Check for end-of-turn marker
            if piece.contains("<|im_end|>") {
                // Add any text before the marker
                if let Some(before) = piece.split("<|im_end|>").next() {
                    output.push_str(before);
                }
                break;
            }

            output.push_str(&piece);

            // Prepare next batch
            let mut next_batch = LlamaBatch::new(1, 1);
            next_batch.add(token, n_decoded, &[0], true)
                .context("Failed to add token to batch")?;
            ctx.decode(&mut next_batch).context("Failed to decode token")?;
            n_decoded += 1;
        }

        // Strip any <think>...</think> tags (Qwen thinking mode artifacts)
        let output = strip_think_tags(&output);
        let final_output = output.trim().to_string();

        Ok(final_output)
    }
}

/// Strip `<think>...</think>` tags from output. (The model is fine tuned to not have these, but just in case
/// Returns the input unchanged (as a new `String`) if no `<think>` tag is
/// present.  When tags are found, the result is built by appending slices,
/// avoiding repeated full-string copies.
///
/// All delimiter bytes (`<think>` / `</think>`) are ASCII, so byte-index
/// boundaries produced by [`str::find`] are always on valid char boundaries.
fn strip_think_tags(input: &str) -> String {
    // Fast path: nothing to strip.
    if !input.contains("<think>") {
        return input.to_string();
    }

    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    let mut result = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(start) = remaining.find(OPEN) {
        // Append everything before the opening tag.
        result.push_str(&remaining[..start]);
        let after_open = &remaining[start + OPEN.len()..];
        if let Some(end) = after_open.find(CLOSE) {
            // Skip the content inside the tag plus the closing tag itself.
            remaining = &after_open[end + CLOSE.len()..];
        } else {
            // Unclosed <think> tag — discard from here to end of string.
            break;
        }
    }

    // If no more tags, append the rest.
    result.push_str(remaining);
    result
}
