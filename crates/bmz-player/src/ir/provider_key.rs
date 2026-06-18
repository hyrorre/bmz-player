use crate::config::profile_config::{IrConfig, IrProviderConfig};

pub fn configured_provider_key(entry: &IrProviderConfig) -> Option<&str> {
    let key = entry.provider_key.trim();
    if key.is_empty() { None } else { Some(key) }
}

pub fn provider_config_for_key<'a>(
    ir_config: &'a IrConfig,
    key: &str,
) -> Option<&'a IrProviderConfig> {
    ir_config.providers.iter().find(|entry| {
        entry.enabled
            && !entry.base_url.is_empty()
            && configured_provider_key(entry).is_some_and(|provider_key| provider_key == key)
    })
}
