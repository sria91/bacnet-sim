/// Segmentation reassembly buffer and ACK logic.
///
/// BACnet segments large APDUs when they exceed `MaxAPDULengthAccepted`.
/// The dispatcher uses this to reassemble inbound segments and to drive
/// the outbound window/retry protocol.

use std::collections::HashMap;
use bacnet_types::error::BacnetError;
use bytes::{Bytes, BytesMut};

/// A partially-reassembled segmented APDU.
pub struct SegmentBuffer {
    pub invoke_id: u8,
    pub sequence_count: usize,
    pub window_size: u8,
    pub segments: HashMap<u8, Bytes>,
    pub total_expected: Option<usize>,
}

impl SegmentBuffer {
    pub fn new(invoke_id: u8, window_size: u8) -> Self {
        Self {
            invoke_id,
            sequence_count: 0,
            window_size,
            segments: HashMap::new(),
            total_expected: None,
        }
    }

    /// Add a segment. Returns `Some(reassembled)` when all segments are present.
    pub fn add_segment(
        &mut self,
        sequence_number: u8,
        more_follows: bool,
        data: Bytes,
    ) -> Option<Bytes> {
        self.segments.insert(sequence_number, data);
        if !more_follows {
            self.total_expected = Some(sequence_number as usize + 1);
        }
        if let Some(total) = self.total_expected {
            if self.segments.len() == total {
                return Some(self.reassemble(total));
            }
        }
        None
    }

    fn reassemble(&self, total: usize) -> Bytes {
        let mut buf = BytesMut::new();
        for i in 0..total as u8 {
            if let Some(seg) = self.segments.get(&i) {
                buf.extend_from_slice(seg);
            }
        }
        buf.freeze()
    }
}
