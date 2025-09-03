use async_trait::async_trait;
use dotenvy::dotenv;
use regex::{Captures, Regex};
use serde::de::DeserializeOwned;
use std::{env, fs};
use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum LoadConfigError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("TOML error: {0}")]
    TomlError(#[from] toml::de::Error),
    #[allow(dead_code)]
    #[error("Error loading config: {0}")]
    ConfigError(String),
}

#[async_trait]
pub trait FluxConfigLoader {
    type SectionType;

    #[allow(dead_code)]
    async fn load_section_from_file(file_name: String) -> Result<Self::SectionType, LoadConfigError>;
}

pub trait FluxConfigLoaderSync {
    type SectionType;

    #[allow(dead_code)]
    fn load_section_from_file_sync(file_name: String) -> Result<Self::SectionType, LoadConfigError>;
}

#[allow(dead_code)]
#[async_trait]
pub trait OptionalFluxConfigLoader {
    type SectionType;

    async fn load_optional_section_from_file(file_name: String) -> Result<Option<Self::SectionType>, LoadConfigError>;
}

#[allow(dead_code)]
pub trait OptionalFluxConfigLoaderSync {
    type SectionType;

    fn load_optional_section_from_file_sync(file_name: String) -> Result<Option<Self::SectionType>, LoadConfigError>;
}

pub async fn load_from_file<T: DeserializeOwned>(file_name: String) -> Result<T, LoadConfigError> {
    dotenv().ok();
    let contents = tokio::fs::read_to_string(file_name).await?;
    let contents = expand_vars(&contents);
    let config: T = toml::from_str(&contents)?;
    Ok(config)
}

pub fn load_from_file_sync<T: DeserializeOwned>(file_name: String) -> Result<T, LoadConfigError> {
    dotenv().ok();
    let contents = fs::read_to_string(file_name)?;
    let contents = expand_vars(&contents);
    let config: T = toml::from_str(&contents)?;
    Ok(config)
}

fn expand_vars(raw_config: &str) -> String {
    // https://stackoverflow.com/questions/62888154/rust-load-environment-variables-into-log4rs-yml-file
    let re = Regex::new(r"\$\{([a-zA-Z_][0-9a-zA-Z_]*)\}").unwrap();
    re.replace_all(raw_config, |caps: &Captures| match env::var(&caps[1]) {
        Ok(val) => val,
        Err(_) => caps[0].to_string(),
    })
    .to_string()
}
