// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};

pub struct PatternSelectScreen {}

impl PatternSelectScreen {
	pub fn new() -> PatternSelectScreen { PatternSelectScreen {} }

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		n_patterns: usize,
		active_pattern: &mut [usize],
		active_arp: &mut usize
	) {
		use GridButtonEvent::*;

		let n_arps = active_pattern.len();

		match event {
			Down(x, y, _) if x < 8 && y < 8 => {
				let x = x as usize;
				let y = y as usize;
				if y < n_arps {
					if x < n_patterns {
						active_pattern[y] = x;
						*active_arp = y;
					}
				}
			}
			_ => ()
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		active_pattern: &[usize],
		active_arp: usize
	) {
		use LightingMode::*;

		let n_arps = active_pattern.len();

		for y in 0..n_arps.min(8) {
			let y = y as usize;
	
			if active_pattern[y] < 8 {
				array[active_pattern[y]][y] = Some(
					if y == active_arp {
						Fade(Color::White(1.0))
					}
					else {
						Solid(Color::Color((360 * y as u16 * 3 / 8) % 360, 0.7))
					}
				)
			}
		}
	}
}
