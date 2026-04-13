use bytes::Bytes;

/// The visual style of a waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaveformStyle {
    /// Original blue monochrome waveform.
    Blue,
    /// Full RGB color waveform.
    Rgb,
    /// Three-band colored waveform (low/mid/high).
    ThreeBand,
}

/// A waveform preview (reduced resolution overview of the track).
#[derive(Debug, Clone)]
pub struct WaveformPreview {
    /// The visual style.
    pub style: WaveformStyle,
    /// Raw segment data (after stripping header/junk bytes).
    pub data: Bytes,
    /// Number of segments.
    pub segment_count: usize,
}

impl WaveformPreview {
    /// Bytes per segment for this style.
    pub fn bytes_per_segment(&self) -> usize {
        match self.style {
            WaveformStyle::Blue => 2,
            WaveformStyle::Rgb => 6,
            WaveformStyle::ThreeBand => 3,
        }
    }

    /// Parse a waveform preview from raw data bytes (from dbserver BinaryField).
    pub fn from_bytes(data: Bytes, style: WaveformStyle) -> crate::error::Result<Self> {
        let skip = match style {
            WaveformStyle::Blue => 0,
            WaveformStyle::Rgb | WaveformStyle::ThreeBand => 28,
        };

        if data.len() < skip {
            return Err(crate::error::ProDjLinkError::Parse(
                "waveform preview data too short".into(),
            ));
        }

        let payload = data.slice(skip..);
        let bps = match style {
            WaveformStyle::Blue => 2,
            WaveformStyle::Rgb => 6,
            WaveformStyle::ThreeBand => 3,
        };
        let segment_count = payload.len() / bps;

        Ok(Self {
            style,
            data: payload,
            segment_count,
        })
    }

    /// Get the height value for a segment.
    ///
    /// For Blue style, returns the lower 5 bits of the first byte (0–31).
    /// For ThreeBand style, returns the maximum of the three band values.
    /// For RGB style, returns the maximum of the three channel heights.
    pub fn segment_height(&self, index: usize) -> Option<u8> {
        if index >= self.segment_count {
            return None;
        }
        let offset = index * self.bytes_per_segment();
        match self.style {
            WaveformStyle::Blue => Some(self.data[offset] & 0x1f),
            WaveformStyle::ThreeBand => {
                let low = self.data[offset];
                let mid = self.data[offset + 1];
                let high = self.data[offset + 2];
                Some(low.max(mid).max(high))
            }
            WaveformStyle::Rgb => {
                // 6 bytes per segment: 2 bytes each for R, G, B channels.
                // Height of each channel is in bits 0–4 of the first byte of
                // each pair.
                let r = self.data[offset] & 0x1f;
                let g = self.data[offset + 2] & 0x1f;
                let b = self.data[offset + 4] & 0x1f;
                Some(r.max(g).max(b))
            }
        }
    }

    /// Get the RGB color of a segment (for RGB and ThreeBand styles).
    ///
    /// For RGB style, returns the color bytes from the segment data.
    /// For ThreeBand style, maps low/mid/high bands to red/green/blue.
    /// Returns `None` for Blue style or out-of-bounds index.
    pub fn color_segment(&self, index: usize) -> Option<(u8, u8, u8)> {
        if index >= self.segment_count {
            return None;
        }
        let offset = index * self.bytes_per_segment();
        match self.style {
            WaveformStyle::Blue => None,
            WaveformStyle::ThreeBand => {
                // low → red, mid → green, high → blue
                let low = self.data[offset];
                let mid = self.data[offset + 1];
                let high = self.data[offset + 2];
                Some((low, mid, high))
            }
            WaveformStyle::Rgb => {
                // 6 bytes: [R_height, R_color, G_height, G_color, B_height, B_color]
                let r = self.data[offset + 1];
                let g = self.data[offset + 3];
                let b = self.data[offset + 5];
                Some((r, g, b))
            }
        }
    }

    /// Get the maximum height across all segments.
    pub fn max_height(&self) -> u8 {
        (0..self.segment_count)
            .filter_map(|i| self.segment_height(i))
            .max()
            .unwrap_or(0)
    }
}

/// A waveform detail (full resolution).
#[derive(Debug, Clone)]
pub struct WaveformDetail {
    /// The visual style.
    pub style: WaveformStyle,
    /// Raw frame data (after stripping header/junk bytes).
    pub data: Bytes,
    /// Number of frames.
    pub frame_count: usize,
}

impl WaveformDetail {
    /// Bytes per frame for this style.
    pub fn bytes_per_frame(&self) -> usize {
        match self.style {
            WaveformStyle::Blue => 1,
            WaveformStyle::Rgb => 2,
            WaveformStyle::ThreeBand => 3,
        }
    }

    /// Get the height value for a frame.
    ///
    /// For Blue style, returns the lower 5 bits of the byte.
    /// For ThreeBand style, returns the maximum of the three band values.
    /// For RGB style, returns the maximum of the two channel heights.
    pub fn frame_height(&self, index: usize) -> Option<u8> {
        if index >= self.frame_count {
            return None;
        }
        let offset = index * self.bytes_per_frame();
        match self.style {
            WaveformStyle::Blue => Some(self.data[offset] & 0x1f),
            WaveformStyle::ThreeBand => {
                let low = self.data[offset];
                let mid = self.data[offset + 1];
                let high = self.data[offset + 2];
                Some(low.max(mid).max(high))
            }
            WaveformStyle::Rgb => {
                // 2 bytes per frame: each byte holds height (bits 0–4) and
                // color (bits 5–7).
                let a = self.data[offset] & 0x1f;
                let b = self.data[offset + 1] & 0x1f;
                Some(a.max(b))
            }
        }
    }

    /// Get the RGB color of a frame (for color styles).
    ///
    /// For RGB style, extracts color info from the upper bits of each byte.
    /// For ThreeBand style, maps low/mid/high bands to red/green/blue.
    /// Returns `None` for Blue style or out-of-bounds index.
    pub fn color_frame(&self, index: usize) -> Option<(u8, u8, u8)> {
        if index >= self.frame_count {
            return None;
        }
        let offset = index * self.bytes_per_frame();
        match self.style {
            WaveformStyle::Blue => None,
            WaveformStyle::ThreeBand => {
                let low = self.data[offset];
                let mid = self.data[offset + 1];
                let high = self.data[offset + 2];
                Some((low, mid, high))
            }
            WaveformStyle::Rgb => {
                // Upper 3 bits of each byte encode color component.
                let r = (self.data[offset] >> 5) & 0x07;
                let g = (self.data[offset + 1] >> 5) & 0x07;
                // Scale 3-bit values (0–7) to 8-bit range (0–255).
                let r8 = (r as u16 * 255 / 7) as u8;
                let g8 = (g as u16 * 255 / 7) as u8;
                Some((r8, g8, 0))
            }
        }
    }

    /// Total time in milliseconds represented by this waveform.
    ///
    /// Each frame represents a half-frame of audio (1/150 of a second).
    pub fn total_time_ms(&self) -> u64 {
        (self.frame_count as u64) * 1000 / 150
    }

    /// Parse a waveform detail from raw data bytes (from dbserver BinaryField).
    pub fn from_bytes(data: Bytes, style: WaveformStyle) -> crate::error::Result<Self> {
        let skip = match style {
            WaveformStyle::Blue => 19,
            WaveformStyle::Rgb | WaveformStyle::ThreeBand => 28,
        };

        if data.len() < skip {
            return Err(crate::error::ProDjLinkError::Parse(
                "waveform detail data too short".into(),
            ));
        }

        let payload = data.slice(skip..);
        let bpf = match style {
            WaveformStyle::Blue => 1,
            WaveformStyle::Rgb => 2,
            WaveformStyle::ThreeBand => 3,
        };
        let frame_count = payload.len() / bpf;

        Ok(Self {
            style,
            data: payload,
            frame_count,
        })
    }
}

/// ANLZ tag identifiers used to request specific waveform types.
pub mod anlz_tags {
    /// Color waveform preview tag.
    pub const PWV4: &str = "PWV4";
    /// Color waveform detail tag.
    pub const PWV5: &str = "PWV5";
    /// Three-band waveform preview tag.
    pub const PWV6: &str = "PWV6";
    /// Three-band waveform detail tag.
    pub const PWV7: &str = "PWV7";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_blue_preview_data(segments: usize) -> Bytes {
        // Blue preview: 2 bytes per segment, no header to skip.
        // Byte 0: lower 5 bits = height, upper bits = whiteness.
        // Byte 1: unused/reserved.
        let mut buf = Vec::with_capacity(segments * 2);
        for i in 0..segments {
            buf.push((i as u8) & 0x1f | 0x40); // height in lower 5 bits
            buf.push(0x00);
        }
        Bytes::from(buf)
    }

    fn make_rgb_preview_data(segments: usize) -> Bytes {
        // RGB preview: 28 junk bytes + 6 bytes per segment.
        let mut buf = vec![0xAA; 28]; // 28 bytes of junk header
        for _ in 0..segments {
            buf.extend_from_slice(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);
        }
        Bytes::from(buf)
    }

    fn make_threeband_preview_data(segments: usize) -> Bytes {
        // ThreeBand preview: 28 junk bytes + 3 bytes per segment.
        let mut buf = vec![0xBB; 28];
        for _ in 0..segments {
            buf.extend_from_slice(&[0x11, 0x22, 0x33]);
        }
        Bytes::from(buf)
    }

    #[test]
    fn preview_blue_from_bytes() {
        let data = make_blue_preview_data(10);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(preview.style, WaveformStyle::Blue);
        assert_eq!(preview.segment_count, 10);
        assert_eq!(preview.bytes_per_segment(), 2);
    }

    #[test]
    fn preview_rgb_skips_28_bytes() {
        let data = make_rgb_preview_data(5);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Rgb).unwrap();
        assert_eq!(preview.style, WaveformStyle::Rgb);
        assert_eq!(preview.segment_count, 5);
        assert_eq!(preview.bytes_per_segment(), 6);
        // Verify junk bytes were skipped: first payload byte should be 0x10, not 0xAA.
        assert_eq!(preview.data[0], 0x10);
    }

    #[test]
    fn preview_threeband_skips_28_bytes() {
        let data = make_threeband_preview_data(8);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::ThreeBand).unwrap();
        assert_eq!(preview.segment_count, 8);
        assert_eq!(preview.bytes_per_segment(), 3);
        assert_eq!(preview.data[0], 0x11);
    }

    #[test]
    fn preview_segment_count_calculation() {
        // 400 segments of blue data (2 bytes each) = 800 bytes total.
        let data = make_blue_preview_data(400);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(preview.segment_count, 400);

        // Partial segment is dropped: 801 bytes / 2 = 400 segments.
        let mut buf = vec![0u8; 801];
        for i in (0..801).step_by(2) {
            buf[i] = (i as u8) & 0x1f;
        }
        let data = Bytes::from(buf);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(preview.segment_count, 400);
    }

    #[test]
    fn preview_segment_height_extraction() {
        let data = make_blue_preview_data(4);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();

        // Heights are (i & 0x1f) from the lower 5 bits of (i | 0x40).
        assert_eq!(preview.segment_height(0), Some(0));
        assert_eq!(preview.segment_height(1), Some(1));
        assert_eq!(preview.segment_height(2), Some(2));
        assert_eq!(preview.segment_height(3), Some(3));
        // Out of bounds returns None.
        assert_eq!(preview.segment_height(4), None);
    }

    #[test]
    fn detail_blue_from_bytes() {
        // Blue detail: 19 junk bytes + 1 byte per frame.
        let mut buf = vec![0xCC; 19];
        buf.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05]);
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(detail.style, WaveformStyle::Blue);
        assert_eq!(detail.frame_count, 5);
        assert_eq!(detail.bytes_per_frame(), 1);
        assert_eq!(detail.data[0], 0x01);
    }

    #[test]
    fn detail_rgb_from_bytes() {
        // RGB detail: 28 junk bytes + 2 bytes per frame.
        let mut buf = vec![0xDD; 28];
        for _ in 0..10 {
            buf.extend_from_slice(&[0xAB, 0xCD]);
        }
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::Rgb).unwrap();
        assert_eq!(detail.frame_count, 10);
        assert_eq!(detail.bytes_per_frame(), 2);
        assert_eq!(detail.data[0], 0xAB);
    }

    #[test]
    fn detail_threeband_from_bytes() {
        // ThreeBand detail: 28 junk bytes + 3 bytes per frame.
        let mut buf = vec![0xEE; 28];
        for _ in 0..7 {
            buf.extend_from_slice(&[0x10, 0x20, 0x30]);
        }
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::ThreeBand).unwrap();
        assert_eq!(detail.frame_count, 7);
        assert_eq!(detail.bytes_per_frame(), 3);
        assert_eq!(detail.data[0], 0x10);
    }

    #[test]
    fn preview_too_short_returns_error() {
        // RGB preview needs at least 28 bytes; provide only 10.
        let data = Bytes::from(vec![0u8; 10]);
        let result = WaveformPreview::from_bytes(data, WaveformStyle::Rgb);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too short"), "unexpected error: {err}");
    }

    #[test]
    fn detail_too_short_returns_error() {
        // Blue detail needs at least 19 bytes; provide only 5.
        let data = Bytes::from(vec![0u8; 5]);
        let result = WaveformDetail::from_bytes(data, WaveformStyle::Blue);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too short"), "unexpected error: {err}");
    }

    // --- New accessor tests ---

    #[test]
    fn preview_blue_max_height() {
        let data = make_blue_preview_data(4);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();
        // Heights are 0, 1, 2, 3 → max = 3
        assert_eq!(preview.max_height(), 3);
    }

    #[test]
    fn preview_max_height_empty() {
        // 0 segments after the header: max_height should be 0
        let data = Bytes::from(vec![0u8; 0]);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(preview.max_height(), 0);
    }

    #[test]
    fn preview_blue_color_segment_returns_none() {
        let data = make_blue_preview_data(2);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(preview.color_segment(0), None);
    }

    #[test]
    fn preview_rgb_color_segment() {
        let data = make_rgb_preview_data(1);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Rgb).unwrap();
        // Bytes are [0x10, 0x20, 0x30, 0x40, 0x50, 0x60]
        // color_segment returns bytes at offsets 1, 3, 5
        assert_eq!(preview.color_segment(0), Some((0x20, 0x40, 0x60)));
    }

    #[test]
    fn preview_rgb_segment_height() {
        let data = make_rgb_preview_data(1);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Rgb).unwrap();
        // Heights are lower 5 bits of bytes at offsets 0, 2, 4
        // 0x10 & 0x1f = 0x10, 0x30 & 0x1f = 0x10, 0x50 & 0x1f = 0x10
        assert_eq!(preview.segment_height(0), Some(0x10));
    }

    #[test]
    fn preview_threeband_color_segment() {
        let data = make_threeband_preview_data(1);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::ThreeBand).unwrap();
        assert_eq!(preview.color_segment(0), Some((0x11, 0x22, 0x33)));
    }

    #[test]
    fn preview_threeband_segment_height() {
        let data = make_threeband_preview_data(1);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::ThreeBand).unwrap();
        // max of 0x11, 0x22, 0x33 = 0x33
        assert_eq!(preview.segment_height(0), Some(0x33));
    }

    #[test]
    fn preview_color_segment_out_of_bounds() {
        let data = make_rgb_preview_data(2);
        let preview = WaveformPreview::from_bytes(data, WaveformStyle::Rgb).unwrap();
        assert_eq!(preview.color_segment(5), None);
    }

    #[test]
    fn detail_blue_frame_height() {
        let mut buf = vec![0xCC; 19];
        buf.extend_from_slice(&[0x0A, 0x1F, 0x00]);
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(detail.frame_height(0), Some(0x0A));
        assert_eq!(detail.frame_height(1), Some(0x1F));
        assert_eq!(detail.frame_height(2), Some(0x00));
        assert_eq!(detail.frame_height(3), None);
    }

    #[test]
    fn detail_blue_color_frame_returns_none() {
        let mut buf = vec![0xCC; 19];
        buf.push(0x0A);
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(detail.color_frame(0), None);
    }

    #[test]
    fn detail_threeband_frame_height_and_color() {
        let mut buf = vec![0xEE; 28];
        buf.extend_from_slice(&[0x05, 0x0A, 0x0F]);
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::ThreeBand).unwrap();
        assert_eq!(detail.frame_height(0), Some(0x0F)); // max of 5, 10, 15
        assert_eq!(detail.color_frame(0), Some((0x05, 0x0A, 0x0F)));
    }

    #[test]
    fn detail_total_time_ms() {
        let mut buf = vec![0xCC; 19];
        // 150 frames = 1000 ms (1 second at 150 half-frames/sec)
        buf.extend_from_slice(&vec![0x0A; 150]);
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(detail.frame_count, 150);
        assert_eq!(detail.total_time_ms(), 1000);
    }

    #[test]
    fn detail_total_time_ms_zero_frames() {
        let buf = vec![0xCC; 19];
        let data = Bytes::from(buf);
        let detail = WaveformDetail::from_bytes(data, WaveformStyle::Blue).unwrap();
        assert_eq!(detail.total_time_ms(), 0);
    }
}
