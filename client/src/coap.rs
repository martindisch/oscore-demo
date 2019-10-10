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

    /// Returns a CoAP request with the first EDHOC message.
    pub fn go(
        &mut self,
        tx: &mut Tx<USART1>,
        edhoc: &mut EdhocHandler,
    ) -> Option<Packet> {
        // Get the first message from the EDHOC handler
        let payload = edhoc.go(tx);
        // Do an early return with None if we got that
        let payload = payload?;

        self.build_edhoc_request(payload)
    }

    /// Handles a CoAP response and returns the next request.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        edhoc: &mut EdhocHandler,
        res: Packet,
    ) -> Option<Packet> {
        if edhoc.get_state() == State::Complete {
            // OSCORE is already going and we received a response
            uprintln!(
                tx,
                "Got response: {}",
                str::from_utf8(&res.payload)
                    .expect("Failed parsing response payload as UTF-8")
            );
            // Send the next request
            Some(self.build_resource_request())
        } else {
            // Get our response from the EDHOC handler
            let payload = edhoc.handle(tx, res.payload);
            // Do an early return with None if we got that
            let payload = payload?;

            self.build_edhoc_request(payload)
        }
    }

    /// Returns a CoAP packet for an EDHOC message.
    fn build_edhoc_request(&mut self, payload: Vec<u8>) -> Option<Packet> {
        let mut req = Packet::new();

        // We're not retrying on failure in this demo, but usually you're
        // sending confirmable messages to achieve reliability
        req.header.set_type(MessageType::Confirmable);
        // This message ID should be acknowledged by the server for
        // reliability
        req.header.message_id = self.get_next_message_id();
        // And the token is used to tie response to request
        req.set_token(self.get_next_token());
        // As per the EDHOC spec, we send a 0.02 (Post)
        req.header.code = MessageClass::Request(RequestType::Post);
        // This would be the EDHOC Content-Format, but it's
        // not standardized yet
        req.set_content_format(ContentFormat::ApplicationOctetStream);
        // Request goes to /.well-known/edhoc
        req.add_option(CoapOption::UriPath, b".well-known".to_vec());
        req.add_option(CoapOption::UriPath, b"edhoc".to_vec());
        // Finally, pack in our EDHOC message
        req.payload = payload;

        Some(req)
    }

    /// Returns a CoAP packet for a GET request to a resource.
    pub fn build_resource_request(&mut self) -> Packet {
        // Build a CoAP request to one of the two resources
        let i = self.oscore_iteration;
        let (uri_path, payload) = if i % 2 == 0 {
            (b"hello".to_vec(), None)
        } else {
            (
                b"echo".to_vec(),
                Some(format!("Iteration {}", i).as_bytes().to_vec()),
            )
        };
        self.oscore_iteration += 1;

        let mut req = Packet::new();
        // We're not retrying on failure in this demo, but usually you're
        // sending confirmable messages to achieve reliability
        req.header.set_type(MessageType::Confirmable);
        // This message ID should be acknowledged by the server for reliability
        req.header.message_id = self.get_next_message_id();
        // And the token is used to tie response to request
        req.set_token(self.get_next_token());
        req.header.code = MessageClass::Request(RequestType::Get);
        req.set_content_format(ContentFormat::TextPlain);
        req.add_option(CoapOption::UriPath, uri_path);
        if let Some(payload) = payload {
            req.payload = payload;
        }

        req
    }

    /// Returns the message ID and increments it.
    ///
    /// While the F4 has a TRNG on board and we could therefore do random
    /// message IDs and tokens, it's not really important for this demo, so
    /// we just use continuously incrementing ones
    fn get_next_message_id(&mut self) -> u16 {
        let curr = self.message_id;
        self.message_id = self.message_id.wrapping_add(1);

        curr
    }

    /// Returns the token and increments it.
    fn get_next_token(&mut self) -> Vec<u8> {
        let curr = self.token;
        self.token = self.token.wrapping_add(1);

        vec![curr]
    }
}
