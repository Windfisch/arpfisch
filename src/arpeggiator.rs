use crate::midi::{Note, NoteEvent};
use heapless::{self, consts::*};

pub enum RepeatMode {
	Clamp,
	Repeat(i32),
	Mirror
}

impl RepeatMode {
	pub fn get(&self, pitches: &[Note], index: isize) -> Option<Note> {
		use RepeatMode::*;
		if pitches.is_empty() {
			return None;
		}
		match *self {
			Clamp =>
				if index >= 0 {
					if index < pitches.len() as isize {
						Some(pitches[index as usize])
					}
					else {
						Some(*pitches.last().unwrap())
					}
				}
				else {
					let reverse_index = pitches.len() as isize + index;
					if reverse_index >= 0 {
						Some(pitches[reverse_index as usize])
					}
					else {
						Some(*pitches.first().unwrap())
					}
				}

			Repeat(transpose) => {
				let repetition = div_floor(index, pitches.len());
				pitches[modulo(index, pitches.len())].transpose(repetition as i32 * transpose)
			}

			Mirror => {
				if pitches.len() == 1 {
					Some(pitches[0])
				}
				else
				{
					let repeated_index = modulo(index, 2*pitches.len()-2);
					if repeated_index < pitches.len() {
						Some(pitches[repeated_index])
					}
					else {
						Some(pitches[2*pitches.len() - 1 - repeated_index - 1])
					}
				}
			}
		}
	}
}

#[derive(Clone, Debug)]
pub struct Entry {
	pub note: isize,
	pub len_steps: u32,
	pub intensity: f32,
	pub transpose: i32
}

impl Entry {
	pub fn actual_len(&self, modifier: f32) -> f32 {
		assert!(0.0 <= modifier && modifier <= 2.0);
		if modifier <= 1.0 {
			(self.len_steps as f32 - 0.5) * modifier
		}
		else {
			self.len_steps as f32 - 1.0 + modifier / 2.0
		}
	}
}

pub struct ArpeggioData {
	pub repeat_mode: RepeatMode,
	pub pattern: heapless::Vec<heapless::Vec<Entry, U16>, U64>,
}

impl ArpeggioData {
	pub fn filter_mut(&mut self, pos: usize, note: isize) -> impl Iterator<Item = &mut Entry> {
		self.pattern[pos].iter_mut().filter(move |e| e.note == note)
	}
	pub fn filter(&self, pos: usize, note: isize) -> impl Iterator<Item = &Entry> {
		self.pattern[pos].iter().filter(move |e| e.note == note)
	}
	pub fn set(&mut self, pos: usize, entry: Entry) -> Result<(), Entry> {
		if let Some(e) = self.pattern[pos].iter_mut().find(|e| e.note == entry.note && e.transpose == entry.transpose) {
			*e = entry;
			Ok(())
		}
		else {
			self.pattern[pos].push(entry)
		}
	}
	pub fn delete_all(&mut self, pos: usize, note: isize) {
		while let Some(i) = self.pattern[pos].iter().position(|e| e.note == note) {
			self.pattern[pos].swap_remove(i);
		}
	}
	pub fn delete(&mut self, pos: usize, entry: Entry) {
		while let Some(i) = self.pattern[pos].iter().position(|e| e.note == entry.note && e.transpose == entry.transpose) {
			self.pattern[pos].swap_remove(i);
		}
	}
}

pub struct Arpeggiator {
	pub global_length_modifier: f32,
	pub global_velocity: f32,
	pub intensity_length_modifier_amount: f32,
	pub intensity_velocity_amount: f32,
	chord: heapless::Vec<Note, U16>,
	step: usize,
}



#[derive(Copy,Clone)]
pub enum ClockMode {
	Internal,
	External,
	Auto
}

impl Arpeggiator {
	pub fn new() -> Arpeggiator {
		Arpeggiator {
			step: 0,
			global_length_modifier: 1.0,
			global_velocity: 1.0,
			intensity_velocity_amount: 1.0,
			intensity_length_modifier_amount: 0.0,
			chord: heapless::Vec::new(),
		}
	}

	pub fn note_on(&mut self, note: Note) {
		if self.chord.iter().position(|n| *n == note).is_none() {
			self.chord.push(note).ok();
			self.chord.sort();
		}
	}
	pub fn note_off(&mut self, note: Note) {
		if let Some(i) = self.chord.iter().position(|n| *n == note) {
			self.chord.swap_remove(i);
			self.chord.sort();
		}
	}
	pub fn process_step<F: FnMut(f32, NoteEvent) -> Result<(),()>>(&mut self, pattern: &ArpeggioData, mut callback: F) -> Result<(),()> {
		let current_step = self.step % pattern.pattern.len(); // pattern length could have changed, in which case we need to do this modulo again
		self.step = (self.step + 1) % pattern.pattern.len();

		for entry in pattern.pattern[current_step].iter() {
			let length_modifier = self.global_length_modifier * (1.0 + (2.0 * entry.intensity - 1.0) * self.intensity_length_modifier_amount);
			let velocity = (self.global_velocity * (0.5 + (entry.intensity - 0.5) * self.intensity_velocity_amount)).clamp(0.0, 1.0);
			let note_length = entry.actual_len(length_modifier);
			if let Some(note) = pattern.repeat_mode.get(&self.chord, entry.note).map(|n| n.transpose(entry.transpose)).flatten() {
				callback(note_length, NoteEvent::NoteOff(note))?;
				callback(0.0, NoteEvent::NoteOn(note, (127.0 * velocity) as u8))?;
			}
		}
		Ok(())
	}
	pub fn reset(&mut self) {
		self.step = 0;
	}

	pub fn step(&self) -> usize { self.step }
}

fn modulo(numerator: isize, denominator: usize) -> usize {
	return ((numerator % (denominator as isize)) + denominator as isize) as usize % denominator;
}

fn div_floor(numerator: isize, denominator: usize) -> isize {
	if numerator >= 0 {
		numerator / denominator as isize
	}
	else {
		(numerator - denominator as isize + 1) / denominator as isize
	}
}


