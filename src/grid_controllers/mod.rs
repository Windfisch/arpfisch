pub mod launchpad_x;

/// Key up or down events. The two `u8`s indicate the position (`(0,0)` is bottom left),
/// the `f32` indicates the velocity in the `0.0 .. 1.0` range. (`0.5` if unsupported)
#[derive(Clone,Copy,Debug)]
pub enum LaunchpadEvent {
	Down(u8, u8, f32),
	Up(u8, u8, f32),
}

/// Color of a grid field. Note that not all controllers support all colors.
/// It is not specified yet how drivers will treat unsupported colors; TODO.
#[derive(Clone,Copy,Debug)]
pub enum Color {
	/// Arguments: hue (0..360), intensity (0.0 .. 1.0, where 0.0 is the darkest color that is not black)
	/// "Intensity" is not necessarily lightness in the HSL sense.
	/// Drivers may quantize values.
	Color(u16, f32),

	/// Argument: intensity (0.0 .. 1.0, where 0.0 is black)
	White(f32)
}

impl Color {
	pub fn bright(&self) -> Color {
		use self::Color::*;
		match *self {
			Color(c, _) => Color(c, 1.0),
			White(_) => White(1.0)
		}
	}
}

/// A grid controller's color specification. Not all controllers support all [Color]s
/// or blink/fade patterns. Drivers will treat unsupported patterns like the closest
/// supported match.
#[derive(Clone,Copy,Debug)]
pub enum LaunchpadColorspec {
	Off,
	Solid(Color),
	Blink(Color),
	Fade(Color),
	Alternate(Color, Color)
}

impl LaunchpadColorspec {
	pub fn bright(&self) -> LaunchpadColorspec {
		use LaunchpadColorspec::*;
		match *self {
			Off => Solid(Color::White(0.3)),
			Solid(c) => Solid(c.bright()),
			Blink(c) => Blink(c.bright()),
			Fade(c) => Fade(c.bright()),
			Alternate(c1,c2) => Alternate(c1.bright(), c2.bright())
		}
	}
}

pub trait GridController {
	/// Handles a incoming MIDI message and calls `callback` for each event.
	///
	/// # Arguments
	/// * `callback` is a user-supplied callback that gets called whenever a [LaunchpadEvent] was received.
	fn handle_midi(&mut self, message: &[u8], callback: impl FnMut(&mut Self, LaunchpadEvent));

	/// Sets a single grid field at position `pos` to the specified [`LaunchpadColorspec`]
	///
	/// # Arguments
	/// * `send_fn` is a user-supplied callback that is expected to send its `&[u8]` argument as a single MIDI message to the GridController
	fn set(&mut self, pos: (u8,u8), colorspec: LaunchpadColorspec, send_fn: impl FnMut(&[u8]));
	
	/// Sets every field of the grid to the internally stored value. Call this after initializing or when the controller is in an unknown state.
	///
	/// # Arguments
	/// * `send_fn` is a user-supplied callback that is expected to send its `&[u8]` argument as a single MIDI message to the GridController
	fn force_update(&self, send_fn: impl FnMut(&[u8]));
}
