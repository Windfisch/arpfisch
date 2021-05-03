mod arpeggiator;
mod grid_controllers;
mod gui;
mod jack_driver;
mod midi;
mod tempo_detector;

use jack_driver::JackDriver;

use heapless;
use heapless::consts::*;
use itertools::Itertools;
use jack;
use jack::{Control, MidiIn, MidiOut, Port, ProcessScope};
use ringbuf;

fn main() {
	let client = jack::Client::new("arpfisch", jack::ClientOptions::NO_START_SERVER)
		.expect("Failed to connect to JACK")
		.0;

	let mut jack_driver = JackDriver::new("fnord", &client).unwrap();

	//let (mut producer, mut consumer) = ringbuf::RingBuffer::<Message>::new(10).split();

	let async_client = client
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
		std::thread::sleep_ms(1000);
	}
}
