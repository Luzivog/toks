use crate::limits::Provider;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }
    };
}

string_id!(AccountId);
string_id!(CredentialProfileId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialProfileKind {
    Current,
    Managed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSource {
    pub profile_id: CredentialProfileId,
    pub kind: CredentialProfileKind,
    pub primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountOrigin {
    Current,
    Managed,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountIdentityKind {
    ProviderPrincipal,
    #[default]
    ProfileFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAccount {
    pub id: AccountId,
    #[serde(default)]
    pub identity_kind: AccountIdentityKind,
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<AccountSource>,
}

impl ProviderAccount {
    pub fn unidentified_for(provider: Provider) -> Self {
        Self {
            id: AccountId::new(format!("{}-unidentified", provider.slug())),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: None,
            sources: Vec::new(),
        }
    }

    pub fn origin(&self) -> AccountOrigin {
        let current = self
            .sources
            .iter()
            .any(|source| source.kind == CredentialProfileKind::Current);
        let managed = self
            .sources
            .iter()
            .any(|source| source.kind == CredentialProfileKind::Managed);
        match (current, managed) {
            (true, true) => AccountOrigin::Mixed,
            (true, false) => AccountOrigin::Current,
            (false, true) => AccountOrigin::Managed,
            (false, false) => AccountOrigin::Unknown,
        }
    }

    pub fn primary_source(&self) -> Option<&AccountSource> {
        self.sources.iter().find(|source| source.primary)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AddAccountStarted {
    pub provider: Provider,
    pub account_id: CredentialProfileId,
}

#[derive(Debug, Clone)]
pub(crate) struct AccountProfile {
    pub provider: Provider,
    pub profile_id: CredentialProfileId,
    pub account: ProviderAccount,
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
    pub managed: bool,
    pub created_at_ms: Option<u128>,
}

impl AccountProfile {
    pub(crate) fn cache_key(&self) -> String {
        format!("{}:{}", self.provider.slug(), self.profile_id)
    }

    pub(crate) fn source(&self) -> AccountSource {
        AccountSource {
            profile_id: self.profile_id.clone(),
            kind: if self.managed {
                CredentialProfileKind::Managed
            } else {
                CredentialProfileKind::Current
            },
            primary: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProfileMetadata {
    pub(super) version: u8,
    pub(super) id: String,
    pub(super) provider: Provider,
    pub(super) created_at_ms: u128,
}
