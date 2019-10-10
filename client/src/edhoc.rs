//! Handling of EDHOC exchange.

use alloc::vec::Vec;
use core::fmt::Write;
use oscore::edhoc::{
    api,
    error::{OwnError, OwnOrPeerError},
    PartyV,
};
use stm32f4xx_hal::{serial::Tx, stm32::USART1};

use crate::{uprint, uprintln};

/// The state in which the EDHOC exchange is.
#[derive(Clone, Copy, PartialEq)]
pub enum State {
    WaitingForSecond,
    WaitingForFourth,
    Complete,
}

/// Handles the EDHOC exchange.
pub struct EdhocHandler {
    auth_priv: [u8; 32],
    auth_pub: [u8; 32],
    kid: Vec<u8>,
    auth_peer: [u8; 32],
    state: State,
    msg1_receiver: Option<PartyV<api::Msg1Receiver>>,
    msg3_receiver: Option<PartyV<api::Msg3Receiver>>,
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
            state: State::WaitingForSecond,
            msg1_receiver: None,
            msg3_receiver: None,
            master_secret: None,
            master_salt: None,
        }
    }

    /// Handles an EDHOC message and returns the reply to send.
    pub fn handle(
        &mut self,
        tx: &mut Tx<USART1>,
        msg: Vec<u8>,
    ) -> Option<Vec<u8>> {
        match self.state {
            State::WaitingForSecond => {
                uprintln!(
                    tx,
                    "Received an EDHOC message while waiting for message_1"
                );
                // Setup
                self.initialize();

                // Take out the receiver (which we know exists at this point)
                let msg1_receiver = self.msg1_receiver.take().unwrap();
                // Try to deal with message_1
                let msg2_sender = match msg1_receiver.handle_message_1(msg) {
                    Err(OwnError(b)) => {
                        uprintln!(
                            tx,
                            "Ran into an issue dealing with the message"
                        );
                        // Since there's a problem, send an error message
                        return Some(b);
                    }
                    Ok(val) => val,
                };
                // If that went well, produce message_2
                let (msg2_bytes, msg3_receiver) = match msg2_sender
                    .generate_message_2()
                {
                    Err(OwnError(b)) => {
                        uprintln!(tx, "Ran into an issue producing message_2");
                        return Some(b);
                    }
                    Ok(val) => val,
                };
                // Store the state and advance our progress
                self.msg3_receiver = Some(msg3_receiver);
                self.state = State::WaitingForFourth;
                uprintln!(tx, "Successfully built message_2");

                // Return message_2 to be sent
                Some(msg2_bytes)
            }
            State::WaitingForFourth => {
                uprintln!(
                    tx,
                    "Received an EDHOC message while waiting for message_3"
                );
                // Retrieve our state (which we know exists at this point)
                let msg3_receiver = self.msg3_receiver.take().unwrap();
                let (_u_kid, msg3_verifier) =
                    match msg3_receiver.extract_peer_kid(msg) {
                        Err(OwnOrPeerError::PeerError(s)) => {
                            uprintln!(tx, "Received an EDHOC error: {}", s);
                            self.state = State::WaitingForSecond;
                            return None;
                        }
                        Err(OwnOrPeerError::OwnError(b)) => {
                            uprintln!(
                                tx,
                                "Ran into an issue dealing with the message"
                            );
                            self.state = State::WaitingForSecond;
                            return Some(b);
                        }
                        Ok(val) => val,
                    };
                let (master_secret, master_salt) = match msg3_verifier
                    .verify_message_3(&self.auth_peer)
                {
                    Err(OwnError(b)) => {
                        uprintln!(tx, "Ran into an issue verifying message_3");
                        self.state = State::WaitingForSecond;
                        return Some(b);
                    }
                    Ok(val) => val,
                };

                self.state = State::Complete;
                uprintln!(
                    tx,
                    "Successfully derived the master secret and salt\r\n\
                     {:?}\r\n\
                     {:?}",
                    master_secret,
                    master_salt
                );
                self.master_secret = Some(master_secret);
                self.master_salt = Some(master_salt);

                // Return an empty message, which results in the final ACK to
                // the client
                Some(vec![])
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
        if self.state == State::Complete {
            // Reset the state
            self.state = State::WaitingForSecond;
            // Take and return the derived context
            Some((
                self.master_secret.take().unwrap(),
                self.master_salt.take().unwrap(),
            ))
        } else {
            None
        }
    }

    /// Initializes the handler to its original state.
    fn initialize(&mut self) {
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
        let msg1_receiver = PartyV::new(
            c_v,
            eph,
            &self.auth_priv,
            &self.auth_pub,
            self.kid.clone(),
        );
        self.msg1_receiver = Some(msg1_receiver);
    }
}
