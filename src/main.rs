#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate panic_semihosting;

use alloc_cortex_m::CortexMHeap;
use coap_lite::{MessageClass, MessageType, Packet, ResponseType};
use core::fmt::Write;
use cortex_m_rt::entry;
use enc28j60::Enc28j60;
use heapless::consts::*;
use heapless::FnvIndexMap;
use jnet::{arp, ether, icmp, ipv4, mac, udp};

use alt_stm32f30x_hal as hal;
use hal::delay::Delay;
use hal::prelude::*;

use oscore_demo::{led::Leds, uprint, uprintln};

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

    let mut rx_buf = [0; 1522];
    let mut tx_buf = [0; 1522];
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

                        if dst_port == COAP_PORT {
                            let req = Packet::from_bytes(rx_udp.payload())
                                .expect("Failed parsing CoAP");
                            uprintln!(tx, "{:?}", req);

                            let mut res = Packet::new();
                            res.header.set_type(MessageType::Acknowledgement);
                            res.header.code =
                                MessageClass::Response(ResponseType::Content);
                            res.header.message_id = req.header.message_id;
                            res.set_token(req.get_token().clone());
                            res.payload = b"Hello, world!".to_vec();

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
                                    udp.set_payload(&res.to_bytes().expect(
                                        "Failed creating CoAP packet",
                                    ));
                                });
                            });

                            leds.spin().expect("Failed advancing led");
                            uprintln!(tx, "Responding with CoAP packet");
                            uprintln!(tx, "Tx({})", eth.len());
                            enc28j60
                                .transmit(eth.as_bytes())
                                .expect("Failed transmitting UDP");
                        } else {
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
                                    udp.set_source(dst_port);
                                    udp.set_destination(src_port);
                                    udp.set_payload(rx_udp.payload());
                                });
                            });

                            leds.spin().expect("Failed advancing led");
                            uprintln!(tx, "Echoing UDP packet");
                            uprintln!(tx, "Tx({})", eth.len());
                            enc28j60
                                .transmit(eth.as_bytes())
                                .expect("Failed transmitting UDP");
                        }
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
