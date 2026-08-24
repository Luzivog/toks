use super::CacheIdentity;
use crate::clients::ClientId;
use crate::message_cache::parser_version;

impl CacheIdentity {
    pub(crate) fn for_client(client: ClientId) -> Self {
        Self {
            namespace: client.as_str(),
            parser_version: parser_version(client),
        }
    }

    pub(crate) const fn synthetic() -> Self {
        Self {
            namespace: "synthetic",
            parser_version: 1,
        }
    }

    pub(super) fn current_for_namespace(namespace: &str) -> Option<Self> {
        if namespace == "synthetic" {
            return Some(Self::synthetic());
        }
        ClientId::from_str(namespace).map(Self::for_client)
    }

    pub(super) fn all() -> impl Iterator<Item = Self> {
        ClientId::iter()
            .map(Self::for_client)
            .chain(std::iter::once(Self::synthetic()))
    }
}
