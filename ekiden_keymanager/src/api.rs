//! Key manager API.

use rand::{rngs::OsRng, Rng};
use serde_derive::{Deserialize, Serialize};

use ekiden_crypto::{impl_bytes, signature::Signature};

impl_bytes!(ContractId, 32, "A 256-bit contract identifier.");
impl_bytes!(PrivateKey, 32, "A private key.");
impl_bytes!(PublicKey, 32, "A public key.");
impl_bytes!(StateKey, 32, "A state key.");

/// Keys for a contract.
#[derive(Clone, Serialize, Deserialize)]
pub struct ContractKey {
    /// Input key pair (pk, sk)
    pub input_keypair: InputKeyPair,
    /// State encryption key
    pub state_key: StateKey,
    /// Checksum of the key manager state.
    #[serde(with = "serde_bytes")]
    pub checksum: Vec<u8>,
}

impl ContractKey {
    /// Generate a new random key (for testing).
    pub fn generate_mock() -> Self {
        let mut rng = OsRng::new().unwrap();
        let sk = x25519_dalek::StaticSecret::new(&mut rng);
        let pk = x25519_dalek::PublicKey::from(&sk);

        let mut state_key = StateKey::default();
        rng.fill(&mut state_key.0);

        ContractKey::new(
            PublicKey(*pk.as_bytes()),
            PrivateKey(sk.to_bytes()),
            state_key,
            vec![],
        )
    }

    /// Create a set of `ContractKey`.
    pub fn new(pk: PublicKey, sk: PrivateKey, k: StateKey, sum: Vec<u8>) -> Self {
        Self {
            input_keypair: InputKeyPair { pk, sk },
            state_key: k,
            checksum: sum,
        }
    }

    /// Create a set of `ContractKey` with only the public key.
    pub fn from_public_key(k: PublicKey, sum: Vec<u8>) -> Self {
        Self {
            input_keypair: InputKeyPair {
                pk: k,
                sk: PrivateKey::default(),
            },
            state_key: StateKey::default(),
            checksum: sum,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InputKeyPair {
    /// Pk
    pk: PublicKey,
    /// sk
    sk: PrivateKey,
}

impl InputKeyPair {
    pub fn new(pk: PublicKey, sk: PrivateKey) -> Self {
        Self { pk, sk }
    }

    pub fn get_pk(&self) -> PublicKey {
        self.pk
    }

    pub fn get_sk(&self) -> PrivateKey {
        self.sk
    }
}

/// Signed public key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedPublicKey {
    /// Public key.
    pub key: PublicKey,
    /// Checksum of the key manager state.
    #[serde(with = "serde_bytes")]
    pub checksum: Vec<u8>,
    /// Sign(sk, (key || checksum)) from the key manager.
    pub signature: Signature,
}
