use bacnet_types::error::{ErrorClass, ErrorCode};
use bytes::{BufMut, BytesMut};

/// A BACnet Error PDU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorPdu {
    pub invoke_id: u8,
    pub service: u8,
    pub error_class: ErrorClass,
    pub error_code: ErrorCode,
}

impl ErrorPdu {
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(0x50); // Error PDU type
        buf.put_u8(self.invoke_id);
        buf.put_u8(self.service);
        // Error class + code as enumerated application tags
        buf.put_u8(0x91);
        buf.put_u8(self.error_class as u8);
        buf.put_u8(0x91);
        buf.put_u8(self.error_code_byte());
    }

    #[allow(dead_code)]
    fn error_class_byte(&self) -> u8 {
        match self.error_class {
            ErrorClass::Device => 0,
            ErrorClass::Object => 1,
            ErrorClass::Property => 2,
            ErrorClass::Resources => 3,
            ErrorClass::Security => 4,
            ErrorClass::Services => 5,
            ErrorClass::Vt => 6,
            ErrorClass::Communication => 7,
        }
    }

    fn error_code_byte(&self) -> u8 {
        match self.error_code {
            ErrorCode::UnknownObject => 31,
            ErrorCode::UnknownProperty => 32,
            ErrorCode::WriteAccessDenied => 40,
            ErrorCode::InvalidDataType => 9,
            ErrorCode::ValueOutOfRange => 37,
            ErrorCode::ServiceRequestDenied => 29,
            ErrorCode::NotConfigured => 133,
            ErrorCode::Other(v) => v as u8,
        }
    }
}

/// A BACnet Reject PDU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectPdu {
    pub invoke_id: u8,
    pub reason: RejectReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    Other = 0,
    BufferOverflow = 1,
    InconsistentParameters = 2,
    InvalidParameterDataType = 3,
    InvalidTag = 4,
    MissingRequiredParameter = 5,
    ParameterOutOfRange = 6,
    TooManyArguments = 7,
    UndefinedEnumeration = 8,
    UnrecognizedService = 9,
}

impl RejectPdu {
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(0x60); // Reject PDU type
        buf.put_u8(self.invoke_id);
        buf.put_u8(self.reason as u8);
    }
}

/// A BACnet Abort PDU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbortPdu {
    pub invoke_id: u8,
    pub server: bool,
    pub reason: AbortReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    Other = 0,
    BufferOverflow = 1,
    InvalidApduInThisState = 2,
    PreemptedByHigherPriorityTask = 3,
    SegmentationNotSupported = 4,
}

impl AbortPdu {
    pub fn encode(&self, buf: &mut BytesMut) {
        let server_bit = if self.server { 0x01 } else { 0x00 };
        buf.put_u8(0x70 | server_bit); // Abort PDU type
        buf.put_u8(self.invoke_id);
        buf.put_u8(self.reason as u8);
    }
}
