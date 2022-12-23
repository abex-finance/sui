// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::node::default_checkpoints_per_epoch;
use crate::{
    genesis,
    genesis_config::{GenesisConfig, ValidatorConfigInfo, ValidatorGenesisInfo},
    p2p::P2pConfig,
    utils, ConsensusConfig, NetworkConfig, NodeConfig, ValidatorInfo, AUTHORITIES_DB_NAME,
    CONSENSUS_DB_NAME,
};
use fastcrypto::encoding::{Encoding, Hex};
use multiaddr::Multiaddr;
use rand::rngs::OsRng;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    AuthorityPublicKeyBytes, KeypairTraits, NetworkKeyPair, NetworkPublicKey, PublicKey,
    SuiKeyPair,
};

pub enum CommitteeConfig {
    Size(NonZeroUsize),
    Validators(Vec<ValidatorConfigInfo>),
}

enum ValidatorIpSelection {
    Localhost,
    Simulator,
}

pub struct ConfigBuilder<R = OsRng> {
    rng: Option<R>,
    config_directory: PathBuf,
    randomize_ports: bool,
    committee: Option<CommitteeConfig>,
    initial_accounts_config: Option<GenesisConfig>,
    with_swarm: bool,
    validator_ip_sel: ValidatorIpSelection,
}

impl ConfigBuilder {
    pub fn new<P: AsRef<Path>>(config_directory: P) -> Self {
        Self {
            rng: Some(OsRng),
            config_directory: config_directory.as_ref().into(),
            randomize_ports: true,
            committee: Some(CommitteeConfig::Size(NonZeroUsize::new(1).unwrap())),
            initial_accounts_config: None,
            with_swarm: false,
            // Set a sensible default here so that most tests can run with or without the
            // simulator.
            validator_ip_sel: if cfg!(msim) {
                ValidatorIpSelection::Simulator
            } else {
                ValidatorIpSelection::Localhost
            },
        }
    }
}

impl<R> ConfigBuilder<R> {
    pub fn randomize_ports(mut self, randomize_ports: bool) -> Self {
        self.randomize_ports = randomize_ports;
        self
    }

    pub fn with_swarm(mut self) -> Self {
        self.with_swarm = true;
        self
    }

    pub fn committee(mut self, committee: CommitteeConfig) -> Self {
        self.committee = Some(committee);
        self
    }

    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee = Some(CommitteeConfig::Size(committee_size));
        self
    }

    pub fn with_validators(mut self, validators: Vec<ValidatorConfigInfo>) -> Self {
        self.committee = Some(CommitteeConfig::Validators(validators));
        self
    }

    pub fn initial_accounts_config(mut self, initial_accounts_config: GenesisConfig) -> Self {
        self.initial_accounts_config = Some(initial_accounts_config);
        self
    }

    pub fn rng<N: rand::RngCore + rand::CryptoRng>(self, rng: N) -> ConfigBuilder<N> {
        ConfigBuilder {
            rng: Some(rng),
            config_directory: self.config_directory,
            randomize_ports: self.randomize_ports,
            committee: self.committee,
            initial_accounts_config: self.initial_accounts_config,
            with_swarm: self.with_swarm,
            validator_ip_sel: self.validator_ip_sel,
        }
    }
}

impl<R: rand::RngCore + rand::CryptoRng> ConfigBuilder<R> {
    //TODO right now we always randomize ports, we may want to have a default port configuration
    pub fn build(mut self) -> NetworkConfig {
        let committee = self.committee.take().unwrap();

        let mut rng = self.rng.take().unwrap();

        let validators = match committee {
            CommitteeConfig::Size(size) => (0..size.get())
                .map(|i| {
                    (
                        i,
                        (
                            get_key_pair_from_rng(&mut rng).1,
                            get_key_pair_from_rng(&mut rng).1,
                            get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng)
                                .1
                                .into(),
                            get_key_pair_from_rng(&mut rng).1,
                        ),
                    )
                })
                .map(
                    |(i, (key_pair, worker_key_pair, account_key_pair, network_key_pair)): (
                        _,
                        (AuthorityKeyPair, NetworkKeyPair, SuiKeyPair, NetworkKeyPair),
                    )| {
                        self.build_validator(
                            i,
                            key_pair,
                            worker_key_pair,
                            account_key_pair,
                            network_key_pair,
                        )
                    },
                )
                .collect::<Vec<_>>(),
            CommitteeConfig::Validators(v) => v,
        };

        self.build_with_validators(rng, validators)
    }

    fn build_validator(
        &self,
        index: usize,
        key_pair: AuthorityKeyPair,
        worker_key_pair: NetworkKeyPair,
        account_key_pair: SuiKeyPair,
        network_key_pair: NetworkKeyPair,
    ) -> ValidatorConfigInfo {
        match self.validator_ip_sel {
            ValidatorIpSelection::Localhost => ValidatorConfigInfo {
                genesis_info: ValidatorGenesisInfo::from_localhost_for_testing(
                    key_pair,
                    worker_key_pair,
                    account_key_pair,
                    network_key_pair,
                ),
                consensus_address: utils::new_tcp_network_address(),
                consensus_internal_worker_address: None,
            },

            ValidatorIpSelection::Simulator => {
                // we will probably never run this many validators in a sim
                let low_octet = index + 1;
                if low_octet > 255 {
                    todo!("smarter IP formatting required");
                }

                let ip = format!("10.10.0.{}", low_octet);
                let make_tcp_addr = |port: u16| -> Multiaddr {
                    format!("/ip4/{}/tcp/{}/http", ip, port).parse().unwrap()
                };

                ValidatorConfigInfo {
                    genesis_info: ValidatorGenesisInfo::from_base_ip(
                        key_pair,
                        worker_key_pair,
                        account_key_pair,
                        network_key_pair,
                        ip.clone(),
                        index,
                    ),
                    consensus_address: make_tcp_addr(4000 + index as u16),
                    consensus_internal_worker_address: None,
                }
            }
        }
    }

    fn build_with_validators(
        self,
        mut rng: R,
        validators: Vec<ValidatorConfigInfo>,
    ) -> NetworkConfig {
        let validator_set = validators
            .iter()
            .enumerate()
            .map(|(i, validator)| {
                let name = format!("validator-{i}");
                let protocol_key: AuthorityPublicKeyBytes =
                    validator.genesis_info.key_pair.public().into();
                let account_key: PublicKey = validator.genesis_info.account_key_pair.public();
                let network_key: NetworkPublicKey =
                    validator.genesis_info.network_key_pair.public().clone();
                let worker_key: NetworkPublicKey =
                    validator.genesis_info.worker_key_pair.public().clone();
                let stake = validator.genesis_info.stake;
                let network_address = validator.genesis_info.network_address.clone();
                let pop = generate_proof_of_possession(
                    &validator.genesis_info.key_pair,
                    (&validator.genesis_info.account_key_pair.public()).into(),
                );

                (
                    ValidatorInfo {
                        name,
                        protocol_key,
                        worker_key,
                        network_key,
                        account_key,
                        stake,
                        delegation: 0, // no delegation yet at genesis
                        gas_price: validator.genesis_info.gas_price,
                        commission_rate: validator.genesis_info.commission_rate,
                        network_address,
                        p2p_address: validator.genesis_info.p2p_address.clone(),
                        narwhal_primary_address: validator
                            .genesis_info
                            .narwhal_primary_address
                            .clone(),
                        narwhal_worker_address: validator
                            .genesis_info
                            .narwhal_worker_address
                            .clone(),
                    },
                    pop,
                )
            })
            .collect::<Vec<_>>();

        let initial_accounts_config = self
            .initial_accounts_config
            .unwrap_or_else(GenesisConfig::for_local_testing);
        let (account_keys, objects) = initial_accounts_config.generate_accounts(&mut rng).unwrap();

        let genesis = {
            let mut builder = genesis::Builder::new()
                .with_parameters(initial_accounts_config.parameters)
                .add_objects(objects);

            for (validator, proof_of_possession) in validator_set {
                builder = builder.add_validator(validator, proof_of_possession);
            }

            builder.build()
        };

        let validator_configs = validators
            .into_iter()
            .map(|validator| {
                let public_key: AuthorityPublicKeyBytes =
                    validator.genesis_info.key_pair.public().into();
                let db_path = self
                    .config_directory
                    .join(AUTHORITIES_DB_NAME)
                    .join(Hex::encode(public_key));
                let network_address = validator.genesis_info.network_address;
                let consensus_address = validator.consensus_address;
                let consensus_db_path = self
                    .config_directory
                    .join(CONSENSUS_DB_NAME)
                    .join(Hex::encode(public_key));
                let internal_worker_address = validator.consensus_internal_worker_address;
                let consensus_config = ConsensusConfig {
                    address: consensus_address,
                    db_path: consensus_db_path,
                    internal_worker_address,
                    timeout_secs: Some(60),
                    narwhal_config: Default::default(),
                };

                let p2p_config = P2pConfig {
                    listen_address: utils::udp_multiaddr_to_listen_address(
                        &validator.genesis_info.p2p_address,
                    )
                    .unwrap(),
                    external_address: Some(validator.genesis_info.p2p_address),
                    ..Default::default()
                };

                NodeConfig {
                    protocol_key_pair: Some(Arc::new(validator.genesis_info.key_pair)),
                    worker_key_pair: Some(Arc::new(validator.genesis_info.worker_key_pair)),
                    account_key_pair: Some(Arc::new(validator.genesis_info.account_key_pair)),
                    network_key_pair: Some(Arc::new(validator.genesis_info.network_key_pair)),

                    // The file paths are not needed here because they are loaded to the values above.
                    protocol_key_pair_path: PathBuf::from(""),
                    worker_key_pair_path: PathBuf::from(""),
                    account_key_pair_path: PathBuf::from(""),
                    network_key_pair_path: PathBuf::from(""),

                    db_path,
                    network_address,
                    metrics_address: utils::available_local_socket_address(),
                    // TODO: admin server is hard coded to start on 127.0.0.1 - we should probably
                    // provide the entire socket address here to avoid confusion.
                    admin_interface_port: utils::get_available_port("127.0.0.1"),
                    json_rpc_address: utils::available_local_socket_address(),
                    consensus_config: Some(consensus_config),
                    enable_event_processing: false,
                    enable_checkpoint: false,
                    checkpoints_per_epoch: default_checkpoints_per_epoch(),
                    genesis: crate::node::Genesis::new(genesis.clone()),
                    grpc_load_shed: initial_accounts_config.grpc_load_shed,
                    grpc_concurrency_limit: initial_accounts_config.grpc_concurrency_limit,
                    p2p_config,
                }
            })
            .collect();

        NetworkConfig {
            validator_configs,
            genesis,
            account_keys,
        }
    }
}
