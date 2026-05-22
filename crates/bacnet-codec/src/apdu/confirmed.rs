use bacnet_types::{
    error::BacnetError,
    ObjectId, PropertyIdentifier,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// BACnet confirmed-service choice codes (ASHRAE 135-2020 §21.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConfirmedServiceChoice {
    AcknowledgeAlarm = 0,
    CovNotification = 1,
    EventNotification = 2,
    GetAlarmSummary = 3,
    GetEnrollmentSummary = 4,
    SubscribeCov = 5,
    AtomicReadFile = 6,
    AtomicWriteFile = 7,
    AddListElement = 8,
    RemoveListElement = 9,
    CreateObject = 10,
    DeleteObject = 11,
    ReadProperty = 12,
    ReadPropertyMultiple = 14,
    WriteProperty = 15,
    WritePropertyMultiple = 16,
    DeviceCommunicationControl = 17,
    PrivateTransfer = 18,
    TextMessage = 19,
    ReinitializeDevice = 20,
    VtOpen = 21,
    VtClose = 22,
    VtData = 23,
    Authenticate = 24,
    RequestKey = 25,
    ReadRange = 26,
    LifeSafetyOperation = 27,
    SubscribeCovProperty = 28,
    GetEventInformation = 29,
    SubscribeCovPropertyMultiple = 30,
    ConfirmedCovNotificationMultiple = 31,
    ConfirmedAuditNotification = 32,
    AuditLogQuery = 33,
}

/// Segmentation support options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Segmentation {
    Both,
    Transmit,
    Receive,
    #[default]
    NoSegmentation,
}

/// Max segments accepted by the peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxSegments {
    Two,
    Four,
    Eight,
    Sixteen,
    ThirtyTwo,
    SixtyFour,
    MoreThan64,
    Unspecified,
}

/// ReadProperty service request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPropertyRequest {
    pub object_id: ObjectId,
    pub property_id: PropertyIdentifier,
    pub array_index: Option<u32>,
}

/// WriteProperty service request.
#[derive(Debug, Clone, PartialEq)]
pub struct WritePropertyRequest {
    pub object_id: ObjectId,
    pub property_id: PropertyIdentifier,
    pub array_index: Option<u32>,
    pub value: bacnet_types::PropertyValue,
    pub priority: Option<u8>,
}

/// SubscribeCOV service request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscribeCovRequest {
    pub subscriber_process_id: u32,
    pub monitored_object: ObjectId,
    pub issue_confirmed: Option<bool>, // None = unsubscribe
    pub lifetime: Option<u32>,
}

/// Who-Is unconfirmed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoIsRequest {
    pub low_limit: Option<u32>,
    pub high_limit: Option<u32>,
}

/// Union of all confirmed service request bodies.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmedServiceRequest {
    ReadProperty(ReadPropertyRequest),
    WriteProperty(WritePropertyRequest),
    SubscribeCov(SubscribeCovRequest),
    ReadPropertyMultiple(Vec<(ObjectId, Vec<(PropertyIdentifier, Option<u32>)>)>),
    // Additional services added as implemented.
}

/// A BACnet Confirmed-Request PDU.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedRequest {
    pub segmented_accepted: bool,
    pub more_follows: bool,
    pub segmented_response_accepted: bool,
    pub max_segments: MaxSegments,
    pub max_response: u16,
    pub invoke_id: u8,
    pub sequence_number: Option<u8>,
    pub proposed_window: Option<u8>,
    pub service: ConfirmedServiceRequest,
}

impl ConfirmedRequest {
    pub fn encode(&self, buf: &mut BytesMut) {
        // PDU type byte: 0x00 for confirmed-request
        // Bits: 7-4 = 0, 3 = SA, 2 = MF, 1-0 = seg bits
        let flags = if self.segmented_accepted { 0x08u8 } else { 0x00 }
            | if self.more_follows { 0x04 } else { 0x00 };
        buf.put_u8(flags);
        buf.put_u8(0x04); // maxsegs/maxresp placeholder
        buf.put_u8(self.invoke_id);
        // Service choice
        let choice = match &self.service {
            ConfirmedServiceRequest::ReadProperty(_) => 12u8,
            ConfirmedServiceRequest::WriteProperty(_) => 15,
            ConfirmedServiceRequest::SubscribeCov(_) => 5,
            ConfirmedServiceRequest::ReadPropertyMultiple(_) => 14,
        };
        buf.put_u8(choice);
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 4 {
            return Err(BacnetError::DecodeError("confirmed request too short".into()));
        }
        let flags = buf[0];
        let invoke_id = buf[2];
        let _service_choice = buf[3];
        Ok(Self {
            segmented_accepted: (flags & 0x08) != 0,
            more_follows: (flags & 0x04) != 0,
            segmented_response_accepted: true,
            max_segments: MaxSegments::Unspecified,
            max_response: 480,
            invoke_id,
            sequence_number: None,
            proposed_window: None,
            service: ConfirmedServiceRequest::ReadProperty(ReadPropertyRequest {
                object_id: ObjectId {
                    object_type: bacnet_types::ObjectType::AnalogInput,
                    instance: 0,
                },
                property_id: PropertyIdentifier::PresentValue,
                array_index: None,
            }),
        })
    }
}
