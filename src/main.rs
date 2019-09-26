#![no_std]
#![no_main]

extern crate panic_itm;

use cortex_m::iprintln;
use cortex_m_rt::entry;
use enc28j60::Enc28j60;
use heapless::consts::*;
use heapless::FnvIndexMap;
use jnet::{arp, ether, icmp, ipv4, mac, udp};

use alt_stm32f30x_hal as hal;
use hal::delay::Delay;
use hal::prelude::*;

use oscore_demo::siprintln;

// uncomment to disable tracing
// macro_rules! siprintln {
//     ($($tt: tt)*) => {};
// }

/* Configuration */
const MAC: mac::Addr = mac::Addr([0x20, 0x18, 0x03, 0x01, 0x00, 0x00]);
const IP: ipv4::Addr = ipv4::Addr([192, 168, 0, 99]);

/* Constants */
const KB: u16 = 1024; // bytes

#[entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let dp = hal::pac::Peripherals::take().unwrap();

    let mut rcc = dp.RCC.constrain();
    let mut flash = dp.FLASH.constrain();
    let gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let stim = &mut cp.ITM.stim[0];

    siprintln!(stim, "Basic initialization done");

    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    cp.DWT.enable_cycle_counter();

    // LED
    let gpioe = dp.GPIOE.split(&mut rcc.ahb);
    let mut led = gpioe.pe9.output().push_pull();

    // SPI
    let mut ncs = gpioa.pa4.output().push_pull();
    ncs.set_high().unwrap();
    let sck = gpioa.pa5;
    let miso = gpioa.pa6;
    let mosi = gpioa.pa7;
    let spi = dp
        .SPI1
        .spi((sck, miso, mosi), enc28j60::MODE, 1.mhz(), clocks);

    // ENC28J60
    let mut reset = gpioa.pa3.output().push_pull();
    reset.set_high().unwrap();
    let mut delay = Delay::new(cp.SYST, clocks);
    let mut enc28j60 = Enc28j60::new(
        spi,
        ncs,
        enc28j60::Unconnected,
        reset,
        &mut delay,
        7 * KB,
        MAC.0,
    )
    .ok()
    .unwrap();

    // LED on after initialization
    led.set_high().unwrap();
    siprintln!(stim, "Complete initialization done");

    // FIXME some frames are lost when sent right after initialization
    delay.delay_ms(100_u8);

    // ARP cache
    let mut cache = FnvIndexMap::<_, _, U8>::new();

    let mut buf = [0; 256];
    loop {
        let len = enc28j60.receive(buf.as_mut()).ok().unwrap();

        if let Ok(mut eth) = ether::Frame::parse(&mut buf[..len as usize]) {
            siprintln!(stim, "\nRx({})", eth.as_bytes().len());
            siprintln!(stim, "* {:?}", eth);

            let src_mac = eth.get_source();

            match eth.get_type() {
                ether::Type::Arp => {
                    if let Ok(arp) = arp::Packet::parse(eth.payload_mut()) {
                        match arp.downcast() {
                            Ok(mut arp) => {
                                siprintln!(stim, "** {:?}", arp);

                                if !arp.is_a_probe() {
                                    cache
                                        .insert(arp.get_spa(), arp.get_sha())
                                        .ok();
                                }

                                // are they asking for us?
                                if arp.get_oper() == arp::Operation::Request
                                    && arp.get_tpa() == IP
                                {
                                    // reply to the ARP request
                                    let tha = arp.get_sha();
                                    let tpa = arp.get_spa();

                                    arp.set_oper(arp::Operation::Reply);
                                    arp.set_sha(MAC);
                                    arp.set_spa(IP);
                                    arp.set_tha(tha);
                                    arp.set_tpa(tpa);
                                    siprintln!(stim, "\n** {:?}", arp);
                                    let arp_len = arp.len();

                                    // update the Ethernet header
                                    eth.set_destination(tha);
                                    eth.set_source(MAC);
                                    siprintln!(stim, "* {:?}", eth);

                                    siprintln!(
                                        stim,
                                        "Tx({})",
                                        eth.as_bytes().len()
                                    );
                                    enc28j60
                                        .transmit(eth.as_bytes())
                                        .ok()
                                        .unwrap();
                                }
                            }
                            Err(_arp) => {
                                // Not a Ethernet/IPv4 ARP packet
                                siprintln!(stim, "** {:?}", _arp);
                            }
                        }
                    } else {
                        // malformed ARP packet
                        siprintln!(stim, "Err(A)");
                    }
                }
                ether::Type::Ipv4 => {
                    if let Ok(mut ip) = ipv4::Packet::parse(eth.payload_mut())
                    {
                        siprintln!(stim, "** {:?}", ip);

                        let src_ip = ip.get_source();

                        if !src_mac.is_broadcast() {
                            cache.insert(src_ip, src_mac).ok();
                        }

                        match ip.get_protocol() {
                            ipv4::Protocol::Icmp => {
                                if let Ok(icmp) =
                                    icmp::Message::parse(ip.payload_mut())
                                {
                                    match icmp.downcast::<icmp::EchoRequest>()
                                    {
                                        Ok(request) => {
                                            // is an echo request
                                            siprintln!(
                                                stim, "*** {:?}", request
                                            );

                                            let src_mac = cache
                                                .get(&src_ip)
                                                .unwrap_or_else(
                                                    || unimplemented!(),
                                                );

                                            let _reply: icmp::Message<
                                                _,
                                                icmp::EchoReply,
                                                _,
                                            > = request.into();
                                            siprintln!(
                                                stim,
                                                "\n*** {:?}",
                                                _reply
                                            );

                                            // update the IP header
                                            let mut ip = ip.set_source(IP);
                                            ip.set_destination(src_ip);
                                            let _ip = ip.update_checksum();
                                            siprintln!(stim, "** {:?}", _ip);

                                            // update the Ethernet header
                                            eth.set_destination(*src_mac);
                                            eth.set_source(MAC);
                                            siprintln!(stim, "* {:?}", eth);

                                            led.toggle().unwrap();
                                            siprintln!(
                                                stim,
                                                "Tx({})",
                                                eth.as_bytes().len()
                                            );
                                            enc28j60
                                                .transmit(eth.as_bytes())
                                                .ok()
                                                .unwrap();
                                        }
                                        Err(_icmp) => {
                                            siprintln!(
                                                stim, "*** {:?}", _icmp
                                            );
                                        }
                                    }
                                } else {
                                    // Malformed ICMP packet
                                    siprintln!(stim, "Err(B)");
                                }
                            }
                            ipv4::Protocol::Udp => {
                                if let Ok(mut udp) =
                                    udp::Packet::parse(ip.payload_mut())
                                {
                                    siprintln!(stim, "*** {:?}", udp);

                                    if let Some(src_mac) = cache.get(&src_ip) {
                                        let src_port = udp.get_source();
                                        let dst_port = udp.get_destination();

                                        // update the UDP header
                                        udp.set_source(dst_port);
                                        udp.set_destination(src_port);
                                        udp.zero_checksum();
                                        siprintln!(stim, "\n*** {:?}", udp);

                                        // update the IP header
                                        let mut ip = ip.set_source(IP);
                                        ip.set_destination(src_ip);
                                        let ip = ip.update_checksum();
                                        let ip_len = ip.len();
                                        siprintln!(stim, "** {:?}", ip);

                                        // update the Ethernet header
                                        eth.set_destination(*src_mac);
                                        eth.set_source(MAC);
                                        siprintln!(stim, "* {:?}", eth);

                                        led.toggle().unwrap();
                                        siprintln!(
                                            stim,
                                            "Tx({})",
                                            eth.as_bytes().len()
                                        );
                                        enc28j60
                                            .transmit(eth.as_bytes())
                                            .ok()
                                            .unwrap();
                                    }
                                } else {
                                    // malformed UDP packet
                                    siprintln!(stim, "Err(C)");
                                }
                            }
                            _ => {}
                        }
                    } else {
                        // malformed IPv4 packet
                        siprintln!(stim, "Err(D)");
                    }
                }
                _ => {}
            }
        } else {
            // malformed Ethernet frame
            siprintln!(stim, "Err(E)");
        }
    }
}
