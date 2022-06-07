/* arpfisch, a MIDI arpeggiator
   Copyright (C) 2021 Florian Jung

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation version 3.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

mod arpeggiator;
mod grid_controllers;
mod gui;
mod jack_driver;
mod midi;
mod tempo_detector;

use jack_driver::JackDriver;

use jack;
use jack::{Control, ProcessScope};

fn main() {
	let client = jack::Client::new("arpfisch", jack::ClientOptions::NO_START_SERVER)
		.expect("Failed to connect to JACK")
		.0;

	let mut jack_driver = JackDriver::new("fnord", 4, &client).unwrap();

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
