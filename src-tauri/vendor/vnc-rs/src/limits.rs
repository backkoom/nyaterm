/// Resource limits applied to all server-controlled RFB values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VncLimits {
    pub max_server_name_bytes: usize,
    pub max_failure_reason_bytes: usize,
    pub min_ra2_key_bits: u32,
    pub max_ra2_key_bits: u32,
    pub max_ra2_record_bytes: usize,
    pub max_clipboard_bytes: usize,
    pub max_framebuffer_width: u16,
    pub max_framebuffer_height: u16,
    pub max_framebuffer_pixels: usize,
    pub max_rectangles_per_update: u16,
    pub max_encoded_payload_bytes: usize,
    pub max_decoded_payload_bytes: usize,
    pub channel_capacity: usize,
}

impl Default for VncLimits {
    fn default() -> Self {
        Self {
            max_server_name_bytes: 64 * 1024,
            max_failure_reason_bytes: 64 * 1024,
            min_ra2_key_bits: 1_024,
            max_ra2_key_bits: 8_192,
            max_ra2_record_bytes: usize::from(u16::MAX),
            max_clipboard_bytes: 16 * 1024 * 1024,
            max_framebuffer_width: 16_384,
            max_framebuffer_height: 16_384,
            max_framebuffer_pixels: 64 * 1024 * 1024,
            max_rectangles_per_update: 4_096,
            max_encoded_payload_bytes: 64 * 1024 * 1024,
            max_decoded_payload_bytes: 256 * 1024 * 1024,
            channel_capacity: 32,
        }
    }
}
