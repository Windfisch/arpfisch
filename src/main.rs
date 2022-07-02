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

mod application;
mod arpeggiator;
mod driver;
mod grid_controllers;
mod gui;
mod midi;
mod tempo_detector;

use application::ArpApplication;
use driver::jack::JackDriver;
use std::io::Write;
use std::thread;
use clap::Parser;

#[cfg(debug_assertions)] // required when disable_release is set (default)
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn temp_name(filename: &Path) -> PathBuf {
	let mut result = OsString::new();
	result.push(".");
	result.push(filename.file_name().unwrap());
	result.push(".tmp");
	filename.with_file_name(result)
}


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = Some("A MIDI arpeggiator and step sequencer"))]
struct Args {
	#[clap(short = 'n', long, default_value = "arpfisch")]
	jack_client_name: String,

	filename: String
}


fn main() {
	let save_buffer = Box::new(application::SaveBuffer(heapless::Vec::new()));
	let (mut save_send_producer, save_send_consumer) = ringbuf::RingBuffer::new(1).split();
	let (save_return_producer, mut save_return_consumer) = ringbuf::RingBuffer::new(1).split();

	let args = Args::parse();

	save_send_producer
		.push(save_buffer)
		.map_err(|_| ())
		.unwrap();


	let app = match std::fs::File::open(args.filename.clone()) {
		Ok(file) => {
			ArpApplication::from_reader(file, save_send_consumer, save_return_producer).expect("Failed to load file")
		}
		Err(err) => match err.kind() {
			std::io::ErrorKind::NotFound => ArpApplication::new(4, save_send_consumer, save_return_producer),
			_ => panic!("Failed to open file for reading")
		}
	};
	

	let filename = args.filename.clone();
	thread::spawn(move || {
		let filename = Path::new(&filename);
		loop {
			if let Some(buffer) = save_return_consumer.pop() {
				let mut buffer: Box<application::SaveBuffer> = buffer;
				let mut file = std::fs::File::create(temp_name(&filename))
					.expect("Failed to open temporary savefile");
				file.write_all(&buffer.0)
					.expect("Failed to write to temporary savefile");
				std::fs::rename(temp_name(&filename), &filename)
					.expect("Failed to rename temporary savefile to real one");
				buffer.0.clear();
				save_send_producer.push(buffer).map_err(|_| ()).unwrap();
			}
			thread::sleep(std::time::Duration::from_secs(1));
		}
	});


	JackDriver::run(
		&args.jack_client_name,
		app
	);
}
