//! UTF-8-safe output budgets shared by transport adapters.
pub const DEFAULT_MAX_BYTES: usize = 12_000;

/// Zero requests the full payload. An incomplete UTF-8 suffix is omitted;
/// other invalid source bytes remain untouched for byte-oriented CLI output.
pub fn prefix(bytes: &[u8], max_bytes: usize) -> &[u8] {
    if max_bytes == 0 || bytes.len() <= max_bytes {
        return bytes;
    }
    let mut selected = &bytes[..max_bytes];
    // Skip invalid bytes when looking for an incomplete final codepoint.
    let mut offset = 0;
    while let Err(error) = std::str::from_utf8(&selected[offset..]) {
        offset += error.valid_up_to();
        if let Some(length) = error.error_len() {
            offset += length;
        } else {
            selected = &selected[..offset];
            break;
        }
    }
    selected
}

/// Limit payload bytes, then append a small explicit notice outside the budget.
pub fn truncate(text: &str, max_bytes: usize) -> String {
    truncate_with_hint(text, max_bytes, "set max_bytes=0 for full output")
}

pub fn truncate_with_hint(text: &str, max_bytes: usize, hint: &str) -> String {
    let selected = prefix(text.as_bytes(), max_bytes);
    if selected.len() == text.len() {
        return text.to_owned();
    }
    let prefix = std::str::from_utf8(selected).expect("text was UTF-8");
    format!(
        "{prefix}\n[truncated: {}/{} bytes; {hint}]\n",
        selected.len(),
        text.len()
    )
}

#[cfg(test)]
mod tests {
    use super::{prefix, truncate};

    #[test]
    fn budgets_preserve_unicode_and_expose_omission() {
        for limit in 1..9 {
            let output = truncate("中文🙂end", limit);
            assert!(output.contains("[truncated:"));
            assert!(!output.contains('\u{fffd}'));
        }
        assert_eq!(truncate("中文", 6), "中文");
        assert_eq!(truncate("中文", 0), "中文");
        assert_eq!(truncate("", 1), "");
    }

    #[test]
    fn invalid_bytes_are_retained_but_incomplete_unicode_suffixes_are_omitted() {
        let bytes = [0xff, b'a', 0xe4, 0xb8, 0xad, b'z'];
        for limit in [3, 4] {
            assert_eq!(prefix(&bytes, limit), [0xff, b'a']);
        }
        assert_eq!(prefix(&bytes, 5), &bytes[..5]);
        assert_eq!(prefix(&bytes, 0), bytes);
    }
}
