const PALETTE: &[&str] = &[
    "#E4572E", "#F3A712", "#17BEBB", "#2E86AB", "#A23B72", "#3B1F2B", "#76B041",
    "#D64045", "#5C80BC", "#E9C46A", "#2A9D8F", "#E76F51", "#264653", "#9B5DE5",
    "#00BBF9", "#F15BB5", "#FEE440", "#00F5D4", "#9C6644", "#6D597A",
];

/// Pick a palette color least used among existing task colors.
pub fn next_color(existing: &[String]) -> String {
    let mut counts = vec![0usize; PALETTE.len()];
    for c in existing {
        if let Some(i) = PALETTE.iter().position(|p| p.eq_ignore_ascii_case(c)) {
            counts[i] += 1;
        }
    }
    let min = *counts.iter().min().unwrap_or(&0);
    let idx = counts.iter().position(|&c| c == min).unwrap_or(0);
    PALETTE[idx].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_unused_first() {
        let existing = vec![PALETTE[0].to_string()];
        assert_eq!(next_color(&existing), PALETTE[1]);
    }
}
