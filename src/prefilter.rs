use std::sync::OnceLock;
use regex::Regex;

/// Maximum output size in characters
const MAX_OUTPUT_CHARS: usize = 2000;
/// Context lines to keep around signal lines
const CONTEXT_LINES: usize = 2;
/// Fallback: keep last N lines if no signal found
const FALLBACK_TAIL_LINES: usize = 30;

fn compile_patterns(patterns: &[&str]) -> Vec<Regex> {
    patterns.iter().map(|p| Regex::new(p).expect("invalid regex pattern")).collect()
}

fn signal_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| compile_patterns(&[
        r"(?i)(error|exception|fatal|failed|panic):",
        r"(?i)traceback \(most recent",
        r"(?i)caused by:",
        r"(?i)at \S+\.\S+\(\S+\.\w+:\d+\)",
        r"(?i)at \S+ \(.*:\d+:\d+\)",
        r#"(?i)File ".*", line \d+"#,
        r"(?i)(exit (code|status) [1-9])",
        r"(?i)(denied|refused|timeout|forbidden)",
        r"(?i)(command not found|no such file)",
        r"(?i)(cannot|can't|could not|unable to)",
        r"(?i)(ENOENT|EACCES|MODULE_NOT_FOUND)",
        r"(?i)(build failed|compilation error)",
        r"(?i)(segmentation fault|core dumped)",
        r"(?i)(connection (refused|reset|timed.?out))",
        r"(^\s*\^+\s*$|^\s*-->)",
        r"error\[E\d{4}\]:",
        r"\S+\.go:\d+:\d+:",
        r"(?i)warning:",
        r"npm ERR!",
        r"(?i)yarn error",
        r"(?i)assertion failed",
        r"(?i)stack overflow",
    ]))
}

fn drop_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| compile_patterns(&[
        r"^\s*$",
        r"^\s*[-=.]{3,}\s*$",
        r"(?i)^\s*at\s+internal/",
    ]))
}

fn is_signal_line(line: &str) -> bool {
    signal_patterns().iter().any(|re| re.is_match(line))
}

fn is_drop_line(line: &str) -> bool {
    drop_patterns().iter().any(|re| re.is_match(line))
}

/// Collapse consecutive duplicate lines: keep first 2 occurrences, replace rest with a summary
fn dedup_consecutive(lines: Vec<&str>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut prev: Option<&str> = None;
    let mut extra_occurrences: usize = 0;

    for line in &lines {
        if Some(*line) == prev {
            extra_occurrences += 1;
            if extra_occurrences == 1 {
                // Second occurrence — still keep it
                result.push(line.to_string());
            }
            // Third and beyond: just increment, summary emitted on transition
        } else {
            // Transitioning away from a run. Emit summary if needed
            if extra_occurrences >= 2 {
                result.push(format!("... (line repeated {} more times)", extra_occurrences - 1));
            }
            prev = Some(line);
            extra_occurrences = 0;
            result.push(line.to_string());
        }
    }
    // End of input: emit summary for trailing run if needed
    if extra_occurrences >= 2 {
        result.push(format!("... (line repeated {} more times)", extra_occurrences - 1));
    }

    result
}

pub fn prefilter(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();

    if lines.is_empty() {
        return String::new();
    }

    // If input is small enough, just do basic cleanup
    if input.len() <= MAX_OUTPUT_CHARS {
        let cleaned: Vec<&str> = lines.into_iter().filter(|l| !is_drop_line(l)).collect();
        let deduped = dedup_consecutive(cleaned);
        return deduped.join("\n");
    }

    // Find signal lines
    let signal_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_signal_line(l))
        .map(|(i, _)| i)
        .collect();

    let selected_lines: Vec<&str> = if signal_indices.is_empty() {
        // No signal found, keep last FALLBACK_TAIL_LINES
        let start = lines.len().saturating_sub(FALLBACK_TAIL_LINES);
        lines[start..].to_vec()
    } else {
        // Collect signal lines + context
        let mut keep = vec![false; lines.len()];
        for &idx in &signal_indices {
            let start = idx.saturating_sub(CONTEXT_LINES);
            let end = (idx + CONTEXT_LINES + 1).min(lines.len());
            for i in start..end {
                keep[i] = true;
            }
        }
        lines
            .iter()
            .enumerate()
            .filter(|(i, _)| keep[*i])
            .map(|(_, l)| *l)
            .collect()
    };

    // Drop noise lines
    let cleaned: Vec<&str> = selected_lines
        .into_iter()
        .filter(|l| !is_drop_line(l))
        .collect();

    // Dedup consecutive
    let deduped = dedup_consecutive(cleaned);

    // Truncate to MAX_OUTPUT_CHARS
    let mut result = String::new();
    for line in deduped {
        let separator_len = if result.is_empty() { 0 } else { 1 };
        if result.len() + line.len() + separator_len > MAX_OUTPUT_CHARS {
            break;
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&line);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefilter_python_traceback() {
        let input = r#"Traceback (most recent call last):
  File "/home/user/projects/app/main.py", line 10, in <module>
    import nonexistent
ModuleNotFoundError: No module named 'nonexistent'"#;
        let result = prefilter(input);
        assert!(result.contains("ModuleNotFoundError"));
        assert!(result.contains("No module named"));
    }

    #[test]
    fn test_prefilter_drops_empty_lines() {
        let input = "line1\n\n\n\nError: something broke\n\n\n";
        let result = prefilter(input);
        assert!(!result.contains("\n\n\n"));
    }

    #[test]
    fn test_prefilter_truncates_long_input() {
        let input = "Error: test\n".repeat(1000);
        let result = prefilter(&input);
        assert!(result.len() < 3000);
    }
}
