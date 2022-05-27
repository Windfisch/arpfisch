// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};
use crate::midi::Note;

pub struct ScaleSelectScreen {
	last_tap: (Note, u64)
}

const MIDI_C0: u8 = 0;

fn note_to_coord(note: Note) -> (usize, usize, bool) {
	match (note.0 - MIDI_C0) % 12 {
		0 => (1, 1, false),
		1 => (2, 2, true),
		2 => (3, 1, false),
		3 => (4, 2, true),
		4 => (5, 1, false),
		5 => (0, 4, false),
		6 => (1, 5, true),
		7 => (2, 4, false),
		8 => (3, 5, true),
		9 => (4, 4, false),
		10 => (5, 5, true),
		11 => (6, 4, false),
		_ => unreachable!()
	}
}

fn coord_to_note(coord: (usize, usize)) -> Option<Note> {
	match coord {
		(1, 1) => Some(Note(0)),
		(2, 2) => Some(Note(1)),
		(3, 1) => Some(Note(2)),
		(4, 2) => Some(Note(3)),
		(5, 1) => Some(Note(4)),
		(0, 4) => Some(Note(5)),
		(1, 5) => Some(Note(6)),
		(2, 4) => Some(Note(7)),
		(3, 5) => Some(Note(8)),
		(4, 4) => Some(Note(9)),
		(5, 5) => Some(Note(10)),
		(6, 4) => Some(Note(11)),
		_ => None
	}
}

impl ScaleSelectScreen {
	pub fn new() -> ScaleSelectScreen {
		ScaleSelectScreen {
			last_tap: (Note(0), 0)
		}
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		scale: &mut heapless::Vec<Note, 16>,
		scale_base_override: &mut Option<Note>,
		time: u64
	) {
		use GridButtonEvent::*;

		match event {
			Down(x, y, _) => {
				if let Some(note) = coord_to_note((x.into(), y.into())) {
					let is_doubletap = note == self.last_tap.0 && time < self.last_tap.1 + 48000 / 4;

					if let Some(index) = scale.iter().position(|n| *n == note) {
						if !is_doubletap {
							scale.swap_remove(index);
							if *scale_base_override == Some(note) {
								*scale_base_override = None;
							}
						}
					}
					else {
						scale.push(note).unwrap();
					}
					scale.sort();

					if is_doubletap {
						*scale_base_override = Some(note);
					}

					self.last_tap = (note, time);
				}
			}
			_ => ()
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		scale: &heapless::Vec<Note, 16>,
		scale_base_override: Option<Note>
	) {
		use LightingMode::*;

		for i in 0..12 {
			let (x, y, black) = note_to_coord(Note(i));
			let selected = scale.iter().find(|n| n.0 == i).is_some();
			let base_color = if !black {
				Color::White(1.0)
			}
			else {
				Color::Color(240, 0.4)
			};
			let color = if scale_base_override == Some(Note(i)) {
				Color::Color(60, 0.7)
			}
			else if selected {
				Color::Color(0, 0.7)
			}
			else {
				base_color
			};
			array[x][y] = Some(Solid(color));
		}
	}
}
