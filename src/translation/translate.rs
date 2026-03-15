use std::borrow::Cow;

use super::PlaceholderStyle;
use super::parsers::{
    is_block_comment_end, is_block_comment_start, is_line_comment_start, matches_tag,
    try_start_dollar_quote,
};
use super::scanner::{State, scan_digits};

pub(super) fn translate_sql(sql: &str, target: PlaceholderStyle, enabled: bool) -> Cow<'_, str> {
    if !enabled {
        return Cow::Borrowed(sql);
    }

    let mut out: Option<String> = None;
    let mut state = State::Normal;
    let mut idx = 0;
    let bytes = sql.as_bytes();

    while idx < bytes.len() {
        let byte = bytes[idx];
        let mut replaced = false;

        match state {
            State::Normal => match byte {
                b'\'' => state = State::SingleQuoted,
                b'"' => state = State::DoubleQuoted,
                _ if is_line_comment_start(bytes, idx) => state = State::LineComment,
                _ if is_block_comment_start(bytes, idx) => state = State::BlockComment(1),
                b'$' => {
                    if let Some((tag, advance)) = try_start_dollar_quote(bytes, idx) {
                        state = State::DollarQuoted(tag);
                        idx = advance;
                    } else if matches!(target, PlaceholderStyle::Sqlite)
                        && let Some((digits_end, digits)) = scan_digits(bytes, idx + 1)
                    {
                        let buf = out.get_or_insert_with(|| sql[..idx].to_string());
                        buf.push('?');
                        buf.push_str(digits);
                        idx = digits_end - 1;
                        replaced = true;
                    }
                }
                b'?' if matches!(target, PlaceholderStyle::Postgres) => {
                    if let Some((digits_end, digits)) = scan_digits(bytes, idx + 1) {
                        let buf = out.get_or_insert_with(|| sql[..idx].to_string());
                        buf.push('$');
                        buf.push_str(digits);
                        idx = digits_end - 1;
                        replaced = true;
                    }
                }
                _ => {}
            },
            State::SingleQuoted => {
                if byte == b'\'' {
                    if bytes.get(idx + 1) == Some(&b'\'') {
                        idx += 1;
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::DoubleQuoted => {
                if byte == b'"' {
                    if bytes.get(idx + 1) == Some(&b'"') {
                        idx += 1;
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::LineComment => {
                if byte == b'\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment(depth) => {
                if is_block_comment_start(bytes, idx) {
                    state = State::BlockComment(depth + 1);
                } else if is_block_comment_end(bytes, idx) {
                    if depth == 1 {
                        state = State::Normal;
                    } else {
                        state = State::BlockComment(depth - 1);
                    }
                }
            }
            State::DollarQuoted(ref tag) => {
                if byte == b'$' && matches_tag(bytes, idx, tag) {
                    idx += tag.len();
                    state = State::Normal;
                }
            }
        }

        if let Some(ref mut buf) = out
            && !replaced
        {
            buf.push(byte as char);
        }

        idx += 1;
    }

    match out {
        Some(buf) => Cow::Owned(buf),
        None => Cow::Borrowed(sql),
    }
}
