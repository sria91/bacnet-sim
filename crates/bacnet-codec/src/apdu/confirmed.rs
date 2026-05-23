use bacnet_types::{
    encoding::tags::{
        decode_application_value, decode_ctx_bool, decode_ctx_object_id, decode_ctx_property_id,
        decode_ctx_u32, has_context_tag, is_closing, is_opening,
    },
    error::BacnetError,
    ObjectId, PropertyIdentifier,
};
use bytes::{BufMut, BytesMut};

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
        let flags = if self.segmented_accepted {
            0x08u8
        } else {
            0x00
        } | if self.more_follows { 0x04 } else { 0x00 };
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

    /// Decode a Confirmed-Request PDU from a raw APDU byte slice.
    ///
    /// Layout: `[PDU-type(1)] [max-segs-resp(1)] [invoke-id(1)] [service-choice(1)] [body...]`
    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 4 {
            return Err(BacnetError::DecodeError(
                "confirmed request too short".into(),
            ));
        }
        let flags = buf[0];
        let invoke_id = buf[2];
        let service_choice = buf[3];
        let body = &buf[4..];

        let service = match service_choice {
            12 => ConfirmedServiceRequest::ReadProperty(decode_read_property(body)?),
            14 => ConfirmedServiceRequest::ReadPropertyMultiple(decode_rpm(body)?),
            15 => ConfirmedServiceRequest::WriteProperty(decode_write_property(body)?),
            5 => ConfirmedServiceRequest::SubscribeCov(decode_subscribe_cov(body)?),
            other => {
                return Err(BacnetError::DecodeError(format!(
                    "unsupported confirmed service choice {other}"
                )));
            }
        };

        Ok(Self {
            segmented_accepted: (flags & 0x08) != 0,
            more_follows: (flags & 0x04) != 0,
            segmented_response_accepted: true,
            max_segments: MaxSegments::Unspecified,
            max_response: 480,
            invoke_id,
            sequence_number: None,
            proposed_window: None,
            service,
        })
    }
}

// ---------------------------------------------------------------------------
// Service body decoders
// ---------------------------------------------------------------------------

/// Decode a ReadProperty-Request body.
///
/// ```text
/// object-identifier    [0] BACnetObjectIdentifier
/// property-identifier  [1] BACnetPropertyIdentifier
/// property-array-index [2] Unsigned OPTIONAL
/// ```
fn decode_read_property(body: &[u8]) -> Result<ReadPropertyRequest, BacnetError> {
    let mut pos = 0;
    let object_id = decode_ctx_object_id(body, &mut pos, 0)?;
    let property_id = decode_ctx_property_id(body, &mut pos, 1)?;
    let array_index = if has_context_tag(body, pos, 2) {
        Some(decode_ctx_u32(body, &mut pos, 2)?)
    } else {
        None
    };
    Ok(ReadPropertyRequest {
        object_id,
        property_id,
        array_index,
    })
}

/// Decode a WriteProperty-Request body.
///
/// ```text
/// object-identifier    [0] BACnetObjectIdentifier
/// property-identifier  [1] BACnetPropertyIdentifier
/// property-array-index [2] Unsigned OPTIONAL
/// property-value       [3] ABSTRACT-SYNTAX.&Type (opening tag + value + closing tag)
/// priority             [4] Unsigned (1..16) OPTIONAL
/// ```
fn decode_write_property(body: &[u8]) -> Result<WritePropertyRequest, BacnetError> {
    let mut pos = 0;
    let object_id = decode_ctx_object_id(body, &mut pos, 0)?;
    let property_id = decode_ctx_property_id(body, &mut pos, 1)?;
    let array_index = if has_context_tag(body, pos, 2) {
        Some(decode_ctx_u32(body, &mut pos, 2)?)
    } else {
        None
    };

    // Opening tag 3
    if !is_opening(body, pos, 3) {
        return Err(BacnetError::DecodeError(
            "WriteProperty: expected opening tag [3] for property-value".into(),
        ));
    }
    pos += 1;

    let value = decode_application_value(body, &mut pos)?;

    // Closing tag 3
    if !is_closing(body, pos, 3) {
        return Err(BacnetError::DecodeError(
            "WriteProperty: expected closing tag [3] for property-value".into(),
        ));
    }
    pos += 1;

    let priority = if has_context_tag(body, pos, 4) {
        let p = decode_ctx_u32(body, &mut pos, 4)?;
        Some(p as u8)
    } else {
        None
    };

    Ok(WritePropertyRequest {
        object_id,
        property_id,
        array_index,
        value,
        priority,
    })
}

/// Decode a ReadPropertyMultiple-Request body.
///
/// ```text
/// Repeat until end-of-buffer:
///   object-identifier              [0] BACnetObjectIdentifier
///   list-of-property-references    [1] LIST OF BACnetPropertyReference
///     (each reference)
///       property-identifier        [0] BACnetPropertyIdentifier
///       property-array-index       [1] Unsigned OPTIONAL
/// ```
fn decode_rpm(
    body: &[u8],
) -> Result<Vec<(ObjectId, Vec<(PropertyIdentifier, Option<u32>)>)>, BacnetError> {
    let mut pos = 0;
    let mut specs = Vec::new();

    while pos < body.len() {
        let object_id = decode_ctx_object_id(body, &mut pos, 0)?;

        if !is_opening(body, pos, 1) {
            return Err(BacnetError::DecodeError(
                "RPM: expected opening tag [1] for property list".into(),
            ));
        }
        pos += 1; // consume opening tag

        let mut props = Vec::new();
        while !is_closing(body, pos, 1) {
            if pos >= body.len() {
                return Err(BacnetError::DecodeError(
                    "RPM: unexpected end of buffer in property list".into(),
                ));
            }
            let prop_id = decode_ctx_property_id(body, &mut pos, 0)?;
            let arr_idx = if has_context_tag(body, pos, 1) {
                Some(decode_ctx_u32(body, &mut pos, 1)?)
            } else {
                None
            };
            props.push((prop_id, arr_idx));
        }
        pos += 1; // consume closing tag

        specs.push((object_id, props));
    }

    Ok(specs)
}

/// Decode a SubscribeCOV-Request body.
///
/// ```text
/// subscriber-process-identifier    [0] Unsigned32
/// monitored-object-identifier      [1] BACnetObjectIdentifier
/// issue-confirmed-notifications    [2] BOOLEAN OPTIONAL
/// lifetime                         [3] Unsigned OPTIONAL
/// ```
fn decode_subscribe_cov(body: &[u8]) -> Result<SubscribeCovRequest, BacnetError> {
    let mut pos = 0;
    let subscriber_process_id = decode_ctx_u32(body, &mut pos, 0)?;
    let monitored_object = decode_ctx_object_id(body, &mut pos, 1)?;

    let issue_confirmed = if has_context_tag(body, pos, 2) {
        Some(decode_ctx_bool(body, &mut pos, 2)?)
    } else {
        None
    };

    let lifetime = if has_context_tag(body, pos, 3) {
        Some(decode_ctx_u32(body, &mut pos, 3)?)
    } else {
        None
    };

    Ok(SubscribeCovRequest {
        subscriber_process_id,
        monitored_object,
        issue_confirmed,
        lifetime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bacnet_types::ObjectType;

    #[test]
    fn decode_read_property_request() {
        // Build: context [0] AnalogInput/1, context [1] PresentValue (85=0x55)
        // Tag [0] obj-id: 0x0C + 4 bytes (AI instance 1 = (0<<22)|1)
        // Tag [1] prop-id: 0x19 (len=1) + 0x55
        let mut buf = vec![0x0C_u8];
        let oid_raw: u32 = (0u32 << 22) | 1; // AI instance 1
        buf.extend_from_slice(&oid_raw.to_be_bytes());
        buf.push(0x19); // context tag 1, len 1
        buf.push(85); // PresentValue = 85

        let req = decode_read_property(&buf).unwrap();
        assert_eq!(req.object_id.object_type, ObjectType::AnalogInput);
        assert_eq!(req.object_id.instance, 1);
        assert_eq!(req.property_id, PropertyIdentifier::PresentValue);
        assert_eq!(req.array_index, None);
    }

    #[test]
    fn decode_read_property_with_array_index() {
        // [0] Device/1234, [1] ObjectList (76=0x4C), [2] array-index=5
        let mut buf = vec![0x0C_u8];
        let dev_type = bacnet_types::ObjectType::Device as u32;
        let oid_raw: u32 = (dev_type << 22) | 1234;
        buf.extend_from_slice(&oid_raw.to_be_bytes());
        buf.extend_from_slice(&[0x19, 76]); // [1] ObjectList
        buf.extend_from_slice(&[0x29, 5]); // [2] array-index = 5
        let req = decode_read_property(&buf).unwrap();
        assert_eq!(req.array_index, Some(5));
    }

    #[test]
    fn decode_write_property_request() {
        // Write PresentValue=22.5 to AnalogInput/1, no priority
        let oid_raw: u32 = (0u32 << 22) | 1;
        let val: f32 = 22.5;
        let mut buf = vec![0x0C_u8];
        buf.extend_from_slice(&oid_raw.to_be_bytes());
        buf.extend_from_slice(&[0x19, 85]); // [1] PresentValue
        buf.push(0x3E); // opening tag [3]
        buf.push(0x44); // Real tag, len 4
        buf.extend_from_slice(&val.to_be_bytes());
        buf.push(0x3F); // closing tag [3]

        let req = decode_write_property(&buf).unwrap();
        assert_eq!(req.property_id, PropertyIdentifier::PresentValue);
        assert_eq!(req.value, bacnet_types::PropertyValue::Real(22.5));
        assert_eq!(req.priority, None);
    }

    #[test]
    fn decode_write_property_with_priority() {
        let oid_raw: u32 = (1u32 << 22) | 2; // AnalogOutput/2
        let val: f32 = 75.0;
        let mut buf = vec![0x0C_u8];
        buf.extend_from_slice(&oid_raw.to_be_bytes());
        buf.extend_from_slice(&[0x19, 85]); // [1] PresentValue
        buf.push(0x3E);
        buf.push(0x44);
        buf.extend_from_slice(&val.to_be_bytes());
        buf.push(0x3F);
        buf.extend_from_slice(&[0x49, 8]); // [4] priority = 8

        let req = decode_write_property(&buf).unwrap();
        assert_eq!(req.priority, Some(8));
        assert_eq!(req.value, bacnet_types::PropertyValue::Real(75.0));
    }

    #[test]
    fn decode_rpm_two_properties() {
        // AnalogInput/1: PresentValue + StatusFlags
        let oid_raw: u32 = 1;
        let mut buf = vec![0x0C_u8];
        buf.extend_from_slice(&oid_raw.to_be_bytes());
        buf.push(0x1E); // opening tag [1]
        buf.extend_from_slice(&[0x09, 85]); // [0] PresentValue
        buf.extend_from_slice(&[0x09, 111]); // [0] StatusFlags
        buf.push(0x1F); // closing tag [1]

        let specs = decode_rpm(&buf).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].1.len(), 2);
        assert_eq!(specs[0].1[0].0, PropertyIdentifier::PresentValue);
        assert_eq!(specs[0].1[1].0, PropertyIdentifier::StatusFlags);
    }
}
