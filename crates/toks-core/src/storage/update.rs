/// The result of applying a change to a read-modify-write store.
pub enum StoreUpdate<T> {
    /// Return the value and persist the modified document.
    Changed(T),
    /// Return the value without writing the document.
    Unchanged(T),
}

impl<T> StoreUpdate<T> {
    /// Selects the persistence behavior from a mutation's existing outcome.
    pub fn from_changed(value: T, changed: bool) -> Self {
        if changed {
            Self::Changed(value)
        } else {
            Self::Unchanged(value)
        }
    }

    pub(crate) fn map<U>(self, map: impl FnOnce(T) -> U) -> StoreUpdate<U> {
        match self {
            Self::Changed(value) => StoreUpdate::Changed(map(value)),
            Self::Unchanged(value) => StoreUpdate::Unchanged(map(value)),
        }
    }

    pub(crate) fn into_parts(self) -> (T, bool) {
        match self {
            Self::Changed(value) => (value, true),
            Self::Unchanged(value) => (value, false),
        }
    }
}
