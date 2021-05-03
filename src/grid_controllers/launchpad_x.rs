use super::*;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum LaunchpadInternalColorspec {
	Solid(u8),
	Alternate(u8,u8),
	Fade(u8)
}

pub struct LaunchpadX {
	state: [[LaunchpadInternalColorspec; 9]; 9]
}

fn id2coord(id: u8) -> Option<(u8, u8)> {
	let x = id % 10;
	let y = id / 10;

	if (1..=9).contains(&x) && (1..=9).contains(&y) {
		Some((x-1, y-1))
	}
	else {
		None
	}
}

impl LaunchpadX {
	pub fn new() -> LaunchpadX {
		LaunchpadX {
			state: [[LaunchpadInternalColorspec::Solid(0); 9]; 9]
		}
	}
	
	fn send(&self, pos: (u8,u8), send: &mut impl FnMut(&[u8])) {
		let note = (pos.0 + 1) + 10 * (pos.1 + 1);
		match self.state[pos.0 as usize][pos.1 as usize] {
			LaunchpadInternalColorspec::Solid(c) => {
				send(&[0x90, note, c]);
			}
			LaunchpadInternalColorspec::Alternate(c1, c2) => {
				send(&[0x90, note, c1]);
				send(&[0x91, note, c2]);
			}
			LaunchpadInternalColorspec::Fade(c) => {
				send(&[0x92, note, c]);
			}
		}
	}
}

impl GridController for LaunchpadX {
	fn force_update(&self, mut send: impl FnMut(&[u8])) {
		for i in 0..8 {
			for j in 0..8 {
				self.send((i,j), &mut send);
			}
		}
	}

	fn handle_midi(&mut self, message: &[u8], mut f: impl FnMut(&mut Self, GridButtonEvent)) {
		use GridButtonEvent::*;
		if message.len() == 3 {
			if (message[0] == 0x90 || message[0] == 0xB0) && message[2] != 0 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Down(x, y, message[2] as f32 / 127.0));
				}
			}
			if (message[0] == 0x90 || message[0] == 0xB0) && message[2] == 0 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Up(x, y, 64.0));
				}
			}
			if message[0] == 0x80 {
				if let Some((x,y)) = id2coord(message[1]) {
					f(self, Up(x, y, message[2] as f32 / 127.0));
				}
			}
		}
	}

	fn set(&mut self, pos: (u8,u8), colorspec: LightingMode, mut send: impl FnMut(&[u8])) {
		fn color(c: Color) -> u8 {
			#[rustfmt::skip]
			let offsets = [
				0x04, 0x04, 0x04, 0x08, 0x08, 0x08,
				0x0c, 0x0c, 0x0c, 0x10, 0x10, 0x10,
				0x14, 0x14, 0x18, 0x18, 0x1c, 0x1c,
				0x20, 0x20, 0x24, 0x24, 0x28, 0x28,
				0x2c, 0x2c, 0x2c, 0x30, 0x30, 0x30,
				0x34, 0x34, 0x34, 0x38, 0x38, 0x38
			];
			use self::Color::*;
			match c {
				White(i) => ((i*4.0) as u8).clamp(0,3),
				Color(hue, i) => offsets[(hue as usize % 360) / 10] + 3 - ((i*4.0) as u8).clamp(0,3)
			}
		}

		assert!( (0..9).contains(&pos.0) );
		assert!( (0..9).contains(&pos.1) );
		use LightingMode::*;
		let new_spec = match colorspec {
			Off => LaunchpadInternalColorspec::Solid(0),
			Solid(c) => LaunchpadInternalColorspec::Solid(color(c)),
			Blink(c) => LaunchpadInternalColorspec::Alternate(0,color(c)),
			Fade(c) => LaunchpadInternalColorspec::Fade(color(c)),
			Alternate(c1,c2) => LaunchpadInternalColorspec::Alternate(color(c1),color(c2))
		};

		let field = &mut self.state[pos.0 as usize][pos.1 as usize];
		if *field != new_spec {
			*field = new_spec;
			self.send(pos, &mut send);
		}
	}
}
