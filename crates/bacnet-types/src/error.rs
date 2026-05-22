use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BacnetError {
    #[error("unknown object")]
    UnknownObject,
    #[error("unknown property")]
    UnknownProperty,
    #[error("write access denied")]
    WriteAccessDenied,
    #[error("value out of range")]
    ValueOutOfRange,
    #[error("invalid data type")]
    InvalidDataType,
    #[error("decode error: {0}")]
    DecodeError(String),
    #[error("encode error: {0}")]
    EncodeError(String),
    #[error("unsupported service")]
    UnsupportedService,
    #[error("error class {error_class:?} code {error_code:?}")]
    ServiceError {
        error_class: ErrorClass,
        error_code: ErrorCode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Device,
    Object,
    Property,
    Resources,
    Security,
    Services,
    Vt,
    Communication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnknownObject,
    UnknownProperty,
    WriteAccessDenied,
    InvalidDataType,
    ValueOutOfRange,
    ServiceRequestDenied,
    NotConfigured,
    Other(u32),
}
