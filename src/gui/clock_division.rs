// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};

pub struct ClockDivisionScreen {
}

fn log2(value: u32) -> u32 { // FIXME use u32::log2 once it's stable
	assert!(value != 0);
	for exponent in 0..32 {
		if value % 2u32.pow(exponent + 1) != 0 {
			return exponent;
		}
	}
	unreachable!();
}

const UNEVEN_Y: u8 = 6;
const POWER2_Y: u8 = 4;

impl ClockDivisionScreen {
	pub fn new() -> ClockDivisionScreen {
		ClockDivisionScreen { }
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		ticks_per_step: &mut u32,
	) {
		use GridButtonEvent::*;

		let log2 = log2(*ticks_per_step);
		let uneven = *ticks_per_step / 2u32.pow(log2);

		match event {
			Down(x, UNEVEN_Y, _) => {
				let new_uneven = (2*x + 1) as u32;
				*ticks_per_step = new_uneven * 2u32.pow(log2);
			}
			Down(x, POWER2_Y, _) => {
				*ticks_per_step = uneven * 2u32.pow(x as u32);
			}
			_ => {}
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		ticks_per_step: u32,
	) {
		use LightingMode::*;
		
		let log2 = log2(ticks_per_step);
		let uneven = ticks_per_step / 2u32.pow(log2);

		for x in 0..8 {
			array[x][UNEVEN_Y as usize] = if (2*x + 1) as u32 == uneven {
				Some(Solid(Color::White(1.0)))
			}
			else {
				Some(Solid(Color::Color(60, 0.7)))
			};

			array[x][POWER2_Y as usize] = if x as u32 == log2 {
				Some(Solid(Color::White(1.0)))
			}
			else {
				Some(Solid(Color::Color(180, 0.7)))
			};
		}
	}
}
