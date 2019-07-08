//! Cryptographic primitives.

#![feature(test)]

extern crate byteorder;
extern crate failure;
extern crate rand;
extern crate ring;
extern crate rustc_hex;
extern crate serde;
extern crate serde_derive;
extern crate untrusted;

#[macro_use]
pub mod bytes;
pub mod hash;
pub mod mrae;
pub mod signature;
