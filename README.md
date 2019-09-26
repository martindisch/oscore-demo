# [WIP] oscore-demo

This will hopefully turn into a demonstration of protecting CoAP with OSCORE on
embedded devices some day.
At the moment, this is built specifically for the STM32F303VCT6.

## Dependencies

To build this you'll need (in your nightly toolchain):

- `rust-std` components (pre-compiled `core` crate) for the ARM Cortex-M
  target. Run:

```
$ rustup target add thumbv7em-none-eabihf
```

## Hardware setup
This is the wiring:
```
ENC28J60  STM32F303
--------  ---------
VCC       3V
GND       GND
MISO      PA6
MOSI      PA7
SCK       PA5
CS        PA4
RST       PA3
```

## Building
Use OpenOCD to connect to the board
```
$ cd /tmp && openocd -f interface/stlink-v2-1.cfg -f target/stm32f3x.cfg
```
In a different terminal, create the ITM file and start reading it
```
$ cd /tmp && touch itm.txt && itmdump -F -f itm.txt
```
In yet another terminal, build and enter GDB. We're using the release flag
since the debug build might be too large for the flash memory.
```
$ cargo run --release
```
This will break at the main function, so you need to
```
(gdb) continue
```

## License
Licensed under either of

 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT license](LICENSE-MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
