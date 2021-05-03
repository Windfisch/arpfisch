use crate::arpeggiator::{Arpeggiator, ArpeggioData, ClockMode, Entry, RepeatMode};
use crate::grid_controllers::launchpad_x::LaunchpadX;
use crate::grid_controllers::GridController;
use heapless;
use heapless::consts::*;
use jack::*;

use crate::gui::GuiController; // FIXME this should not be in the jack driver
use crate::midi::{Note, NoteEvent};
use crate::tempo_detector::TempoDetector;

pub struct JackDriver {
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
	periods: u64,
	last_midiclock_received: u64,
	next_midiclock_to_send: u64,
	time_between_midiclocks: u64,
	clock_mode: ClockMode // FIXME this should go to ArpeggiatorController or something like that
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
				#[rustfmt::skip]
				pattern: heapless::Vec::from_slice(&[
					heapless::Vec::from_slice(&[Entry{note: 0, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note:-1, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 0, len_steps: 1, intensity: 0.5, transpose:12 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 1, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 2, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 3, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 4, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
					heapless::Vec::from_slice(&[Entry{note: 5, len_steps: 1, intensity: 0.5, transpose: 0 }]).unwrap(),
				]).unwrap(),
				repeat_mode: RepeatMode::Repeat(12)
			},
			tempo: TempoDetector::new(),
			channel: 0,
			out_channel: 0,
			periods: 0,
			last_midiclock_received: 0,
			next_midiclock_to_send: 0,
			time_between_midiclocks: 24000 / 24,
			clock_mode: ClockMode::Auto
		};
		Ok(driver)
	}

	pub fn autoconnect(&self, client: &jack::Client) {
		for p in client.ports(
			Some(".*playback.*Launchpad X MIDI 2"),
			None,
			jack::PortFlags::empty()
		) {
			client
				.connect_ports(&self.ui_out_port, &client.port_by_name(&p).unwrap())
				.expect("Failed to connect");
		}
		for p in client.ports(
			Some(".*capture.*Launchpad X MIDI 2"),
			None,
			jack::PortFlags::empty()
		) {
			client
				.connect_ports(&client.port_by_name(&p).unwrap(), &self.ui_in_port)
				.expect("Failed to connect");
		}
	}

	pub fn process(&mut self, client: &jack::Client, scope: &ProcessScope) {
		let external_clock_present = self.time - self.last_midiclock_received <= 48000;
		let use_external_clock = match self.clock_mode {
			ClockMode::Internal => false,
			ClockMode::External => true,
			ClockMode::Auto => external_clock_present
		};

		for ev in self.ui_in_port.iter(scope) {
			println!("event!");
			let gui_controller = &mut self.gui_controller;
			let time_between_midiclocks = &mut self.time_between_midiclocks;
			let pattern = &mut self.pattern;
			let clock_mode = &mut self.clock_mode;
			let time = self.time;
			self.ui.handle_midi(ev.bytes, |ui, event| {
				gui_controller.handle_input(
					event,
					pattern,
					use_external_clock,
					clock_mode,
					time_between_midiclocks,
					time
				);
			});
		}

		let mut ui_writer = self.ui_out_port.writer(scope);

		for event in self.in_port.iter(scope) {
			let timestamp = self.time + event.time as u64;

			if event.bytes[0] == 0xFA {
				// start
				self.tick_counter = 0;
				self.arp.reset();
				self.tempo.reset();
			}
			if event.bytes[0] == 0xF8 || event.bytes[0] == 0xFA {
				// clock or start
				self.last_midiclock_received = self.time;

				if use_external_clock {
					ui_writer.write(&event);
					self.pending_events.push((
						timestamp,
						if event.bytes[0] == 0xF8 {
							NoteEvent::Clock
						}
						else {
							NoteEvent::Start
						}
					));
					self.tick_counter += 1;
					if self.tick_counter >= self.ticks_per_step {
						self.tick_counter -= self.ticks_per_step;

						self.tempo.beat(timestamp);
						let time_per_beat = self.tempo.time_per_beat();

						let pending_events = &mut self.pending_events;
						self.arp
							.process_step(&self.pattern, |timestamp_steps, event| {
								let event_timestamp =
									timestamp + (time_per_beat as f32 * timestamp_steps) as u64;
								pending_events
									.push((event_timestamp, event))
									.map_err(|_| ())
							})
							.expect("process_step failed (buffer overflow?)");
					}
				}
			}
			if event.bytes[0] == 0x90 | self.channel {
				self.arp.note_on(Note(event.bytes[1]));
			}
			if event.bytes[0] == 0x80 | self.channel {
				self.arp.note_off(Note(event.bytes[1]));
			}
		}

		if !use_external_clock {
			self.next_midiclock_to_send = self.next_midiclock_to_send.max(self.time);

			if self.next_midiclock_to_send < self.time + scope.n_frames() as u64 {
				ui_writer.write(&jack::RawMidi {
					time: (self.next_midiclock_to_send - self.time) as jack::Frames,
					bytes: &[0xF8]
				});
				self.pending_events
					.push((self.next_midiclock_to_send, NoteEvent::Clock));

				self.tick_counter += 1; // FIXME duplicated code :(
				if self.tick_counter >= self.ticks_per_step {
					self.tick_counter -= self.ticks_per_step;

					let time_per_beat = self.time_between_midiclocks * 24;
					let timestamp = self.next_midiclock_to_send;

					let pending_events = &mut self.pending_events;
					self.arp
						.process_step(&self.pattern, |timestamp_steps, event| {
							let event_timestamp =
								timestamp + (time_per_beat as f32 * timestamp_steps) as u64;
							pending_events
								.push((event_timestamp, event))
								.map_err(|_| ())
						})
						.expect("process_step failed (buffer overflow?)");
				}
				self.next_midiclock_to_send =
					self.next_midiclock_to_send + self.time_between_midiclocks;
			}
		}

		let ui = &mut self.ui;
		self.gui_controller.draw(
			&self.pattern,
			self.arp.step() as f32 + self.tick_counter as f32 / self.ticks_per_step as f32,
			use_external_clock,
			external_clock_present,
			self.clock_mode,
			&mut self.time_between_midiclocks,
			|pos, color| {
				ui.set(pos, color, |bytes| {
					ui_writer
						.write(&jack::RawMidi {
							time: scope.n_frames() - 1,
							bytes
						})
						.expect("Writing to UI MIDI buffer failed");
				});
			}
		);

		let before_sort = format!("{:?}", self.pending_events);
		self.pending_events.sort_by_key(|e| e.0);
		let end = self
			.pending_events
			.iter()
			.enumerate()
			.filter(|(_, ev)| ev.0 >= self.time + (scope.n_frames() as u64))
			.map(|(i, _)| i)
			.next()
			.unwrap_or(self.pending_events.len());

		let mut writer = self.out_port.writer(scope);
		if end != 0 {
			println!("==== {}", end);
			println!("{}", before_sort);
			println!("{:?}", self.pending_events);
		}
		for event in &self.pending_events[0..end] {
			println!("event: {:?}", event);
			let bytes: heapless::Vec<_, U4> = match event.1 {
				NoteEvent::NoteOn(note, velo) => {
					heapless::Vec::from_slice(&[0x90 | self.out_channel, note.0, velo])
				}
				NoteEvent::NoteOff(note) => {
					heapless::Vec::from_slice(&[0x80 | self.out_channel, note.0, 64])
				}
				NoteEvent::Clock => heapless::Vec::from_slice(&[0xF8]),
				NoteEvent::Start => heapless::Vec::from_slice(&[0xFA])
			}
			.unwrap();
			writer
				.write(&jack::RawMidi {
					time: (event.0 - self.time) as u32,
					bytes: &bytes
				})
				.expect("Writing to MIDI buffer failed");
		}

		for i in 0..(self.pending_events.len() - end) {
			self.pending_events[i] = self.pending_events[i + end];
		}
		self.pending_events
			.truncate(self.pending_events.len() - end);

		self.time += scope.n_frames() as u64;

		if self.periods == 0 {
			self.autoconnect(client);
		}
		if self.periods == 10 {
			let mut writer = self.ui_out_port.writer(scope);
			writer
				.write(&jack::RawMidi {
					time: 0,
					bytes: &[0xF0, 0x00, 0x20, 0x29, 0x02, 0x0C, 0x0E, 0x01, 0xF7]
				})
				.expect("Writing to the MIDI buffer failed");

			self.ui.force_update(|bytes| {
				writer
					.write(&jack::RawMidi { time: 0, bytes })
					.expect("Writing to UI MIDI buffer failed");
			});
		}
		self.periods += 1;
	}
}
