//! Audio server — low-latency audio routing and mixing.

pub struct AudioServer {
    pub streams: Vec<AudioStream>,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub next_id: u64,
}

#[derive(Debug, Clone)]
pub struct AudioStream {
    pub id: u64,
    pub name: String,
    pub channels: u8,
    pub sample_rate: u32,
    pub is_game: bool,
    pub volume: f32,
}

impl AudioServer {
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            sample_rate: 48000,
            buffer_frames: 256,  // ~5.3 ms at 48 kHz — well below the 20 ms perceptual threshold
            next_id: 1,
        }
    }

    pub fn create_stream(&mut self, name: impl Into<String>, channels: u8, sample_rate: u32, is_game: bool) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let s = AudioStream { id, name: name.into(), channels, sample_rate, is_game, volume: 1.0 };
        log::info!("[audio] +stream id={} '{}'", id, s.name);
        self.streams.push(s);
        id
    }

    pub fn destroy_stream(&mut self, id: u64) {
        self.streams.retain(|s| s.id != id);
    }

    /// Latency in microseconds for the current buffer size.
    pub fn latency_us(&self) -> u64 {
        (self.buffer_frames as u64 * 1_000_000) / self.sample_rate as u64
    }
}
