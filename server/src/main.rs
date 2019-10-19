#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate panic_semihosting;

use alloc_cortex_m::CortexMHeap;
use alt_stm32f30x_hal::{pac, prelude::*};
use core::fmt::Write;
use cortex_m_rt::entry;
use embedded_hal::spi::{Mode, Phase, Polarity};
use util::{uprint, uprintln};
use w5500::{
    ArpResponses, ConnectionType, IntoUdpSocket, IpAddress, MacAddress,
    OnPingRequest, OnWakeOnLan, Socket, Udp, W5500,
};

use server::{
    coap::CoapHandler, edhoc::EdhocHandler, led::Leds, oscore::OscoreHandler,
};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

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

#[entry]
fn main() -> ! {
    /* Network configuration */
    let own_mac = MacAddress::new(0x20, 0x18, 0x03, 0x01, 0x00, 0x00);
    let own_ip = IpAddress::new(192, 168, 0, 99);
    let coap_port = 5683;

    // Initialize the allocator BEFORE you use it
    let start = cortex_m_rt::heap_start() as usize;
    let size = 10 * 1024 as usize;
    unsafe { ALLOCATOR.init(start, size) }

    let dp = pac::Peripherals::take().expect("Failed taking dp");
    let mut rcc = dp.RCC.constrain();
    let mut flash = dp.FLASH.constrain();
    let gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let gpiob = dp.GPIOB.split(&mut rcc.ahb);
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    // USART1
    let serial =
        dp.USART1
            .serial((gpiob.pb6, gpiob.pb7), 115_200.bps(), clocks);
    let (mut tx, mut _rx) = serial.split();

    // LEDs
    let gpioe = dp.GPIOE.split(&mut rcc.ahb);
    let mut leds = Leds::new(gpioe);

    uprintln!(tx, "Basic initialization done");

    // SPI
    let mut ncs = gpioa.pa15.output().push_pull();
    let sck = gpiob.pb3;
    let miso = gpiob.pb4;
    let mosi = gpiob.pb5;
    let spi_mode = Mode {
        phase: Phase::CaptureOnFirstTransition,
        polarity: Polarity::IdleLow,
    };
    let mut spi = dp.SPI1.spi((sck, miso, mosi), spi_mode, 1.mhz(), clocks);

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
    let mut buffer = [0u8; 1522];

    // This is doing the EDHOC exchange
    let edhoc =
        EdhocHandler::new(AUTH_PRIV, AUTH_PUB, KID.to_vec(), AUTH_PEER);
    // This will be responsible for dealing with CoAP messages
    let coap = CoapHandler::new();
    // And finally this is the layer for OSCORE
    let mut oscore =
        OscoreHandler::new(edhoc, coap, KID.to_vec(), KID_PEER.to_vec());

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

        leds.spin().expect("Failed advancing led");
        uprintln!(tx, "Responding with CoAP packet");
        uprintln!(tx, "Tx({})", res.len());
        udp.blocking_send(&ip, port, &res).expect("Failed sending");
    }
}

#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    panic!("We're officially OOM");
}
