pub struct TempoDetector {
	last_timestamp: Option<u64>,
	time_per_beat: u32
}

impl TempoDetector {
	pub fn new() -> TempoDetector {
		TempoDetector {
			last_timestamp: None,
			time_per_beat: 0
		}
	}
	pub fn time_per_beat(&self) -> u32 { self.time_per_beat }
	pub fn beat(&mut self, timestamp: u64) {
		if let Some(last_timestamp) = self.last_timestamp {
			self.time_per_beat = (timestamp - last_timestamp) as u32;
		}
		self.last_timestamp = Some(timestamp);
	}
	pub fn reset(&mut self) { self.last_timestamp = None; }
}
