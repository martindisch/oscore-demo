#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate panic_semihosting;

use alloc_cortex_m::CortexMHeap;
use coap_lite::Packet;
use core::fmt::Write;
use cortex_m_rt::entry;
use enc28j60::Enc28j60;
use heapless::consts::*;
use heapless::FnvIndexMap;
use jnet::{arp, ether, icmp, ipv4, mac, udp};

use alt_stm32f30x_hal as hal;
use hal::delay::Delay;
use hal::prelude::*;

use oscore_demo::{
    coap::CoapHandler, edhoc::EdhocHandler, led::Leds, oscore::OscoreHandler,
    uprint, uprintln,
};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

// uncomment to disable tracing
// macro_rules! uprintln {
//     ($($tt: tt)*) => {};
// }

/* Configuration */
const MAC: mac::Addr = mac::Addr([0x20, 0x18, 0x03, 0x01, 0x00, 0x00]);
const IP: ipv4::Addr = ipv4::Addr([192, 168, 0, 99]);

/* Constants */
const KB: u16 = 1024; // bytes
const COAP_PORT: u16 = 5683;

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

#[entry]
fn main() -> ! {
    // Initialize the allocator BEFORE you use it
    let start = cortex_m_rt::heap_start() as usize;
    let size = 10 * KB as usize;
    unsafe { ALLOCATOR.init(start, size) }

    let cp = cortex_m::Peripherals::take().expect("Failed taking cp");
    let dp = hal::pac::Peripherals::take().expect("Failed taking dp");

    let mut rcc = dp.RCC.constrain();
    let mut flash = dp.FLASH.constrain();
    let gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    // USART1
    let serial =
        dp.USART1
            .serial((gpioa.pa9, gpioa.pa10), 115_200.bps(), clocks);
    let (mut tx, mut _rx) = serial.split();
    uprintln!(tx, "Basic initialization done");

    // LEDs
    let gpioe = dp.GPIOE.split(&mut rcc.ahb);
    let mut leds = Leds::new(gpioe);

    // SPI
    let mut ncs = gpioa.pa4.output().push_pull();
    ncs.set_high().expect("Failed setting ncs");
    let sck = gpioa.pa5;
    let miso = gpioa.pa6;
    let mosi = gpioa.pa7;
    let spi = dp
        .SPI1
        .spi((sck, miso, mosi), enc28j60::MODE, 1.mhz(), clocks);

    // ENC28J60
    let mut delay = Delay::new(cp.SYST, clocks);
    let mut enc28j60 = Enc28j60::new(
        spi,
        ncs,
        enc28j60::Unconnected,
        enc28j60::Unconnected,
        &mut delay,
        6 * KB,
        MAC.0,
    )
    .expect("Failed initializing driver");

    uprintln!(tx, "Complete initialization done");

    // ARP cache
    let mut cache = FnvIndexMap::<_, _, U16>::new();
    // Send and receiver buffers
    let mut rx_buf = [0; 1522];
    let mut tx_buf = [0; 1522];

    // This is doing the EDHOC exchange
    let edhoc =
        EdhocHandler::new(AUTH_PRIV, AUTH_PUB, KID.to_vec(), AUTH_PEER);
    // This will be responsible for dealing with CoAP messages
    let coap = CoapHandler::new();
    // And finally this is the layer for OSCORE
    let mut oscore = OscoreHandler::new(edhoc, coap);

    loop {
        let len = match enc28j60.receive(rx_buf.as_mut()) {
            Ok(len) => len,
            Err(err) => {
                uprintln!(tx, "Error receiving, resetting device: {:?}", err);
                // Reclaim resources currently owned by it
                let (spi, ncs, _, _) = enc28j60.free();
                // Reinitialize it
                enc28j60 = Enc28j60::new(
                    spi,
                    ncs,
                    enc28j60::Unconnected,
                    enc28j60::Unconnected,
                    &mut delay,
                    6 * KB,
                    MAC.0,
                )
                .expect("Failed initializing driver");
                continue;
            }
        };

        let parsed = ether::Frame::parse(&mut rx_buf[..len as usize]);
        if parsed.is_err() {
            // malformed Ethernet frame
            uprintln!(tx, "Malformed Ethernet frame");
            continue;
        }
        let mut rx_eth = parsed.unwrap();

        uprintln!(tx, "\nRx({})", rx_eth.len());
        let src_mac = rx_eth.get_source();
        match rx_eth.get_type() {
            ether::Type::Arp => {
                let parsed = arp::Packet::parse(rx_eth.payload());
                if parsed.is_err() {
                    // malformed ARP packet
                    uprintln!(tx, "Malformed ARP packet");
                    continue;
                }
                let rx_arp = parsed.unwrap();

                let downcast = rx_arp.downcast();
                if downcast.is_err() {
                    // Not a Ethernet/IPv4 ARP packet
                    uprintln!(tx, "ARP downcast fail");
                    continue;
                }
                let rx_arp = downcast.unwrap();

                let sha = rx_arp.get_sha();
                let spa = rx_arp.get_spa();
                uprintln!(tx, "ARP packet from {}", spa);

                if !rx_arp.is_a_probe() {
                    cache.insert(spa, sha).expect("Cache full");
                }

                // are they asking for us?
                if rx_arp.get_oper() == arp::Operation::Request
                    && rx_arp.get_tpa() == IP
                {
                    // Build Ethernet frame from scratch
                    let mut eth = ether::Frame::new(&mut tx_buf[..]);
                    eth.set_destination(sha);
                    eth.set_source(MAC);

                    // Insert an ARP packet
                    eth.arp(|arp| {
                        arp.set_oper(arp::Operation::Reply);
                        arp.set_spa(IP);
                        arp.set_tha(sha);
                        arp.set_tpa(spa);
                    });

                    uprintln!(tx, "Asked for us, sending reply");
                    uprintln!(tx, "Tx({})", eth.len());
                    enc28j60
                        .transmit(eth.as_bytes())
                        .expect("Failed transmitting ARP");
                }
            }
            ether::Type::Ipv4 => {
                let parsed = ipv4::Packet::parse(rx_eth.payload_mut());
                if parsed.is_err() {
                    // malformed IPv4 packet
                    uprintln!(tx, "Malformed IPv4 packet");
                    continue;
                }
                let mut rx_ip = parsed.unwrap();

                let src_ip = rx_ip.get_source();
                uprintln!(tx, "IP packet from {}", src_ip);

                if !src_mac.is_broadcast() {
                    cache.insert(src_ip, src_mac).expect("Cache full");
                }

                match rx_ip.get_protocol() {
                    ipv4::Protocol::Icmp => {
                        let parsed = icmp::Message::parse(rx_ip.payload_mut());
                        if parsed.is_err() {
                            // Malformed ICMP packet
                            uprintln!(tx, "Malformed ICMP packet");
                            continue;
                        }
                        let icmp = parsed.unwrap();

                        let downcast = icmp.downcast::<icmp::EchoRequest>();
                        if downcast.is_err() {
                            uprintln!(tx, "ICMP downcast err");
                            continue;
                        }
                        let request = downcast.unwrap();

                        let lookup = cache.get(&src_ip);
                        if lookup.is_none() {
                            uprintln!(tx, "Sender not in ARP cache");
                            continue;
                        }
                        let src_mac = lookup.unwrap();

                        // convert to a reply
                        let _reply: icmp::Message<_, icmp::EchoReply, _> =
                            request.into();

                        // update the IP header
                        let mut ip = rx_ip.set_source(IP);
                        ip.set_destination(src_ip);
                        ip.update_checksum();

                        // update the Ethernet header
                        rx_eth.set_destination(*src_mac);
                        rx_eth.set_source(MAC);

                        leds.spin().expect("Failed advancing led");
                        uprintln!(tx, "ICMP request, responding");
                        uprintln!(tx, "Tx({})", rx_eth.len());
                        enc28j60
                            .transmit(rx_eth.as_bytes())
                            .expect("Failed transmitting ICMP");
                    }
                    ipv4::Protocol::Udp => {
                        let parsed = udp::Packet::parse(rx_ip.payload());
                        if parsed.is_err() {
                            // malformed UDP packet
                            uprintln!(tx, "Malformed UDP packet");
                            continue;
                        }
                        let rx_udp = parsed.unwrap();

                        let lookup = cache.get(&src_ip);
                        if lookup.is_none() {
                            uprintln!(tx, "Sender not in ARP cache");
                            continue;
                        }
                        let src_mac = lookup.unwrap();

                        let src_port = rx_udp.get_source();
                        let dst_port = rx_udp.get_destination();

                        if dst_port != COAP_PORT {
                            continue;
                        }

                        // Handle the request
                        let res = oscore.handle(&mut tx, rx_udp.payload());
                        if res.is_none() {
                            continue;
                        }
                        let res = res.unwrap();

                        // Build Ethernet frame from scratch
                        let mut eth = ether::Frame::new(&mut tx_buf[..]);
                        eth.set_destination(*src_mac);
                        eth.set_source(MAC);

                        eth.ipv4(|ip| {
                            // Update the IP header
                            ip.set_source(IP);
                            ip.set_destination(src_ip);
                            ip.udp(|udp| {
                                // Update the UDP header
                                udp.set_source(COAP_PORT);
                                udp.set_destination(src_port);
                                // Wrap CoAP packet
                                udp.set_payload(&res);
                            });
                        });

                        leds.spin().expect("Failed advancing led");
                        uprintln!(tx, "Responding with CoAP packet");
                        uprintln!(tx, "Tx({})", eth.len());
                        enc28j60
                            .transmit(eth.as_bytes())
                            .expect("Failed transmitting UDP");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    panic!("We're officially OOM");
}
