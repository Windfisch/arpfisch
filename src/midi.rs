// this file is part of arpfisch. For copyright and licensing details, see main.rs

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Note(pub u8);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Channel(pub u8);

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
pub enum NoteEvent { // FIXME misnomer
	NoteOn(Note, u8, Channel),
	NoteOff(Note, Channel),
	Clock,
	Start,
}

impl NoteEvent {
	pub fn to_bytes(&self) -> heapless::Vec<u8, 3> {
		use NoteEvent::*;
		match self {
			NoteOn(note, velo, channel) => {
				heapless::Vec::from_slice(&[0x90 | channel.0, note.0, *velo])
			}
			NoteOff(note, channel) => {
				heapless::Vec::from_slice(&[0x80 | channel.0, note.0, 64])
			}
			Clock => heapless::Vec::from_slice(&[0xF8]),
			Start => heapless::Vec::from_slice(&[0xFA]),
		}
		.unwrap()
	}

	pub fn parse(bytes: &[u8]) -> Option<NoteEvent> {
		use NoteEvent::*;
		if bytes[0] & 0xF0 == 0x90 {
			Some(NoteOn(Note(bytes[1]), bytes[2], Channel(bytes[0] & 0x0F)))
		}
		else if bytes[0] & 0xF0 == 0x80 {
			Some(NoteOff(Note(bytes[1]), Channel(bytes[0] & 0x0F)))
		}
		else if bytes[0] == 0xFA {
			Some(Start)
		}
		else if bytes[0] == 0xF8 {
			Some(Clock)
		}
		else {
			None
		}
	}

	pub fn with_channel(self, channel: Channel) -> NoteEvent {
		use NoteEvent::*;
		match self {
			NoteOn(note, velo, _) => NoteOn(note, velo, channel),
			NoteOff(note, _) => NoteOff(note, channel),
			other => other
		}
	}
}
