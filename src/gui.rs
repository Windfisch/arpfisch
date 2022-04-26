// this file is part of arpfisch. For copyright and licensing details, see main.rs

use crate::arpeggiator::*;
use crate::grid_controllers::*;
use crate::tempo_detector::TempoDetector;

enum GuiState {
	Edit,
	Config,
	Sliders
}

#[derive(Copy, Clone)]
struct HeldKey {
	coords: (u8, u8),
	pos: usize,
	note: isize,
	time: u64,
	just_set: bool
}

pub struct GuiController {
	state: GuiState,
	pane_height: usize,
	first_x: isize,
	target_first_x: isize,
	last_scroll_update: u64,
	first_y: isize,
	currently_held_key: Option<HeldKey>,
	current_octave: i32,
	tempo: TempoDetector,
	down_times: [[Option<u64>; 8]; 8],
	state_down_time: u64,
	fader_history: [[f32; 2]; 8],
	fader_history_last_update: u64
}

fn octave_hue(octave: i32) -> u16 { (octave + 1) as u16 * 90 }
fn octave_color(octave: i32) -> Color { Color::Color(octave_hue(octave), 1.0) }

impl GuiController {
	pub fn new() -> GuiController {
		GuiController {
			state: GuiState::Edit,
			pane_height: 4,
			first_x: 0,
			target_first_x: 0,
			last_scroll_update: 0,
			first_y: 0,
			currently_held_key: None,
			current_octave: 0,
			tempo: TempoDetector::new(),
			down_times: [[None; 8]; 8],
			fader_history: [[0.0; 2]; 8],
			fader_history_last_update: 0,
			state_down_time: 0
		}
	}

	fn handle_grid_down(
		&mut self,
		(xx, yy): (u8, u8),
		velo: f32,
		pattern: &mut ArpeggioData,
		time: u64
	) {
		assert!(xx < 8 && yy < 8);

		let n_panes = 8 / self.pane_height;
		let pane = yy as usize / self.pane_height;
		let x = xx as isize + self.first_x + 8 * (n_panes - pane - 1) as isize;
		let y = (yy as isize % self.pane_height as isize) + self.first_y;
		if x >= 0 && (x as usize) < pattern.pattern.len() {
			let step_has_any_note = pattern.filter(x as usize, y).count() > 0;

			#[derive(PartialEq)]
			enum PressMode {
				Primary,
				SecondaryUnrelated,
				SetLength
			}
			let mode = if let Some(held) = self.currently_held_key {
				if held.note == y && held.pos < x as usize {
					PressMode::SetLength
				}
				else {
					PressMode::SecondaryUnrelated
				}
			}
			else {
				self.currently_held_key = Some(HeldKey {
					coords: (xx, yy),
					pos: x as usize,
					note: y,
					time,
					just_set: !step_has_any_note
				});
				PressMode::Primary
			};

			if !step_has_any_note {
				match mode {
					PressMode::Primary | PressMode::SecondaryUnrelated => {
						pattern
							.set(
								x as usize,
								Entry {
									note: y,
									len_steps: 1,
									intensity: velo,
									transpose: 12 * self.current_octave
								}
							)
							.ok();
					}
					PressMode::SetLength => {
						let begin_x = self.currently_held_key.unwrap().pos;
						for entry in pattern.filter_mut(begin_x, y) {
							entry.len_steps = (x as isize - begin_x as isize + 1) as u32;
						}
					}
				}
			}
			else {
				match mode {
					PressMode::SecondaryUnrelated => {
						pattern.delete_all(x as usize, y);
					}
					PressMode::Primary | PressMode::SetLength => {}
				}
			}
		}
	}

	fn handle_grid_up(&mut self, (xx, yy): (u8, u8), pattern: &mut ArpeggioData, time: u64) {
		assert!(xx < 8 && yy < 8);

		let n_panes = 8 / self.pane_height;
		let pane = yy as usize / self.pane_height;
		let x = xx as isize + self.first_x + 8 * (n_panes - pane - 1) as isize;
		let y = (yy as isize % self.pane_height as isize) + self.first_y;

		let mut dont_delete = true;
		if let Some(held) = self.currently_held_key {
			if held.coords == (xx, yy) {
				dont_delete = time - held.time >= 10000 || held.just_set;
				self.currently_held_key = None;
			}
		}
		if !dont_delete && x >= 0 && x < pattern.pattern.len() as isize {
			pattern.delete_all(x as usize, y);
		}
	}

	pub fn handle_input(
		&mut self,
		event: GridButtonEvent,
		pattern: &mut ArpeggioData,
		use_external_clock: bool,
		clock_mode: &mut ClockMode,
		time_between_midiclocks: &mut u64,
		chord_hold: &mut bool,
		chord_settle_time: &mut u64,
		fader_values: &mut [Option<(&mut f32, std::ops::RangeInclusive<f32>)>],
		time: u64
	) {
		use GridButtonEvent::*;
		use GuiState::*;

		println!("Handle input: {:?}", event);

		match event {
			Down(x, y, _) => {
				if x < 8 && y < 8 {
					self.down_times[x as usize][y as usize] = Some(time);
				}
			}
			Up(x, y, _) => {
				if x < 8 && y < 8 {
					self.down_times[x as usize][y as usize] = None;
				}
			}
			_ => {}
		}

		match event {
			Down(8, 0, _) => {
				self.state_down_time = time;
				self.state = match self.state {
					Config => Edit,
					_ => Config
				};
			}
			Up(8, 0, _) => {
				match self.state {
					Config => if time > self.state_down_time + 48000 / 3 {
						self.state = Edit
					}
					_ => ()
				}
			}
			Down(8, 1, _) => {
				self.state_down_time = time;
				self.state = match self.state {
					Sliders => Edit,
					_ => Sliders
				};
			}
			Up(8, 1, _) => {
				match self.state {
					Sliders => if time > self.state_down_time + 48000 / 3 {
						self.state = Edit
					}
					_ => ()
				}
			}
			Down(8, 7, _) => {
				*chord_hold = !*chord_hold;
				*chord_settle_time = if *chord_hold { 48000 / 40 } else { 0 };
			}
			event => {
				match self.state {
					Edit => {
						match event {
							Down(0, 8, _) => {
								self.first_y += 1;
							}
							Down(1, 8, _) => {
								self.first_y -= 1;
							}
							Down(2, 8, _) => {
								self.target_first_x -= 8;
							}
							Down(3, 8, _) => {
								self.target_first_x += 8;
							}
							Down(x, 8, _) => {
								let octave = x as i32 - 5;
								if let Some(held) = self.currently_held_key {
									if pattern.filter(held.pos, held.note).count() > 0 {
										// the step can become un-set by disabling all octaves, yet it is still held down
										let entry_opt = pattern
											.filter(held.pos, held.note)
											.find(|e| e.transpose == octave * 12)
											.cloned();
										if let Some(entry) = entry_opt {
											let delete_entry = entry.clone();
											pattern.delete(held.pos, delete_entry);
										}
										else {
											let mut new_entry = pattern
												.filter(held.pos, held.note)
												.next()
												.unwrap()
												.clone();
											new_entry.transpose = octave * 12;
											pattern.set(held.pos, new_entry).ok(); // all we can do is ignore an error
										}
									}
								}
								else {
									self.current_octave = octave;
								}
							}
							Down(xx, yy, velo) if xx < 8 && yy < 8 => {
								self.handle_grid_down((xx, yy), velo, pattern, time);
							}
							Up(xx, yy, _) if xx < 8 && yy < 8 => {
								self.handle_grid_up((xx, yy), pattern, time);
							}
							_ => {}
						}
					}
					Config => match event {
						Down(2, 0, _) => {
							pattern.repeat_mode = RepeatMode::Clamp;
						}
						Down(2, 1, _) => {
							pattern.repeat_mode = RepeatMode::Mirror;
						}
						Down(2, 2, _) => {
							pattern.repeat_mode = RepeatMode::Repeat(12);
						}
						Down(7, 2, _) => {
							if !use_external_clock {
								self.tempo.beat(time);
								if self.tempo.time_per_beat() <= 48000 * 2
									&& self.tempo.time_per_beat() >= 10
								{
									*time_between_midiclocks =
										self.tempo.time_per_beat() as u64 / 24;
								}
							}
						}
						Down(7, 1, _) => {
							use ClockMode::*;
							*clock_mode = match *clock_mode {
								Internal => Auto,
								Auto => External,
								External => Internal
							};
						}
						Down(x, y, _) if (4..8).contains(&y) && x < 8 => {
							let new_len = x + 8 * (8 - y - 1) + 1;
							pattern.pattern.resize_default(new_len as usize).ok();
						}
						Down(0, y, _) if y < 4 => {
							self.pane_height = 8 / (y + 1) as usize;
						}
						Down(3, y, _) if y < 4 => match pattern.repeat_mode {
							RepeatMode::Repeat(_) => {
								pattern.repeat_mode = RepeatMode::Repeat((y as i32 - 1) * 12);
							}
							_ => {}
						},
						_ => {}
					},
					Sliders => {
						match event {
							Down(x, y, _) if x < 8 && y < 8 => {
								for (fader_x, fader) in fader_values.iter_mut().enumerate() {
									if let Some((value, range)) = fader {
										if x as usize == fader_x {
											**value = y as f32 / 7.0
												* (range.end() - range.start()) + range
												.start();
											self.fader_history[x as usize] = [**value; 2];
										}
									}
								}
							}
							Pressure(x, y, pressure) => {
								if x < 8 && y < 8 && (x as usize) < fader_values.len() {
									if let Some((value, range)) = fader_values[x as usize].as_mut()
									{
										if let Some(down_time) =
											self.down_times[x as usize][y as usize]
										{
											if time >= down_time + 48000 / 6 {
												**value = pressure * (range.end() - range.start())
													+ range.start();
											}
										}
									}
								}
							}
							Up(x, y, _) => {
								if x < 8 && y < 8 && (x as usize) < fader_values.len() {
									if let Some((value, _)) = fader_values[x as usize].as_mut() {
										**value = self.fader_history[x as usize][0];
									}
								}
							}
							_ => {}
						}

						if time >= self.fader_history_last_update + 48000 / 40 {
							for (i, fader) in fader_values.iter_mut().enumerate() {
								if let Some((value, _)) = fader {
									self.fader_history[i][0] = self.fader_history[i][1];
									self.fader_history[i][1] = **value;
								}
							}
							self.fader_history_last_update = time;
						}
					}
				}
			}
		}
	}

	pub fn draw(
		&mut self,
		pattern: &ArpeggioData,
		step: f32,
		use_external_clock: bool,
		external_clock_present: bool,
		clock_mode: ClockMode,
		chord_hold: bool,
		fader_values: &[Option<(f32, std::ops::RangeInclusive<f32>)>],
		time: u64,
		mut set_led: impl FnMut((u8, u8), LightingMode)
	) {
		use GuiState::*;
		use LightingMode::*;

		if self.target_first_x != self.first_x && time >= self.last_scroll_update + 1024 {
			self.first_x += (self.target_first_x - self.first_x).signum();
			self.last_scroll_update = time;
		}

		let mut array = [[None; 8]; 8];
		set_led(
			(8, 7),
			if chord_hold {
				Solid(Color::Color(215, 0.7))
			}
			else {
				Solid(Color::Color(300, 0.1))
			}
		);
		match self.state {
			Edit => {
				set_led((8, 0), Off);
				set_led((8, 1), Off);

				let mut octave_buttons = [Off; 4];
				if let Some(held) = self.currently_held_key {
					for entry in pattern.filter(held.pos, held.note) {
						assert!(entry.transpose % 12 == 0);
						let octave = entry.transpose / 12;
						assert!((-1..=2).contains(&octave));
						octave_buttons[(octave + 1) as usize] = Solid(octave_color(octave));
					}
				}
				else {
					octave_buttons[(self.current_octave + 1) as usize] =
						Solid(octave_color(self.current_octave));
				}
				for i in 0..4 {
					set_led((i + 4, 8), octave_buttons[i as usize]);
				}

				let n_panes = 8 / self.pane_height;
				for pane in 0..n_panes {
					draw_into(
						&mut array,
						(0, self.pane_height * (n_panes - pane - 1)),
						(8, self.pane_height),
						(self.first_x + 8 * pane as isize, self.first_y),
						&pattern,
						step
					);
				}
			}
			Config => {
				set_led((8, 0), Fade(Color::Color(0, 0.74)));
				set_led((8, 1), Off);

				array[7][2] = Some(match (use_external_clock, external_clock_present) {
					(true, true) => Alternate(Color::Color(150, 0.7), Color::White(1.0)),
					(true, false) => Solid(Color::Color(175, 0.0)),
					(false, _) => Alternate(Color::Color(30, 0.7), Color::White(1.0))
				});
				array[7][1] = Some(Solid(match clock_mode {
					ClockMode::Internal => Color::Color(30, 0.7),
					ClockMode::External => Color::Color(150, 0.7),
					ClockMode::Auto => Color::White(0.7)
				}));

				// display the pattern length
				let pattern_len = pattern.pattern.len();
				for y in 4..8 {
					for x in 0..8 {
						let curr_pos = x + (8 - y - 1) * 8 + 1;
						array[x][y] = if curr_pos < pattern_len {
							Some(Solid(Color::Color(0, 0.7)))
						}
						else if curr_pos == pattern_len {
							Some(Solid(Color::White(1.0)))
						}
						else {
							Some(Solid(Color::Color(30, 0.1)))
						}
					}
				}

				// display the number of panes
				let n_panes = 8 / self.pane_height;
				for i in 0..4 {
					if i + 1 == n_panes {
						array[0][i] = Some(Solid(Color::White(1.0)));
					}
					else {
						array[0][i] = Some(Solid(Color::Color(240, 0.2)));
					}
				}

				// repeat mode
				for i in 0..3 {
					array[2][i] = Some(Solid(Color::White(0.3)));
				}
				match pattern.repeat_mode {
					RepeatMode::Clamp => {
						array[2][0] = Some(Solid(Color::Color(60, 1.0)));
					}
					RepeatMode::Mirror => {
						array[2][1] = Some(Solid(Color::Color(180, 1.0)));
					}
					RepeatMode::Repeat(transpose) => {
						array[2][2] = Some(Solid(Color::Color(300, 1.0)));
						for i in 0..4 {
							array[3][i] = if transpose == (i as i32 - 1) * 12 {
								Some(Solid(Color::White(1.0)))
							}
							else {
								Some(Solid(Color::Color(300, 0.1)))
							}
						}
					}
				}
			}
			Sliders => {
				set_led((8, 0), Off);
				set_led((8, 1), Fade(Color::Color(0, 0.74)));

				for (x, fader) in fader_values.iter().enumerate() {
					if let Some((value, range)) = fader {
						let leds = 1
							+ (7.0 * (value - range.start()) / (range.end() - range.start()))
								as usize;
						for y in 0..leds {
							array[x][y] = Some(Solid(Color::Color(x as u16 * 107, 0.7)));
						}
					}
				}
			}
		}

		for x in 0..8 {
			for y in 0..8 {
				set_led((x, y), array[x as usize][y as usize].unwrap_or(Off));
			}
		}
	}
}

fn draw_into(
	array: &mut [[Option<LightingMode>; 8]; 8],
	canvas_offset: (usize, usize),
	canvas_size: (usize, usize),
	pattern_offset: (isize, isize),
	pattern: &ArpeggioData,
	step: f32
) {
	use LightingMode::*;
	// draw notes
	for pos in 0..pattern.pattern.len() {
		for e in pattern.pattern[pos as usize].iter() {
			let y = e.note - pattern_offset.1;
			if (0..canvas_size.1 as isize).contains(&y) {
				for i in 0..e.len_steps {
					let xx = pos as isize - pattern_offset.0 + i as isize;
					if 0 <= xx && xx < canvas_size.0 as isize {
						let xx = xx as usize;
						let foo = &mut array[xx + canvas_offset.0][y as usize + canvas_offset.1];
						if foo.is_some() {
							*foo = Some(Solid(Color::White(1.0)));
						}
						else {
							assert!(e.transpose % 12 == 0);
							let octave = e.transpose / 12;
							assert!((-1..=2).contains(&octave));
							let hue = octave_hue(octave) + (30.0 * e.intensity) as u16;
							let color = if i == 0 {
								Color::Color(hue, 0.25 + 0.75 * e.intensity)
							}
							else {
								Color::Color(hue, 0.1)
							};
							*foo = Some(Solid(color));
						}
					}
				}
			}
		}
	}

	// draw invalid area
	for x in 0..canvas_size.0 {
		let pos = x as isize + pattern_offset.0;
		if pos < 0 || pos >= pattern.pattern.len() as isize {
			for y in 0..canvas_size.1 {
				array[x + canvas_offset.0][y + canvas_offset.1] = Some(Solid(Color::Color(0, 0.3)));
			}
		}
	}

	// draw horizontal zero indicator
	let hl_y = -pattern_offset.1;
	if (0..canvas_size.1 as isize).contains(&hl_y) {
		for x in 0..canvas_size.0 {
			array[x + canvas_offset.0][hl_y as usize + canvas_offset.1]
				.get_or_insert(Solid(Color::White(0.3)));
		}
	}

	// draw vertical step indicator
	let hl_x = step as isize - pattern_offset.0;
	if (0..canvas_size.0 as isize).contains(&hl_x) {
		for y in 0..canvas_size.1 {
			let foo = &mut array[hl_x as usize + canvas_offset.0][y + canvas_offset.1];
			*foo = Some(foo.unwrap_or(Off).bright());
		}
	}
}
