//! Small formatting helpers.

/// "1 page", "12 pages".
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", thousands(n), if n == 1 { one } else { many })
}

/// 1,204
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_counts() {
        assert_eq!(count(1, "page", "pages"), "1 page");
        assert_eq!(count(1204, "page", "pages"), "1,204 pages");
        assert_eq!(thousands(123), "123");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
