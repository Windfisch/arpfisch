// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};

pub struct RoutingScreen {}

impl RoutingScreen {
	pub fn new() -> RoutingScreen { RoutingScreen {} }

	pub fn handle_input(&mut self, event: GridButtonEvent, routing_matrix: &mut Vec<Vec<bool>>) {
		use GridButtonEvent::*;

		assert!(routing_matrix.len() == routing_matrix[0].len());
		let len = routing_matrix.len();

		match event {
			Down(x, y, _) => {
				if x < y && (y as usize) < len {
					routing_matrix[x as usize][y as usize] =
						!routing_matrix[x as usize][y as usize];
				}
			}
			_ => ()
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		routing_matrix: &Vec<Vec<bool>>
	) {
		use LightingMode::*;

		assert!(routing_matrix.len() == routing_matrix[0].len());
		let len = routing_matrix.len();
		let len = len.min(8);

		for x in 0..len {
			for y in 0..len {
				if x < y {
					array[x][y] = match routing_matrix[x][y] {
						true => Some(Solid(Color::White(1.0))),
						false => Some(Off)
					};
				}
				else {
					array[x][y] = Some(Solid(Color::Color(0, 0.3)));
				}
			}
		}
	}
}
