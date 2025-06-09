use alloy_reth_provider::alloy_db::DBTransportError;
use alloy_transport::{RpcError, TransportErrorKind};
use eyre::Report;
use reth_errors::ProviderError;
use reth_errors::RethError;
use reth_revm::context::DBErrorMarker;

#[derive(Debug, thiserror::Error)]
pub enum FluxDBError {
    // Reth provider
    #[error("header not found")]
    HeaderNotFound(String),
    #[error("unknown block or tx index")]
    UnknownBlockOrTxIndex,
    #[error(transparent)]
    RethInternal(RethError),
    #[error(transparent)]
    InternalEyre(Report),

    // AlloyDB
    #[error(transparent)]
    MiddlewareError(#[from] RpcError<TransportErrorKind>),
    #[error(transparent)]
    DBTransportError(#[from] DBTransportError),
}

impl DBErrorMarker for FluxDBError {}

impl From<ProviderError> for FluxDBError {
    fn from(error: ProviderError) -> Self {
        match error {
            ProviderError::HeaderNotFound(hash) => Self::HeaderNotFound(hash.to_string()),
            ProviderError::BlockHashNotFound(hash) | ProviderError::UnknownBlockHash(hash) => Self::HeaderNotFound(hash.to_string()),
            ProviderError::BestBlockNotFound => Self::HeaderNotFound("latest".to_string()),
            ProviderError::BlockNumberForTransactionIndexNotFound => Self::UnknownBlockOrTxIndex,
            ProviderError::TotalDifficultyNotFound(num) => Self::HeaderNotFound(num.to_string()),
            ProviderError::FinalizedBlockNotFound => Self::HeaderNotFound("finalized".to_string()),
            ProviderError::SafeBlockNotFound => Self::HeaderNotFound("safe".to_string()),
            err => Self::RethInternal(err.into()),
        }
    }
}

impl From<Report> for FluxDBError {
    fn from(error: Report) -> Self {
        Self::InternalEyre(error)
    }
}
