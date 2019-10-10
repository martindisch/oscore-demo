//! Handling of EDHOC exchange.

use alloc::vec::Vec;
use core::fmt::Write;
use oscore::edhoc::{
    api,
    error::{OwnError, OwnOrPeerError},
    PartyU,
};
use stm32f4xx_hal::{serial::Tx, stm32::USART1};

use crate::{uprint, uprintln};

/// The state in which the EDHOC exchange is.
#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Init,
    WaitingForSecond,
    WaitingForAck,
    Complete,
}

/// Handles the EDHOC exchange.
pub struct EdhocHandler {
    auth_priv: [u8; 32],
    auth_pub: [u8; 32],
    kid: Vec<u8>,
    auth_peer: [u8; 32],
    state: State,
    msg2_receiver: Option<PartyU<api::Msg2Receiver>>,
    master_secret: Option<Vec<u8>>,
    master_salt: Option<Vec<u8>>,
}

impl EdhocHandler {
    /// Creates a new `EdhocHandler`.
    pub fn new(
        auth_priv: [u8; 32],
        auth_pub: [u8; 32],
        kid: Vec<u8>,
        auth_peer: [u8; 32],
    ) -> EdhocHandler {
        EdhocHandler {
            auth_priv,
            auth_pub,
            kid,
            auth_peer,
            state: State::Init,
            msg2_receiver: None,
            master_secret: None,
            master_salt: None,
        }
    }

    /// Returns the first EDHOC message.
    pub fn go(&mut self, tx: &mut Tx<USART1>) -> Option<Vec<u8>> {
        // "Generate" an ECDH key pair (this is hardcoded, but MUST be
        // ephemeral and generated randomly)
        let eph = [
            0xD4, 0xD8, 0x1A, 0xBA, 0xFA, 0xD9, 0x08, 0xA0, 0xCC, 0xEF, 0xEF,
            0x5A, 0xD6, 0xB0, 0x5D, 0x50, 0x27, 0x02, 0xF1, 0xC1, 0x6F, 0x23,
            0x2C, 0x25, 0x92, 0x93, 0x09, 0xAC, 0x44, 0x1B, 0x95, 0x8E,
        ];
        // Choose a connection identifier (also hardcoded, could be
        // chosen by user or generated randomly)
        let c_v = [0xC3].to_vec();

        // Initialize what we need to handle messages
        let msg1_sender = PartyU::new(
            c_v,
            eph,
            &self.auth_priv,
            &self.auth_pub,
            self.kid.clone(),
        );
        // type = 1 is the case in CoAP, where party U can correlate
        // message_1 and message_2 with the token.
        // If an error happens here, we just abort. No need to send a message,
        // since the protocol hasn't started yet.
        let (msg1_bytes, msg2_receiver) = msg1_sender
            .generate_message_1(1)
            .expect("Error generating message_1");
        self.msg2_receiver = Some(msg2_receiver);
        self.state = State::WaitingForSecond;

        uprintln!(tx, "Successfully built message_1");

        Some(msg1_bytes)
    }

    /// Handles an EDHOC message and returns the reply to send.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        msg: Vec<u8>,
    ) -> Option<Vec<u8>> {
        match self.state {
            State::Init => {
                uprintln!(tx, "Received EDHOC message, but we're not ready");
                None
            }
            State::WaitingForSecond => {
                uprintln!(
                    tx,
                    "Received an EDHOC message while waiting for message_2"
                );
                // Take out the receiver, which we know exists at this point
                let msg2_receiver = self.msg2_receiver.take().unwrap();

                // This is a case where we could receive an error message (just
                // abort then), or cause an error (send it to the peer)
                let (_v_kid, msg2_verifier) = match msg2_receiver
                    .extract_peer_kid(msg)
                {
                    Err(OwnOrPeerError::PeerError(s)) => {
                        panic!("Received error msg: {}", s)
                    }
                    Err(OwnOrPeerError::OwnError(b)) => {
                        uprintln!(tx, "Ran into an issue dealing with msg_2");
                        return Some(b);
                    }
                    Ok(val) => val,
                };
                let msg3_sender = match msg2_verifier
                    .verify_message_2(&self.auth_peer)
                {
                    Err(OwnError(b)) => {
                        uprintln!(tx, "Ran into an issue verifying message_2");
                        return Some(b);
                    }
                    Ok(val) => val,
                };
                let (msg3_bytes, master_secret, master_salt) =
                    match msg3_sender.generate_message_3() {
                        Err(OwnError(b)) => {
                            uprintln!(
                                tx,
                                "Ran into an issue generating message_3"
                            );
                            return Some(b);
                        }
                        Ok(val) => val,
                    };
                uprintln!(
                    tx,
                    "Successfully derived the master secret and salt\r\n\
                     {:?}\r\n\
                     {:?}",
                    master_secret,
                    master_salt
                );
                // Store the state and advance our progress
                self.master_secret = Some(master_secret);
                self.master_salt = Some(master_salt);
                self.state = State::WaitingForAck;
                uprintln!(tx, "Successfully built message_3");

                // Return message_3 to be sent
                Some(msg3_bytes)
            }
            State::WaitingForAck => {
                uprintln!(
                    tx,
                    "Received an EDHOC message while waiting for ACK"
                );
                self.state = State::Complete;

                // No response necessary
                None
            }
            State::Complete => {
                uprintln!(
                    tx,
                    "Received an EDHOC message, but we're already complete"
                );
                None
            }
        }
    }

    /// Returns the current state.
    pub fn get_state(&self) -> State {
        self.state
    }

    /// Returns the negotiated master secret & salt, resetting the EDHOC state.
    pub fn take_params(&mut self) -> Option<(Vec<u8>, Vec<u8>)> {
        if self.state == State::Complete && self.master_secret.is_some() {
            // Take and return the derived context
            Some((
                self.master_secret.take().unwrap(),
                self.master_salt.take().unwrap(),
            ))
        } else {
            None
        }
    }
}
