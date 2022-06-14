// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::application::ArpApplication;
use crate::driver::{DriverFrame, TimestampedMidiEvent, TimestampedRawMidiEvent};
use crate::midi::MidiEvent;
use jack::*;

pub struct JackDriver {
	ui_in_port: Port<MidiIn>,
	ui_out_port: Port<MidiOut>,
	periods: u64,

	arp_in_ports: Vec<Port<MidiIn>>,
	arp_out_ports: Vec<Port<MidiOut>>,

	application: ArpApplication
}

impl JackDriver {
	pub fn run(name: &str, application: ArpApplication) {
		let client = jack::Client::new(name, jack::ClientOptions::NO_START_SERVER)
			.expect("Failed to connect to JACK")
			.0;

		let mut jack_driver = JackDriver::new_with_client(name, application, &client).unwrap();

		let _async_client = client
			.activate_async(
				(),
				jack::ClosureProcessHandler::new(
					move |client: &jack::Client, scope: &ProcessScope| -> Control {
						jack_driver.process(client, scope);
						return Control::Continue;
					}
				)
			)
			.expect("Failed to activate client");

		loop {
			std::thread::sleep(std::time::Duration::from_secs(1));
		}
	}

	pub fn new_with_client(
		name: &str,
		application: ArpApplication,
		client: &jack::Client
	) -> Result<JackDriver, jack::Error> {
		let mut arp_in_ports = Vec::new();
		let mut arp_out_ports = Vec::new();
		for i in 0..application.n_arps() {
			arp_in_ports.push(client.register_port(&format!("{}_{}_in", name, i), MidiIn)?);
			arp_out_ports.push(client.register_port(&format!("{}_{}_out", name, i), MidiOut)?);
		}

		let driver = JackDriver {
			ui_in_port: client.register_port(&format!("{}_launchpad_in", name), MidiIn)?,
			ui_out_port: client.register_port(&format!("{}_launchpad_out", name), MidiOut)?,
			arp_in_ports,
			arp_out_ports,
			periods: 0,
			application
		};

		Ok(driver)
	}

	fn autoconnect(&self, client: &jack::Client) {
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

	fn process(&mut self, client: &jack::Client, scope: &ProcessScope) {
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
			type Item = TimestampedRawMidiEvent;
			fn next(&mut self) -> Option<Self::Item> {
				use std::convert::TryInto;
				self.0.find_map(|ev| {
					ev.bytes.try_into().ok().map(|vec| TimestampedRawMidiEvent {
						time: ev.time,
						event: vec
					})
				})
			}
		}

		struct MyEventIterator<'a>(jack::MidiIter<'a>);

		impl<'a> Iterator for MyEventIterator<'a> {
			type Item = TimestampedMidiEvent;
			fn next(&mut self) -> Option<Self::Item> {
				self.0.find_map(|ev| {
					MidiEvent::parse(ev.bytes).map(|event| TimestampedMidiEvent {
						time: ev.time,
						event
					})
				})
			}
		}

		impl<'a> DriverFrame for MyDriverFrame<'a> {
			type RawMidiIterator = MyRawMidiIterator<'a>;
			type EventIterator = MyEventIterator<'a>;

			fn ui_just_connected(&self) -> bool { self.ui_just_connected }

			fn read_ui_events(&self) -> Self::RawMidiIterator {
				MyRawMidiIterator(self.ui_input.iter(self.scope))
			}

			fn send_ui_event(&mut self, time: u32, bytes: &[u8]) -> Result<(), ()> {
				self.ui_writer
					.write(&jack::RawMidi { time, bytes })
					.map_err(|_| ())
			}

			fn read_events(&self, port_number: usize) -> Self::EventIterator {
				MyEventIterator(self.arp_inputs[port_number].iter(self.scope))
			}

			fn send_event(
				&mut self,
				port_number: usize,
				time: u32,
				event: MidiEvent
			) -> Result<(), ()> {
				self.arp_writers[port_number]
					.write(&jack::RawMidi {
						time,
						bytes: &event.to_bytes()
					})
					.map_err(|_| ())
			}

			fn len(&self) -> u32 { self.scope.n_frames() }
		}

		let mut frame = MyDriverFrame {
			arp_writers: self
				.arp_out_ports
				.iter_mut()
				.map(|p| p.writer(&scope))
				.collect(),
			arp_inputs: &self.arp_in_ports,
			ui_writer: self.ui_out_port.writer(&scope),
			ui_input: &self.ui_in_port,
			scope,
			ui_just_connected: self.periods == 10
		};

		self.application.process(&mut frame);

		if self.periods == 0 {
			self.autoconnect(client);
		}
		self.periods += 1;
	}
}
