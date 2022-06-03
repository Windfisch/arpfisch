// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};

pub struct ClockDivisionScreen {
	restart_transport_hit_time: u64
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
		ClockDivisionScreen {
			restart_transport_hit_time: 0
		}
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		ticks_per_step: &mut u32,
		restart_transport_pending: &mut bool,
		time: u64
	) {
		use GridButtonEvent::*;

		let log2 = log2(*ticks_per_step);
		let uneven = *ticks_per_step / 2u32.pow(log2);

		match event {
			Down(6, 1, _) => {
				*restart_transport_pending = true;
				self.restart_transport_hit_time = time;
			}
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
		step: u32,
		time: u64
	) {
		use LightingMode::*;
		
		let log2 = log2(ticks_per_step);
		let uneven = ticks_per_step / 2u32.pow(log2);

		let blinking = if step % 2 == 0 {
			Solid(Color::White(1.0))
		}
		else {
			Solid(Color::White(0.3))
		};

		for x in 0..8 {
			array[x][UNEVEN_Y as usize] = if (2*x + 1) as u32 == uneven {
				Some(blinking)
			}
			else {
				Some(Solid(Color::Color(60, 0.7)))
			};

			array[x][POWER2_Y as usize] = if x as u32 == log2 {
				Some(blinking)
			}
			else {
				Some(Solid(Color::Color(180, 0.7)))
			};
		}
		
		// restart transport button
		array[6][1] = Some(Solid(
			if time < self.restart_transport_hit_time + 48000/2 {
				Color::Color(0, 1.0)
			}
			else {
				Color::Color(0, 0.7)
			}
		));
	}
}
