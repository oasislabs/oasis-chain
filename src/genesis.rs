//! Genesis state.
use std::io::Cursor;

use ethcore::spec::Spec;
use ethereum_types::U256;
use lazy_static::lazy_static;

use crate::BLOCK_GAS_LIMIT;

lazy_static! {
    /// Block gas limit.
    pub static ref GAS_LIMIT: U256 = U256::from(BLOCK_GAS_LIMIT);

    /// Genesis spec.
    pub static ref SPEC: Spec = {
        let spec_json = include_str!("../resources/genesis.json");

        Spec::load(Cursor::new(spec_json)).expect("must have a valid genesis spec")
    };
}
