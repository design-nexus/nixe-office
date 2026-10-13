//! Parsing XML from files: roxmltree, but refusing documents nested so deep that
//! parsing them would overflow the stack (roxmltree goes one call deeper per level).

/// Deeper than any real document goes, which keeps the stack below small.
pub const MAX_DEPTH: usize = 1000;

/// Documents deeper than this are parsed on a thread of their own with a big stack
/// (unoptimised, each level takes about 15 KB).
const DEEP: usize = 100;

/// Parses `text`, or refuses it when its elements nest more than `MAX_DEPTH` deep.
pub fn parse(text: &str) -> Result<roxmltree::Document<'_>, roxmltree::Error> {
    let d = depth(text);
    if d > MAX_DEPTH {
        return Err(roxmltree::Error::NodesLimitReached);
    }
    if d <= DEEP {
        return roxmltree::Document::parse(text);
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn_scoped(scope, || roxmltree::Document::parse(text))
            .map_err(|_| roxmltree::Error::NodesLimitReached)?
            .join()
            .unwrap_or(Err(roxmltree::Error::NodesLimitReached))
    })
}

/// How deep the elements nest, at most (comments, CDATA and the like left out).
fn depth(text: &str) -> usize {
    let b = text.as_bytes();
    let (mut d, mut max, mut i) = (0usize, 0usize, 0usize);
    let skip_to = |from: usize, end: &[u8]| -> usize {
        b[from..].windows(end.len()).position(|w| w == end).map_or(b.len(), |p| from + p + end.len())
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            i = skip_to(i + 4, b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = skip_to(i + 9, b"]]>");
        } else if rest.starts_with(b"<?") {
            i = skip_to(i + 2, b"?>");
        } else if rest.starts_with(b"<!") {
            i = skip_to(i + 2, b">");
        } else if rest.starts_with(b"</") {
            d = d.saturating_sub(1);
            i = skip_to(i + 2, b">");
        } else {
            // An opening tag: to its end, past quoted values (which may hold '>').
            let mut j = i + 1;
            let mut quote = 0u8;
            while j < b.len() {
                match b[j] {
                    q @ (b'"' | b'\'') if quote == 0 => quote = q,
                    q if q == quote => quote = 0,
                    b'>' if quote == 0 => break,
                    _ => {}
                }
                j += 1;
            }
            if b.get(j.wrapping_sub(1)) != Some(&b'/') {
                d += 1;
                max = max.max(d);
            }
            i = j + 1;
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_limits_depth() {
        assert_eq!(depth(r#"<?xml version="1.0"?><a x="1>2"><b/><c><!-- <d><e> --><![CDATA[<f>]]></c></a>"#), 2);
        let deep = "<a>".repeat(MAX_DEPTH + 1) + &"</a>".repeat(MAX_DEPTH + 1);
        assert!(parse(&deep).is_err());
        let fine = "<a>".repeat(MAX_DEPTH) + &"</a>".repeat(MAX_DEPTH);
        assert!(parse(&fine).is_ok());
    }
}
