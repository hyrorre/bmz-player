use crate::config::profile_config::{IrConfig, IrProviderConfig};

pub fn configured_provider_key(entry: &IrProviderConfig) -> Option<&str> {
    let key = entry.provider_key.trim();
    if key.is_empty() { None } else { Some(key) }
}

pub fn configured_provider_display_name(entry: &IrProviderConfig) -> Option<&str> {
    let provider_key = configured_provider_key(entry)?;
    if crate::ir::bokutachi::is_config(entry) {
        Some("Bokutachi")
    } else if crate::ir::bms_ir::is_bms_ir_config(entry) {
        Some("BMS-IR")
    } else if crate::ir::rian_ir::is_rian_ir_config(entry) {
        Some("rianIR")
    } else if matches!(entry.provider.trim().to_ascii_lowercase().as_str(), "bmz" | "bmz-official")
        || matches!(provider_key, "bmz" | "bmz-official")
    {
        Some("BMZ IR")
    } else {
        Some(provider_key)
    }
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

pub fn primary_provider_config(ir_config: &IrConfig) -> Option<&IrProviderConfig> {
    let usable = |entry: &IrProviderConfig| {
        entry.enabled && !entry.base_url.is_empty() && configured_provider_key(entry).is_some()
    };
    let primary_provider = ir_config.primary_provider.trim();
    if primary_provider.is_empty() {
        return ir_config
            .providers
            .iter()
            .find(|entry| usable(entry) && !crate::ir::bokutachi::is_config(entry));
    }
    ir_config
        .providers
        .iter()
        .filter(|entry| usable(entry))
        .find(|entry| configured_provider_key(entry).is_some_and(|key| key == primary_provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bms_ir_can_be_selected_as_primary_provider() {
        let mut provider = IrProviderConfig::bms_ir();
        provider.provider_key = crate::ir::bms_ir::BMS_IR_PROVIDER.to_string();
        provider.enabled = true;
        let config = IrConfig {
            primary_provider: crate::ir::bms_ir::BMS_IR_PROVIDER.to_string(),
            providers: vec![provider],
            ..IrConfig::default()
        };

        let selected = primary_provider_config(&config).expect("BMS-IR primary provider");
        assert!(crate::ir::bms_ir::is_bms_ir_config(selected));
    }

    #[test]
    fn configured_primary_does_not_fall_back_to_another_provider() {
        let mut fallback = IrProviderConfig::rian_ir();
        fallback.provider_key = crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string();
        fallback.enabled = true;
        let config = IrConfig {
            primary_provider: crate::ir::bms_ir::BMS_IR_PROVIDER.to_string(),
            providers: vec![fallback],
            ..IrConfig::default()
        };

        assert!(primary_provider_config(&config).is_none());
    }

    #[test]
    fn legacy_empty_primary_uses_the_first_usable_provider() {
        let mut provider = IrProviderConfig::rian_ir();
        provider.provider_key = crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string();
        provider.enabled = true;
        let config = IrConfig {
            primary_provider: String::new(),
            providers: vec![provider],
            ..IrConfig::default()
        };

        let selected = primary_provider_config(&config).expect("legacy fallback provider");
        assert!(crate::ir::rian_ir::is_rian_ir_config(selected));
    }
}
