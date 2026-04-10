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

    /// Get the height value for a segment (blue style only).
    pub fn segment_height(&self, index: usize) -> Option<u8> {
        if index >= self.segment_count {
            return None;
        }
        let offset = index * self.bytes_per_segment();
        Some(self.data[offset] & 0x1f) // Height is lower 5 bits
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
}
