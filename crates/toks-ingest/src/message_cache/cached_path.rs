#[cfg(not(any(unix, windows)))]
mod fallback;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(any(unix, windows)))]
pub(crate) use fallback::CachedPath;
#[cfg(unix)]
pub(crate) use unix::CachedPath;
#[cfg(windows)]
pub(crate) use windows::CachedPath;
