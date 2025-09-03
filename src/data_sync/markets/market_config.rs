use crate::utils::config_loader::{FluxConfigLoader, FluxConfigLoaderSync, LoadConfigError, load_from_file, load_from_file_sync};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Clone, Deserialize, Debug)]
pub struct MarketConfigRoot {
    pub market: MarketConfigSection,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MarketConfigSection {
    pub max_hops: u8,
}

impl MarketConfigSection {
    pub fn with_max_hops(&self, max_hops: u8) -> Self {
        Self { max_hops }
    }
}

impl Default for MarketConfigSection {
    fn default() -> Self {
        Self { max_hops: 3 }
    }
}

#[async_trait]
impl FluxConfigLoader for MarketConfigSection {
    type SectionType = MarketConfigSection;

    async fn load_section_from_file(file_name: String) -> Result<Self::SectionType, LoadConfigError> {
        let root: MarketConfigRoot = load_from_file(file_name).await?;
        Ok(root.market)
    }
}

impl FluxConfigLoaderSync for MarketConfigSection {
    type SectionType = MarketConfigSection;

    fn load_section_from_file_sync(file_name: String) -> Result<Self::SectionType, LoadConfigError> {
        let root: MarketConfigRoot = load_from_file_sync(file_name)?;
        Ok(root.market)
    }
}
