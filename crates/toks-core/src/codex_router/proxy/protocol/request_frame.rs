use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::thread_identity::ThreadIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codex_router::proxy) enum ClientRequestFrame {
    ResponseCreate(ThreadIdentity),
    Other,
    Denied,
}

impl ClientRequestFrame {
    pub(in crate::codex_router::proxy) fn from_payload(payload: &[u8]) -> Self {
        let fields = match payload_fields(payload) {
            Ok(Some(fields)) => fields,
            Ok(None) => return Self::Other,
            Err(_) => return Self::Denied,
        };
        if fields.duplicate_type {
            return Self::Denied;
        }
        if fields.kind.as_deref() == Some("response.create") {
            Self::ResponseCreate(fields.identity)
        } else {
            Self::Other
        }
    }
}

pub(super) fn payload_identity(payload: &[u8]) -> ThreadIdentity {
    payload_fields(payload).map_or(ThreadIdentity::Denied, |fields| {
        fields.map_or(ThreadIdentity::Absent, |fields| {
            if fields.duplicate_type {
                ThreadIdentity::Denied
            } else {
                fields.identity
            }
        })
    })
}

fn payload_fields(payload: &[u8]) -> Result<Option<PayloadFields>, serde_json::Error> {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return Ok(None);
    };
    if !value.is_object() {
        return Ok(None);
    }
    serde_json::from_slice(payload).map(Some)
}

struct PayloadFields {
    kind: Option<String>,
    duplicate_type: bool,
    identity: ThreadIdentity,
}

impl<'de> Deserialize<'de> for PayloadFields {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PayloadVisitor)
    }
}

struct PayloadVisitor;

impl<'de> Visitor<'de> for PayloadVisitor {
    type Value = PayloadFields;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a Codex request object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut identity = ThreadIdentity::Absent;
        let mut kind = None;
        let mut saw_type = false;
        let mut duplicate_type = false;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "type" => {
                    let value = map.next_value::<Value>()?;
                    duplicate_type |= std::mem::replace(&mut saw_type, true);
                    if kind.is_none() {
                        kind = value.as_str().map(str::to_owned);
                    }
                }
                "client_metadata" => {
                    identity = identity.merge(map.next_value::<MetadataIdentity>()?.0);
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(PayloadFields {
            kind,
            duplicate_type,
            identity,
        })
    }
}

struct MetadataIdentity(ThreadIdentity);

impl<'de> Deserialize<'de> for MetadataIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(MetadataVisitor)
    }
}

struct MetadataVisitor;

impl<'de> Visitor<'de> for MetadataVisitor {
    type Value = MetadataIdentity;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Codex client metadata")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut identity = ThreadIdentity::Absent;
        while let Some(key) = map.next_key::<String>()? {
            if key == "thread_id" {
                let value = map.next_value::<Value>()?;
                identity = identity.merge(value.as_str().map_or(ThreadIdentity::Denied, |value| {
                    ThreadIdentity::from_values([value])
                }));
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(MetadataIdentity(identity))
    }
}
