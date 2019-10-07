//! Protection and unprotection of OSCORE messages.

use alloc::vec::Vec;
use alt_stm32f30x_hal::{device::USART1, serial::Tx};
use coap_lite::{CoapOption, Packet};
use core::fmt::Write;

use crate::{coap::CoapHandler, edhoc::EdhocHandler, uprint, uprintln};

/// Unprotects and protects OSCORE message and invokes `CoapHandler`.
pub struct OscoreHandler {
    edhoc: EdhocHandler,
    coap: CoapHandler,
}

impl OscoreHandler {
    /// Creates a new `OscoreHandler`.
    pub fn new(edhoc: EdhocHandler, coap: CoapHandler) -> OscoreHandler {
        OscoreHandler { edhoc, coap }
    }

    /// Unprotects an OSCORE message if it is one, passes the CoAP to the
    /// `CoapHandler` and protects the response if necessary.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        req_bytes: &[u8],
    ) -> Option<Vec<u8>> {
        let req = Packet::from_bytes(req_bytes).expect("Unable to parse CoAP");
        // Use CoAP handler to deal with it
        let res = self.coap.handle(tx, &mut self.edhoc, req)?;

        Some(res.to_bytes().expect("Error building CoAP bytes"))
    }
}
