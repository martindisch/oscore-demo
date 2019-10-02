//! Handling of CoAP messages.

use alloc::vec::Vec;
use coap_lite::{
    CoapOption, ContentFormat, MessageClass, MessageType, Packet, ResponseType,
};

/// Handles a CoAP message and returns a response.
pub fn handle(req: &Packet) -> Packet {
    if let Some(path) = req.get_option(CoapOption::UriPath) {
        // Copy the linked list of references so we can manipulate it for
        // easier traversal
        let mut path = path.clone();

        if let Some(first) = path.pop_front() {
            if first == b".well-known" {
                if let Some(second) = path.pop_front() {
                    if second == b"core" {
                        // Response to /.well-known/core
                        return generate_link_format(
                            req,
                            b"</hello>;rt=\"test\";ct=0,\
                              </echo>;rt=\"echo\";ct=42"
                                .to_vec(),
                        );
                    }
                }

                // Response to /.well-known
                return generate_link_format(
                    req,
                    br#"</.well-known/core>;rt="core";ct=40"#.to_vec(),
                );
            } else if first == b"hello" {
                // Response to /hello
                return generate_response(
                    req,
                    b"Hello, world!".to_vec(),
                    ContentFormat::TextPlain,
                );
            } else if first == b"echo" {
                // Response to /echo
                let payload = req.payload.clone();
                return generate_response(
                    req,
                    payload,
                    ContentFormat::ApplicationOctetStream,
                );
            }
        }
    }

    // If we made it here, the requested resource was not found
    let mut res = Packet::new();
    res.header.set_type(MessageType::Acknowledgement);
    res.header.code = MessageClass::Response(ResponseType::NotFound);
    res.header.message_id = req.header.message_id;
    res.set_token(req.get_token().clone());
    res.set_content_format(ContentFormat::TextPlain);
    res.payload = b"Not found".to_vec();

    res
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

/// Returns a "normal" response with a payload and content-format.
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
