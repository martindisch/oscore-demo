//! Handling of CoAP messages.

use alloc::vec::Vec;
use coap_lite::{
    CoapOption, ContentFormat, MessageClass, MessageType, Packet, RequestType,
};
use core::{fmt::Write, str};
use stm32f4xx_hal::{serial::Tx, stm32::USART1};

use crate::{
    edhoc::{EdhocHandler, State},
    uprint, uprintln,
};

/// Handles CoAP messages.
pub struct CoapHandler {
    message_id: u16,
    token: u8,
    oscore_iteration: usize,
}

impl Default for CoapHandler {
    fn default() -> CoapHandler {
        CoapHandler {
            message_id: 100,
            token: 0,
            oscore_iteration: 0,
        }
    }
}

impl CoapHandler {
    /// Creates a new `CoapHandler`.
    pub fn new() -> CoapHandler {
        Default::default()
    }

    /// Handles a CoAP response and returns the next request.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        edhoc: &mut EdhocHandler,
        res: Packet,
    ) -> Option<Packet> {
        match edhoc.get_state() {
            State::WaitingForSecond => None,
            State::WaitingForFourth => None,
            State::Complete => {
                // OSCORE is already going and we received a response
                uprintln!(
                    tx,
                    "Got response: {}",
                    str::from_utf8(&res.payload)
                        .expect("Failed parsing response payload as UTF-8")
                );
                // Build a CoAP request to one of the two resources
                let i = self.oscore_iteration;
                let coap = if i % 2 == 0 {
                    self.build_resource_request(b"hello".to_vec(), None)
                } else {
                    self.build_resource_request(
                        b"echo".to_vec(),
                        Some(format!("Iteration {}", i).as_bytes().to_vec()),
                    )
                };
                self.oscore_iteration += 1;

                Some(coap)
            }
        }
    }

    /// Returns a CoAP packet for a GET request to the given resource with
    /// payload.
    fn build_resource_request(
        &mut self,
        uri_path: Vec<u8>,
        payload: Option<Vec<u8>>,
    ) -> Packet {
        let mut req = Packet::new();

        // We're not retrying on failure in this demo, but usually you're
        // sending confirmable messages to achieve reliability
        req.header.set_type(MessageType::Confirmable);
        // This message ID should be acknowledged by the server for reliability
        req.header.message_id = self.message_id;
        self.message_id = self.message_id.wrapping_add(1);
        // And the token is used to tie response to request
        req.set_token(vec![self.token]);
        self.token = self.token.wrapping_add(1);
        // Note:
        // While the F4 has a TRNG on board and we could therefore do random
        // message IDs and tokens, it's not really important for this demo, so
        // we just use continuously incrementing ones
        req.header.code = MessageClass::Request(RequestType::Get);
        req.set_content_format(ContentFormat::TextPlain);
        req.add_option(CoapOption::UriPath, uri_path);
        if let Some(payload) = payload {
            req.payload = payload;
        }

        req
    }
}
