// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::midi::MidiEvent;
use heapless;

pub type RawMidiEvent = heapless::Vec<u8, 3>;

pub struct TimestampedMidiEvent {
	pub time: u32,
	pub event: MidiEvent
}

pub struct TimestampedRawMidiEvent {
	pub time: u32,
	pub event: RawMidiEvent
}

pub trait DriverFrame {
	type EventIterator: Iterator<Item = TimestampedMidiEvent>;
	type RawMidiIterator: Iterator<Item = TimestampedRawMidiEvent>;

	fn send_event(&mut self, port_number: usize, time: u32, event: MidiEvent) -> Result<(), ()>;
	fn read_events(&self, port_number: usize) -> Self::EventIterator;
	fn send_ui_event(&mut self, time: u32, event: &[u8]) -> Result<(), ()>;
	fn read_ui_events(&self) -> Self::RawMidiIterator;
	fn ui_just_connected(&self) -> bool;
	fn len(&self) -> u32;
}
