//! Small text-formatting helpers shared across tool implementations.

/// Clips `s` to at most `max` characters — by count, not bytes, so a
/// multi-byte UTF-8 character is never split in half. No ellipsis: callers
/// that want one append it themselves, since what precedes it (extra
/// trimming, a reserved budget for the marker itself) varies by caller —
/// see [`truncate_chars`] and `trim_description_to_char_limit` in
/// `crate::tools`, which both build on this.
pub fn clip_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

/// Truncates `s` to at most `max` characters, appending `...` when it
/// actually clips something. The marker is the whole point: a reader (a log,
/// a model, a search-result excerpt) needs to be able to tell "this is the
/// complete text" from "this was cut off" — a bare `.chars().take(max)`
/// can't do that.
pub fn truncate_chars(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    format!("{}...", clip_chars(trimmed, max))
}

#[cfg(test)]
mod tests {
    use super::truncate_chars;

    #[test]
    fn short_text_is_returned_unchanged() {
        assert_eq!(truncate_chars("  hello  ", 80), "hello");
    }

    #[test]
    fn long_text_is_clipped_with_a_marker() {
        let out = truncate_chars(&"a".repeat(400), 300);
        assert_eq!(out.chars().count(), 303, "300 chars + \"...\"");
        assert!(out.ends_with("..."));
    }

    #[test]
    fn never_splits_a_multibyte_char() {
        let s = "认购认沽行权价".repeat(50);
        let out = truncate_chars(&s, 80);
        assert!(out.ends_with("..."));
        // Every char in the head is still a valid, whole Chinese character.
        assert!(out.trim_end_matches("...").chars().all(|c| !c.is_ascii()));
    }
}
