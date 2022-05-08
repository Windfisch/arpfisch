use crate::grid_controllers::{GridButtonEvent, LightingMode, Color};
use crate::tempo_detector::TempoDetector;
use crate::arpeggiator::{ArpeggioData, ClockMode, RepeatMode};

pub struct ConfigScreen {
	tempo: TempoDetector,
}

impl ConfigScreen {
	pub fn new() -> ConfigScreen {
		ConfigScreen {
			tempo: TempoDetector::new(),
		}
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		pattern: &mut ArpeggioData,
		pane_height: &mut usize,
		use_external_clock: bool,
		clock_mode: &mut ClockMode,
		time_between_midiclocks: &mut u64,
		time: u64
	) {
		use GridButtonEvent::*;

		match event {
			Down(2, 0, _) => {
				pattern.repeat_mode = RepeatMode::Clamp;
			}
			Down(2, 1, _) => {
				pattern.repeat_mode = RepeatMode::Mirror;
			}
			Down(2, 2, _) => {
				pattern.repeat_mode = RepeatMode::Repeat(12);
			}
			Down(7, 2, _) => {
				if !use_external_clock {
					self.tempo.beat(time);
					if self.tempo.time_per_beat() <= 48000 * 2
						&& self.tempo.time_per_beat() >= 10
					{
						*time_between_midiclocks =
							self.tempo.time_per_beat() as u64 / 24;
					}
				}
			}
			Down(7, 1, _) => {
				use ClockMode::*;
				*clock_mode = match *clock_mode {
					Internal => Auto,
					Auto => External,
					External => Internal
				};
			}
			Down(x, y, _) if (4..8).contains(&y) && x < 8 => {
				let new_len = x + 8 * (8 - y - 1) + 1;
				pattern.pattern.resize_default(new_len as usize).ok();
			}
			Down(0, y, _) if y < 4 => {
				*pane_height = 8 / (y + 1) as usize;
			}
			Down(3, y, _) if y < 4 => match pattern.repeat_mode {
				RepeatMode::Repeat(_) => {
					pattern.repeat_mode = RepeatMode::Repeat((y as i32 - 1) * 12);
				}
				_ => {}
			},
			_ => {}
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		pattern: &ArpeggioData,
		pane_height: usize,
		use_external_clock: bool,
		external_clock_present: bool,
		clock_mode: ClockMode,
	) {
		use LightingMode::*;

		array[7][2] = Some(match (use_external_clock, external_clock_present) {
			(true, true) => Alternate(Color::Color(150, 0.7), Color::White(1.0)),
			(true, false) => Solid(Color::Color(175, 0.0)),
			(false, _) => Alternate(Color::Color(30, 0.7), Color::White(1.0))
		});
		array[7][1] = Some(Solid(match clock_mode {
			ClockMode::Internal => Color::Color(30, 0.7),
			ClockMode::External => Color::Color(150, 0.7),
			ClockMode::Auto => Color::White(0.7)
		}));

		// display the pattern length
		let pattern_len = pattern.pattern.len();
		for y in 4..8 {
			for x in 0..8 {
				let curr_pos = x + (8 - y - 1) * 8 + 1;
				array[x][y] = if curr_pos < pattern_len {
					Some(Solid(Color::Color(0, 0.7)))
				}
				else if curr_pos == pattern_len {
					Some(Solid(Color::White(1.0)))
				}
				else {
					Some(Solid(Color::Color(30, 0.1)))
				}
			}
		}

		// display the number of panes
		let n_panes = 8 / pane_height;
		for i in 0..4 {
			if i + 1 == n_panes {
				array[0][i] = Some(Solid(Color::White(1.0)));
			}
			else {
				array[0][i] = Some(Solid(Color::Color(240, 0.2)));
			}
		}

		// repeat mode
		for i in 0..3 {
			array[2][i] = Some(Solid(Color::White(0.3)));
		}
		match pattern.repeat_mode {
			RepeatMode::Clamp => {
				array[2][0] = Some(Solid(Color::Color(60, 1.0)));
			}
			RepeatMode::Mirror => {
				array[2][1] = Some(Solid(Color::Color(180, 1.0)));
			}
			RepeatMode::Repeat(transpose) => {
				array[2][2] = Some(Solid(Color::Color(300, 1.0)));
				for i in 0..4 {
					array[3][i] = if transpose == (i as i32 - 1) * 12 {
						Some(Solid(Color::White(1.0)))
					}
					else {
						Some(Solid(Color::Color(300, 0.1)))
					}
				}
			}
		}
	}
}

