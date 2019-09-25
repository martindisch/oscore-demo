#![no_std]
#![no_main]

extern crate panic_itm;

use cortex_m::iprintln;
use cortex_m_rt::entry;
use f3::{
    hal::{delay::Delay, prelude::*, stm32f30x},
    led::Leds,
};

use oscore_demo::siprintln;

#[entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let dp = stm32f30x::Peripherals::take().unwrap();

    let stim = &mut cp.ITM.stim[0];

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();
    let gpioe = dp.GPIOE.split(&mut rcc.ahb);

    // clock configuration using the default settings (all clocks run at 8 MHz)
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    let mut leds = Leds::new(gpioe);
    let mut delay = Delay::new(cp.SYST, clocks);

    siprintln!(stim, "Initialized and ready.");

    oscore_demo::spin_leds(&mut delay, &mut leds);

    #[allow(clippy::empty_loop)]
    loop {}
}
