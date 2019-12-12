use oscore::oscore::SecurityContext;

const MASTER_SECRET: [u8; 16] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
    0x0D, 0x0E, 0x0F, 0x10,
];
const MASTER_SALT: [u8; 8] = [0x9E, 0x7C, 0xA9, 0x22, 0x23, 0x78, 0x63, 0x40];
const CLIENT_ID: [u8; 0] = [];
const SERVER_ID: [u8; 1] = [0x01];

const REQ_UNPROTECTED: [u8; 22] = [
    0x44, 0x01, 0x5D, 0x1F, 0x00, 0x00, 0x39, 0x74, 0x39, 0x6C, 0x6F, 0x63,
    0x61, 0x6C, 0x68, 0x6F, 0x73, 0x74, 0x83, 0x74, 0x76, 0x31,
];
const REQ_PROTECTED: [u8; 35] = [
    0x44, 0x02, 0x5D, 0x1F, 0x00, 0x00, 0x39, 0x74, 0x39, 0x6C, 0x6F, 0x63,
    0x61, 0x6C, 0x68, 0x6F, 0x73, 0x74, 0x62, 0x09, 0x14, 0xFF, 0x61, 0x2F,
    0x10, 0x92, 0xF1, 0x77, 0x6F, 0x1C, 0x16, 0x68, 0xB3, 0x82, 0x5E,
];

pub fn context_derivation(_: ()) {
    SecurityContext::new(
        MASTER_SECRET.to_vec(),
        MASTER_SALT.to_vec(),
        CLIENT_ID.to_vec(),
        SERVER_ID.to_vec(),
    )
    .unwrap();
}

pub fn protection_request_prepare() -> SecurityContext {
    SecurityContext::new(
        MASTER_SECRET.to_vec(),
        MASTER_SALT.to_vec(),
        CLIENT_ID.to_vec(),
        SERVER_ID.to_vec(),
    )
    .unwrap()
}

pub fn protection_request(mut req_context: SecurityContext) {
    req_context.protect_request(&REQ_UNPROTECTED).unwrap();
}

pub fn unprotection_request_prepare() -> SecurityContext {
    SecurityContext::new(
        MASTER_SECRET.to_vec(),
        MASTER_SALT.to_vec(),
        SERVER_ID.to_vec(),
        CLIENT_ID.to_vec(),
    )
    .unwrap()
}

pub fn unprotection_request(mut req_context: SecurityContext) {
    req_context.unprotect_request(&REQ_PROTECTED).unwrap();
}
