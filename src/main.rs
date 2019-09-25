#![no_std]
#![no_main]

extern crate panic_itm;

use cortex_m::iprintln;
use cortex_m_rt::entry;

use oscore_demo::siprintln;

#[entry]
fn main() -> ! {
    let (mut itm, mut leds, mut delay) = oscore_demo::init();
    let stim = &mut itm.stim[0];

    siprintln!(stim, "Initialized and ready.");
    oscore_demo::spin_leds(&mut delay, &mut leds);

    #[allow(clippy::empty_loop)]
    loop {}
}
