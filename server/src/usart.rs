//! Functionality for sending arbitrary data over USART2.

use alt_stm32f30x_hal::{
    device::USART2,
    serial::{Error, Rx, Tx},
};
use embedded_hal::serial::{Read, Write};
use nb::block;

/// Sends the bytes over USART.
pub fn send(tx2: &mut Tx<USART2>, bytes: &[u8]) {
    // Send the data length as two bytes
    let n = (bytes.len() as u16).to_be_bytes();
    send_n(tx2, &n);
    // Send the data
    send_n(tx2, bytes);
}

/// Receives the next packet of bytes over USART into the buffer.
pub fn receive(rx2: &mut Rx<USART2>, buf: &mut [u8]) -> u16 {
    // Receive the data length
    let mut n = [0; 2];
    if let Err(e) = receive_n(2, rx2, &mut n) {
        panic!("Failed receiving the length: {:?}", e);
    }
    let n = u16::from_be_bytes(n);

    // Receive the data
    if let Err(e) = receive_n(n, rx2, buf) {
        panic!("Failed receiving data: {:?}", e);
    }

    n
}

/// Sends multiple bytes.
fn send_n(tx2: &mut Tx<USART2>, bytes: &[u8]) {
    // Send the data
    for b in bytes {
        // Unwrap is safe here, according to the HAL documentation
        block!(tx2.write(*b)).unwrap();
    }
}

/// Tries to receive `n` bytes from USART into the buffer.
fn receive_n(
    n: u16,
    rx2: &mut Rx<USART2>,
    buf: &mut [u8],
) -> Result<(), Error> {
    for i in 0..n {
        buf[i as usize] = block!(rx2.read())?;
    }
    Ok(())
}
