#![no_std]

use f3::{
    hal::{delay::Delay, prelude::*},
    led::Leds,
};

mod macros;

/// Lights the LEDs in the circle one by one.
pub fn spin_leds(delay: &mut Delay, leds: &mut Leds) {
    let n = leds.len();
    loop {
        for curr in 0..n {
            let next = (curr + 1) % n;
            leds[curr].off();
            leds[next].on();

            delay.delay_ms(100_u8);
        }
    }
}
