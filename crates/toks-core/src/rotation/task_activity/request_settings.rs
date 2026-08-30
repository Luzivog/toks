use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::rotation::ThreadRequestSettings;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedThreadRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service_tier: Option<String>,
}

pub(super) fn serialize<S>(
    settings: &ThreadRequestSettings,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let ThreadRequestSettings {
        model,
        reasoning_effort,
        service_tier,
    } = settings;
    PersistedThreadRequestSettings {
        model: model.clone(),
        reasoning_effort: reasoning_effort.clone(),
        service_tier: service_tier.clone(),
    }
    .serialize(serializer)
}

pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<ThreadRequestSettings, D::Error>
where
    D: Deserializer<'de>,
{
    let persisted = PersistedThreadRequestSettings::deserialize(deserializer)?;
    Ok(ThreadRequestSettings {
        model: persisted.model,
        reasoning_effort: persisted.reasoning_effort,
        service_tier: persisted.service_tier,
    })
}
