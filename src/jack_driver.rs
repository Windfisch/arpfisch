// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::arpeggiator::{ArpeggiatorInstance, ClockMode};
use crate::grid_controllers::launchpad_x::LaunchpadX;
use crate::grid_controllers::GridController;
use heapless;
use jack::*;

use crate::gui::GuiController; // FIXME this should not be in the jack driver
use crate::midi::{NoteEvent, Channel};

type TransportEventVec = heapless::Vec<(u64, NoteEvent), 16>;

pub struct JackDriver {
	ui_in_port: Port<MidiIn>,
	ui_out_port: Port<MidiOut>,
	periods: u64,

	arp_in_ports: Vec<Port<MidiIn>>,
	arp_out_ports: Vec<Port<MidiOut>>,

	midi_driver: MidiDriver
}

pub type RawMidiEvent = heapless::Vec<u8, 3>;

pub struct TimestampedNoteEvent { // FIXME misnomer
	time: u32,
	event: NoteEvent
}

pub struct TimeStampedRawMidiEvent {
	time: u32,
	event: RawMidiEvent
}

pub trait DriverFrame {
	type EventIterator: Iterator<Item = TimestampedNoteEvent>;
	type RawMidiIterator: Iterator<Item = TimeStampedRawMidiEvent>;

	fn send_event(&mut self, port_number: usize, time: u32, event: NoteEvent) -> Result<(), ()>;
	fn read_events(&self, port_number: usize) -> Self::EventIterator;
	fn send_ui_event(&mut self, time: u32, event: &[u8]) -> Result<(), ()>;
	fn read_ui_events(&self) -> Self::RawMidiIterator;
	fn ui_just_connected(&self) -> bool;
	fn len(&self) -> u32;
}

pub struct MidiDriver {
	ui: LaunchpadX,
	gui_controller: GuiController,

	time: u64,
	in_channel: Channel,
	out_channel: Channel,
	last_midiclock_received: u64,
	next_midiclock_to_send: u64,
	time_between_midiclocks: u64,
	clock_mode: ClockMode,

	restart_transport_pending: bool,

	routing_matrix: Vec<Vec<bool>>,
	old_routing_matrix: Vec<Vec<bool>>,
	active_arp: usize,
	arp_instances: Vec<ArpeggiatorInstance>
}

fn check_routing_matrix(matrix: &Vec<Vec<bool>>) -> bool {
	assert!(matrix.iter().all(|arr| arr.len() == matrix.len()), "Routing matrix must be quadratic");

	for i in 0..matrix.len() {
		for j in 0..i {
			assert!(matrix[i][j] == false);
		}
	}

	true
}

impl JackDriver {
	pub fn new(
		name: &str,
		n_arps: usize,
		client: &jack::Client
	) -> Result<JackDriver, jack::Error> {
		let mut arp_in_ports = Vec::new();
		let mut arp_out_ports = Vec::new();
		for i in 0..n_arps {
			arp_in_ports.push(client.register_port(&format!("{}_{}_in", name, i), MidiIn)?);
			arp_out_ports.push(client.register_port(&format!("{}_{}_out", name, i), MidiOut)?);
		}

		let driver = JackDriver {
			ui_in_port: client.register_port(&format!("{}_launchpad_in", name), MidiIn)?,
			ui_out_port: client.register_port(&format!("{}_launchpad_out", name), MidiOut)?,
			arp_in_ports,
			arp_out_ports,
			periods: 0,
			midi_driver: MidiDriver::new(n_arps)
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

		struct MyDriverFrame<'a> {
			arp_writers: Vec<jack::MidiWriter<'a>>,
			arp_inputs: &'a [Port<MidiIn>],
			ui_writer: jack::MidiWriter<'a>,
			ui_input: &'a Port<MidiIn>,
			scope: &'a ProcessScope,
			ui_just_connected: bool
		}

		struct MyRawMidiIterator<'a>(jack::MidiIter<'a>);

		impl<'a> Iterator for MyRawMidiIterator<'a> {
			type Item = TimeStampedRawMidiEvent;
			fn next(&mut self) -> Option<Self::Item> {
				use std::convert::TryInto;
				self.0.find_map(|ev| 
					ev.bytes.try_into().ok().map(|vec|
						TimeStampedRawMidiEvent {
							time: ev.time,
							event: vec
						}
					)
				)
			}
		}

		struct MyEventIterator<'a>(jack::MidiIter<'a>);

		impl<'a> Iterator for MyEventIterator<'a> {
			type Item = TimestampedNoteEvent;
			fn next(&mut self) -> Option<Self::Item> {
				self.0.find_map(|ev| {
					NoteEvent::parse(ev.bytes).map(|event| TimestampedNoteEvent {
						time: ev.time,
						event
					})
				}) // FIXME
			}
		}

		impl<'a> DriverFrame for MyDriverFrame<'a> {
			type RawMidiIterator = MyRawMidiIterator<'a>;
			type EventIterator = MyEventIterator<'a>;

			fn ui_just_connected(&self) -> bool {
				self.ui_just_connected
			}

			fn read_ui_events(&self) -> Self::RawMidiIterator {
				MyRawMidiIterator(self.ui_input.iter(self.scope))
			}

			fn send_ui_event(&mut self, time: u32, bytes: &[u8]) -> Result<(), ()> {
				self.ui_writer.write(&jack::RawMidi {
					time,
					bytes
				}).map_err(|_| ())
			}

			fn read_events(&self, port_number: usize) -> Self::EventIterator {
				MyEventIterator(self.arp_inputs[port_number].iter(self.scope))
			}

			fn send_event(&mut self, port_number: usize, time: u32, event: NoteEvent) -> Result<(), ()> {
				self.arp_writers[port_number].write(&jack::RawMidi {
					time,
					bytes: &event.to_bytes()
				}).map_err(|_| ())
			}

			fn len(&self) -> u32 {
				self.scope.n_frames()
			}
		}

		let mut frame = MyDriverFrame {
			arp_writers: self.arp_out_ports.iter_mut().map(|p| p.writer(&scope)).collect(),
			arp_inputs: &self.arp_in_ports,
			ui_writer: self.ui_out_port.writer(&scope),
			ui_input: &self.ui_in_port,
			scope,
			ui_just_connected: self.periods == 10
		};

		self.midi_driver.process(&mut frame);


		if self.periods == 0 {
			self.autoconnect(client);
		}
		self.periods += 1;
	}
}

impl MidiDriver {
	pub fn new(n_arps: usize) -> MidiDriver {
		let mut arp_instances = Vec::new();
		for _ in 0..n_arps {
			arp_instances.push(ArpeggiatorInstance::new());
		}

		MidiDriver{
			active_arp: 0,
			time: 0,
			restart_transport_pending: false,
			in_channel: Channel(0),
			out_channel: Channel(0),
			last_midiclock_received: 0,
			next_midiclock_to_send: 0,
			time_between_midiclocks: 24000 / 24,
			clock_mode: ClockMode::Auto,
			arp_instances,
			routing_matrix: vec![vec![false; n_arps]; n_arps],
			old_routing_matrix: vec![vec![false; n_arps]; n_arps],
			ui: LaunchpadX::new(),
			gui_controller: GuiController::new(),
		}
	}

	pub fn process_ui_input(&mut self, use_external_clock: bool, frame: &mut impl DriverFrame) {
		// FIXME magic (huge) constant
		let mut active_patterns: heapless::Vec<usize, 64> = self
			.arp_instances
			.iter()
			.map(|instance| instance.active_pattern)
			.collect();

		let gui_controller = &mut self.gui_controller;
		let time_between_midiclocks = &mut self.time_between_midiclocks;
		let clock_mode = &mut self.clock_mode;
		let time = self.time;
		let arp_instance = &mut self.arp_instances[self.active_arp];
		let active_arp = &mut self.active_arp;
		let routing_matrix = &mut self.routing_matrix;
		let restart_transport_pending = &mut self.restart_transport_pending;

		for ev in frame.read_ui_events() {
			println!("event!");
			self.ui.handle_midi(&ev.event, |_ui, event| {
				gui_controller.handle_input(
					event,
					&mut arp_instance.patterns[arp_instance.active_pattern],
					8, // FIXME
					&mut active_patterns,
					active_arp,
					restart_transport_pending,
					use_external_clock,
					clock_mode,
					time_between_midiclocks,
					&mut arp_instance.ticks_per_step,
					&mut arp_instance.arp.chord_hold,
					&mut arp_instance.arp.chord_settle_time,
					&mut arp_instance.arp.scale,
					&mut arp_instance.arp.scale_base_override,
					&mut [
						Some((&mut arp_instance.arp.global_length_modifier, 0.0..=2.0)),
						None,
						Some((
							&mut arp_instance.arp.intensity_length_modifier_amount,
							0.0..=2.0
						)),
						None,
						Some((&mut arp_instance.arp.global_velocity, 0.0..=2.0)),
						None,
						Some((&mut arp_instance.arp.intensity_velocity_amount, 0.0..=2.0))
					],
					routing_matrix,
					time
				);
			});
		}

		for (active_pattern, instance) in active_patterns.iter().zip(self.arp_instances.iter_mut()) {
			instance.active_pattern = *active_pattern;
		}
	}

	pub fn process_clocks(
		&mut self,
		use_external_clock: bool,
		frame: &mut impl DriverFrame
	) -> TransportEventVec {
		let mut transport_events = TransportEventVec::new();

		if self.restart_transport_pending {
			transport_events.push((self.time, NoteEvent::Start)).ok();
			self.restart_transport_pending = false;
		}

		for event in frame.read_events(0) {
			let timestamp = self.time + event.time as u64;

			match event.event {
				NoteEvent::Clock => {
					self.last_midiclock_received = self.time;

					if use_external_clock {
						transport_events.push((timestamp, NoteEvent::Clock)).ok();
					}
				}
				NoteEvent::Start => {
					transport_events.push((timestamp, NoteEvent::Start)).ok();
				}
				_ => ()
			}
		}

		if !use_external_clock {
			self.next_midiclock_to_send = self.next_midiclock_to_send.max(self.time);

			while self.next_midiclock_to_send < self.time + frame.len() as u64 {
				transport_events
					.push((self.next_midiclock_to_send, NoteEvent::Clock))
					.ok();
				self.next_midiclock_to_send += self.time_between_midiclocks;
			}
		}

		transport_events
	}

	pub fn process_ui_output(
		&mut self,
		transport_events: &TransportEventVec,
		use_external_clock: bool,
		external_clock_present: bool,
		frame: &mut impl DriverFrame
	) {
		for (timestamp, event) in transport_events.iter() {
			match event {
				NoteEvent::Clock => {
					frame.send_ui_event((timestamp - self.time) as u32, &[0xF8]).ok();
				}
				_ => ()
			}
		}

		let ui = &mut self.ui;
		// FIXME magic (huge) constant
		let active_patterns: heapless::Vec<usize, 64> = self
			.arp_instances
			.iter()
			.map(|instance| instance.active_pattern)
			.collect();
		let arp_instance = &mut self.arp_instances[self.active_arp];
		self.gui_controller.draw(
			&arp_instance.patterns[arp_instance.active_pattern],
			&active_patterns,
			self.active_arp,
			arp_instance.currently_playing_tick(),
			use_external_clock,
			external_clock_present,
			self.clock_mode,
			arp_instance.ticks_per_step,
			arp_instance.arp.chord_hold,
			&arp_instance.arp.scale,
			arp_instance.arp.scale_base_override,
			&[
				Some((arp_instance.arp.global_length_modifier, 0.0..=2.0)),
				None,
				Some((arp_instance.arp.intensity_length_modifier_amount, 0.0..=2.0)),
				None,
				Some((arp_instance.arp.global_velocity, 0.0..=2.0)),
				None,
				Some((arp_instance.arp.intensity_velocity_amount, 0.0..=2.0))
			],
			&self.routing_matrix,
			self.time,
			|pos, color| {
				ui.set(pos, color, |bytes| {
					frame.send_ui_event(frame.len() - 1, bytes).ok();
				});
			}
		);
	}

	pub fn process(&mut self, frame: &mut impl DriverFrame) {
		if frame.ui_just_connected() {
			self.ui.init(|bytes| {
				frame.send_ui_event(0, bytes)
					.expect("Writing to UI MIDI buffer failed");
			});
		}

		let external_clock_present = self.time - self.last_midiclock_received <= 48000;
		let use_external_clock = match self.clock_mode {
			ClockMode::Internal => false,
			ClockMode::External => true,
			ClockMode::Auto => external_clock_present
		};

		let transport_events = self.process_clocks(use_external_clock, frame);

		self.process_ui_input(use_external_clock, frame);
		self.process_ui_output(
			&transport_events,
			use_external_clock,
			external_clock_present,
			frame
		);

		let n_instances = self.arp_instances.len();
		// TODO FIXME clean this up
		for i in 0..n_instances {
			let (instance, instance_tail) = self.arp_instances[i..].split_first_mut().unwrap();

			// input
			for event in frame.read_events(i) {
				let timestamp = self.time + event.time as u64;

				match event.event {
					NoteEvent::NoteOn(note, _velocity, channel) =>
						if channel == self.in_channel {
							instance.arp.note_on(note, timestamp)
						},
					NoteEvent::NoteOff(note, channel) =>
						if channel == self.in_channel {
							instance.arp.note_off(note, timestamp)
						},
					_ => ()
				}
			}

			// tick
			for (timestamp, event) in transport_events.iter() {
				instance
					.add_pending_event(*timestamp, *event)
					.expect("Failed to write tick event");
				match event {
					NoteEvent::Clock => {
						instance.tick_clock(*timestamp);
					}
					NoteEvent::Start => {
						instance.restart_transport();
					}
					_ => ()
				}
			}
			
			// output
			let time = self.time;
			let out_channel = self.out_channel;
			let routing_matrix = &self.routing_matrix;
			let old_routing_matrix = &mut self.old_routing_matrix;
			assert!(check_routing_matrix(routing_matrix));

			// send note offs when a routing was just disabled
			for j in (i+1) .. n_instances {
				if old_routing_matrix[i][j] && !routing_matrix[i][j] {
					for note in instance.pending_note_offs() {
						let other_context = &mut instance_tail[j - (i+1)];
						other_context.arp.note_off(note, self.time);
					}
				}
				old_routing_matrix[i][j] = routing_matrix[i][j];
			}

			instance.process_pending_events(
				self.time + (frame.len() as u64),
				|events| {
					for event in events {
						for j in (i+1) .. n_instances {
							let other_context = &mut instance_tail[j - (i+1)];
							if routing_matrix[i][j] {
								match event.1 {
									NoteEvent::NoteOn(note, _, _) => {
										other_context.arp.note_on(note, event.0);
									}
									NoteEvent::NoteOff(note, _) => {
										other_context.arp.note_off(note, event.0);
									}
									_ => ()
								}
							}
						}

						frame.send_event(i, (event.0 - time) as u32, event.1.with_channel(out_channel))
							.expect("Writing to MIDI buffer failed");
					}
				}
			);
		}

		self.time += frame.len() as u64;
	}
}
