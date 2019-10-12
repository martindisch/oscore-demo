//! On-board user LEDs.

// We need this because stm32f4xx_hal uses the deprecated traits
#![allow(deprecated)]

use hal::gpio::gpiod::{PD, PD12, PD13, PD14, PD15};
use hal::gpio::{Output, PushPull};
use hal::prelude::*;
use stm32f4xx_hal as hal;

/// Top LED (orange).
pub type LD3 = PD12<Output<PushPull>>;

/// Left LED (green).
pub type LD4 = PD13<Output<PushPull>>;

/// Right LED (red).
pub type LD5 = PD15<Output<PushPull>>;

/// Bottom LED (blue).
pub type LD6 = PD14<Output<PushPull>>;

/// User LED colors.
pub enum LedColor {
    /// Green LED / LD4.
    Green,
    /// Orange LED / LD3.
    Orange,
    /// Red LED / LD5.
    Red,
    /// Blue LED / LD6.
    Blue,
}

/// Array of the on-board user LEDs.
pub struct Leds {
    curr: usize,
    leds: [Led; 4],
}

impl Leds {
    /// Initializes all the user LEDs.
    pub fn new(
        pd12: PD12<Output<PushPull>>,
        pd13: PD13<Output<PushPull>>,
        pd14: PD14<Output<PushPull>>,
        pd15: PD15<Output<PushPull>>,
    ) -> Self {
        let top = pd12;
        let left = pd13;
        let right = pd14;
        let bottom = pd15;

        Leds {
            curr: 0,
            leds: [top.into(), left.into(), right.into(), bottom.into()],
        }
    }

    /// Turns the current LED off and the next one on.
    pub fn spin(&mut self) {
        let next = (self.curr + 1) % self.leds.len();

        self.leds[next].on();
        self.leds[self.curr].off();

        self.curr = next;
    }
}

impl core::ops::Deref for Leds {
    type Target = [Led];

    fn deref(&self) -> &[Led] {
        &self.leds
    }
}

impl core::ops::DerefMut for Leds {
    fn deref_mut(&mut self) -> &mut [Led] {
        &mut self.leds
    }
}

impl core::ops::Index<usize> for Leds {
    type Output = Led;

    fn index(&self, i: usize) -> &Led {
        &self.leds[i]
    }
}

impl core::ops::Index<LedColor> for Leds {
    type Output = Led;

    fn index(&self, c: LedColor) -> &Led {
        &self.leds[c as usize]
    }
}

impl core::ops::IndexMut<usize> for Leds {
    fn index_mut(&mut self, i: usize) -> &mut Led {
        &mut self.leds[i]
    }
}

impl core::ops::IndexMut<LedColor> for Leds {
    fn index_mut(&mut self, c: LedColor) -> &mut Led {
        &mut self.leds[c as usize]
    }
}

/// One of the on-board user LEDs.
pub struct Led {
    pin: PD<Output<PushPull>>,
}

macro_rules! ctor {
	($($ldx:ident),+) => {
		$(
			impl Into<Led> for $ldx {
				fn into(self) -> Led {
					Led {
						pin: self.downgrade(),
					}
				}
			}
		)+
	}
}

ctor!(LD3, LD4, LD5, LD6);

impl Led {
    /// Turns the LED off.
    pub fn off(&mut self) {
        self.pin.set_low();
    }

    /// Turns the LED on.
    pub fn on(&mut self) {
        self.pin.set_high();
    }

    /// Toggles the LED.
    pub fn toggle(&mut self) {
        if self.pin.is_low() {
            self.pin.set_high();
        } else {
            self.pin.set_low();
        }
    }
}
