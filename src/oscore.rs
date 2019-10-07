//! Protection and unprotection of OSCORE messages.

use alloc::vec::Vec;
use alt_stm32f30x_hal::{device::USART1, serial::Tx};
use coap_lite::{CoapOption, Packet};
use core::fmt::Write;
use oscore::oscore::SecurityContext;

use crate::{coap::CoapHandler, edhoc::EdhocHandler, uprint, uprintln};

/// Unprotects and protects OSCORE message and invokes `CoapHandler`.
pub struct OscoreHandler {
    edhoc: EdhocHandler,
    coap: CoapHandler,
    oscore: Option<SecurityContext>,
    sender_id: Option<Vec<u8>>,
    recipient_id: Option<Vec<u8>>,
}

impl OscoreHandler {
    /// Creates a new `OscoreHandler`.
    pub fn new(
        edhoc: EdhocHandler,
        coap: CoapHandler,
        sender_id: Vec<u8>,
        recipient_id: Vec<u8>,
    ) -> OscoreHandler {
        OscoreHandler {
            edhoc,
            coap,
            oscore: None,
            sender_id: Some(sender_id),
            recipient_id: Some(recipient_id),
        }
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

        // Check if EDHOC has advanced
        if let Some((master_secret, master_salt)) = self.edhoc.take_params() {
            // Since EDHOC is done, we can initialize OSCORE
            self.oscore = Some(
                SecurityContext::new(
                    master_secret,
                    master_salt,
                    self.sender_id.take().unwrap(),
                    self.recipient_id.take().unwrap(),
                )
                .expect("Failed intializing OSCORE"),
            );
        }

        // Return the bytes of the CoAP response packet
        Some(res.to_bytes().expect("Error building CoAP bytes"))
    }
}
