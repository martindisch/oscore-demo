#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate panic_semihosting;

use alloc_cortex_m::CortexMHeap;
use alt_stm32f30x_hal::{pac, prelude::*};
use core::fmt::Write;
use cortex_m::{
    asm,
    peripheral::{Peripherals, DWT},
};
use cortex_m_rt::entry;
use util::{uprint, uprintln};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[entry]
fn main() -> ! {
    // Initialize the allocator BEFORE you use it
    let start = cortex_m_rt::heap_start() as usize;
    let size = 10 * 1024 as usize;
    unsafe { ALLOCATOR.init(start, size) }

    let dp = pac::Peripherals::take().expect("Failed taking dp");
    let mut rcc = dp.RCC.constrain();
    let mut flash = dp.FLASH.constrain();
    // This is how we would set a faster clock (can get even faster with PLL)
    // let clocks = rcc.cfgr.sysclk(36.mhz()).freeze(&mut flash.acr);
    let clocks = rcc.cfgr.freeze(&mut flash.acr);
    let gpiob = dp.GPIOB.split(&mut rcc.ahb);

    let mut peripherals = Peripherals::take().unwrap();
    peripherals.DWT.enable_cycle_counter();

    // USART1
    let serial =
        dp.USART1
            .serial((gpiob.pb6, gpiob.pb7), 115_200.bps(), clocks);
    let (mut tx, mut _rx) = serial.split();

    uprintln!(tx, "Basic initialization done");

    bench(
        &mut tx,
        10,
        || 200_000,
        |v| {
            for _ in 0..v {
                asm::nop();
            }
        },
    );

    #[allow(clippy::empty_loop)]
    loop {}
}

/// Runs the given closure `iterations` times and prints CPU cycles.
///
/// The preparation closure is called before every iteration and its return
/// type passed into the closure that is measured.
fn bench<W, P, R, F>(tx: &mut W, iterations: u32, preparation: P, to_bench: F)
where
    W: Write,
    P: Fn() -> R,
    F: Fn(R),
{
    for i in 0..iterations {
        let data = preparation();
        let start = DWT::get_cycle_count();
        to_bench(data);
        let duration = DWT::get_cycle_count() - start;
        uprintln!(tx, "Iteration {} took {} CPU cycles", i, duration);
    }
}

#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    panic!("We're officially OOM");
}
