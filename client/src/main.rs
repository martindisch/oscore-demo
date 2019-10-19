#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate panic_semihosting;

use alloc_cortex_m::CortexMHeap;
use core::fmt::Write;
use cortex_m_rt::entry;
use embedded_hal::spi::{Mode, Phase, Polarity};
use stm32f4xx_hal::{
    prelude::*,
    serial::{config::Config, Serial},
    spi::Spi,
    stm32,
};
use util::{uprint, uprintln};
use w5500::{
    ArpResponses, ConnectionType, IntoUdpSocket, IpAddress, MacAddress,
    OnPingRequest, OnWakeOnLan, Socket, Udp, W5500,
};

use client::{
    coap::CoapHandler, edhoc::EdhocHandler, led::Leds, oscore::OscoreHandler,
};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

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
// Key ID of peer
const KID_PEER: [u8; 1] = [0xA3];

#[entry]
fn main() -> ! {
    /* Network configuration */
    let own_mac = MacAddress::new(0x20, 0x18, 0x03, 0x01, 0x00, 0x01);
    let own_ip = IpAddress::new(192, 168, 0, 98);
    let peer_ip = IpAddress::new(192, 168, 0, 99);
    let coap_port = 5683;

    // Initialize the allocator BEFORE you use it
    let start = cortex_m_rt::heap_start() as usize;
    let size = 10 * 1024 as usize;
    unsafe { ALLOCATOR.init(start, size) }

    let dp = stm32::Peripherals::take().expect("Failed taking dp");
    let rcc = dp.RCC.constrain();
    let gpiob = dp.GPIOB.split();
    let clocks = rcc.cfgr.freeze();

    // USART1
    let pin_tx = gpiob.pb6.into_alternate_af7();
    let pin_rx = gpiob.pb7.into_alternate_af7();
    let ser_conf = Config::default().baudrate(115_200.bps());
    let serial = Serial::usart1(dp.USART1, (pin_tx, pin_rx), ser_conf, clocks)
        .expect("Failed initializing USART1");
    let (mut tx, mut _rx) = serial.split();

    // LEDs
    let gpiod = dp.GPIOD.split();
    let pd12 = gpiod.pd12.into_push_pull_output();
    let pd13 = gpiod.pd13.into_push_pull_output();
    let pd14 = gpiod.pd14.into_push_pull_output();
    let pd15 = gpiod.pd15.into_push_pull_output();
    let mut leds = Leds::new(pd12, pd13, pd14, pd15);

    uprintln!(tx, "Basic initialization done");

    // SPI
    let gpioa = dp.GPIOA.split();
    let mut ncs = gpioa.pa15.into_push_pull_output();
    let sck = gpiob.pb3.into_alternate_af5();
    let miso = gpiob.pb4.into_alternate_af5();
    let mosi = gpiob.pb5.into_alternate_af5();
    let spi_mode = Mode {
        phase: Phase::CaptureOnFirstTransition,
        polarity: Polarity::IdleLow,
    };
    let mut spi = Spi::spi1(
        dp.SPI1,
        (sck, miso, mosi),
        spi_mode,
        1.mhz().into(),
        clocks,
    );

    // W5500
    let mut w5500 = W5500::with_initialisation(
        &mut ncs,
        &mut spi,
        OnWakeOnLan::Ignore,
        OnPingRequest::Respond,
        ConnectionType::Ethernet,
        ArpResponses::Cache,
    )
    .expect("Failed initializing W5500");

    let mut active =
        w5500.activate(&mut spi).expect("Failed activating W5500");
    active.set_mac(own_mac).expect("Failed setting MAC");
    active.set_ip(own_ip).expect("Failed setting IP");
    active
        .set_subnet(IpAddress::new(255, 255, 255, 0))
        .expect("Failed setting subnet");
    active
        .set_gateway(IpAddress::new(192, 168, 0, 1))
        .expect("Failed setting gateway");

    let socket0 = active
        .take_socket(Socket::Socket0)
        .expect("Failed taking socket");
    let udp_socket = (&mut active, socket0)
        .try_into_udp_server_socket(coap_port)
        .ok()
        .expect("Failed converting to UDP socket");
    let mut udp = (&mut active, &udp_socket);

    uprintln!(tx, "Complete initialization done");

    // Receive buffer
    let mut buffer = [0; 1522];

    // This is doing the EDHOC exchange
    let edhoc =
        EdhocHandler::new(AUTH_PRIV, AUTH_PUB, KID.to_vec(), AUTH_PEER);
    // This will be responsible for dealing with CoAP messages
    let coap = CoapHandler::new();
    // And finally this is the layer for OSCORE
    let mut oscore =
        OscoreHandler::new(edhoc, coap, KID.to_vec(), KID_PEER.to_vec());

    // Since we're the client, we need to initiate the whole interaction. We
    // start by sending the first EDHOC message, everything after that will be
    // handled in the main receiver loop like in the server. After that, the
    // client will also respond to any ARP and ICMP packets it receives.

    let req = oscore.go(&mut tx).unwrap();
    uprintln!(tx, "Sending the first EDHOC packet");
    uprintln!(tx, "Tx({})", req.len());
    udp.blocking_send(&peer_ip, coap_port, &req)
        .expect("Failed sending");

    loop {
        let (ip, port, len) = match udp.receive(&mut buffer) {
            Ok(Some(triple)) => triple,
            Ok(None) => {
                // Got nothing, continue to busy wait
                continue;
            }
            Err(err) => {
                uprintln!(tx, "Error receiving: {:?}", err);
                continue;
            }
        };

        uprintln!(tx, "\nRx({})", len);
        uprintln!(tx, "IP packet from {}", ip);

        // Handle the request
        let res = oscore.handle(&mut tx, &buffer[..len]);
        if res.is_none() {
            continue;
        }
        let res = res.unwrap();

        leds.spin();
        uprintln!(tx, "Responding with CoAP packet");
        uprintln!(tx, "Tx({})", res.len());
        udp.blocking_send(&ip, port, &res).expect("Failed sending");
    }
}

#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    panic!("We're officially OOM");
}
