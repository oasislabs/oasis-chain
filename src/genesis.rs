//! Genesis state.
use std::io::Cursor;

use ethcore::spec::Spec;
use lazy_static::lazy_static;

lazy_static! {
    /// Genesis spec.
    pub static ref SPEC: Spec = {
        let spec_json = include_str!("../resources/genesis.json");

        Spec::load(Cursor::new(spec_json)).expect("must have a valid genesis spec")
    };
}
