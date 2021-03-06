// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::arpeggiator::*;
use crate::grid_controllers::*;
use crate::midi::Note;

mod clock_division;
mod config;
mod edit;
mod pattern_select;
mod routing;
mod scale_select;
mod sliders;

use clock_division::ClockDivisionScreen;
use config::ConfigScreen;
use edit::EditScreen;
use pattern_select::PatternSelectScreen;
use routing::RoutingScreen;
use scale_select::ScaleSelectScreen;
use sliders::SlidersScreen;

enum ScreenOverlay {
	Sliders(SlidersScreen),
	PatternSelect(PatternSelectScreen),
	Routing(RoutingScreen),
	Config(ConfigScreen),
	ScaleSelect(ScaleSelectScreen),
	ClockDivision(ClockDivisionScreen),
	None
}

pub struct GuiController {
	state_down_time: u64,
	flash_scale_button_until: u64,

	edit_screen: EditScreen,
	screen_overlay: ScreenOverlay
}

impl GuiController {
	pub fn new() -> GuiController {
		GuiController {
			edit_screen: EditScreen::new(),
			flash_scale_button_until: 0,
			screen_overlay: ScreenOverlay::None,
			state_down_time: 0
		}
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		pattern: &mut ArpeggioData,
		n_patterns: usize,
		active_pattern: &mut [usize],
		active_arp: &mut usize,
		restart_transport_pending: &mut bool,
		use_external_clock: bool,
		clock_mode: &mut ClockMode,
		time_between_midiclocks: &mut u64,
		ticks_per_step: &mut u32,
		chord_hold: &mut bool,
		chord_settle_time: &mut u64,
		scale: &mut heapless::Vec<Note, 16>,
		scale_base_override: &mut Option<Note>,
		fader_values: &mut [Option<(&mut f32, std::ops::RangeInclusive<f32>)>],
		routing_matrix: &mut Vec<Vec<bool>>,
		time: u64
	) {
		use GridButtonEvent::*;

		println!("Handle input: {:?}", event);

		let state_y = match self.screen_overlay {
			ScreenOverlay::Config(_) => Some(0),
			ScreenOverlay::Sliders(_) => Some(1),
			ScreenOverlay::PatternSelect(_) => Some(2),
			ScreenOverlay::Routing(_) => Some(3),
			ScreenOverlay::ScaleSelect(_) => Some(4),
			ScreenOverlay::ClockDivision(_) => Some(5),
			ScreenOverlay::None => None
		};

		match event {
			Down(8, 7, _) => {
				if scale_base_override.is_none() {
					*chord_hold = !*chord_hold;
					*chord_settle_time = if *chord_hold { 48000 / 40 } else { 0 };
				}
				else {
					self.flash_scale_button_until = time + 2 * 48000;
				}
			}
			Down(8, y, _) => {
				self.state_down_time = time;

				if state_y == Some(y) {
					self.screen_overlay = ScreenOverlay::None;
				}
				else {
					match y {
						0 => self.screen_overlay = ScreenOverlay::Config(ConfigScreen::new()),
						1 => self.screen_overlay = ScreenOverlay::Sliders(SlidersScreen::new()),
						2 => {
							self.screen_overlay =
								ScreenOverlay::PatternSelect(PatternSelectScreen::new())
						}
						3 => self.screen_overlay = ScreenOverlay::Routing(RoutingScreen::new()),
						4 => {
							self.screen_overlay =
								ScreenOverlay::ScaleSelect(ScaleSelectScreen::new())
						}
						5 => {
							self.screen_overlay =
								ScreenOverlay::ClockDivision(ClockDivisionScreen::new())
						}
						_ => ()
					}
				}
			}
			Up(8, y, _) => {
				if state_y == Some(y) {
					if time > self.state_down_time + 48000 / 3 {
						self.screen_overlay = ScreenOverlay::None;
					}
				}
			}
			event => match self.screen_overlay {
				ScreenOverlay::None => {
					self.edit_screen.handle_input(event, pattern, time);
				}
				ScreenOverlay::Config(ref mut config) => {
					config.handle_input(
						event,
						pattern,
						&mut self.edit_screen.pane_height,
						restart_transport_pending,
						use_external_clock,
						clock_mode,
						time_between_midiclocks,
						time
					);
				}
				ScreenOverlay::Sliders(ref mut sliders) => {
					sliders.handle_input(event, fader_values, time);
				}
				ScreenOverlay::PatternSelect(ref mut screen) => {
					screen.handle_input(event, n_patterns, active_pattern, active_arp);
				}
				ScreenOverlay::ScaleSelect(ref mut screen) => {
					screen.handle_input(event, scale, scale_base_override, time);
				}
				ScreenOverlay::Routing(ref mut screen) => {
					screen.handle_input(event, routing_matrix);
				}
				ScreenOverlay::ClockDivision(ref mut screen) => {
					screen.handle_input(event, ticks_per_step, restart_transport_pending, time);
				}
			}
		}

		if !scale.is_empty() && pattern.repeat_mode != RepeatMode::Repeat(12) {
			pattern.repeat_mode = RepeatMode::Repeat(12);
			match self.screen_overlay {
				ScreenOverlay::ScaleSelect(_) => (),
				_ => self.flash_scale_button_until = time + 2 * 48000
			}
		}
	}

	pub fn draw(
		&mut self,
		pattern: &ArpeggioData,
		active_pattern: &[usize],
		active_arp: usize,
		step: f32,
		use_external_clock: bool,
		external_clock_present: bool,
		clock_mode: ClockMode,
		ticks_per_step: u32,
		chord_hold: bool,
		scale: &heapless::Vec<Note, 16>,
		scale_base_override: Option<Note>,
		fader_values: &[Option<(f32, std::ops::RangeInclusive<f32>)>],
		routing_matrix: &Vec<Vec<bool>>,
		time: u64,
		mut set_led: impl FnMut((u8, u8), LightingMode)
	) {
		use std::convert::TryInto;
		use LightingMode::*;

		const MENU_SELECTED: LightingMode = Fade(Color::Color(0, 0.74));

		let mut array = [[None; 9]; 9];
		let (right_buttons, grid_and_top) = (&mut array).split_last_mut().unwrap();
		let grid_and_top = grid_and_top.try_into().unwrap();

		right_buttons[7] = Some(if scale_base_override.is_none() {
			if chord_hold {
				Solid(Color::Color(215, 0.7))
			}
			else {
				Solid(Color::Color(300, 0.1))
			}
		}
		else {
			Solid(Color::Color(60, 0.7))
		});

		right_buttons[4] = if time < self.flash_scale_button_until {
			if (time / (48000 / 10)) % 2 == 0 {
				Some(Off)
			}
			else {
				Some(Solid(Color::Color(215, 0.7)))
			}
		}
		else {
			if scale.is_empty() {
				None
			}
			else {
				Some(Solid(Color::Color(215, 0.7)))
			}
		};

		match self.screen_overlay {
			ScreenOverlay::None => {
				self.edit_screen.draw(grid_and_top, pattern, step, time);
			}
			ScreenOverlay::Config(ref mut screen) => {
				right_buttons[0] = Some(MENU_SELECTED);
				screen.draw(
					grid_and_top,
					pattern,
					self.edit_screen.pane_height,
					use_external_clock,
					external_clock_present,
					clock_mode,
					time
				);
			}
			ScreenOverlay::Sliders(ref mut screen) => {
				right_buttons[1] = Some(MENU_SELECTED);
				screen.draw(grid_and_top, fader_values);
			}
			ScreenOverlay::PatternSelect(ref mut screen) => {
				right_buttons[2] = Some(MENU_SELECTED);
				screen.draw(grid_and_top, active_pattern, active_arp)
			}
			ScreenOverlay::Routing(ref mut screen) => {
				right_buttons[3] = Some(MENU_SELECTED);
				screen.draw(grid_and_top, routing_matrix);
			}
			ScreenOverlay::ScaleSelect(ref mut screen) => {
				right_buttons[4] = Some(MENU_SELECTED);
				screen.draw(grid_and_top, scale, scale_base_override);
			}
			ScreenOverlay::ClockDivision(ref mut screen) => {
				right_buttons[5] = Some(MENU_SELECTED);
				screen.draw(grid_and_top, ticks_per_step, step as u32, time);
			}
		}

		for x in 0..9 {
			for y in 0..9 {
				set_led((x, y), array[x as usize][y as usize].unwrap_or(Off));
			}
		}
	}
}
