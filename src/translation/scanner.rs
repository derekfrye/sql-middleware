#[derive(Clone)]
pub(super) enum State {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment(u32),
    DollarQuoted(String),
}

pub(super) fn scan_digits(bytes: &[u8], start: usize) -> Option<(usize, &str)> {
    let mut idx = start;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == start {
        None
    } else {
        std::str::from_utf8(&bytes[start..idx])
            .ok()
            .map(|digits| (idx, digits))
    }
}
