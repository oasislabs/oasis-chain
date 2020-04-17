//! Cryptographic primitives.

#![feature(test)]

extern crate byteorder;
extern crate ed25519_dalek;
#[macro_use]
extern crate failure;
extern crate rand;
extern crate rustc_hex;
extern crate serde;
extern crate serde_derive;
extern crate sha2;
extern crate untrusted;
extern crate zeroize;

#[macro_use]
pub mod bytes;
pub mod hash;
pub mod mrae;
pub mod signature;
