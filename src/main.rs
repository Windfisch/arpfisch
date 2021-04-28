use jack;
use jack::{Port,MidiIn,MidiOut,ProcessScope,Control};
use ringbuf;
use heapless;
use heapless::consts::*;
use itertools::Itertools;

enum RepeatMode {
	Clamp,
	Repeat(i32),
	Mirror
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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Note(u8);

impl Note {
	fn transpose(&self, amount: i32) -> Option<Note> {
		let result = amount + self.0 as i32;
		if 0 <= result && result < 128 {
			Some(Note(result as u8))
		}
		else {
			None
		}
	}
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
				let repeated_index = modulo(index, 2*pitches.len()-2);
				if repeated_index < pitches.len() {
					Some(pitches[repeated_index])
				}
				else {
					Some(pitches[2*pitches.len() - 1 - repeated_index])
				}
			}
		}
	}
}

#[derive(Clone)]
struct Entry {
	note: isize,
	len_steps: u32,
	intensity: f32,
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

struct ArpeggioData {
	pub repeat_mode: RepeatMode,
	pub pattern: heapless::Vec<heapless::Vec<Entry, U16>, U64>,
}

struct Arpeggiator {
	step: usize,
	pub global_length_modifier: f32,
	pub global_velocity: f32,
	pub intensity_length_modifier_amount: f32,
	pub intensity_velocity_amount: f32,
	chord: heapless::Vec<Note, U16>,
}

struct TempoDetector {
	last_timestamp: Option<u64>,
	time_per_beat: u32
}

impl TempoDetector {
	pub fn new() -> TempoDetector {
		TempoDetector {
			last_timestamp: None,
			time_per_beat: 0
		}
	}
	pub fn time_per_beat(&self) -> u32 { self.time_per_beat }
	pub fn beat(&mut self, timestamp: u64) {
		if let Some(last_timestamp) = self.last_timestamp {
			self.time_per_beat = (timestamp - last_timestamp) as u32;
		}
		self.last_timestamp = Some(timestamp);
	}
	pub fn reset(&mut self) {
		self.last_timestamp = None;
	}
}

struct JackDriver {
	in_port: Port<MidiIn>,
	out_port: Port<MidiOut>,
	ticks_per_step: u32,
	tick_counter: u32,
	time: u64,
	pending_events: heapless::Vec<(u64, NoteEvent), U32>,
	arp: Arpeggiator,
	pattern: ArpeggioData,
	tempo: TempoDetector,
	channel: u8,
	out_channel: u8,
}

impl JackDriver {
	pub fn new(name: &str, client: &jack::Client) -> Result<JackDriver, jack::Error> {
		Ok(JackDriver {
			in_port: client.register_port(&format!("{}_in", name), MidiIn)?,
			out_port: client.register_port(&format!("{}_out", name), MidiOut)?,
			ticks_per_step: 12,
			tick_counter: 0,
			time: 0,
			pending_events: heapless::Vec::new(),
			arp: Arpeggiator::new(),
			pattern: ArpeggioData {
				pattern: heapless::Vec::from_slice(&[
					heapless::Vec::from_slice(&[Entry{note: 0, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: -1, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 0, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 1, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 2, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 3, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 4, len_steps: 1, intensity: 0.5}]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 5, len_steps: 1, intensity: 0.5}]).unwrap(),
				]).unwrap(),
				repeat_mode: RepeatMode::Repeat(12),
			},
			tempo: TempoDetector::new(),
			channel: 0,
			out_channel: 0
		})
	}

	pub fn process(&mut self, scope: &ProcessScope) {
		for event in self.in_port.iter(scope) {
			let timestamp = self.time + event.time as u64;
			if event.bytes[0] == 0xFA { // start
				self.tick_counter = 0;
				self.arp.reset();
				self.tempo.reset();
			}
			if event.bytes[0] == 0xF8 || event.bytes[0] == 0xFA { // clock or start
				self.tick_counter += 1;
				if self.tick_counter >= self.ticks_per_step {
					self.tick_counter -= self.ticks_per_step;

					self.tempo.beat(timestamp);
					let time_per_beat = self.tempo.time_per_beat();

					let pending_events = &mut self.pending_events;
					self.arp.process_step(&self.pattern, |timestamp_steps, event| {
						let event_timestamp = timestamp + (time_per_beat as f32 * timestamp_steps) as u64;
						pending_events.push((event_timestamp, event)).map_err(|_|())
					});

					println!("beat (time since last beat = {}). pending: {:?}", time_per_beat, self.pending_events);
				}
			}
			if event.bytes[0] == 0x90 | self.channel {
				self.arp.note_on(Note(event.bytes[1]));
			}
			if event.bytes[0] == 0x80 | self.channel {
				self.arp.note_off(Note(event.bytes[1]));
			}
		}

		self.pending_events.sort_by_key(|e| e.0);
		let end = self.pending_events.iter()
			.enumerate()
			.filter(|(_, ev)| ev.0 >= self.time + (scope.n_frames() as u64))
			.map(|(i, _)| i)
			.next()
			.unwrap_or(self.pending_events.len());

		let mut writer = self.out_port.writer(scope);
		for event in &self.pending_events[0..end] {
			println!("event: {:?}", event);
			let bytes = match event.1 {
				NoteEvent::NoteOn(note, velo) => [0x90 | self.out_channel, note.0, velo],
				NoteEvent::NoteOff(note) => [0x80 | self.out_channel, note.0, 64]
			};
			writer.write(&jack::RawMidi {
				time: (event.0 - self.time) as u32,
				bytes: &bytes
			});
		}

		for i in 0..end {
			if end + i < self.pending_events.len() {
				self.pending_events[i] = self.pending_events[i+end];
			}
		}
		self.pending_events.truncate(self.pending_events.len() - end);

		self.time += scope.n_frames() as u64;
	}
}

#[derive(Copy, Clone, Debug)]
enum NoteEvent {
	NoteOn(Note, u8),
	NoteOff(Note)
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
			self.chord.push(note);
			self.chord.sort();
		}
		println!("chord is {:?}", self.chord);
	}
	pub fn note_off(&mut self, note: Note) {
		if let Some(i) = self.chord.iter().position(|n| *n == note) {
			self.chord.swap_remove(i);
			self.chord.sort();
		}
		println!("chord is {:?}", self.chord);
	}
	pub fn process_step<F: FnMut(f32, NoteEvent) -> Result<(),()>>(&mut self, pattern: &ArpeggioData, mut callback: F) -> Result<(),()> {
		let current_step = self.step;
		self.step = (self.step + 1) % pattern.pattern.len();

		for entry in pattern.pattern[current_step].iter() {
			let length_modifier = self.global_length_modifier * (1.0 + (2.0 * entry.intensity - 1.0) * self.intensity_length_modifier_amount);
			let velocity = (self.global_velocity * (0.5 + (entry.intensity - 0.5) * self.intensity_velocity_amount)).clamp(0.0, 1.0);
			let note_length = entry.actual_len(length_modifier);
			if let Some(note) = pattern.repeat_mode.get(&self.chord, entry.note) {
				callback(note_length, NoteEvent::NoteOff(note))?;
				callback(0.0, NoteEvent::NoteOn(note, (127.0 * velocity) as u8))?;
			}
		}
		Ok(())
	}
	pub fn reset(&mut self) {
		self.step = 0;
	}
}

trait UserInterface {
	fn update_arpeggio();
}

struct LaunchpadX {
	state: [[LaunchpadInternalColorspec; 9]; 9]
}

#[derive(Clone,Copy,Debug)]
enum LaunchpadEvent {
	Down(u8, u8, f32),
	Up(u8, u8, f32),
}

#[derive(Clone,Copy,Debug)]
enum Color {
	Color(u16, f32),
	White(f32)
}

#[derive(Clone,Copy,Debug)]
enum LaunchpadColorspec {
	Off,
	Solid(Color),
	Blink(Color),
	Fade(Color),
	Alternate(Color, Color)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum LaunchpadInternalColorspec {
	Solid(u8),
	Alternate(u8,u8),
	Fade(u8)
}

impl LaunchpadX {
	pub fn new() -> LaunchpadX {
		LaunchpadX {
			state: [[LaunchpadInternalColorspec::Solid(0); 9]; 9]
		}
	}

	pub fn handle_midi(&mut self, message: &[u8], mut f: impl FnMut(&mut Self, LaunchpadEvent)) {
		fn id2coord(id: u8) -> Option<(u8, u8)> {
			let x = id % 10;
			let y = id / 10;
			println!("x,y = {}, {}", x, y);

			if (1..9).contains(&x) && (1..9).contains(&y) {
				Some((x-1, y-1))
			}
			else {
				None
			}
		}
		use LaunchpadEvent::*;
		println!("{}", message.len());
		if message.len() == 3 {
			println!("{}, {:02x} {:02x} {:02x}", message.len(), message[0], message[1], message[2]);
			if message[0] == 0x90 && message[2] != 0 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Down(x, y, message[2] as f32 / 127.0));
				}
			}
			if message[0] == 0x90 && message[2] == 0 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Up(x, y, 64.0));
				}
			}
			if message[0] == 0x80 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Up(x, y, message[2] as f32 / 127.0));
				}
			}
		}
	}

	pub fn set(&mut self, pos: (u8,u8), colorspec: LaunchpadColorspec, mut send: impl FnMut(&[u8])) {
		fn color(c: Color) -> u8 {
			let offsets = [
				0x04, 0x04, 0x04, 0x08, 0x08, 0x08,
				0x0c, 0x0c, 0x0c, 0x10, 0x10, 0x10,
				0x14, 0x14, 0x18, 0x18, 0x1c, 0x1c,
				0x20, 0x20, 0x24, 0x24, 0x28, 0x28,
				0x2c, 0x2c, 0x2c, 0x30, 0x30, 0x30,
				0x34, 0x34, 0x34, 0x38, 0x38, 0x38
			];
			use crate::Color::*;
			match c {
				White(i) => ((i*4.0) as u8).clamp(0,3),
				Color(hue, i) => offsets[(hue as usize % 360) / 10] + 3 - ((i*4.0) as u8).clamp(0,3)
			}
		}

		assert!( (0..9).contains(&pos.0) );
		assert!( (0..9).contains(&pos.1) );
		let note = (pos.0 + 1) + 10 * (pos.1 + 1);
		use LaunchpadColorspec::*;
		let new_spec = match colorspec {
			Off => LaunchpadInternalColorspec::Solid(0),
			Solid(c) => LaunchpadInternalColorspec::Solid(color(c)),
			Blink(c) => LaunchpadInternalColorspec::Alternate(0,color(c)),
			Fade(c) => LaunchpadInternalColorspec::Fade(color(c)),
			Alternate(c1,c2) => LaunchpadInternalColorspec::Alternate(color(c1),color(c2))
		};

		let field = &mut self.state[pos.0 as usize][pos.1 as usize];
		println!("field was {:?}, is {:?}", *field, new_spec);
		if *field != new_spec {
			*field = new_spec;
			match new_spec {
				LaunchpadInternalColorspec::Solid(c) => {
					send(&[0x90, note, c]);
				}
				LaunchpadInternalColorspec::Alternate(c1, c2) => {
					send(&[0x90, note, c1]);
					send(&[0x91, note, c2]);
				}
				LaunchpadInternalColorspec::Fade(c) => {
					send(&[0x92, note, c]);
				}
			}
		}
	}
}

fn main() {
	let client = jack::Client::new("arpfisch", jack::ClientOptions::NO_START_SERVER).expect("Failed to connect to JACK").0;

	let launchpad_in_port = client.register_port("launchpad_in", MidiIn).unwrap();
	let launchpad_out_port = client.register_port("launchpad_out", MidiIn).unwrap();

	let mut jack_driver = JackDriver::new("fnord", &client).unwrap();

	//let (mut producer, mut consumer) = ringbuf::RingBuffer::<Message>::new(10).split();

	let _async_client = client.activate_async((), jack::ClosureProcessHandler::new(move |_client: &jack::Client, scope: &ProcessScope| -> Control {
		jack_driver.process(scope);
		return Control::Continue;
	})).expect("Failed to activate client");

	loop { std::thread::sleep_ms(1000); }
}
