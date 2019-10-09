//! Handling of CoAP messages.

use alloc::vec::Vec;
use coap_lite::{
    CoapOption, ContentFormat, MessageClass, MessageType, Packet, ResponseType,
};
use core::fmt::Write;
use stm32f4xx_hal::{serial::Tx, stm32::USART1};

use crate::{edhoc::EdhocHandler, uprint, uprintln};

/// Handles CoAP messages.
pub struct CoapHandler;

impl Default for CoapHandler {
    fn default() -> CoapHandler {
        CoapHandler
    }
}

impl CoapHandler {
    /// Creates a new `CoapHandler`.
    pub fn new() -> CoapHandler {
        // Since this is an empty struct, the constructor is not necessary, but
        // it's still here in case that changes.
        Default::default()
    }

    /// Handles a CoAP message and returns a response.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        edhoc: &mut EdhocHandler,
        req: Packet,
    ) -> Option<Packet> {
        if let Some(path) = req.get_option(CoapOption::UriPath) {
            // Copy the linked list of references so we can manipulate it for
            // easier traversal
            let mut path = path.clone();

            if let Some(first) = path.pop_front() {
                if first == b".well-known" {
                    if let Some(second) = path.pop_front() {
                        if second == b"core" {
                            uprintln!(
                                tx,
                                "Request for the /.well-known/core resource"
                            );
                            // Response to /.well-known/core
                            return Some(generate_link_format(
                                &req,
                                // About EDHOC: should have a custom ct, but
                                // since the EDHOC Content-Format is not part
                                // of the IANA registry yet, we don't know it
                                b"</hello>;rt=\"test\";ct=0,\
                                  </echo>;rt=\"echo\";ct=42,\
                                  </.well-known/edhoc>;rt=\"edhoc\";ct=42"
                                    .to_vec(),
                            ));
                        } else if second == b"edhoc" {
                            // Response to /.well-known/edhoc

                            // Duplicate the token for later use
                            let token = req.get_token().clone();
                            // Get our response from the EDHOC handler
                            let payload = edhoc.handle(tx, req.payload);
                            // Do an early return with None if we got that
                            let payload = payload?;

                            let mut res = Packet::new();
                            // We acknowledge that we've received the message
                            res.header.set_type(MessageType::Acknowledgement);
                            // and send the message ID to confirm this
                            res.header.message_id = req.header.message_id;
                            // Use the token to tie response to request
                            res.set_token(token);
                            // As per EDHOC spec, we return a 2.04 (Changed)
                            res.header.code =
                                MessageClass::Response(ResponseType::Changed);
                            // Again, there's no EDHOC Content-Format yet
                            res.set_content_format(
                                ContentFormat::ApplicationOctetStream,
                            );
                            // Finally, pack in our EDHOC message
                            res.payload = payload;

                            return Some(res);
                        }
                    }

                    // Response to /.well-known
                    return Some(generate_link_format(
                        &req,
                        br#"</.well-known/core>;rt="core";ct=40"#.to_vec(),
                    ));
                } else if first == b"hello" {
                    uprintln!(tx, "Request for the /hello resource");
                    // Response to /hello
                    return Some(generate_response(
                        &req,
                        b"Hello, world!".to_vec(),
                        ContentFormat::TextPlain,
                    ));
                } else if first == b"echo" {
                    uprintln!(tx, "Request for the /echo resource");
                    // Response to /echo
                    let payload = req.payload.clone();
                    return Some(generate_response(
                        &req,
                        payload,
                        ContentFormat::ApplicationOctetStream,
                    ));
                }
            }
        }

        // If we made it here, the requested resource was not found
        uprintln!(tx, "Requested resource was not found");
        let mut res = Packet::new();
        res.header.set_type(MessageType::Acknowledgement);
        res.header.code = MessageClass::Response(ResponseType::NotFound);
        res.header.message_id = req.header.message_id;
        res.set_token(req.get_token().clone());
        res.set_content_format(ContentFormat::TextPlain);
        res.payload = b"Not found".to_vec();

        Some(res)
    }
}

/// Returns a link-format type response with the given payload.
fn generate_link_format(req: &Packet, payload: Vec<u8>) -> Packet {
    let mut res = Packet::new();
    res.header.set_type(MessageType::Acknowledgement);
    res.header.code = MessageClass::Response(ResponseType::Content);
    res.header.message_id = req.header.message_id;
    res.set_token(req.get_token().clone());
    res.set_content_format(ContentFormat::ApplicationLinkFormat);
    res.payload = payload;

    res
}

/// Returns a "normal" response with a payload and Content-Format.
fn generate_response(
    req: &Packet,
    payload: Vec<u8>,
    cf: ContentFormat,
) -> Packet {
    let mut res = Packet::new();
    res.header.set_type(MessageType::Acknowledgement);
    res.header.code = MessageClass::Response(ResponseType::Content);
    res.header.message_id = req.header.message_id;
    res.set_token(req.get_token().clone());
    res.set_content_format(cf);
    res.payload = payload;

    res
}
