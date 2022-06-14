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

fn main() { JackDriver::run("arpfisch", ArpApplication::new(4)); }
