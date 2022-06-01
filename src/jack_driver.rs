// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::arpeggiator::{ArpeggiatorInstance, ClockMode};
use crate::grid_controllers::launchpad_x::LaunchpadX;
use crate::grid_controllers::GridController;
use heapless;
use jack::*;

use crate::gui::GuiController; // FIXME this should not be in the jack driver
use crate::midi::{Note, NoteEvent};

struct ArpContext {
	in_port: Port<MidiIn>,
	out_port: Port<MidiOut>,
	arp_instance: ArpeggiatorInstance
}

type TransportEventVec = heapless::Vec<(u64, NoteEvent), 16>;

pub struct JackDriver {
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

	routing_matrix: Vec<Vec<bool>>,
	old_routing_matrix: Vec<Vec<bool>>,
	active_arp: usize,
	arp_contexts: Vec<ArpContext>
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
		let mut arp_contexts = Vec::new();
		for i in 0..n_arps {
			arp_contexts.push(ArpContext {
				in_port: client.register_port(&format!("{}_{}_in", name, i), MidiIn)?,
				out_port: client.register_port(&format!("{}_{}_out", name, i), MidiOut)?,
				arp_instance: ArpeggiatorInstance::new()
			})
		}

		let driver = JackDriver {
			ui_in_port: client.register_port(&format!("{}_launchpad_in", name), MidiIn)?,
			ui_out_port: client.register_port(&format!("{}_launchpad_out", name), MidiOut)?,
			ui: LaunchpadX::new(),
			gui_controller: GuiController::new(),
			periods: 0,

			active_arp: 0,

			time: 0,
			channel: 0,
			out_channel: 0,
			last_midiclock_received: 0,
			next_midiclock_to_send: 0,
			time_between_midiclocks: 24000 / 24,
			clock_mode: ClockMode::Auto,
			arp_contexts,
			routing_matrix: vec![vec![false; n_arps]; n_arps],
			old_routing_matrix: vec![vec![false; n_arps]; n_arps]
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

	pub fn process_ui_input(&mut self, use_external_clock: bool, scope: &ProcessScope) {
		// FIXME magic (huge) constant
		let mut active_patterns: heapless::Vec<usize, 64> = self
			.arp_contexts
			.iter()
			.map(|context| context.arp_instance.active_pattern)
			.collect();

		let gui_controller = &mut self.gui_controller;
		let time_between_midiclocks = &mut self.time_between_midiclocks;
		let clock_mode = &mut self.clock_mode;
		let time = self.time;
		let arp_instance = &mut self.arp_contexts[self.active_arp].arp_instance;
		let active_arp = &mut self.active_arp;
		let routing_matrix = &mut self.routing_matrix;

		for ev in self.ui_in_port.iter(scope) {
			println!("event!");
			self.ui.handle_midi(ev.bytes, |_ui, event| {
				gui_controller.handle_input(
					event,
					&mut arp_instance.patterns[arp_instance.active_pattern],
					8, // FIXME
					&mut active_patterns,
					active_arp,
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

		for (active_pattern, context) in active_patterns.iter().zip(self.arp_contexts.iter_mut()) {
			context.arp_instance.active_pattern = *active_pattern;
		}
	}

	pub fn process_clocks(
		&mut self,
		use_external_clock: bool,
		scope: &ProcessScope
	) -> TransportEventVec {
		let mut transport_events = TransportEventVec::new();

		for event in self.arp_contexts[0].in_port.iter(scope) {
				let timestamp = self.time + event.time as u64;

				if event.bytes[0] == 0xFA {
					transport_events.push((timestamp, NoteEvent::Start)).ok();
				}
				if event.bytes[0] == 0xF8 {
					self.last_midiclock_received = self.time;

					if use_external_clock {
						transport_events.push((timestamp, NoteEvent::Clock)).ok();
					}
				}
		}

		if !use_external_clock {
			self.next_midiclock_to_send = self.next_midiclock_to_send.max(self.time);

			while self.next_midiclock_to_send < self.time + scope.n_frames() as u64 {
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
		scope: &ProcessScope
	) {
		let mut ui_writer = self.ui_out_port.writer(scope);

		for (timestamp, event) in transport_events.iter() {
			match event {
				NoteEvent::Clock => {
					ui_writer
						.write(&jack::RawMidi {
							time: (timestamp - self.time) as jack::Frames,
							bytes: &[0xF8]
						})
						.ok();
				}
				_ => ()
			}
		}

		let ui = &mut self.ui;
		// FIXME magic (huge) constant
		let active_patterns: heapless::Vec<usize, 64> = self
			.arp_contexts
			.iter()
			.map(|context| context.arp_instance.active_pattern)
			.collect();
		let arp_instance = &mut self.arp_contexts[self.active_arp].arp_instance;
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
					ui_writer
						.write(&jack::RawMidi {
							time: scope.n_frames() - 1,
							bytes
						})
						.expect("Writing to UI MIDI buffer failed");
				});
			}
		);
	}

	pub fn process(&mut self, client: &jack::Client, scope: &ProcessScope) {
		let external_clock_present = self.time - self.last_midiclock_received <= 48000;
		let use_external_clock = match self.clock_mode {
			ClockMode::Internal => false,
			ClockMode::External => true,
			ClockMode::Auto => external_clock_present
		};

		let transport_events = self.process_clocks(use_external_clock, scope);

		self.process_ui_input(use_external_clock, scope);
		self.process_ui_output(
			&transport_events,
			use_external_clock,
			external_clock_present,
			scope
		);

		let n_contexts = self.arp_contexts.len();
		// TODO FIXME clean this up
		for i in 0..n_contexts {
			let (context, context_tail) = self.arp_contexts[i..].split_first_mut().unwrap();

			// input
			for event in context.in_port.iter(scope) {
				let timestamp = self.time + event.time as u64;

				if event.bytes[0] == 0x90 | self.channel {
					context
						.arp_instance
						.arp
						.note_on(Note(event.bytes[1]), timestamp);
				}
				if event.bytes[0] == 0x80 | self.channel {
					context
						.arp_instance
						.arp
						.note_off(Note(event.bytes[1]), timestamp);
				}
			}

			// tick
			for (timestamp, event) in transport_events.iter() {
				context
					.arp_instance
					.add_pending_event(*timestamp, *event)
					.expect("Failed to write tick event");
				match event {
					NoteEvent::Clock => {
						context.arp_instance.tick_clock(*timestamp);
					}
					NoteEvent::Start => {
						context.arp_instance.restart_transport();
					}
					_ => ()
				}
			}
			
			// output
			let mut writer = context.out_port.writer(scope);
			let time = self.time;
			let out_channel = self.out_channel;
			let routing_matrix = &self.routing_matrix;
			let old_routing_matrix = &mut self.old_routing_matrix;
			assert!(check_routing_matrix(routing_matrix));

			// send note offs when a routing was just disabled
			for j in (i+1) .. n_contexts {
				if old_routing_matrix[i][j] && !routing_matrix[i][j] {
					for note in context.arp_instance.pending_note_offs() {
						let other_context = &mut context_tail[j - (i+1)];
						other_context.arp_instance.arp.note_off(note, self.time);
					}
				}
				old_routing_matrix[i][j] = routing_matrix[i][j];
			}

			context.arp_instance.process_pending_events(
				self.time + (scope.n_frames() as u64),
				|events| {
					for event in events {
						for j in (i+1) .. n_contexts {
							let other_context = &mut context_tail[j - (i+1)];
							if routing_matrix[i][j] {
								match event.1 {
									NoteEvent::NoteOn(note, _) => {
										other_context.arp_instance.arp.note_on(note, event.0);
									}
									NoteEvent::NoteOff(note) => {
										other_context.arp_instance.arp.note_off(note, event.0);
									}
									_ => ()
								}
							}
						}

						let bytes: heapless::Vec<_, 4> = match event.1 {
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
				}
			);
		}

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
