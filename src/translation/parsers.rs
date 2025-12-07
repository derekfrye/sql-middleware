pub(super) fn is_line_comment_start(bytes: &[u8], idx: usize) -> bool {
    bytes.get(idx) == Some(&b'-') && bytes.get(idx + 1) == Some(&b'-')
}

pub(super) fn is_block_comment_start(bytes: &[u8], idx: usize) -> bool {
    bytes.get(idx) == Some(&b'/') && bytes.get(idx + 1) == Some(&b'*')
}

pub(super) fn is_block_comment_end(bytes: &[u8], idx: usize) -> bool {
    bytes.get(idx) == Some(&b'*') && bytes.get(idx + 1) == Some(&b'/')
}

pub(super) fn try_start_dollar_quote(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut idx = start + 1;
    while idx < bytes.len() && bytes[idx] != b'$' {
        let b = bytes[idx];
        if !(b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        idx += 1;
    }

    if idx < bytes.len() && bytes[idx] == b'$' {
        let tag = String::from_utf8(bytes[start + 1..idx].to_vec()).ok()?;
        Some((tag, idx))
    } else {
        None
    }
}

pub(super) fn matches_tag(bytes: &[u8], idx: usize, tag: &str) -> bool {
    let end = idx + 1 + tag.len();
    end < bytes.len()
        && bytes[idx + 1..=end].starts_with(tag.as_bytes())
        && bytes.get(end) == Some(&b'$')
}
