use crate::grid_controllers::GridButtonEvent::Down;
use crate::grid_controllers::*;
use crate::tempo_detector::TempoDetector;
use crate::arpeggiator::*;

enum GuiState {
	Edit,
	Config,
	Sliders,
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
	first_y: isize,
	currently_held_key: Option<HeldKey>,
	current_octave: i32,
	tempo: TempoDetector
}

fn octave_hue(octave: i32) -> u16 {
	(octave + 1) as u16 * 90
}
fn octave_color(octave: i32) -> Color {
	Color::Color(octave_hue(octave), 1.0)
}

impl GuiController {
	pub fn new() -> GuiController {
		GuiController {
			state: GuiState::Edit,
			pane_height: 4,
			first_x: 0,
			first_y: 0,
			currently_held_key: None,
			current_octave: 0,
			tempo: TempoDetector::new()
		}
	}

	fn handle_grid_down(&mut self, (xx,yy): (u8, u8), velo: f32, pattern: &mut ArpeggioData, time: u64) {
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
			let mode =
				if let Some(held) = self.currently_held_key {
					if held.note == y && held.pos < x as usize {
						PressMode::SetLength
					}
					else {
						PressMode::SecondaryUnrelated
					}
				}
				else {
					self.currently_held_key = Some(HeldKey { coords: (xx,yy), pos: x as usize, note: y, time, just_set: !step_has_any_note });
					PressMode::Primary
				};

			if !step_has_any_note {
				match mode {
					PressMode::Primary | PressMode::SecondaryUnrelated => {
						pattern.set(x as usize, Entry {
							note: y,
							len_steps: 1,
							intensity: velo,
							transpose: 12 * self.current_octave
						}).ok();
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

	fn handle_grid_up(&mut self, (xx,yy): (u8,u8), pattern: &mut ArpeggioData, time: u64) {
		let n_panes = 8 / self.pane_height;
		let pane = yy as usize / self.pane_height;
		let x = xx as isize + self.first_x + 8 * (n_panes - pane - 1) as isize;
		let y = (yy as isize % self.pane_height as isize) + self.first_y;

		let mut dont_delete = true;
		if let Some(held) = self.currently_held_key {
			if held.coords == (xx,yy) {
				dont_delete = time - held.time >= 10000 || held.just_set;
				self.currently_held_key = None;
			}
		}
		if !dont_delete && x >= 0 && x < pattern.pattern.len() as isize {
			pattern.delete_all(x as usize, y);
		}
	}

	pub fn handle_input(&mut self, event: GridButtonEvent, pattern: &mut ArpeggioData, use_external_clock: bool, clock_mode: &mut ClockMode, time_between_midiclocks: &mut u64, time: u64) {
		use GridButtonEvent::*;
		use GuiState::*;

		println!("Handle input: {:?}", event);

		match self.state {
			Edit => {
				match event {
					Down(8, 0, _) => {
						self.state = Config;
					},
					Down(8, 1, _) => {
						self.state = Sliders;
					},
					Down(0, 8, _) => {
						self.first_y += 1;
					},
					Down(1, 8, _) => {
						self.first_y -= 1;
					},
					Down(2, 8, _) => {
						self.first_x -= 1;
					},
					Down(3, 8, _) => {
						self.first_x += 1;
					},
					Down(x, 8, _) => {
						let octave = x as i32 - 5;
						if let Some(held) = self.currently_held_key {
							if pattern.filter(held.pos, held.note).count() > 0 { // the step can become un-set by disabling all octaves, yet it is still held down
								let entry_opt = pattern.filter(held.pos, held.note).find(|e| e.transpose == octave * 12).cloned();
								if let Some(entry) = entry_opt {
									let delete_entry = entry.clone();
									pattern.delete(held.pos, delete_entry);
								}
								else {
									let mut new_entry = pattern.filter(held.pos, held.note).next().unwrap().clone();
									new_entry.transpose = octave * 12;
									pattern.set(held.pos, new_entry);
								}
							}
						}
						else {
							self.current_octave = octave;
						}
					},
					Down(xx, yy, velo) => {
						if xx <= 8 && yy <= 8 {
							self.handle_grid_down((xx,yy), velo, pattern, time);
						}
					},
					Up(xx, yy, _) => {
						if xx < 8 && yy < 8 {
							self.handle_grid_up((xx,yy), pattern, time);
						}
					}
				}
			}
			Config => {
				match event {
					Down(8, 0, _) => {
						self.state = Edit;
					},
					Down(8, 1, _) => {
						self.state = Sliders;
					},
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
							if self.tempo.time_per_beat() <= 48000*2 && self.tempo.time_per_beat() >= 10 {
								*time_between_midiclocks = self.tempo.time_per_beat() as u64 / 24;
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
					Down(x, y, _) => {
						if y >= 4 {
							let new_len = x + 8 * (8 - y - 1) + 1;
							pattern.pattern.resize_default(new_len as usize).ok();
						}
						else {
							if x == 0 {
								self.pane_height = 8 / (y+1) as usize;
							}
							if x == 3 {
								match pattern.repeat_mode {
									RepeatMode::Repeat(_) => { pattern.repeat_mode = RepeatMode::Repeat((y as i32 - 1) * 12); }
									_ => {}
								}
							}
						}
					}
					_ => {}
				}
			}
			Sliders => {
				match event {
					Down(8, 0, _) => {
						self.state = Config;
					},
					Down(8, 1, _) => {
						self.state = Edit;
					},
					_ => {}
				}
			}
		}
	}

	pub fn draw(&mut self, pattern: &ArpeggioData, step: f32, use_external_clock: bool, external_clock_present: bool, clock_mode: ClockMode, time_between_midiclocks: &mut u64, mut set_led: impl FnMut((u8,u8), LightingMode)) {
		use GuiState::*;
		use LightingMode::*;
		let mut array = [[None; 8]; 8];
		match self.state {
			Edit => {
				set_led((8,0), Off);
				set_led((8,1), Off);

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
					octave_buttons[(self.current_octave + 1) as usize] = Solid(octave_color(self.current_octave));
				}
				for i in 0..4 {
					set_led((i + 4,8), octave_buttons[i as usize]);
				}

				fn draw_into(array: &mut [[Option<LightingMode>; 8]; 8], canvas_offset: (usize, usize), canvas_size: (usize, usize), pattern_offset: (isize, isize), pattern: &ArpeggioData, step: f32) {
					// draw notes
					for x in 0..canvas_size.0 {
						let pos = x as isize + pattern_offset.0;
						if pos >= 0 && pos < pattern.pattern.len() as isize  {
							for e in pattern.pattern[pos as usize].iter() {
								let y = e.note - pattern_offset.1;
								if (0..canvas_size.1 as isize).contains(&y) {
									for i in 0..e.len_steps {
										let xx = x + i as usize;
										if xx < canvas_size.0 {
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
						else {
							for y in 0..canvas_size.1 {
								array[x + canvas_offset.0][y + canvas_offset.1] = Some(Solid(Color::Color(0,0.3)));
							}
						}
					}
				
					// draw horizontal zero indicator
					let hl_y = -pattern_offset.1;
					if (0..canvas_size.1 as isize).contains(&hl_y) {
						for x in 0..canvas_size.0 {
							array[x + canvas_offset.0][hl_y as usize+ canvas_offset.1].get_or_insert(Solid(Color::White(0.3)));
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

				let n_panes = 8 / self.pane_height;
				for pane in 0..n_panes {
					draw_into(&mut array, (0,self.pane_height * (n_panes - pane - 1)), (8,self.pane_height), (self.first_x + 8 * pane as isize, self.first_y), &pattern, step);
				}
			},
			Config => {
				set_led((8,0), Fade(Color::Color(0, 0.74)));
				set_led((8,1), Off);

				array[7][2] = Some(match (use_external_clock, external_clock_present) {
					(true, true) => Alternate(Color::Color(150, 0.7), Color::White(1.0)),
					(true, false) => Solid(Color::Color(175, 0.0)),
					(false, _) => Alternate(Color::Color(30, 0.7), Color::White(1.0))
				});
				array[7][1] = Some(Solid( match clock_mode {
					ClockMode::Internal => Color::Color(30, 0.7),
					ClockMode::External => Color::Color(150, 0.7),
					ClockMode::Auto => Color::White(0.7)
				}));

				// display the pattern length
				let pattern_len = pattern.pattern.len();
				for y in 4..8 {
					for x in 0..8 {
						let curr_pos = x + (8-y-1)*8 + 1;
						array[x][y] = if curr_pos < pattern_len {
							Some(Solid(Color::Color(0, 0.7)))
						}
						else if curr_pos == pattern_len {
							Some(Solid(Color::White(1.0)))
						}
						else {
							Some(Solid(Color::Color(30, 0.1)))
						}
					};
				}
				
				// display the number of panes
				let n_panes = 8 / self.pane_height;
				for i in 0..4 {
					if i+1 == n_panes {
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
								Some(Solid(Color::Color(300,0.1)))
							}
						}
					}
				}
			},
			Sliders => {
				set_led((8,0), Off);
				set_led((8,1), Fade(Color::Color(0, 0.74)));

			}
		}

		for x in 0..8 {
			for y in 0..8 {
				set_led((x,y), array[x as usize][y as usize].unwrap_or(Off));
			}
		}
	}
}

