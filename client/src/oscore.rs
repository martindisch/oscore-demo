//! Protection and unprotection of OSCORE messages.

use alloc::vec::Vec;
use coap_lite::{CoapOption, Packet};
use core::fmt::Write;
use oscore::oscore::SecurityContext;
use stm32f4xx_hal::{serial::Tx, stm32::USART1};

use crate::{coap::CoapHandler, edhoc::EdhocHandler, uprint, uprintln};

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

    /// Initiates everything by creating the first EDHOC message.
    pub fn go(&mut self, tx: &mut Tx<USART1>) -> Option<Vec<u8>> {
        let req = self.coap.go(tx, &mut self.edhoc);
        Some(req?.to_bytes().expect("Error building CoAP bytes"))
    }

    /// Unprotects an OSCORE message if it is one, passes the CoAP to the
    /// `CoapHandler` and protects the request if necessary.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        res_bytes: &[u8],
    ) -> Option<Vec<u8>> {
        let mut res =
            Packet::from_bytes(res_bytes).expect("Unable to parse CoAP");
        let mut is_oscore = false;

        // Check if the response is OSCORE and we are ready to deal with it
        if res.get_option(CoapOption::Oscore).is_some()
            && self.oscore.is_some()
        {
            uprintln!(tx, "Unprotecting OSCORE response");
            is_oscore = true;
            // Temporarily take the oscore context
            let mut oscore = self.oscore.take().unwrap();
            // Unprotect the response and replace the original with it
            res = Packet::from_bytes(
                &oscore
                    .unprotect_response(res_bytes)
                    .expect("Failed unprotecting response"),
            )
            .expect("Unable to parse unprotected response");
            // Put the context back in
            self.oscore = Some(oscore);
        }

        // Use CoAP handler to deal with it
        let mut req = self.coap.handle(tx, &mut self.edhoc, res);

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
            is_oscore = true;
            // Since EDHOC has just completed, our response is currently None.
            // What we want to do now, is to send the first OSCORE request to
            // get the whole receive -> send -> repeat cycle going.
            req = Some(self.coap.build_resource_request());
        }

        let mut req = req?.to_bytes().expect("Error building CoAP bytes");

        // If the exchange is protected with OSCORE, protect the request
        if is_oscore {
            uprintln!(tx, "Protecting OSCORE request");
            // Temporarily take the oscore context
            let mut oscore = self.oscore.take().unwrap();
            // Protect the request and replace the original with it
            req = oscore
                .protect_request(&req)
                .expect("Failed protecting request");
            // Put the context back in
            self.oscore = Some(oscore);
        }

        // Return the bytes of the CoAP request packet
        Some(req)
    }
}
