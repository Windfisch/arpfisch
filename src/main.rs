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

impl ArpeggioData {
	pub fn get_mut(&mut self, pos: usize, note: isize) -> Option<&mut Entry> {
		self.pattern[pos].iter_mut().find(|e| e.note == note)
	}
	pub fn get(&mut self, pos: usize, note: isize) -> Option<&Entry> {
		self.pattern[pos].iter().find(|e| e.note == note)
	}
	pub fn set(&mut self, pos: usize, entry: Entry) -> Result<(), Entry> {
		if let Some(e) = self.get_mut(pos, entry.note) {
			*e = entry;
			Ok(())
		}
		else {
			self.pattern[pos].push(entry)
		}
	}
	pub fn delete(&mut self, pos: usize, note: isize) -> bool {
		if let Some((i, _)) = self.pattern[pos].iter().find_position(|e| e.note == note) {
			self.pattern[pos].swap_remove(i);
			return true;
		}
		return false;
	}
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


enum GuiState {
	Edit,
	Config,
	Sliders,
}

struct GuiController {
	state: GuiState,
	pane_height: usize,
	first_x: isize,
	first_y: isize,
}

impl GuiController {
	pub fn new() -> GuiController {
		GuiController {
			state: GuiState::Edit,
			pane_height: 4,
			first_x: 0,
			first_y: 0
		}
	}

	pub fn handle_input(&mut self, event: LaunchpadEvent, pattern: &mut ArpeggioData) {
		use LaunchpadEvent::*;
		use GuiState::*;

		println!("Handle input: {:?}", event);

		match self.state {
			Edit => {
				match event {
					Down(8, 0, _) => {
						self.state = Config;
					},
					Down(8, 1, _) => {
						self.state = Sliders;
					},
					Down(0, 8, _) => {
						self.first_y += 1;
					},
					Down(1, 8, _) => {
						self.first_y -= 1;
					},
					Down(2, 8, _) => {
						self.first_x -= 1;
					},
					Down(3, 8, _) => {
						self.first_x += 1;
					},
					Down(xx, yy, velo) => {
						if xx <= 8 && yy <= 8 {
							let n_panes = 8 / self.pane_height;
							let pane = yy as usize / self.pane_height;
							let x = xx as isize + self.first_x + 8 * (n_panes - pane - 1) as isize;
							let y = (yy as isize % self.pane_height as isize) + self.first_y;
							if x >= 0 && (x as usize) < pattern.pattern.len() {
								if pattern.get(x as usize, y).is_none() {
									pattern.set(x as usize, Entry {
										note: y,
										len_steps: 1,
										intensity: velo
									}).ok();
								}
								else {
									pattern.delete(x as usize, y);
								}
							}
						}
					},
					Up(_, _, _) => {}
				}
			}
			Config => {
				match event {
					Down(8, 0, _) => {
						self.state = Edit;
					},
					Down(8, 1, _) => {
						self.state = Sliders;
					},
					Down(2, 0, _) => {
						pattern.repeat_mode = RepeatMode::Clamp;
					}
					Down(2, 1, _) => {
						pattern.repeat_mode = RepeatMode::Mirror;
					}
					Down(2, 2, _) => {
						pattern.repeat_mode = RepeatMode::Repeat(12);
					}
					Down(x, y, _) => {
						if y >= 4 {
							let new_len = x + 8 * (8 - y - 1) + 1;
							pattern.pattern.resize_default(new_len as usize).ok();
						}
						else {
							if x == 0 {
								self.pane_height = 8 / (y+1) as usize;
							}
							if x == 3 {
								match pattern.repeat_mode {
									RepeatMode::Repeat(_) => { pattern.repeat_mode = RepeatMode::Repeat((y as i32 - 1) * 12); }
									_ => {}
								}
							}
						}
					}
					_ => {}
				}
			}
			Sliders => {
				match event {
					Down(8, 0, _) => {
						self.state = Config;
					},
					Down(8, 1, _) => {
						self.state = Edit;
					},
					_ => {}
				}
			}
		}
	}

	pub fn draw(&mut self, pattern: &ArpeggioData, step: f32, mut set_led: impl FnMut((u8,u8), LaunchpadColorspec)) {
		use GuiState::*;
		use LaunchpadColorspec::*;
		let mut array = [[None; 8]; 8];
		match self.state {
			Edit => {
				set_led((8,0), Off);
				set_led((8,1), Off);

				fn draw_into(array: &mut [[Option<LaunchpadColorspec>; 8]; 8], canvas_offset: (usize, usize), canvas_size: (usize, usize), pattern_offset: (isize, isize), pattern: &ArpeggioData, step: f32) {
					// draw notes
					for x in 0..canvas_size.0 {
						let pos = x as isize + pattern_offset.0;
						if pos >= 0 && pos < pattern.pattern.len() as isize  {
							for e in pattern.pattern[pos as usize].iter() {
								let y = e.note - pattern_offset.1;
								if (0..canvas_size.1 as isize).contains(&y) {
									for i in 0..e.len_steps {
										let xx = x + i as usize;
										if xx < canvas_size.0 {
											let foo = &mut array[xx + canvas_offset.0][y as usize + canvas_offset.1];
											if foo.is_some() {
												*foo = Some(Solid(Color::White(1.0)));
											}
											else {
												let color = if i == 0 {
													Color::Color((120.0 + 60.0 * e.intensity) as u16, 0.25 + 0.75 * e.intensity)
												}
												else {
													Color::Color(0, 0.3)
												};
												*foo = Some(Solid(color));
											}
										}
									}
								}
							}
						}
						else {
							for y in 0..canvas_size.1 {
								array[x + canvas_offset.0][y + canvas_offset.1] = Some(Solid(Color::Color(0,0.3)));
							}
						}
					}
				
					// draw horizontal zero indicator
					let hl_y = -pattern_offset.1;
					if (0..canvas_size.1 as isize).contains(&hl_y) {
						for x in 0..canvas_size.0 {
							array[x + canvas_offset.0][hl_y as usize+ canvas_offset.1].get_or_insert(Solid(Color::White(0.3)));
						}
					}

					// draw vertical step indicator
					let hl_x = step as isize - pattern_offset.0;
					if (0..canvas_size.0 as isize).contains(&hl_x) {
						for y in 0..canvas_size.1 {
							let foo = &mut array[hl_x as usize + canvas_offset.0][y + canvas_offset.1];
							*foo = Some(foo.unwrap_or(Off).bright());
						}
					}
				}

				let n_panes = 8 / self.pane_height;
				for pane in 0..n_panes {
					draw_into(&mut array, (0,self.pane_height * (n_panes - pane - 1)), (8,self.pane_height), (self.first_x + 8 * pane as isize, self.first_y), &pattern, step);
				}
			},
			Config => {
				set_led((8,0), Fade(Color::Color(0, 0.74)));
				set_led((8,1), Off);

				// display the pattern length
				let pattern_len = pattern.pattern.len();
				for y in 4..8 {
					for x in 0..8 {
						let curr_pos = x + (8-y-1)*8 + 1;
						array[x][y] = if curr_pos < pattern_len {
							Some(Solid(Color::Color(0, 0.7)))
						}
						else if curr_pos == pattern_len {
							Some(Solid(Color::White(1.0)))
						}
						else {
							Some(Solid(Color::Color(30, 0.1)))
						}
					};
				}
				
				// display the number of panes
				let n_panes = 8 / self.pane_height;
				for i in 0..4 {
					if i+1 == n_panes {
						array[0][i] = Some(Solid(Color::White(1.0)));
					}
					else {
						array[0][i] = Some(Solid(Color::Color(240, 0.2)));
					}
				}

				// repeat mode
				for i in 0..3 {
					array[2][i] = Some(Solid(Color::White(0.3)));
				}
				match pattern.repeat_mode {
					RepeatMode::Clamp => {
						array[2][0] = Some(Solid(Color::Color(60, 1.0)));
					}
					RepeatMode::Mirror => {
						array[2][1] = Some(Solid(Color::Color(180, 1.0)));
					}
					RepeatMode::Repeat(transpose) => {
						array[2][2] = Some(Solid(Color::Color(300, 1.0)));
						for i in 0..4 {
							array[3][i] = if transpose == (i as i32 - 1) * 12 {
								Some(Solid(Color::White(1.0)))
							}
							else {
								Some(Solid(Color::Color(300,0.1)))
							}
						}
					}
				}
			},
			Sliders => {
				set_led((8,0), Off);
				set_led((8,1), Fade(Color::Color(0, 0.74)));

			}
		}

		for x in 0..8 {
			for y in 0..8 {
				set_led((x,y), array[x as usize][y as usize].unwrap_or(Off));
			}
		}
	}
}

struct JackDriver {
	in_port: Port<MidiIn>,
	out_port: Port<MidiOut>,
	ui_in_port: Port<MidiIn>,
	ui_out_port: Port<MidiOut>,
	ui: LaunchpadX,
	ticks_per_step: u32,
	tick_counter: u32,
	time: u64,
	pending_events: heapless::Vec<(u64, NoteEvent), U32>,
	arp: Arpeggiator,
	gui_controller: GuiController,
	pattern: ArpeggioData,
	tempo: TempoDetector,
	channel: u8,
	out_channel: u8,
	periods: u64
}

impl JackDriver {
	pub fn new(name: &str, client: &jack::Client) -> Result<JackDriver, jack::Error> {
		let driver = JackDriver {
			in_port: client.register_port(&format!("{}_in", name), MidiIn)?,
			out_port: client.register_port(&format!("{}_out", name), MidiOut)?,
			ui_in_port: client.register_port(&format!("{}_launchpad_in", name), MidiIn)?,
			ui_out_port: client.register_port(&format!("{}_launchpad_out", name), MidiOut)?,
			ui: LaunchpadX::new(),
			ticks_per_step: 6,
			tick_counter: 0,
			time: 0,
			gui_controller: GuiController::new(),
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
			out_channel: 0,
			periods: 0
		};
		Ok(driver)
	}

	pub fn autoconnect(&self, client: &jack::Client) {
		for p in client.ports(Some(".*playback.*Launchpad X MIDI 2"), None, jack::PortFlags::empty()) {
			client.connect_ports(&self.ui_out_port, &client.port_by_name(&p).unwrap()).expect("Failed to connect");
		}
		for p in client.ports(Some(".*capture.*Launchpad X MIDI 2"), None, jack::PortFlags::empty()) {
			client.connect_ports(&client.port_by_name(&p).unwrap(), &self.ui_in_port).expect("Failed to connect");
		}
	}

	pub fn process(&mut self, client: &jack::Client, scope: &ProcessScope) {
		for ev in self.ui_in_port.iter(scope) {
			println!("event!");
			let gui_controller = &mut self.gui_controller;
			let pattern = &mut self.pattern;
			self.ui.handle_midi(ev.bytes, |ui, event| {
				gui_controller.handle_input(event, pattern);
			});
		}

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
				}
			}
			if event.bytes[0] == 0x90 | self.channel {
				self.arp.note_on(Note(event.bytes[1]));
			}
			if event.bytes[0] == 0x80 | self.channel {
				self.arp.note_off(Note(event.bytes[1]));
			}
		}

		let mut ui_writer = self.ui_out_port.writer(scope);
		let ui = &mut self.ui;
		self.gui_controller.draw(&self.pattern, self.arp.step as f32 + self.tick_counter as f32 / self.ticks_per_step as f32, |pos, color| {
			ui.set(pos, color, |bytes| { ui_writer.write(&jack::RawMidi { time: 0, bytes }).unwrap(); });
		});

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
		
		if self.periods == 0 {
			self.autoconnect(client);
		}
		if self.periods == 1 {
			let mut writer = self.ui_out_port.writer(scope);
			writer.write(
				&jack::RawMidi {
					time: 0,
					bytes: &[0xF0, 0x00, 0x20, 0x29, 0x02, 0x0C, 0x0E, 0x01, 0xF7]
				}
			).ok();

			self.ui.force_update(|bytes| { writer.write(&jack::RawMidi{time:0, bytes}).expect("write failed"); });
		}
		self.periods += 1;

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

impl Color {
	pub fn bright(&self) -> Color {
		use crate::Color::*;
		match *self {
			Color(c, _) => Color(c, 1.0),
			White(_) => White(1.0)
		}
	}
}

#[derive(Clone,Copy,Debug)]
enum LaunchpadColorspec {
	Off,
	Solid(Color),
	Blink(Color),
	Fade(Color),
	Alternate(Color, Color)
}

impl LaunchpadColorspec {
	pub fn bright(&self) -> LaunchpadColorspec {
		use LaunchpadColorspec::*;
		match *self {
			Off => Solid(Color::White(0.3)),
			Solid(c) => Solid(c.bright()),
			Blink(c) => Blink(c.bright()),
			Fade(c) => Fade(c.bright()),
			Alternate(c1,c2) => Alternate(c1.bright(), c2.bright())
		}
	}
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

	pub fn force_update(&self, mut send: impl FnMut(&[u8])) {
		for i in 0..8 {
			for j in 0..8 {
				self.send((i,j), &mut send);
			}
		}
	}

	pub fn handle_midi(&mut self, message: &[u8], mut f: impl FnMut(&mut Self, LaunchpadEvent)) {
		fn id2coord(id: u8) -> Option<(u8, u8)> {
			let x = id % 10;
			let y = id / 10;

			if (1..=9).contains(&x) && (1..=9).contains(&y) {
				Some((x-1, y-1))
			}
			else {
				None
			}
		}
		use LaunchpadEvent::*;
		if message.len() == 3 {
			if (message[0] == 0x90 || message[0] == 0xB0) && message[2] != 0 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Down(x, y, message[2] as f32 / 127.0));
				}
			}
			if (message[0] == 0x90 || message[0] == 0xB0) && message[2] == 0 {
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
		use LaunchpadColorspec::*;
		let new_spec = match colorspec {
			Off => LaunchpadInternalColorspec::Solid(0),
			Solid(c) => LaunchpadInternalColorspec::Solid(color(c)),
			Blink(c) => LaunchpadInternalColorspec::Alternate(0,color(c)),
			Fade(c) => LaunchpadInternalColorspec::Fade(color(c)),
			Alternate(c1,c2) => LaunchpadInternalColorspec::Alternate(color(c1),color(c2))
		};

		let field = &mut self.state[pos.0 as usize][pos.1 as usize];
		if *field != new_spec {
			*field = new_spec;
			self.send(pos, &mut send);
		}
	}
	
	fn send(&self, pos: (u8,u8), send: &mut impl FnMut(&[u8])) {
		let note = (pos.0 + 1) + 10 * (pos.1 + 1);
		match self.state[pos.0 as usize][pos.1 as usize] {
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

fn main() {
	let client = jack::Client::new("arpfisch", jack::ClientOptions::NO_START_SERVER).expect("Failed to connect to JACK").0;

	let mut jack_driver = JackDriver::new("fnord", &client).unwrap();

	//let (mut producer, mut consumer) = ringbuf::RingBuffer::<Message>::new(10).split();

	let async_client = client.activate_async((), jack::ClosureProcessHandler::new(move |client: &jack::Client, scope: &ProcessScope| -> Control {
		jack_driver.process(client, scope);
		return Control::Continue;
	})).expect("Failed to activate client");

	loop { std::thread::sleep_ms(1000); }
}
