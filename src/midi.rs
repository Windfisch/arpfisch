// this file is part of arpfisch. For copyright and licensing details, see main.rs

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Note(pub u8);

impl Note {
	pub fn transpose(&self, amount: i32) -> Option<Note> {
		let result = amount + self.0 as i32;
		if 0 <= result && result < 128 {
			Some(Note(result as u8))
		}
		else {
			None
		}
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoteEvent {
	NoteOn(Note, u8),
	NoteOff(Note),
	Clock,
	Start
}
