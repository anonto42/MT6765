//! SMS PDU codec (3GPP TS 23.040 / 23.038), pure Rust: SMS-SUBMIT for sending and SMS-DELIVER for
//! receiving, GSM 7-bit (with the extension table) and UCS2 (Bangla and everything else), long
//! messages as concatenated parts (8-bit reference UDH). Text and numbers are private: nothing in
//! this module prints them.

mod codec;
mod deliver;
mod submit;
#[cfg(test)]
mod tests;

pub use codec::{
    gsm7_decode, gsm7_encode, hex_decode, hex_encode, pack7, ucs2_decode, ucs2_encode, unpack7,
    MAX_PARTS, PART_GSM7, PART_UCS2, SINGLE_GSM7, SINGLE_UCS2,
};
pub use deliver::{decode_deliver, Deliver};
pub use submit::{alphabet, encode_submit, Alphabet, EncodeError, Part};
