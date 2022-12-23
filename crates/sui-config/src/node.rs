// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis;
use crate::p2p::P2pConfig;
use crate::Config;
use anyhow::Result;
use multiaddr::Multiaddr;
use narwhal_config::Parameters as ConsensusParameters;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_keys::keypair_file::{
    read_authority_keypair_from_file, read_keypair_from_file, read_network_keypair_from_file,
};
use sui_types::base_types::SuiAddress;
use sui_types::committee::StakeUnit;
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::NetworkKeyPair;
use sui_types::crypto::NetworkPublicKey;
use sui_types::crypto::PublicKey as AccountsPublicKey;
use sui_types::crypto::SuiKeyPair;
use sui_types::sui_serde::KeyPairBase64;

// Default max number of concurrent requests served
pub const DEFAULT_GRPC_CONCURRENCY_LIMIT: usize = 20000000000;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeConfig {
    /// The keypair that is used to deal with consensus transactions
    #[serde(default = "default_key_pair", skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<Arc<KeyPairBase64>>")]
    pub protocol_key_pair: Option<Arc<AuthorityKeyPair>>,
    /// The keypair that is used by the narwhal worker.
    #[serde(
        default = "default_worker_key_pair",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde_as(as = "Option<Arc<KeyPairBase64>>")]
    pub worker_key_pair: Option<Arc<NetworkKeyPair>>,
    /// The keypair that the authority uses to receive payments
    #[serde(
        default = "default_sui_key_pair",
        skip_serializing_if = "Option::is_none"
    )]
    pub account_key_pair: Option<Arc<SuiKeyPair>>,
    #[serde(
        default = "default_worker_key_pair",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde_as(as = "Option<Arc<KeyPairBase64>>")]
    pub network_key_pair: Option<Arc<NetworkKeyPair>>,

    /// File path to read the protocol_key_pair from.
    pub protocol_key_pair_path: PathBuf,
    /// File path to read the worker_key_pair from.
    pub worker_key_pair_path: PathBuf,
    /// File path to read the account_key_pair from.
    pub account_key_pair_path: PathBuf,
    /// File path to read the network_key_pair from.
    pub network_key_pair_path: PathBuf,

    pub db_path: PathBuf,
    #[serde(default = "default_grpc_address")]
    pub network_address: Multiaddr,
    #[serde(default = "default_json_rpc_address")]
    pub json_rpc_address: SocketAddr,

    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_admin_interface_port")]
    pub admin_interface_port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,

    #[serde(default)]
    pub enable_event_processing: bool,

    #[serde(default)]
    pub enable_checkpoint: bool,

    /// Number of checkpoints per epoch.
    /// Some means reconfiguration is enabled.
    /// None means reconfiguration is disabled.
    /// Exposing this in config to allow easier testing with shorter epoch.
    /// TODO: It will be removed down the road.
    #[serde(default = "default_checkpoints_per_epoch")]
    pub checkpoints_per_epoch: Option<u64>,

    #[serde(default)]
    pub grpc_load_shed: Option<bool>,

    #[serde(default = "default_concurrency_limit")]
    pub grpc_concurrency_limit: Option<usize>,

    #[serde(default)]
    pub p2p_config: P2pConfig,

    pub genesis: Genesis,
}

fn default_key_pair() -> Option<Arc<AuthorityKeyPair>> {
    None
}

fn default_worker_key_pair() -> Option<Arc<NetworkKeyPair>> {
    None
}

fn default_sui_key_pair() -> Option<Arc<SuiKeyPair>> {
    None
}

fn default_grpc_address() -> Multiaddr {
    use multiaddr::multiaddr;
    multiaddr!(Ip4([0, 0, 0, 0]), Tcp(8080u16))
}

fn default_metrics_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9184)
}

pub fn default_admin_interface_port() -> u16 {
    1337
}

pub fn default_json_rpc_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000)
}

pub fn default_websocket_address() -> Option<SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr};
    Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9001))
}

pub fn default_concurrency_limit() -> Option<usize> {
    Some(DEFAULT_GRPC_CONCURRENCY_LIMIT)
}

pub fn default_checkpoints_per_epoch() -> Option<u64> {
    None
}

pub fn bool_true() -> bool {
    true
}

impl Config for NodeConfig {}

impl NodeConfig {
    pub fn load_key_pairs(mut self) -> Result<Self> {
        let path = &self.protocol_key_pair_path;
        let kp = read_authority_keypair_from_file(path)
            .unwrap_or_else(|e| panic!("Invalid protocol key at path {:?} {:?}", path, e));
        self.protocol_key_pair = Some(Arc::new(kp));

        let path = &self.worker_key_pair_path;
        let kp = read_network_keypair_from_file(path)
            .unwrap_or_else(|e| panic!("Invalid worker key at path {:?} {:?}", path, e));
        self.worker_key_pair = Some(Arc::new(kp));

        let path = &self.network_key_pair_path;
        let kp = read_network_keypair_from_file(path)
            .unwrap_or_else(|e| panic!("Invalid network key at path {:?} {:?}", path, e));
        self.network_key_pair = Some(Arc::new(kp));

        let path = &self.account_key_pair_path;
        let kp = read_keypair_from_file(path)
            .unwrap_or_else(|e| panic!("Invalid account key at path {:?} {:?}", path, e));
        self.account_key_pair = Some(Arc::new(kp));
        Ok(self)
    }

    pub fn protocol_key_pair(&self) -> &AuthorityKeyPair {
        self.protocol_key_pair.as_ref().unwrap()
    }

    pub fn worker_key_pair(&self) -> &NetworkKeyPair {
        self.worker_key_pair.as_ref().unwrap()
    }

    pub fn network_key_pair(&self) -> &NetworkKeyPair {
        self.network_key_pair.as_ref().unwrap()
    }

    pub fn account_key_pair(&self) -> &SuiKeyPair {
        self.account_key_pair.as_ref().unwrap()
    }

    pub fn protocol_public_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key_pair().public().into()
    }

    pub fn sui_address(&self) -> SuiAddress {
        (&self.account_key_pair().public()).into()
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn consensus_config(&self) -> Option<&ConsensusConfig> {
        self.consensus_config.as_ref()
    }

    pub fn genesis(&self) -> Result<&genesis::Genesis> {
        self.genesis.genesis()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsensusConfig {
    pub address: Multiaddr,
    pub db_path: PathBuf,

    // Optional alternative address preferentially used by a primary to talk to its own worker.
    // For example, this could be used to connect to co-located workers over a private LAN address.
    pub internal_worker_address: Option<Multiaddr>,

    // Timeout to retry sending transaction to consensus internally.
    // Default to 60s.
    pub timeout_secs: Option<u64>,

    pub narwhal_config: ConsensusParameters,
}

impl ConsensusConfig {
    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn narwhal_config(&self) -> &ConsensusParameters {
        &self.narwhal_config
    }
}

/// Publicly known information about a validator
/// TODO read most of this from on-chain
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorInfo {
    pub name: String,
    pub account_key: AccountsPublicKey,
    pub protocol_key: AuthorityPublicKeyBytes,
    pub worker_key: NetworkPublicKey,
    pub network_key: NetworkPublicKey,
    pub stake: StakeUnit,
    pub delegation: StakeUnit,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub narwhal_primary_address: Multiaddr,
    pub narwhal_worker_address: Multiaddr,
}

impl ValidatorInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sui_address(&self) -> SuiAddress {
        self.account_key().into()
    }

    pub fn protocol_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key
    }

    pub fn worker_key(&self) -> &NetworkPublicKey {
        &self.worker_key
    }

    pub fn network_key(&self) -> &NetworkPublicKey {
        &self.network_key
    }

    pub fn account_key(&self) -> &AccountsPublicKey {
        &self.account_key
    }

    pub fn stake(&self) -> StakeUnit {
        self.stake
    }

    pub fn delegation(&self) -> StakeUnit {
        self.delegation
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    pub fn commission_rate(&self) -> u64 {
        self.commission_rate
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn narwhal_primary_address(&self) -> &Multiaddr {
        &self.narwhal_primary_address
    }

    pub fn narwhal_worker_address(&self) -> &Multiaddr {
        &self.narwhal_worker_address
    }

    pub fn p2p_address(&self) -> &Multiaddr {
        &self.p2p_address
    }

    pub fn voting_rights(validator_set: &[Self]) -> BTreeMap<AuthorityPublicKeyBytes, u64> {
        validator_set
            .iter()
            .map(|validator| {
                (
                    validator.protocol_key(),
                    validator.stake() + validator.delegation(),
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
pub struct Genesis {
    #[serde(flatten)]
    location: GenesisLocation,

    #[serde(skip)]
    genesis: once_cell::sync::OnceCell<genesis::Genesis>,
}

impl Genesis {
    pub fn new(genesis: genesis::Genesis) -> Self {
        Self {
            location: GenesisLocation::InPlace { genesis },
            genesis: Default::default(),
        }
    }

    pub fn new_from_file<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            location: GenesisLocation::File {
                genesis_file_location: path.into(),
            },
            genesis: Default::default(),
        }
    }

    pub fn genesis(&self) -> Result<&genesis::Genesis> {
        match &self.location {
            GenesisLocation::InPlace { genesis } => Ok(genesis),
            GenesisLocation::File {
                genesis_file_location,
            } => self
                .genesis
                .get_or_try_init(|| genesis::Genesis::load(genesis_file_location)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
#[serde(untagged)]
enum GenesisLocation {
    InPlace {
        genesis: genesis::Genesis,
    },
    File {
        #[serde(rename = "genesis-file-location")]
        genesis_file_location: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rand::{rngs::StdRng, SeedableRng};
    use sui_keys::keypair_file::{write_authority_keypair_to_file, write_keypair_to_file};
    use sui_types::crypto::{
        get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, NetworkKeyPair, SuiKeyPair,
    };

    use super::Genesis;
    use crate::{genesis, NodeConfig};

    #[test]
    fn serialize_genesis_config_from_file() {
        let g = Genesis::new_from_file("path/to/file");

        let s = serde_yaml::to_string(&g).unwrap();
        assert_eq!("---\ngenesis-file-location: path/to/file\n", s);
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn serialize_genesis_config_in_place() {
        let g = Genesis::new(genesis::Genesis::get_default_genesis());

        let mut s = serde_yaml::to_string(&g).unwrap();
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);

        // If both in-place and file location are provided, prefer the in-place variant
        s.push_str("\ngenesis-file-location: path/to/file");
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn load_genesis_config_from_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let genesis_config = Genesis::new_from_file(file.path());

        let genesis = genesis::Genesis::get_default_genesis();
        genesis.save(file.path()).unwrap();

        let loaded_genesis = genesis_config.genesis().unwrap();
        assert_eq!(&genesis, loaded_genesis);
    }

    #[test]
    fn fullnode_template() {
        const TEMPLATE: &str = include_str!("../data/fullnode-template.yaml");

        let _template: NodeConfig = serde_yaml::from_str(TEMPLATE).unwrap();
    }

    #[test]
    fn load_key_pairs_to_node_config() {
        let protocol_key_pair: AuthorityKeyPair =
            get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
        let worker_key_pair: NetworkKeyPair =
            get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
        let account_key_pair: SuiKeyPair =
            get_key_pair_from_rng::<AccountKeyPair, _>(&mut StdRng::from_seed([0; 32]))
                .1
                .into();
        let network_key_pair: NetworkKeyPair =
            get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;

        write_authority_keypair_to_file(&protocol_key_pair, &PathBuf::from("protocol.key"))
            .unwrap();
        write_keypair_to_file(
            &SuiKeyPair::Ed25519(worker_key_pair),
            &PathBuf::from("worker.key"),
        )
        .unwrap();
        write_keypair_to_file(
            &SuiKeyPair::Ed25519(network_key_pair),
            &PathBuf::from("network.key"),
        )
        .unwrap();
        write_keypair_to_file(&account_key_pair, &PathBuf::from("account.key")).unwrap();

        const TEMPLATE: &str = include_str!("../data/fullnode-template.yaml");
        let template: NodeConfig = serde_yaml::from_str(TEMPLATE).unwrap();
        assert!(template.protocol_key_pair.is_none());
        assert!(template.account_key_pair.is_none());
        assert!(template.worker_key_pair.is_none());
        assert!(template.network_key_pair.is_none());

        let res = template.load_key_pairs().unwrap();

        assert!(res.protocol_key_pair.is_some());
        assert!(res.account_key_pair.is_some());
        assert!(res.worker_key_pair.is_some());
        assert!(res.network_key_pair.is_some());
    }
}
