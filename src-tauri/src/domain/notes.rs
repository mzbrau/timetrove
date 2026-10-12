/// Trim, drop empties, drop exact duplicate strings (after trim), join in order with " · ".
pub fn combine_notes(notes: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut out: Vec<String> = Vec::new();
    for n in notes {
        let t = n.as_ref().trim();
        if t.is_empty() {
            continue;
        }
        if out.iter().any(|existing| existing == t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join(" · ")
}

/// Normalize a note for storage: trim; whitespace-only becomes empty.
pub fn normalize_note(note: &str) -> String {
    note.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_notes_trims_drops_empties_dedupes() {
        assert_eq!(combine_notes(["", "  ", "a"]), "a");
        assert_eq!(combine_notes(["a", "b"]), "a · b");
        assert_eq!(combine_notes(["a", "a", "b"]), "a · b");
        assert_eq!(combine_notes(["  a  ", "a", "b"]), "a · b");
        assert_eq!(combine_notes(Vec::<&str>::new()), "");
        assert_eq!(combine_notes(["first", "second", "first"]), "first · second");
    }

    #[test]
    fn normalize_note_trims() {
        assert_eq!(normalize_note("  hello  "), "hello");
        assert_eq!(normalize_note("   "), "");
        assert_eq!(normalize_note(""), "");
    }
}
