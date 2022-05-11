use crate::grid_controllers::{Color, GridButtonEvent, LightingMode};
use crate::midi::Note;

pub struct ScaleSelectScreen;

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
	pub fn new() -> ScaleSelectScreen { ScaleSelectScreen {} }

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		scale: &mut heapless::Vec<Note, heapless::consts::U16>
	) {
		use GridButtonEvent::*;

		match event {
			Down(x, y, _) => {
				if let Some(note) = coord_to_note((x.into(), y.into())) {
					let index = scale.iter().position(|n| *n == note);

					if let Some(index) = index {
						scale.swap_remove(index);
					}
					else {
						scale.push(note).unwrap();
					}
					scale.sort();
				}
			}
			_ => ()
		}
	}

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		scale: &heapless::Vec<Note, heapless::consts::U16>
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
			let color = if selected {
				Color::Color(0, 0.7)
			}
			else {
				base_color
			};
			array[x][y] = Some(Solid(color));
		}
	}
}
