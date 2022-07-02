// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::arpeggiator::{ArpeggiatorInstance, ClockMode};
use crate::driver::DriverFrame;
use crate::grid_controllers::launchpad_x::LaunchpadX;
use crate::grid_controllers::GridController;
use crate::gui::GuiController;
use crate::midi::{Channel, MidiEvent};
use heapless;
use serde::{Deserialize, Serialize};

fn check_routing_matrix(matrix: &Vec<Vec<bool>>) -> bool {
	assert!(
		matrix.iter().all(|arr| arr.len() == matrix.len()),
		"Routing matrix must be quadratic"
	);

	for i in 0..matrix.len() {
		for j in 0..i {
			assert!(matrix[i][j] == false);
		}
	}

	true
}

type TransportEventVec = heapless::Vec<(u64, MidiEvent), 16>;

impl std::io::Write for &mut SaveBuffer {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		self.0.extend_from_slice(buf).map_err(|_| {
			std::io::Error::new(std::io::ErrorKind::OutOfMemory, "SaveBuffer exceeded.")
		})?;
		Ok(buf.len())
	}
	fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

pub struct SaveBuffer(pub heapless::Vec<u8, 1048576>); // 1 MiB should be *definitely* enough...

pub struct ArpApplication {
	ui: LaunchpadX,
	gui_controller: GuiController,

	time: u64,
	last_midiclock_received: u64,
	next_midiclock_to_send: u64,
	old_routing_matrix: Vec<Vec<bool>>,
	restart_transport_pending: bool,

	save_buffer_receive: ringbuf::Consumer<Box<SaveBuffer>>,
	save_buffer_return: ringbuf::Producer<Box<SaveBuffer>>,

	serializable: ArpApplicationSerializable
}

#[derive(Serialize, Deserialize, Clone)]
struct ArpApplicationSerializable {
	in_channel: Channel,
	out_channel: Channel,

	time_between_midiclocks: u64,
	clock_mode: ClockMode,

	routing_matrix: Vec<Vec<bool>>,
	active_arp: usize,

	arp_instances: Vec<ArpeggiatorInstance>
}

impl ArpApplication {
	pub fn from_reader(
		reader: impl std::io::Read,
		save_buffer_receive: ringbuf::Consumer<Box<SaveBuffer>>,
		save_buffer_return: ringbuf::Producer<Box<SaveBuffer>>
	) -> anyhow::Result<Box<ArpApplication>> {
		let serializable: Box<ArpApplicationSerializable> = Box::new(serde_json::from_reader(reader)?);
		let n_arps = serializable.arp_instances.len();
		if n_arps <= 0 {
			anyhow::bail!("Illegal number of arpeggiators");
		}
		if serializable.routing_matrix.len() != n_arps || serializable.routing_matrix[0].len() != n_arps {
			anyhow::bail!("Routing matrix size must match number of arpeggiators");
		}

		Ok(Box::new(ArpApplication {
			time: 0,
			restart_transport_pending: false,
			last_midiclock_received: 0,
			next_midiclock_to_send: 0,
			serializable: *serializable,
			old_routing_matrix: vec![vec![false; n_arps]; n_arps],
			ui: LaunchpadX::new(),
			gui_controller: GuiController::new(),
			save_buffer_receive,
			save_buffer_return
		}))
	}

	pub fn new(
		n_arps: usize,
		save_buffer_receive: ringbuf::Consumer<Box<SaveBuffer>>,
		save_buffer_return: ringbuf::Producer<Box<SaveBuffer>>
	) -> Box<ArpApplication> {
		let mut arp_instances = Vec::new();
		for _ in 0..n_arps {
			arp_instances.push(ArpeggiatorInstance::new());
		}

		Box::new(ArpApplication {
			time: 0,
			restart_transport_pending: false,
			last_midiclock_received: 0,
			next_midiclock_to_send: 0,
			serializable: ArpApplicationSerializable {
				time_between_midiclocks: 24000 / 24,
				clock_mode: ClockMode::Auto,
				arp_instances,
				routing_matrix: vec![vec![false; n_arps]; n_arps],
				active_arp: 0,
				in_channel: Channel(0),
				out_channel: Channel(0)
			},
			old_routing_matrix: vec![vec![false; n_arps]; n_arps],
			ui: LaunchpadX::new(),
			gui_controller: GuiController::new(),
			save_buffer_receive,
			save_buffer_return
		})
	}

	pub fn n_arps(&self) -> usize { self.serializable.arp_instances.len() }

	fn process_ui_input(&mut self, use_external_clock: bool, frame: &mut impl DriverFrame) {
		// FIXME magic (huge) constant
		let mut active_patterns: heapless::Vec<usize, 64> = self
			.serializable
			.arp_instances
			.iter()
			.map(|instance| instance.active_pattern)
			.collect();

		let gui_controller = &mut self.gui_controller;
		let time_between_midiclocks = &mut self.serializable.time_between_midiclocks;
		let clock_mode = &mut self.serializable.clock_mode;
		let time = self.time;
		let arp_instance = &mut self.serializable.arp_instances[self.serializable.active_arp];
		let active_arp = &mut self.serializable.active_arp;
		let routing_matrix = &mut self.serializable.routing_matrix;
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

		for (active_pattern, instance) in active_patterns
			.iter()
			.zip(self.serializable.arp_instances.iter_mut())
		{
			instance.active_pattern = *active_pattern;
		}
	}

	fn process_clocks(
		&mut self,
		use_external_clock: bool,
		frame: &mut impl DriverFrame
	) -> TransportEventVec {
		let mut transport_events = TransportEventVec::new();

		if self.restart_transport_pending {
			transport_events.push((self.time, MidiEvent::Start)).ok();
			self.restart_transport_pending = false;
		}

		for event in frame.read_events(0) {
			let timestamp = self.time + event.time as u64;

			match event.event {
				MidiEvent::Clock => {
					self.last_midiclock_received = self.time;

					if use_external_clock {
						transport_events.push((timestamp, MidiEvent::Clock)).ok();
					}
				}
				MidiEvent::Start => {
					transport_events.push((timestamp, MidiEvent::Start)).ok();
				}
				_ => ()
			}
		}

		if !use_external_clock {
			self.next_midiclock_to_send = self.next_midiclock_to_send.max(self.time);

			while self.next_midiclock_to_send < self.time + frame.len() as u64 {
				transport_events
					.push((self.next_midiclock_to_send, MidiEvent::Clock))
					.ok();
				self.next_midiclock_to_send += self.serializable.time_between_midiclocks;
			}
		}

		transport_events
	}

	fn process_ui_output(
		&mut self,
		transport_events: &TransportEventVec,
		use_external_clock: bool,
		external_clock_present: bool,
		frame: &mut impl DriverFrame
	) {
		for (timestamp, event) in transport_events.iter() {
			match event {
				MidiEvent::Clock => {
					frame
						.send_ui_event((timestamp - self.time) as u32, &[0xF8])
						.ok();
				}
				_ => ()
			}
		}

		let ui = &mut self.ui;
		// FIXME magic (huge) constant
		let active_patterns: heapless::Vec<usize, 64> = self
			.serializable
			.arp_instances
			.iter()
			.map(|instance| instance.active_pattern)
			.collect();
		let arp_instance = &mut self.serializable.arp_instances[self.serializable.active_arp];
		self.gui_controller.draw(
			&arp_instance.patterns[arp_instance.active_pattern],
			&active_patterns,
			self.serializable.active_arp,
			arp_instance.currently_playing_tick(),
			use_external_clock,
			external_clock_present,
			self.serializable.clock_mode,
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
			&self.serializable.routing_matrix,
			self.time,
			|pos, color| {
				ui.set(pos, color, |bytes| {
					frame.send_ui_event(frame.len() - 1, bytes).ok();
				});
			}
		);
	}

	pub fn process(&mut self, frame: &mut impl DriverFrame) {
		assert_no_alloc::assert_no_alloc(|| self.process_all(frame))
	}

	fn process_all(&mut self, frame: &mut impl DriverFrame) {
		if frame.ui_just_connected() {
			self.ui.init(|bytes| {
				frame
					.send_ui_event(0, bytes)
					.expect("Writing to UI MIDI buffer failed");
			});
		}

		let external_clock_present = self.time - self.last_midiclock_received <= 48000;
		let use_external_clock = match self.serializable.clock_mode {
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

		let n_instances = self.serializable.arp_instances.len();
		// TODO FIXME clean this up
		for i in 0..n_instances {
			let (instance, instance_tail) = self.serializable.arp_instances[i..]
				.split_first_mut()
				.unwrap();

			// input
			for event in frame.read_events(i) {
				let timestamp = self.time + event.time as u64;

				match event.event {
					MidiEvent::NoteOn(note, _velocity, channel) => {
						if channel == self.serializable.in_channel {
							instance.arp.note_on(note, timestamp)
						}
					}
					MidiEvent::NoteOff(note, channel) => {
						if channel == self.serializable.in_channel {
							instance.arp.note_off(note, timestamp)
						}
					}
					_ => ()
				}
			}

			// tick
			for (timestamp, event) in transport_events.iter() {
				instance
					.add_pending_event(*timestamp, *event)
					.expect("Failed to write tick event");
				match event {
					MidiEvent::Clock => {
						instance.tick_clock(*timestamp);
					}
					MidiEvent::Start => {
						instance.restart_transport();
					}
					_ => ()
				}
			}

			// output
			let time = self.time;
			let out_channel = self.serializable.out_channel;
			let routing_matrix = &self.serializable.routing_matrix;
			let old_routing_matrix = &mut self.old_routing_matrix;
			assert!(check_routing_matrix(routing_matrix));

			// send note offs when a routing was just disabled
			for j in (i + 1)..n_instances {
				if old_routing_matrix[i][j] && !routing_matrix[i][j] {
					for note in instance.pending_note_offs() {
						let other_context = &mut instance_tail[j - (i + 1)];
						other_context.arp.note_off(note, self.time);
					}
				}
				old_routing_matrix[i][j] = routing_matrix[i][j];
			}

			instance.process_pending_events(self.time + (frame.len() as u64), |events| {
				for event in events {
					for j in (i + 1)..n_instances {
						let other_context = &mut instance_tail[j - (i + 1)];
						if routing_matrix[i][j] {
							match event.1 {
								MidiEvent::NoteOn(note, _, _) => {
									other_context.arp.note_on(note, event.0);
								}
								MidiEvent::NoteOff(note, _) => {
									other_context.arp.note_off(note, event.0);
								}
								_ => ()
							}
						}
					}

					frame
						.send_event(
							i,
							(event.0 - time) as u32,
							event.1.with_channel(out_channel)
						)
						.expect("Writing to MIDI buffer failed");
				}
			});
		}

		self.time += frame.len() as u64;

		if let Some(mut buffer) = self.save_buffer_receive.pop() {
			serde_json::to_writer_pretty(buffer.as_mut(), &self.serializable).ok();
			self.save_buffer_return
				.push(buffer)
				.map_err(|_| ())
				.unwrap();
		}
	}
}
