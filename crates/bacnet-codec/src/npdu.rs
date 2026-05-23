/// BACnet NPDU (Network Protocol Data Unit) codec.
///
/// The NPDU carries routing information between the BVLL and APDU layers.
use bacnet_types::error::BacnetError;
use bytes::{BufMut, Bytes, BytesMut};

/// NPDU control flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct NpduControl {
    pub network_message: bool,
    pub destination_specifier: bool,
    pub source_specifier: bool,
    pub expecting_reply: bool,
    pub priority: u8, // 0-3
}

/// Decoded NPDU header.
#[derive(Debug, Clone)]
pub struct Npdu {
    pub version: u8,
    pub control: NpduControl,
    pub destination: Option<NpduAddress>,
    pub source: Option<NpduAddress>,
    pub hop_count: Option<u8>,
    pub apdu: Bytes,
}

/// A BACnet network address embedded in an NPDU.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NpduAddress {
    pub network: u16,
    pub mac: Vec<u8>,
}

impl Npdu {
    /// Encode a local-delivery NPDU (no destination/source specifiers).
    pub fn encode_local(expecting_reply: bool, apdu: &[u8]) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u8(0x01); // BACnet version
        let ctrl = if expecting_reply { 0x04u8 } else { 0x00 };
        buf.put_u8(ctrl);
        buf.extend_from_slice(apdu);
        buf.freeze()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 2 {
            return Err(BacnetError::DecodeError("NPDU too short".into()));
        }
        let version = buf[0];
        if version != 0x01 {
            return Err(BacnetError::DecodeError(format!(
                "unknown NPDU version {version:#02x}"
            )));
        }
        let ctrl_byte = buf[1];
        let control = NpduControl {
            network_message: (ctrl_byte & 0x80) != 0,
            destination_specifier: (ctrl_byte & 0x20) != 0,
            source_specifier: (ctrl_byte & 0x08) != 0,
            expecting_reply: (ctrl_byte & 0x04) != 0,
            priority: ctrl_byte & 0x03,
        };

        let mut pos = 2usize;

        // Optional destination network address
        let destination = if control.destination_specifier {
            if buf.len() < pos + 3 {
                return Err(BacnetError::DecodeError("NPDU too short for DNET".into()));
            }
            let dnet = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
            pos += 2;
            let dlen = buf[pos] as usize;
            pos += 1;
            let mac = buf[pos..pos + dlen].to_vec();
            pos += dlen;
            Some(NpduAddress { network: dnet, mac })
        } else {
            None
        };

        // Optional source network address
        let source = if control.source_specifier {
            if buf.len() < pos + 3 {
                return Err(BacnetError::DecodeError("NPDU too short for SNET".into()));
            }
            let snet = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
            pos += 2;
            let slen = buf[pos] as usize;
            pos += 1;
            let mac = buf[pos..pos + slen].to_vec();
            pos += slen;
            Some(NpduAddress { network: snet, mac })
        } else {
            None
        };

        // Hop count present when destination specifier set
        let hop_count = if control.destination_specifier {
            if buf.len() <= pos {
                return Err(BacnetError::DecodeError(
                    "NPDU too short for hop count".into(),
                ));
            }
            let h = buf[pos];
            pos += 1;
            Some(h)
        } else {
            None
        };

        let apdu = Bytes::copy_from_slice(&buf[pos..]);
        Ok(Self {
            version,
            control,
            destination,
            source,
            hop_count,
            apdu,
        })
    }
}
