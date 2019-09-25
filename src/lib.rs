#![no_std]

use cortex_m::peripheral::ITM;
use f3::{
    hal::{delay::Delay, prelude::*, stm32f30x},
    led::Leds,
};

mod macros;

/// Returns the `ITM`, `Leds` and `Delay`.
pub fn init() -> (ITM, Leds, Delay) {
    let cp = cortex_m::Peripherals::take().unwrap();
    let dp = stm32f30x::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();
    let gpioe = dp.GPIOE.split(&mut rcc.ahb);

    // clock configuration using the default settings (all clocks run at 8 MHz)
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    let leds = Leds::new(gpioe);
    let delay = Delay::new(cp.SYST, clocks);

    (cp.ITM, leds, delay)
}

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
