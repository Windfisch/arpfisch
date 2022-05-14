// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};

pub struct SlidersScreen {
	down_times: [[Option<u64>; 8]; 8],
	fader_history: [[f32; 2]; 8],
	fader_history_last_update: u64
}

impl SlidersScreen {
	pub fn new() -> SlidersScreen {
		SlidersScreen {
			down_times: [[None; 8]; 8],
			fader_history: [[0.0; 2]; 8],
			fader_history_last_update: 0
		}
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		fader_values: &mut [Option<(&mut f32, std::ops::RangeInclusive<f32>)>],
		time: u64
	) {
		use GridButtonEvent::*;

		match event {
			Down(x, y, _) => {
				if x < 8 && y < 8 {
					self.down_times[x as usize][y as usize] = Some(time);
				}
			}
			Up(x, y, _) => {
				if x < 8 && y < 8 {
					self.down_times[x as usize][y as usize] = None;
				}
			}
			_ => {}
		}

		match event {
			Down(x, y, _) if x < 8 && y < 8 => {
				for (fader_x, fader) in fader_values.iter_mut().enumerate() {
					if let Some((value, range)) = fader {
						if x as usize == fader_x {
							**value =
								y as f32 / 7.0 * (range.end() - range.start()) + range.start();
							self.fader_history[x as usize] = [**value; 2];
						}
					}
				}
			}
			Pressure(x, y, pressure) => {
				if x < 8 && y < 8 && (x as usize) < fader_values.len() {
					if let Some((value, range)) = fader_values[x as usize].as_mut() {
						if let Some(down_time) = self.down_times[x as usize][y as usize] {
							if time >= down_time + 48000 / 6 {
								**value = pressure * (range.end() - range.start()) + range.start();
							}
						}
					}
				}
			}
			Up(x, y, _) => {
				if x < 8 && y < 8 && (x as usize) < fader_values.len() {
					if let Some((value, _)) = fader_values[x as usize].as_mut() {
						**value = self.fader_history[x as usize][0];
					}
				}
			}
			_ => {}
		}

		if time >= self.fader_history_last_update + 48000 / 40 {
			for (i, fader) in fader_values.iter_mut().enumerate() {
				if let Some((value, _)) = fader {
					self.fader_history[i][0] = self.fader_history[i][1];
					self.fader_history[i][1] = **value;
				}
			}
			self.fader_history_last_update = time;
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		fader_values: &[Option<(f32, std::ops::RangeInclusive<f32>)>]
	) {
		use LightingMode::*;

		for (x, fader) in fader_values.iter().enumerate() {
			if let Some((value, range)) = fader {
				let leds =
					1 + (7.0 * (value - range.start()) / (range.end() - range.start())) as usize;
				for y in 0..leds {
					array[x][y] = Some(Solid(Color::Color(x as u16 * 107, 0.7)));
				}
			}
		}
	}
}
