pub(in crate::pricing::lookup) fn contains_delimited_fragment(
    haystack: &str,
    fragment: &str,
) -> bool {
    if fragment.is_empty() {
        return false;
    }

    for (pos, _) in haystack.match_indices(fragment) {
        let before_ok = pos == 0 || !haystack[..pos].chars().last().unwrap().is_alphanumeric();
        let after_pos = pos + fragment.len();
        let after_ok = after_pos == haystack.len()
            || !haystack[after_pos..]
                .chars()
                .next()
                .unwrap()
                .is_alphanumeric();

        if before_ok && after_ok {
            return true;
        }
    }

    false
}

pub(in crate::pricing::lookup) fn contains_delimited_major_minor(
    haystack: &str,
    major: char,
) -> bool {
    for (pos, _) in haystack.match_indices(major) {
        let before_ok = pos == 0 || !haystack[..pos].chars().last().unwrap().is_alphanumeric();
        let after_pos = pos + major.len_utf8();
        let mut after = haystack[after_pos..].chars();
        let Some(separator) = after.next() else {
            continue;
        };
        let Some(minor_start) = after.next() else {
            continue;
        };

        if before_ok && matches!(separator, '.' | '-') && minor_start.is_ascii_digit() {
            return true;
        }
    }

    false
}
