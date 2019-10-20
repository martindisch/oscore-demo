use coap_lite::{CoapOption, Packet};
use std::{net::UdpSocket, time::Duration};

use proxy::ProxyUri;

const COAP_PORT: u32 = 5683;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", COAP_PORT))
        .expect("Unable to bind to port");
    let mut buf = [0; 2048];

    loop {
        // Reset timeout so we can wait for as long as we want
        socket
            .set_read_timeout(None)
            .expect("Failed resetting timeout");

        // Receive next UDP packet and attempt parsing as CoAP
        let (amt, sender) =
            socket.recv_from(&mut buf).expect("Failed while receiving");
        let mut packet =
            Packet::from_bytes(&buf[..amt]).expect("Failed parsing CoAP");
        println!("Received packet from {}:\n{:?}", sender, packet);

        // Check if it contains a Proxy-Uri option
        if let Some(option_val) = packet
            .get_option(CoapOption::ProxyUri)
            .and_then(|l| l.front())
        {
            // Split the Proxy-Uri into its components
            let proxy_uri = ProxyUri::from(&option_val[..]);

            // Add the path and query segments to the packet
            if let Some(paths) = proxy_uri.get_path_list() {
                packet.set_option(CoapOption::UriPath, paths);
            }
            if let Some(query) = proxy_uri.get_query_list() {
                packet.set_option(CoapOption::UriQuery, query);
            }
            // Remove the Proxy-Uri option
            packet.clear_option(CoapOption::ProxyUri);

            // Send the updated packet to its destination
            let bytes =
                packet.to_bytes().expect("Failed converting CoAP to bytes");
            let destination = format!(
                "{}:{}",
                proxy_uri.uri_host,
                proxy_uri.uri_port.unwrap_or_else(|| COAP_PORT.to_string())
            );
            socket
                .send_to(&bytes, destination)
                .expect("Unable to send packet");

            // Receive the next packet, which we naively assume is the
            // response, and redirect it to the sender of the initial request
            // (yes, this is not a proper proxy, just a quick & dirty demo)
            socket
                .set_read_timeout(Some(READ_TIMEOUT))
                .expect("Failed setting timeout");
            let (amt, responder) = match socket.recv_from(&mut buf) {
                Ok(val) => val,
                Err(_) => {
                    println!("Timed out waiting for response");
                    continue;
                }
            };
            let packet =
                Packet::from_bytes(&buf[..amt]).expect("Failed parsing CoAP");
            println!("Received response from {}:\n{:?}\n", responder, packet);
            socket
                .send_to(&buf[..amt], sender)
                .expect("Unable to send packet");
        }
    }
}
