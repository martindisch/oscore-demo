//! Functionality for sending arbitrary data over serial.

use core::fmt::Debug;
use embedded_hal::serial::{Read, Write};
use nb::block;

/// Sends the bytes as a packet over serial.
pub fn send<W, E>(tx: &mut W, bytes: &[u8])
where
    W: Write<u8, Error = E>,
    E: Debug,
{
    // Send the data length as two bytes
    let n = (bytes.len() as u16).to_be_bytes();
    send_n(tx, &n);
    // Send the data
    send_n(tx, bytes);
}

/// Receives the next packet of bytes over serial into the buffer.
pub fn receive<R, E>(rx: &mut R, buf: &mut [u8]) -> u16
where
    R: Read<u8, Error = E>,
    E: Debug,
{
    // Receive the data length
    let mut n = [0; 2];
    if let Err(e) = receive_n(2, rx, &mut n) {
        panic!("Failed receiving the length: {:?}", e);
    }
    let n = u16::from_be_bytes(n);

    // Receive the data
    if let Err(e) = receive_n(n, rx, buf) {
        panic!("Failed receiving data: {:?}", e);
    }

    n
}

/// Sends multiple bytes.
fn send_n<W, E>(tx: &mut W, bytes: &[u8])
where
    W: Write<u8, Error = E>,
    E: Debug,
{
    // Send the data
    for b in bytes {
        // Unwrap is safe here, according to the HAL documentation
        block!(tx.write(*b)).unwrap();
    }
}

/// Tries to receive `n` bytes from serial into the buffer.
fn receive_n<R, E>(n: u16, rx: &mut R, buf: &mut [u8]) -> Result<(), E>
where
    R: Read<u8, Error = E>,
    E: Debug,
{
    for i in 0..n {
        buf[i as usize] = block!(rx.read())?;
    }
    Ok(())
}
