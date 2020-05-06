use clap::{App, Arg};
use std::net::UdpSocket;

use desktop_server::{
    coap::CoapHandler, edhoc::EdhocHandler, oscore::OscoreHandler,
};

/* EDHOC configuration */
// Private authentication key
const AUTH_PRIV: [u8; 32] = [
    0x74, 0x56, 0xB3, 0xA3, 0xE5, 0x8D, 0x8D, 0x26, 0xDD, 0x36, 0xBC, 0x75,
    0xD5, 0x5B, 0x88, 0x63, 0xA8, 0x5D, 0x34, 0x72, 0xF4, 0xA0, 0x1F, 0x02,
    0x24, 0x62, 0x1B, 0x1C, 0xB8, 0x16, 0x6D, 0xA9,
];
// Public authentication key (known by peer)
const AUTH_PUB: [u8; 32] = [
    0x1B, 0x66, 0x1E, 0xE5, 0xD5, 0xEF, 0x16, 0x72, 0xA2, 0xD8, 0x77, 0xCD,
    0x5B, 0xC2, 0x0F, 0x46, 0x30, 0xDC, 0x78, 0xA1, 0x14, 0xDE, 0x65, 0x9C,
    0x7E, 0x50, 0x4D, 0x0F, 0x52, 0x9A, 0x6B, 0xD3,
];
// Key ID used to identify the public authentication key
const KID: [u8; 1] = [0xA3];
// Public authentication key of the peer
const AUTH_PEER: [u8; 32] = [
    0x42, 0x4C, 0x75, 0x6A, 0xB7, 0x7C, 0xC6, 0xFD, 0xEC, 0xF0, 0xB3, 0xEC,
    0xFC, 0xFF, 0xB7, 0x53, 0x10, 0xC0, 0x15, 0xBF, 0x5C, 0xBA, 0x2E, 0xC0,
    0xA2, 0x36, 0xE6, 0x65, 0x0C, 0x8A, 0xB9, 0xC7,
];
// Key ID of peer
const KID_PEER: [u8; 1] = [0xA2];

fn main() {
    let matches = App::new(clap::crate_name!())
        .version(clap::crate_version!())
        .about("Desktop server for OSCORE clients.")
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
        .get_matches();
    let port = matches.value_of("port").unwrap();

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))
        .expect("Unable to bind to port");

    // This is doing the EDHOC exchange
    let edhoc =
        EdhocHandler::new(AUTH_PRIV, AUTH_PUB, KID.to_vec(), AUTH_PEER);
    // This will be responsible for dealing with CoAP messages
    let coap = CoapHandler::new();
    // And finally this is the layer for OSCORE
    let mut oscore =
        OscoreHandler::new(edhoc, coap, KID.to_vec(), KID_PEER.to_vec());

    loop {
        let mut buf = [0; 2048];
        let (amt, src) =
            socket.recv_from(&mut buf).expect("Failed while receiving");
        println!("\nRx({})", amt);
        println!("IP packet from {}", src.ip());

        // Handle the request
        let res = oscore.handle(&buf[..amt]);
        if res.is_none() {
            continue;
        }
        let res = res.unwrap();

        println!("Responding with CoAP packet");
        println!("Tx({})", res.len());
        socket.send_to(&res, src).expect("Failed sending");
    }
}
