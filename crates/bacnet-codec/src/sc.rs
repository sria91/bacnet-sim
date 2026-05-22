/// BACnet/SC WebSocket frame codec (Addendum bj).
///
/// SC frames wrap BACnet NPDUs with a small header carrying function code,
/// control flags, message ID, and optional virtual addresses.

use bacnet_types::error::BacnetError;
use bytes::{BufMut, Bytes, BytesMut};

/// BACnet/SC BVLC function codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScFunction {
    BvlcResult = 0x00,
    EncapsulatedNpdu = 0x01,
    AddressResolution = 0x02,
    AddressResolutionAck = 0x03,
    Advertisement = 0x04,
    AdvertisementSolicitation = 0x05,
    ConnectRequest = 0x06,
    ConnectAccept = 0x07,
    DisconnectRequest = 0x08,
    DisconnectAck = 0x09,
    Heartbeat = 0x0A,
    HeartbeatAck = 0x0B,
    SecurePath = 0x0C,
}

/// Control flag bits for SC frames.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScControlFlags {
    pub orig_vaddr_present: bool,
    pub dest_vaddr_present: bool,
    pub dest_options_present: bool,
    pub data_options_present: bool,
}

/// A BACnet/SC frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ScFrame {
    pub function: u8,
    pub control: ScControlFlags,
    pub message_id: u16,
    pub originating_vmac: Option<[u8; 6]>,
    pub destination_vmac: Option<[u8; 6]>,
    pub payload: Bytes,
}

impl ScFrame {
    pub fn encapsulated_npdu(
        message_id: u16,
        orig: Option<[u8; 6]>,
        dest: Option<[u8; 6]>,
        npdu: Bytes,
    ) -> Self {
        Self {
            function: ScFunction::EncapsulatedNpdu as u8,
            control: ScControlFlags {
                orig_vaddr_present: orig.is_some(),
                dest_vaddr_present: dest.is_some(),
                ..Default::default()
            },
            message_id,
            originating_vmac: orig,
            destination_vmac: dest,
            payload: npdu,
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u8(self.function);
        let mut ctrl = 0u8;
        if self.control.orig_vaddr_present { ctrl |= 0x08; }
        if self.control.dest_vaddr_present { ctrl |= 0x04; }
        if self.control.dest_options_present { ctrl |= 0x02; }
        if self.control.data_options_present { ctrl |= 0x01; }
        buf.put_u8(ctrl);
        buf.put_u16(self.message_id);
        if let Some(vmac) = self.originating_vmac {
            buf.extend_from_slice(&vmac);
        }
        if let Some(vmac) = self.destination_vmac {
            buf.extend_from_slice(&vmac);
        }
        buf.extend_from_slice(&self.payload);
        buf.freeze()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 4 {
            return Err(BacnetError::DecodeError("SC frame too short".into()));
        }
        let function = buf[0];
        let ctrl_byte = buf[1];
        let message_id = u16::from_be_bytes([buf[2], buf[3]]);
        let control = ScControlFlags {
            orig_vaddr_present: (ctrl_byte & 0x08) != 0,
            dest_vaddr_present: (ctrl_byte & 0x04) != 0,
            dest_options_present: (ctrl_byte & 0x02) != 0,
            data_options_present: (ctrl_byte & 0x01) != 0,
        };
        let mut pos = 4usize;
        let originating_vmac = if control.orig_vaddr_present {
            if buf.len() < pos + 6 {
                return Err(BacnetError::DecodeError("SC frame: originating vMAC truncated".into()));
            }
            let mut vmac = [0u8; 6];
            vmac.copy_from_slice(&buf[pos..pos + 6]);
            pos += 6;
            Some(vmac)
        } else {
            None
        };
        let destination_vmac = if control.dest_vaddr_present {
            if buf.len() < pos + 6 {
                return Err(BacnetError::DecodeError("SC frame: destination vMAC truncated".into()));
            }
            let mut vmac = [0u8; 6];
            vmac.copy_from_slice(&buf[pos..pos + 6]);
            pos += 6;
            Some(vmac)
        } else {
            None
        };
        let payload = Bytes::copy_from_slice(&buf[pos..]);
        Ok(Self { function, control, message_id, originating_vmac, destination_vmac, payload })
    }
}
