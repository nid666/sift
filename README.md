<div align="center">

# Sift
*Strip noise from error output*

[![Build Status](https://img.shields.io/github/actions/workflow/status/nid666/sift/ci.yml?style=flat-square)](https://github.com/nid666/sift/actions)
[![Crates.io](https://img.shields.io/crates/v/sift?style=flat-square)](https://crates.io/crates/sift-cli/)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![HuggingFace](https://img.shields.io/badge/HuggingFace-Model-orange?logo=huggingface&style=flat-square)](https://huggingface.co/Sid77449/sift)

[Features](#features) • [Installation](#installation) • [Usage](#usage)

</div>

`sift` takes raw error output: stack traces, build failures, cloud provider logs, and returns a clean, minimal version useful for AI agents, debugging, pasting into LLMs, or searching. Powered by a fine-tuned [model](https://huggingface.co/Sid77449/sift/resolve/main/model.gguf) running locally via llama.cpp. No API keys. No telemetry. No GPU required.


> [!NOTE]
> The model (~500MB, Q4_K_M quantization) is downloaded automatically on first run and cached locally. Subsequent runs take ~1–2 seconds on CPU.

## Features

- **Clean mode** strips timestamps, UUIDs, absolute paths, IPs, internal stack frames, and other noise while preserving the actual error message, error codes, and causal chain
- **Search mode** produces a short, Google/StackOverflow-optimized keyword query from the error
- **Heuristic pre-filter** a fast deterministic stage that extracts signal lines before the model runs, keeping inference input under ~2000 characters regardless of log size
- **Clipboard copy** output is automatically copied to the clipboard
- **TUI help screen** running `sift` without piped input launches an interactive help screen
- **Fully offline** everything runs for free locally and nothing leaves your machine

## Installation

### Download prebuilt binary

Grab the latest binary from [GitHub Releases](https://github.com/nid666/sift/releases/latest) and put it on your `PATH`:

**macOS (Apple Silicon)**
```bash
curl -L https://github.com/nid666/sift/releases/latest/download/sift-macos-aarch64 -o sift
chmod +x sift
sudo mv sift /usr/local/bin/
```

**macOS (Intel)**
```bash
curl -L https://github.com/nid666/sift/releases/latest/download/sift-macos-x86_64 -o sift
chmod +x sift
sudo mv sift /usr/local/bin/
```

**Linux (x86_64)**
```bash
curl -L https://github.com/nid666/sift/releases/latest/download/sift-linux-x86_64 -o sift
chmod +x sift
sudo mv sift /usr/local/bin/
```

**Windows (x86_64)**

Download [`sift-windows-x86_64.exe`](https://github.com/nid666/sift/releases/latest/download/sift-windows-x86_64.exe), rename it to `sift.exe`, and place it in a directory on your `PATH` (e.g., `C:\Users\<you>\bin\`).

### Build from source

Requires: [Rust toolchain](https://rustup.rs) (stable), a C++ compiler (clang or gcc), and cmake.

```bash
git clone https://github.com/nid666/sift
cd sift
cargo build --release
cp target/release/sift ~/.local/bin/
```

> [!NOTE]
> The first build compiles llama.cpp from source (vendored). This takes a few minutes but produces a fully self-contained binary with no runtime dependencies.

## Usage

```
USAGE:
    some_command 2>&1 | sift [OPTIONS]
    sift [OPTIONS] < file.log
    sift                          (launches TUI help screen)

OPTIONS:
    -s, --search       Output a search query instead of cleaned error
    -v, --verbose      Output both cleaned error and search query
    -r, --raw          Skip heuristic pre-filter, send raw input to model
    -n, --no-model     Only run heuristic pre-filter, no model inference
        --no-copy      Don't copy output to clipboard
        --download     Force re-download of the model
    -h, --help         Print help
    -V, --version      Print version
```

### Examples

```bash
# Clean a Python traceback
python app.py 2>&1 | sift

# Get a StackOverflow-ready search query
cargo build 2>&1 | sift -s

# Clean from a saved log file
sift < error.log

# See both cleaned error and search query
kubectl logs pod/my-pod 2>&1 | sift -v

# Run just the heuristic filter, skip model inference
sift --no-model < large-error.log
```

## Example

**Input** (piped via stdin):
```
Traceback (most recent call last):
  File "/home/sid/projects/api/src/routes.py", line 42, in handler
    result = db.execute(query)
  File "/home/sid/.venv/lib/python3.12/site-packages/sqlalchemy/orm/session.py", line 2308, in execute
    return self._execute_internal(
  File "/home/sid/.venv/lib/python3.12/site-packages/sqlalchemy/engine/base.py", line 1965, in _exec_single_context
    self.dialect.do_execute(
sqlalchemy.exc.OperationalError: (psycopg2.OperationalError) connection to server at "10.0.1.55", port 5432 failed: Connection timed out
    Is the server running on that host and accepting TCP/IP connections?
```

**Default output** (`sift`):
```
sqlalchemy.exc.OperationalError: (psycopg2.OperationalError) connection to server failed: Connection timed out
Is the server running on that host and accepting TCP/IP connections?
```

**Search output** (`sift -s`):
```
sqlalchemy psycopg2 OperationalError connection timed out PostgreSQL
```


> [!IMPORTANT]
> Only the cleaned error or search query is written to stdout. All progress bars, spinners, and status messages go to stderr, making `sift` safe to use in pipes, scripts, or as a tool for agents.

## Model Storage

The GGUF model file is stored in your platform's standard data directory:

| Platform | Path |
|----------|------|
| macOS    | `~/Library/Application Support/sift/` |
| Linux    | `~/.local/share/sift/` |
| Windows  | `%APPDATA%\sift\` |

To force a fresh download (e.g., after a corrupted file):

```bash
sift --download
```