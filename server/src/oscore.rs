//! Protection and unprotection of OSCORE messages.

use alloc::vec::Vec;
use alt_stm32f30x_hal::{device::USART1, serial::Tx};
use coap_lite::{CoapOption, Packet};
use core::fmt::Write;
use oscore::oscore::SecurityContext;
use util::{uprint, uprintln};

use crate::{coap::CoapHandler, edhoc::EdhocHandler};

/// Unprotects and protects OSCORE message and invokes `CoapHandler`.
pub struct OscoreHandler {
    edhoc: EdhocHandler,
    coap: CoapHandler,
    oscore: Option<SecurityContext>,
    sender_id: Vec<u8>,
    recipient_id: Vec<u8>,
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
            sender_id,
            recipient_id,
        }
    }

    /// Unprotects an OSCORE message if it is one, passes the CoAP to the
    /// `CoapHandler` and protects the response if necessary.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        req_bytes: &[u8],
    ) -> Option<Vec<u8>> {
        let mut req =
            Packet::from_bytes(req_bytes).expect("Unable to parse CoAP");
        let mut is_oscore = false;

        // Check if the request is OSCORE and we are ready to deal with it
        if req.get_option(CoapOption::Oscore).is_some()
            && self.oscore.is_some()
        {
            uprintln!(tx, "Unprotecting OSCORE request");
            is_oscore = true;
            // Temporarily take the oscore context
            let mut oscore = self.oscore.take().unwrap();
            // Unprotect the request and replace the original with it
            req = Packet::from_bytes(
                &oscore
                    .unprotect_request(req_bytes)
                    .expect("Failed unprotecting request"),
            )
            .expect("Unable to parse unprotected request");
            // Put the context back in
            self.oscore = Some(oscore);
        }

        // Use CoAP handler to deal with it
        let mut res = self
            .coap
            .handle(tx, &mut self.edhoc, req)?
            .to_bytes()
            .expect("Error building CoAP bytes");

        // Check if EDHOC has advanced
        if let Some((master_secret, master_salt)) = self.edhoc.take_params() {
            // Since EDHOC is done, we can initialize OSCORE
            self.oscore = Some(
                SecurityContext::new(
                    master_secret,
                    master_salt,
                    self.sender_id.clone(),
                    self.recipient_id.clone(),
                )
                .expect("Failed intializing OSCORE"),
            );
        }

        // If the exchange is protected with OSCORE, protect the response
        if is_oscore {
            uprintln!(tx, "Protecting OSCORE response");
            // Temporarily take the oscore context
            let mut oscore = self.oscore.take().unwrap();
            // Protect the response and replace the original with it
            res = oscore
                .protect_response(&res, req_bytes, true)
                .expect("Failed protecting response");
            // Put the context back in
            self.oscore = Some(oscore);
        }

        // Return the bytes of the CoAP response packet
        Some(res)
    }
}
