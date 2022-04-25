// this file is part of arpfisch. For copyright and licensing details, see main.rs

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
	gui_controller: GuiController,
	periods: u64,

	// FIXME these should probably go to MidiDriver
	time: u64,
	channel: u8, // FIXME this is currently a single channel setting for all ArpeggiatorInstances
	out_channel: u8, // FIXME same here
	last_midiclock_received: u64,
	next_midiclock_to_send: u64,
	time_between_midiclocks: u64,
	clock_mode: ClockMode, // FIXME this should go to MasterController or something like that

	arp_instance: ArpeggiatorInstance
}

// FIXME this does not belong here
struct ArpeggiatorInstance {
	ticks_per_step: u32,
	tick_counter: u32,
	pattern: ArpeggioData,
	arp: Arpeggiator,
	tempo: TempoDetector,
	pending_events: heapless::Vec<(u64, NoteEvent), U32>
}

impl ArpeggiatorInstance {
	fn restart_transport(&mut self) {
		self.tempo.reset();
		self.tick_counter = self.ticks_per_step - 1;
		self.arp.reset();
	}

	fn tick_clock(&mut self, timestamp: u64) {
		self.tick_counter += 1;
		if self.tick_counter >= self.ticks_per_step {
			self.tick_counter -= self.ticks_per_step;

			self.tempo.beat(timestamp);
			let time_per_beat = self.tempo.time_per_beat();

			let pending_events = &mut self.pending_events;
			self.arp
				.process_step(&self.pattern, timestamp, |timestamp_steps, event| {
					let event_timestamp =
						timestamp + (time_per_beat as f32 * timestamp_steps) as u64;
					pending_events
						.push((event_timestamp, event))
						.map_err(|_| ())
				})
				.expect("process_step failed (buffer overflow?)");
		}
	}

	fn currently_playing_tick(&self) -> f32 {
		(self.arp.step() as f32 - 1.0 + self.tick_counter as f32 / self.ticks_per_step as f32)
			.rem_euclid(self.pattern.pattern.len() as f32)
	}

	fn process_pending_events(
		&mut self,
		time_limit: u64,
		mut callback: impl FnMut(&[(u64, NoteEvent)])
	) {
		let before_sort = format!("{:?}", self.pending_events);
		self.pending_events.sort();
		let end = self
			.pending_events
			.iter()
			.enumerate()
			.filter(|(_, ev)| ev.0 >= time_limit)
			.map(|(i, _)| i)
			.next()
			.unwrap_or(self.pending_events.len());

		if end != 0 {
			println!("==== {}", end);
			println!("{}", before_sort);
			println!("{:?}", self.pending_events);
		}

		callback(&self.pending_events[0..end]);

		for i in 0..(self.pending_events.len() - end) {
			self.pending_events[i] = self.pending_events[i + end];
		}
		self.pending_events
			.truncate(self.pending_events.len() - end);
	}

	fn new() -> ArpeggiatorInstance {
		ArpeggiatorInstance {
			ticks_per_step: 6,
			tick_counter: 0,
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
			pending_events: heapless::Vec::new()
		}
	}
}

impl JackDriver {
	pub fn new(name: &str, client: &jack::Client) -> Result<JackDriver, jack::Error> {
		let driver = JackDriver {
			in_port: client.register_port(&format!("{}_in", name), MidiIn)?,
			out_port: client.register_port(&format!("{}_out", name), MidiOut)?,
			ui_in_port: client.register_port(&format!("{}_launchpad_in", name), MidiIn)?,
			ui_out_port: client.register_port(&format!("{}_launchpad_out", name), MidiOut)?,
			ui: LaunchpadX::new(),
			gui_controller: GuiController::new(),
			periods: 0,

			arp_instance: ArpeggiatorInstance::new(),

			time: 0,
			channel: 0,
			out_channel: 0,
			last_midiclock_received: 0,
			next_midiclock_to_send: 0,
			time_between_midiclocks: 24000 / 24,
			clock_mode: ClockMode::Auto
		};
		Ok(driver)
	}

	pub fn autoconnect(&self, client: &jack::Client) {
		for p in client.ports(
			Some(".*playback.*Launchpad X (LPX MIDI In|MIDI 2)"),
			None,
			jack::PortFlags::empty()
		) {
			client
				.connect_ports(&self.ui_out_port, &client.port_by_name(&p).unwrap())
				.expect("Failed to connect");
		}
		for p in client.ports(
			Some(".*capture.*Launchpad X (LPX MIDI In|MIDI2)"),
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
			let clock_mode = &mut self.clock_mode;
			let time = self.time;
			let arp_instance = &mut self.arp_instance;
			self.ui.handle_midi(ev.bytes, |_ui, event| {
				gui_controller.handle_input(
					event,
					&mut arp_instance.pattern,
					use_external_clock,
					clock_mode,
					time_between_midiclocks,
					&mut arp_instance.arp.chord_hold,
					&mut arp_instance.arp.chord_settle_time,
					&mut [
						Some((&mut arp_instance.arp.global_length_modifier, 0.0..=2.0)),
						None,
						Some((&mut arp_instance.arp.intensity_length_modifier_amount, 0.0..=2.0)),
						None,
						Some((&mut arp_instance.arp.global_velocity, 0.0..=2.0)),
						None,
						Some((&mut arp_instance.arp.intensity_velocity_amount, 0.0..=2.0))
					],
					time
				);
			});
		}

		let mut ui_writer = self.ui_out_port.writer(scope);

		for event in self.in_port.iter(scope) {
			let timestamp = self.time + event.time as u64;

			if event.bytes[0] == 0xFA {
				// start
				self.arp_instance.restart_transport();
				self.arp_instance
					.pending_events
					.push((timestamp, NoteEvent::Start))
					.expect("Failed to write tick event");
			}
			if event.bytes[0] == 0xF8 {
				// clock
				self.last_midiclock_received = self.time;

				if use_external_clock {
					ui_writer.write(&event).ok();
					self.arp_instance
						.pending_events
						.push((timestamp, NoteEvent::Clock))
						.expect("Failed to write tick event");
					self.arp_instance.tick_clock(timestamp);
				}
			}
			if event.bytes[0] == 0x90 | self.channel {
				self.arp_instance.arp.note_on(Note(event.bytes[1]), timestamp);
			}
			if event.bytes[0] == 0x80 | self.channel {
				self.arp_instance.arp.note_off(Note(event.bytes[1]), timestamp);
			}
		}

		if !use_external_clock {
			self.next_midiclock_to_send = self.next_midiclock_to_send.max(self.time);

			if self.next_midiclock_to_send < self.time + scope.n_frames() as u64 {
				ui_writer
					.write(&jack::RawMidi {
						time: (self.next_midiclock_to_send - self.time) as jack::Frames,
						bytes: &[0xF8]
					})
					.ok();
				self.arp_instance
					.pending_events
					.push((self.next_midiclock_to_send, NoteEvent::Clock))
					.expect("Failed to write tick event");

				self.arp_instance.tick_clock(self.next_midiclock_to_send);

				self.next_midiclock_to_send =
					self.next_midiclock_to_send + self.time_between_midiclocks;
			}
		}

		let ui = &mut self.ui;
		self.gui_controller.draw(
			&self.arp_instance.pattern,
			self.arp_instance.currently_playing_tick(),
			use_external_clock,
			external_clock_present,
			self.clock_mode,
			self.arp_instance.arp.chord_hold,
			&[
				Some((self.arp_instance.arp.global_length_modifier, 0.0..=2.0)),
				None,
				Some((self.arp_instance.arp.intensity_length_modifier_amount, 0.0..=2.0)),
				None,
				Some((self.arp_instance.arp.global_velocity, 0.0..=2.0)),
				None,
				Some((self.arp_instance.arp.intensity_velocity_amount, 0.0..=2.0))
			],
			self.time,
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

		let mut writer = self.out_port.writer(scope);
		let time = self.time;
		let out_channel = self.out_channel;
		self.arp_instance
			.process_pending_events(self.time + (scope.n_frames() as u64), |events| {
				for event in events {
					println!("event: {:?}", event);
					let bytes: heapless::Vec<_, U4> = match event.1 {
						NoteEvent::NoteOn(note, velo) => {
							heapless::Vec::from_slice(&[0x90 | out_channel, note.0, velo])
						}
						NoteEvent::NoteOff(note) => {
							heapless::Vec::from_slice(&[0x80 | out_channel, note.0, 64])
						}
						NoteEvent::Clock => heapless::Vec::from_slice(&[0xF8]),
						NoteEvent::Start => heapless::Vec::from_slice(&[0xFA])
					}
					.unwrap();
					writer
						.write(&jack::RawMidi {
							time: (event.0 - time) as u32,
							bytes: &bytes
						})
						.expect("Writing to MIDI buffer failed");
				}
			});

		self.time += scope.n_frames() as u64;

		if self.periods == 0 {
			self.autoconnect(client);
		}
		if self.periods == 10 {
			let mut writer = self.ui_out_port.writer(scope);
			self.ui.init(|bytes| {
				writer
					.write(&jack::RawMidi { time: 0, bytes })
					.expect("Writing to UI MIDI buffer failed");
			});
		}
		self.periods += 1;
	}
}
