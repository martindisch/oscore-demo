#![no_std]
#![no_main]

extern crate panic_itm;

use cortex_m::{iprintln, Peripherals};
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    let mut p = Peripherals::take().unwrap();
    let stim = &mut p.ITM.stim[0];

    iprintln!(stim, "Hello, world!");
    panic!("Oh, snap!");

    loop {}
}
