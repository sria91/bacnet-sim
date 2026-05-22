/// Context-tagged encoding helpers.
///
/// BACnet uses context tags (bit 3 = 1) to disambiguate fields within
/// structured PDUs.  These helpers wrap the common patterns.

use bytes::BytesMut;

/// Emit an opening/closing context tag pair around `inner`.
pub fn with_context_tag(buf: &mut BytesMut, tag: u8, inner: impl FnOnce(&mut BytesMut)) {
    // Opening tag: (tag << 4) | 0x0E
    buf.extend_from_slice(&[(tag << 4) | 0x0E]);
    inner(buf);
    // Closing tag: (tag << 4) | 0x0F
    buf.extend_from_slice(&[(tag << 4) | 0x0F]);
}
