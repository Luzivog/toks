use crate::{ReportOptions, UnifiedMessage};

/// Resolve the home directory every `from_dir`-style parser scans from.
///
/// An explicit `--home` always wins. Everything else goes through
/// [`crate::paths::home_dir`], which is the *only* place allowed to read
/// `$HOME`.
///
/// Reading `$HOME` here directly — as this used to — defeated that resolver
/// entirely, because the raw read ran first and always won. On Windows a Git
/// Bash `HOME=/home/user` therefore still reached every caller, and `Path`
/// resolves that against the current drive, so the model/monthly/hourly
/// reports and local parsing scanned `C:\home\user` instead of the real
/// profile — precisely the case `paths::home_dir` was written to prevent. An
/// exported-but-empty `HOME` was worse: it produced `Ok("")`, and the
/// `format!("{home}/...")` joins downstream turned that into absolute scans
/// from the filesystem root.
pub fn get_home_dir_string(home_dir_option: &Option<String>) -> Result<String, String> {
    home_dir_option
        .clone()
        .or_else(|| crate::paths::home_dir().map(|p| p.to_string_lossy().into_owned()))
        .ok_or_else(|| {
            "HOME directory not specified and could not determine home directory".to_string()
        })
}

pub(crate) fn filter_messages_for_report(
    messages: Vec<UnifiedMessage>,
    options: &ReportOptions,
) -> Vec<UnifiedMessage> {
    let mut filtered = messages;

    if let Some(year) = &options.year {
        let year_prefix = format!("{}-", year);
        filtered.retain(|m| m.date.starts_with(&year_prefix));
    }

    if let Some(since) = &options.since {
        filtered.retain(|m| m.date.as_str() >= since.as_str());
    }

    if let Some(until) = &options.until {
        filtered.retain(|m| m.date.as_str() <= until.as_str());
    }
    filtered
}
