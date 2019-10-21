use clap::{App, Arg};
use coap_lite::{
    CoapOption, ContentFormat, MessageClass, MessageType, Packet, RequestType,
};
use oscore::{
    edhoc::{
        error::{OwnError, OwnOrPeerError},
        PartyU,
    },
    oscore::SecurityContext,
};
use rand::prelude::*;
use std::{net::UdpSocket, str};

/* EDHOC configuration */
// Private authentication key
const AUTH_PRIV: [u8; 32] = [
    0x53, 0x21, 0xFC, 0x01, 0xC2, 0x98, 0x20, 0x06, 0x3A, 0x72, 0x50, 0x8F,
    0xC6, 0x39, 0x25, 0x1D, 0xC8, 0x30, 0xE2, 0xF7, 0x68, 0x3E, 0xB8, 0xE3,
    0x8A, 0xF1, 0x64, 0xA5, 0xB9, 0xAF, 0x9B, 0xE3,
];
// Public authentication key (known by peer)
const AUTH_PUB: [u8; 32] = [
    0x42, 0x4C, 0x75, 0x6A, 0xB7, 0x7C, 0xC6, 0xFD, 0xEC, 0xF0, 0xB3, 0xEC,
    0xFC, 0xFF, 0xB7, 0x53, 0x10, 0xC0, 0x15, 0xBF, 0x5C, 0xBA, 0x2E, 0xC0,
    0xA2, 0x36, 0xE6, 0x65, 0x0C, 0x8A, 0xB9, 0xC7,
];
// Key ID used to identify the public authentication key
const KID: [u8; 1] = [0xA2];
// Public authentication key of the peer
const AUTH_PEER: [u8; 32] = [
    0x1B, 0x66, 0x1E, 0xE5, 0xD5, 0xEF, 0x16, 0x72, 0xA2, 0xD8, 0x77, 0xCD,
    0x5B, 0xC2, 0x0F, 0x46, 0x30, 0xDC, 0x78, 0xA1, 0x14, 0xDE, 0x65, 0x9C,
    0x7E, 0x50, 0x4D, 0x0F, 0x52, 0x9A, 0x6B, 0xD3,
];
const KID_PEER: [u8; 1] = [0xA3];

fn main() {
    let matches = App::new(clap::crate_name!())
        .version(clap::crate_version!())
        .about("Desktop client using an OSCORE server.")
        .author(clap::crate_authors!())
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("NUM")
                .takes_value(true)
                .help("The local port to bind to")
                .required(true),
        )
        .arg(
            Arg::with_name("DEST")
                .help("The destination server")
                .required(true),
        )
        .get_matches();
    let port = matches.value_of("port").unwrap();
    let destination = matches.value_of("DEST").unwrap();

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))
        .expect("Unable to bind to port");

    // Do EDHOC exchange
    let (master_secret, master_salt) = edhoc(&socket, destination);
    // Initialize OSCORE context
    let mut oscore = SecurityContext::new(
        master_secret,
        master_salt,
        KID.to_vec(),
        KID_PEER.to_vec(),
    )
    .expect("Unable to build security context");
    // Start making OSCORE requests
    oscore_requests(&socket, destination, &mut oscore);
}

/// Does an EDHOC exchange with the given destination.
fn edhoc(socket: &UdpSocket, destination: &str) -> (Vec<u8>, Vec<u8>) {
    // "Generate" an ECDH key pair (this is hardcoded, but MUST be
    // ephemeral and generated randomly)
    let eph = [
        0xD4, 0xD8, 0x1A, 0xBA, 0xFA, 0xD9, 0x08, 0xA0, 0xCC, 0xEF, 0xEF,
        0x5A, 0xD6, 0xB0, 0x5D, 0x50, 0x27, 0x02, 0xF1, 0xC1, 0x6F, 0x23,
        0x2C, 0x25, 0x92, 0x93, 0x09, 0xAC, 0x44, 0x1B, 0x95, 0x8E,
    ];
    // Choose a connection identifier
    let c_u = [0xC3].to_vec();

    // Initialize what we need to handle messages
    let msg1_sender =
        PartyU::new(c_u, eph, &AUTH_PRIV, &AUTH_PUB, KID.to_vec());
    // type = 1 is the case in CoAP, where party U can correlate
    // message_1 and message_2 with the token
    let (msg1_bytes, msg2_receiver) =
        // If an error happens here, we just abort. No need to send a message,
        // since the protocol hasn't started yet.
        msg1_sender.generate_message_1(1).expect("Error generating message_1");

    let msg2_bytes = send_receive(
        &socket,
        destination,
        &build_edhoc_request(msg1_bytes),
        true,
    );
    println!("Sent message_1 to peer and received message_2");
    let (_v_kid, msg2_verifier) =
        // This is a case where we could receive an error message (just abort
        // then), or cause an error (send it to the peer)
        match msg2_receiver.extract_peer_kid(msg2_bytes) {
            Err(OwnOrPeerError::PeerError(s)) => {
                panic!("Received error msg: {}", s)
            }
            Err(OwnOrPeerError::OwnError(b)) => {
                send(&socket, destination, &build_edhoc_request(b));
                panic!("Ran into an issue dealing with msg_2")
            }
            Ok(val) => val,
        };
    let msg3_sender = match msg2_verifier.verify_message_2(&AUTH_PEER) {
        Err(OwnError(b)) => {
            send(&socket, destination, &build_edhoc_request(b));
            panic!("Ran into an issue verifying message_2")
        }
        Ok(val) => val,
    };
    let (msg3_bytes, master_secret, master_salt) =
        match msg3_sender.generate_message_3() {
            Err(OwnError(b)) => {
                send(&socket, destination, &build_edhoc_request(b));
                panic!("Ran into an issue generating message_3")
            }
            Ok(val) => val,
        };
    println!(
        "Successfully derived the master secret and salt\n\
         {:?}\n\
         {:?}",
        master_secret, master_salt
    );

    send_receive(
        &socket,
        destination,
        &build_edhoc_request(msg3_bytes),
        false,
    );
    println!("Sent message_3 to peer");

    (master_secret, master_salt)
}

/// Makes repeated OSCORE requests to the target's /hello and /echo resources.
fn oscore_requests(
    socket: &UdpSocket,
    destination: &str,
    oscore: &mut SecurityContext,
) {
    for i in 0.. {
        // Build a CoAP request to one of the two resources
        let coap = if i % 2 == 0 {
            build_resource_request(b"hello".to_vec(), None)
        } else {
            build_resource_request(
                b"echo".to_vec(),
                Some(format!("Iteration {}", i).as_bytes().to_vec()),
            )
        };

        // Protect it with OSCORE
        let protected =
            oscore.protect_request(&coap).expect("Protection failed");
        // Send it to the server
        let res = send_receive(socket, destination, &protected, false);
        // Unprotect from OSCORE
        let unprotected = oscore
            .unprotect_response(&res)
            .expect("Unprotection failed");

        // Parse the embedded CoAP packet
        let coap = Packet::from_bytes(&unprotected)
            .expect("Failed parsing unprotected response");
        // Log the payload
        println!(
            "Got response: {}",
            str::from_utf8(&coap.payload)
                .expect("Failed parsing response payload as UTF-8")
        );
    }
}

/// Returns a CoAP packet for an EDHOC message.
fn build_edhoc_request(msg: Vec<u8>) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut req = Packet::new();

    // We're not retrying on failure in this demo, but usually you're sending
    // confirmable messages to achieve reliability
    req.header.set_type(MessageType::Confirmable);
    // This message ID should be acknowledged by the server for reliability
    req.header.message_id = rng.gen();
    // And the token is used to tie response to request
    req.set_token(vec![rng.gen()]);
    // As per the EDHOC spec, we send a 0.02 (Post)
    req.header.code = MessageClass::Request(RequestType::Post);
    // This would be the EDHOC Content-Format, but it's
    // not standardized yet
    req.set_content_format(ContentFormat::ApplicationOctetStream);
    // Request goes to /.well-known/edhoc
    req.add_option(CoapOption::UriPath, b".well-known".to_vec());
    req.add_option(CoapOption::UriPath, b"edhoc".to_vec());
    // Finally, pack in our EDHOC message
    req.payload = msg;

    req.to_bytes().expect("Failed getting bytes from packet")
}

/// Returns a CoAP packet for a GET request to the given resource with payload.
fn build_resource_request(
    uri_path: Vec<u8>,
    payload: Option<Vec<u8>>,
) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut req = Packet::new();

    // We're not retrying on failure in this demo, but usually you're sending
    // confirmable messages to achieve reliability
    req.header.set_type(MessageType::Confirmable);
    // This message ID should be acknowledged by the server for reliability
    req.header.message_id = rng.gen();
    // And the token is used to tie response to request
    req.set_token(vec![rng.gen()]);
    req.header.code = MessageClass::Request(RequestType::Get);
    req.set_content_format(ContentFormat::TextPlain);
    req.add_option(CoapOption::UriPath, uri_path);
    if let Some(payload) = payload {
        req.payload = payload;
    }

    req.to_bytes().expect("Failed getting bytes from packet")
}

/// Sends a CoAP packet to the destination and returns the full response or
/// just the payload.
fn send_receive(
    socket: &UdpSocket,
    destination: &str,
    packet: &[u8],
    payload_only: bool,
) -> Vec<u8> {
    let mut buf = [0; 2048];

    socket
        .send_to(packet, destination)
        .expect("Unable to send packet");
    let (amt, _src) =
        socket.recv_from(&mut buf).expect("Failed while receiving");

    if payload_only {
        Packet::from_bytes(&buf[..amt])
            .expect("Unable to parse packet")
            .payload
    } else {
        buf[..amt].to_vec()
    }
}

/// Sends a CoAP packet to the destination.
fn send(socket: &UdpSocket, destination: &str, packet: &[u8]) {
    socket
        .send_to(packet, destination)
        .expect("Unable to send packet");
}
