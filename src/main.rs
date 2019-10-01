#![no_std]
#![no_main]

extern crate panic_semihosting;

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

// uncomment to disable tracing
// macro_rules! uprintln {
//     ($($tt: tt)*) => {};
// }

/* Configuration */
const MAC: mac::Addr = mac::Addr([0x20, 0x18, 0x03, 0x01, 0x00, 0x00]);
const IP: ipv4::Addr = ipv4::Addr([192, 168, 0, 99]);

/* Constants */
const KB: u16 = 1024; // bytes

#[entry]
fn main() -> ! {
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
    let mut cache = FnvIndexMap::<_, _, U8>::new();

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

        if let Ok(mut rx_eth) =
            ether::Frame::parse(&mut rx_buf[..len as usize])
        {
            let eth_len = rx_eth.len();
            // TODO: reuse len
            uprintln!(tx, "\nRx({})", rx_eth.as_bytes().len());
            // TODO: figure out when to use this over the ARP cache
            let src_mac = rx_eth.get_source();

            match rx_eth.get_type() {
                ether::Type::Arp => {
                    if let Ok(rx_arp) = arp::Packet::parse(rx_eth.payload()) {
                        match rx_arp.downcast() {
                            Ok(rx_arp) => {
                                uprintln!(
                                    tx,
                                    "ARP packet from {}",
                                    rx_arp.get_spa()
                                );

                                if !rx_arp.is_a_probe() {
                                    cache
                                        .insert(
                                            rx_arp.get_spa(),
                                            rx_arp.get_sha(),
                                        )
                                        .expect("Failed inserting cache");
                                }

                                // are they asking for us?
                                if rx_arp.get_oper() == arp::Operation::Request
                                    && rx_arp.get_tpa() == IP
                                {
                                    // TODO: reuse by storing in variable
                                    // reply to the ARP request
                                    let tha = rx_arp.get_sha();
                                    let tpa = rx_arp.get_spa();

                                    // Build Ethernet frame from scratch
                                    let mut eth =
                                        ether::Frame::new(&mut tx_buf[..]);
                                    eth.set_destination(tha);
                                    eth.set_source(MAC);

                                    // Insert an ARP packet
                                    eth.arp(|arp| {
                                        arp.set_oper(arp::Operation::Reply);
                                        arp.set_spa(IP);
                                        arp.set_tha(tha);
                                        arp.set_tpa(tpa);
                                    });

                                    uprintln!(
                                        tx,
                                        "Asked for us, sending reply"
                                    );
                                    uprintln!(
                                        tx,
                                        "Tx({})",
                                        eth.as_bytes().len()
                                    );
                                    enc28j60
                                        .transmit(eth.as_bytes())
                                        .expect("Failed transmitting ARP");
                                }
                            }
                            Err(_arp) => {
                                // Not a Ethernet/IPv4 ARP packet
                                uprintln!(tx, "ARP downcast fail");
                            }
                        }
                    } else {
                        // malformed ARP packet
                        uprintln!(tx, "Malformed ARP packet");
                    }
                }
                ether::Type::Ipv4 => {
                    if let Ok(mut rx_ip) =
                        ipv4::Packet::parse(rx_eth.payload_mut())
                    {
                        let src_ip = rx_ip.get_source();
                        uprintln!(tx, "IP packet from {}", src_ip);

                        if !src_mac.is_broadcast() {
                            cache
                                .insert(src_ip, src_mac)
                                .expect("Failed inserting cache");
                        }

                        match rx_ip.get_protocol() {
                            ipv4::Protocol::Icmp => {
                                if let Ok(icmp) =
                                    icmp::Message::parse(rx_ip.payload_mut())
                                {
                                    match icmp.downcast::<icmp::EchoRequest>()
                                    {
                                        Ok(request) => {
                                            // is an echo request
                                            let src_mac = cache
                                                .get(&src_ip)
                                                .unwrap_or_else(
                                                    || unimplemented!(),
                                                );

                                            // Convert to a reply
                                            // TODO: Here and below remove _v
                                            let _reply: icmp::Message<
                                                _,
                                                icmp::EchoReply,
                                                _,
                                            > = request.into();

                                            // update the IP header
                                            let mut ip = rx_ip.set_source(IP);
                                            ip.set_destination(src_ip);
                                            let _ip = ip.update_checksum();

                                            // update the Ethernet header
                                            rx_eth.set_destination(*src_mac);
                                            rx_eth.set_source(MAC);

                                            leds.spin().expect(
                                                "Failed advancing led",
                                            );
                                            uprintln!(
                                                tx,
                                                "ICMP request, responding"
                                            );
                                            uprintln!(
                                                tx,
                                                "Tx({})",
                                                rx_eth.as_bytes().len()
                                            );
                                            enc28j60
                                                .transmit(rx_eth.as_bytes())
                                                .expect(
                                                    "Failed transmitting ICMP",
                                                );
                                        }
                                        Err(_icmp) => {
                                            uprintln!(tx, "ICMP downcast err");
                                        }
                                    }
                                } else {
                                    // Malformed ICMP packet
                                    uprintln!(tx, "Malformed ICMP packet");
                                }
                            }
                            ipv4::Protocol::Udp => {
                                if let Ok(rx_udp) =
                                    udp::Packet::parse(rx_ip.payload())
                                {
                                    if let Some(src_mac) = cache.get(&src_ip) {
                                        let src_port = rx_udp.get_source();
                                        let dst_port =
                                            rx_udp.get_destination();

                                        // Build Ethernet frame from scratch
                                        let mut eth =
                                            ether::Frame::new(&mut tx_buf[..]);
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
                                                udp.set_payload(
                                                    rx_udp.payload(),
                                                );
                                            });
                                        });

                                        leds.spin()
                                            .expect("Failed advancing led");
                                        uprintln!(tx, "Echoing UDP packet");
                                        uprintln!(
                                            tx,
                                            "Tx({})",
                                            eth.as_bytes().len()
                                        );
                                        enc28j60
                                            .transmit(eth.as_bytes())
                                            .expect("Failed transmitting UDP");
                                    } else {
                                        uprintln!(
                                            tx,
                                            "Sender not in ARP cache"
                                        );
                                    }
                                } else {
                                    // malformed UDP packet
                                    uprintln!(tx, "Malformed UDP packet");
                                }
                            }
                            _ => {}
                        }
                    } else {
                        // malformed IPv4 packet
                        uprintln!(tx, "Malformed IPv4 packet");
                    }
                }
                _ => {}
            }
        } else {
            // malformed Ethernet frame
            uprintln!(tx, "Malformed Ethernet frame");
        }
    }
}
