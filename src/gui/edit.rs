use crate::grid_controllers::{GridButtonEvent, LightingMode, Color};
use crate::arpeggiator::{ArpeggioData, Entry};

#[derive(Copy, Clone)]
struct HeldKey {
	coords: (u8, u8),
	pos: usize,
	note: isize,
	time: u64,
	just_set: bool
}

pub struct EditScreen {
	pub(super) pane_height: usize,
	first_x: isize,
	target_first_x: isize,
	last_scroll_update: u64,
	first_y: isize,
	currently_held_key: Option<HeldKey>,
	current_octave: i32,
}

impl EditScreen {
	pub fn new() -> EditScreen {
		EditScreen {
			pane_height: 4,
			first_x: 0,
			target_first_x: 0,
			last_scroll_update: 0,
			first_y: 0,
			currently_held_key: None,
			current_octave: 0,
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
		time: u64
	) {
		use GridButtonEvent::*;

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

	pub fn draw(
		&mut self,
		array: &mut [[Option<LightingMode>; 9]; 8],
		pattern: &ArpeggioData,
		step: f32,
		time: u64,
	) {
		use LightingMode::*;
		
		if self.target_first_x != self.first_x && time >= self.last_scroll_update + 1024 {
			self.first_x += (self.target_first_x - self.first_x).signum();
			self.last_scroll_update = time;
		}

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
			array[i+4][8] = Some(octave_buttons[i as usize]);
		}

		let n_panes = 8 / self.pane_height;
		for pane in 0..n_panes {
			draw_into(
				array,
				(0, self.pane_height * (n_panes - pane - 1)),
				(8, self.pane_height),
				(self.first_x + 8 * pane as isize, self.first_y),
				&pattern,
				step
			);
		}
	}
}

fn octave_hue(octave: i32) -> u16 { (octave + 1) as u16 * 90 }
fn octave_color(octave: i32) -> Color { Color::Color(octave_hue(octave), 1.0) }

fn draw_into(
	array: &mut [[Option<LightingMode>; 9]; 8],
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
