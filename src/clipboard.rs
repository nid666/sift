use arboard::Clipboard;

/// Attempt to copy text to the system clipboard.
/// Returns true on success, false on any failure (never panics/crashes)
pub fn copy_to_clipboard(text: &str) -> bool {
    let Ok(mut clipboard) = Clipboard::new() else {
        return false;
    };
    clipboard.set_text(text).is_ok()
}
