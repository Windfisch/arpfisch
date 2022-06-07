// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::midi::{Note, NoteEvent, Channel};
use crate::tempo_detector::TempoDetector;
use heapless;

#[derive(Clone, PartialEq, Eq)]
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
			Clamp => {
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
			}

			Repeat(transpose) => {
				let repetition = div_floor(index, pitches.len());
				pitches[modulo(index, pitches.len())].transpose(repetition as i32 * transpose)
			}

			Mirror => {
				if pitches.len() == 1 {
					Some(pitches[0])
				}
				else {
					let repeated_index = modulo(index, 2 * pitches.len() - 2);
					if repeated_index < pitches.len() {
						Some(pitches[repeated_index])
					}
					else {
						Some(pitches[2 * pitches.len() - 1 - repeated_index - 1])
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
			// clamp the modifier to not-quite-1.0, in order to avoid racy NoteOff and NoteOn events
			self.len_steps as f32 - 1.0 + (modifier / 2.0).clamp(0.0, 0.9999)
		}
	}
}

#[derive(Clone)]
pub struct ArpeggioData {
	pub repeat_mode: RepeatMode,
	pub pattern: heapless::Vec<heapless::Vec<Entry, 16>, 64>
}

impl ArpeggioData {
	pub fn filter_mut(&mut self, pos: usize, note: isize) -> impl Iterator<Item = &mut Entry> {
		self.pattern[pos].iter_mut().filter(move |e| e.note == note)
	}
	pub fn filter(&self, pos: usize, note: isize) -> impl Iterator<Item = &Entry> {
		self.pattern[pos].iter().filter(move |e| e.note == note)
	}
	/// Returns an error if the step can hold no more events
	pub fn set(&mut self, pos: usize, entry: Entry) -> Result<(), Entry> {
		if let Some(e) = self.pattern[pos]
			.iter_mut()
			.find(|e| e.note == entry.note && e.transpose == entry.transpose)
		{
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
		while let Some(i) = self.pattern[pos]
			.iter()
			.position(|e| e.note == entry.note && e.transpose == entry.transpose)
		{
			self.pattern[pos].swap_remove(i);
		}
	}
}

pub struct Arpeggiator {
	pub global_length_modifier: f32,
	pub global_velocity: f32,
	pub intensity_length_modifier_amount: f32,
	pub intensity_velocity_amount: f32,
	pub chord_settle_time: u64,
	pub chord_hold: bool,
	chord_hold_old: bool, // FIXME this should really not be there... use a setter instead
	chord: heapless::Vec<Note, 16>,
	stable_chord: heapless::Vec<Note, 16>,
	chord_next_update_time: Option<u64>,
	step: usize,
	pub scale: heapless::Vec<Note, 16>,
	pub scale_base_override: Option<Note>,
	scale_base_override_old: Option<Note>, // meeeeh FIXME
}

#[derive(Copy, Clone)]
pub enum ClockMode {
	Internal,
	External,
	Auto
}

fn scale_from<const LEN: usize>(
	scale: &[Note],
	bottom: Note
) -> heapless::Vec<Note, LEN> {
	match scale
		.iter()
		.position(|note| (note.0 as isize - bottom.0 as isize) % 12 == 0)
	{
		Some(bottom_index) => {
			let mut result = heapless::Vec::new();
			let offset = bottom.0 as isize - scale[bottom_index].0 as isize;
			for i in bottom_index..(bottom_index + scale.len()) {
				let octave = if i < scale.len() { 0 } else { 12 };
				let pitch = scale[i % scale.len()].0 as isize + offset + octave;
				if ((u8::MIN as isize)..=(u8::MAX as isize)).contains(&pitch) {
					result.push(Note(pitch as u8)).ok();
				}
			}

			result
		}
		None => heapless::Vec::new()
	}
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
			stable_chord: heapless::Vec::new(),
			chord_next_update_time: None,
			chord_settle_time: 0,
			chord_hold: false,
			chord_hold_old: false,
			scale: heapless::Vec::new(),
			scale_base_override: None,
			scale_base_override_old: None,
		}
	}

	pub fn note_on(&mut self, note: Note, time: u64) {
		if self.scale.is_empty() {
			if self.chord.iter().position(|n| *n == note).is_none() {
				self.chord.push(note).ok();
				self.chord.sort();
				self.chord_next_update_time = Some(time + self.chord_settle_time);
			}
		}
		else if self.scale_base_override.is_none() {
			self.stable_chord = scale_from(&self.scale, note);
		}
	}
	pub fn note_off(&mut self, note: Note, time: u64) {
		if self.scale.is_empty() {
			if let Some(i) = self.chord.iter().position(|n| *n == note) {
				self.chord.swap_remove(i);
				self.chord.sort();
				if self.chord_hold && self.chord.is_empty() {
					self.chord_next_update_time = None;
				}
				else {
					self.chord_next_update_time = Some(time + self.chord_settle_time);
				}
			}
		}
		else if self.scale_base_override.is_none() {
			if !self.chord_hold {
				if let Some(bottom_note) = self.stable_chord.first() {
					if *bottom_note == note {
						self.stable_chord.clear();
					}
				}
			}
		}
	}
	pub fn process_step<F: FnMut(f32, NoteEvent) -> Result<(), ()>>(
		&mut self,
		pattern: &ArpeggioData,
		time: u64,
		mut callback: F
	) -> Result<(), ()> {
		if self.chord_hold != self.chord_hold_old {
			if !self.chord_hold {
				self.chord_next_update_time = Some(time);
			}
			self.chord_hold_old = self.chord_hold;
		}
		if self.scale_base_override != self.scale_base_override_old {
			if let Some(note) = self.scale_base_override {
				self.stable_chord = scale_from(&self.scale, note);
			}
			else {
				self.chord_next_update_time = Some(time);
			}
			self.scale_base_override_old = self.scale_base_override;
		}
		if let Some(chord_next_update_time) = self.chord_next_update_time {
			if time >= chord_next_update_time {
				self.stable_chord = self.chord.clone();
				self.chord_next_update_time = None;
			}
		}

		let current_step = self.step % pattern.pattern.len(); // pattern length could have changed, in which case we need to do this modulo again
		self.step = (self.step + 1) % pattern.pattern.len();

		for entry in pattern.pattern[current_step].iter() {
			let length_modifier = (self.global_length_modifier
				* (1.0 + (2.0 * entry.intensity - 1.0) * self.intensity_length_modifier_amount))
				.clamp(0.0, 2.0);
			let velocity = (self.global_velocity
				* (0.5 + (entry.intensity - 0.5) * self.intensity_velocity_amount))
				.clamp(0.0, 1.0);
			let note_length = entry.actual_len(length_modifier);
			if let Some(note) = pattern
				.repeat_mode
				.get(&self.stable_chord, entry.note)
				.map(|n| n.transpose(entry.transpose))
				.flatten()
			{
				callback(note_length, NoteEvent::NoteOff(note, Channel(0)))?;
				callback(0.0, NoteEvent::NoteOn(note, (127.0 * velocity) as u8, Channel(0)))?;
			}
		}
		Ok(())
	}
	pub fn reset(&mut self) { self.step = 0; }

	pub fn step(&self) -> usize { self.step }
}

pub struct ArpeggiatorInstance {
	pub ticks_per_step: u32,
	tick_counter: u32,
	pub patterns: [ArpeggioData; 8],
	pub active_pattern: usize,
	pub arp: Arpeggiator,
	tempo: TempoDetector,
	pending_events: heapless::Vec<(u64, NoteEvent), 32>
}

impl ArpeggiatorInstance {
	pub fn restart_transport(&mut self) {
		self.tempo.reset();
		self.tick_counter = self.ticks_per_step - 1;
		self.arp.reset();
	}

	pub fn active_pattern(&self) -> &ArpeggioData { &self.patterns[self.active_pattern] }

	pub fn tick_clock(&mut self, timestamp: u64) {
		self.tick_counter += 1;
		if self.tick_counter >= self.ticks_per_step {
			self.tick_counter -= self.ticks_per_step;

			self.tempo.beat(timestamp);
			let time_per_beat = self.tempo.time_per_beat();

			let pending_events = &mut self.pending_events;
			self.arp
				.process_step(
					&self.patterns[self.active_pattern],
					timestamp,
					|timestamp_steps, event| {
						let event_timestamp =
							timestamp + (time_per_beat as f32 * timestamp_steps) as u64;
						pending_events
							.push((event_timestamp, event))
							.map_err(|_| ())
					}
				)
				.expect("process_step failed (buffer overflow?)");
		}
	}

	pub fn currently_playing_tick(&self) -> f32 {
		(self.arp.step() as f32 - 1.0 + self.tick_counter as f32 / self.ticks_per_step as f32)
			.rem_euclid(self.active_pattern().pattern.len() as f32)
	}

	pub fn add_pending_event(&mut self, timestamp: u64, event: NoteEvent) -> Result<(), ()> {
		self.pending_events.push((timestamp, event)).map_err(|_| ())
	}

	pub fn pending_note_offs<'a>(&'a self) -> impl Iterator<Item = Note> + 'a {
		self.pending_events.iter().filter_map(|tup| match tup.1 {
			NoteEvent::NoteOff(note, _) => Some(note),
			_ => None
		})
	}

	/// Calls `callback` with all pending events that occur earlier than `time_limit`,
	/// sorted by time stamp, and removes them from the pending event queue.
	pub fn process_pending_events(
		&mut self,
		time_limit: u64,
		mut callback: impl FnMut(&[(u64, NoteEvent)])
	) {
		self.pending_events.sort();
		let end = self
			.pending_events
			.iter()
			.enumerate()
			.filter(|(_, ev)| ev.0 >= time_limit)
			.map(|(i, _)| i)
			.next()
			.unwrap_or(self.pending_events.len());

		callback(&self.pending_events[0..end]);

		for i in 0..(self.pending_events.len() - end) {
			self.pending_events[i] = self.pending_events[i + end];
		}
		self.pending_events
			.truncate(self.pending_events.len() - end);
	}

	pub fn new() -> ArpeggiatorInstance {
		let pattern = ArpeggioData {
			pattern: heapless::Vec::from_slice(&[
				heapless::Vec::new(),
				heapless::Vec::new(),
				heapless::Vec::new(),
				heapless::Vec::new(),
				heapless::Vec::new(),
				heapless::Vec::new(),
				heapless::Vec::new(),
				heapless::Vec::new()
			])
			.unwrap(),
			repeat_mode: RepeatMode::Repeat(12)
		};
		ArpeggiatorInstance {
			ticks_per_step: 6,
			tick_counter: 0,
			arp: Arpeggiator::new(),
			patterns: [
				pattern.clone(),
				pattern.clone(),
				pattern.clone(),
				pattern.clone(),
				pattern.clone(),
				pattern.clone(),
				pattern.clone(),
				pattern.clone()
			],
			active_pattern: 0,
			tempo: TempoDetector::new(),
			pending_events: heapless::Vec::new()
		}
	}
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

#[cfg(test)]
mod tests {
	fn assert_slice_eq<T: PartialEq + std::fmt::Debug>(a: &[T], b: &[T]) {
		assert!(a.len() == b.len());
		a.iter()
			.zip(b.iter())
			.enumerate()
			.for_each(|(i, (aa, bb))| {
				assert!(aa == bb, "mismatch at index {} of {:?} == {:?}", i, a, b)
			});
	}

	#[test]
	pub fn scale_from() {
		use super::scale_from;
		use super::Note;

		let scale = [Note(30), Note(32), Note(33), Note(35)];

		assert_slice_eq(
			&scale_from::<32>(&scale, Note(30)),
			&[Note(30), Note(32), Note(33), Note(35)]
		);

		assert_slice_eq(
			&scale_from::<32>(&scale, Note(33)),
			&[Note(33), Note(35), Note(42), Note(44)]
		);

		assert_slice_eq(
			&scale_from::<32>(&scale, Note(42)),
			&[Note(42), Note(44), Note(45), Note(47)]
		);

		assert_slice_eq(&scale_from::<32>(&scale, Note(31)), &[]);
	}
}
