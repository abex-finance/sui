// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use consensus::{
    bullshark::Bullshark,
    dag::Dag,
    metrics::{ChannelMetrics, ConsensusMetrics},
    Consensus,
};

use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use executor::{get_restored_consensus_output, ExecutionState, Executor, SubscriberResult};
use fastcrypto::traits::{KeyPair as _, VerifyingKey};
use primary::{NetworkModel, NUM_SHUTDOWN_RECEIVERS, Primary, PrimaryChannelMetrics};
use prometheus::{IntGauge, Registry};
use std::sync::Arc;
pub use storage::NodeStorage;
use tokio::sync::oneshot;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info};
use types::{
    metered_channel, Certificate, ConditionalBroadcastReceiver, PreSubscribedBroadcastSender, Round,
};
use worker::{metrics::initialise_metrics, TransactionValidator, Worker};

pub mod execution_state;
pub mod metrics;

/// High level functions to spawn the primary and the workers.
pub struct Node;

impl Node {
    /// The default channel capacity.
    pub const CHANNEL_CAPACITY: usize = 1_000;

    /// Spawn a new primary. Optionally also spawn the consensus and a client executing transactions.
    pub async fn spawn_primary<State>(
        // The private-public key pair of this authority.
        keypair: KeyPair,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
        // The node's storage.
        store: &NodeStorage,
        // The configuration parameters.
        parameters: Parameters,
        // Whether to run consensus (and an executor client) or not.
        // If true, an internal consensus will be used, else an external consensus will be used.
        // If an external consensus will be used, then this bool will also ensure that the
        // corresponding gRPC server that is used for communication between narwhal and
        // external consensus is also spawned.
        internal_consensus: bool,
        // The state used by the client to execute transactions.
        execution_state: Arc<State>,
        // A prometheus exporter Registry to use for the metrics
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

        // These gauge is porcelain: do not modify it without also modifying `primary::metrics::PrimaryChannelMetrics::replace_registered_new_certificates_metric`
        // This hack avoids a cyclic dependency in the initialization of consensus and primary
        let new_certificates_counter = IntGauge::new(
            PrimaryChannelMetrics::NAME_NEW_CERTS,
            PrimaryChannelMetrics::DESC_NEW_CERTS,
        )
        .unwrap();
        let (tx_new_certificates, rx_new_certificates) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &new_certificates_counter);

        let committed_certificates_counter = IntGauge::new(
            PrimaryChannelMetrics::NAME_COMMITTED_CERTS,
            PrimaryChannelMetrics::DESC_COMMITTED_CERTS,
        )
        .unwrap();
        let (tx_committed_certificates, rx_committed_certificates) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &committed_certificates_counter);

        // Compute the public key of this authority.
        let name = keypair.public().clone();
        let mut handles = Vec::new();
        let (tx_executor_network, rx_executor_network) = oneshot::channel();
        let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
        let (dag, network_model) = if !internal_consensus {
            debug!("Consensus is disabled: the primary will run w/o Bullshark");
            let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
            let (handle, dag) = Dag::new(&committee.load(), rx_new_certificates, consensus_metrics);

            handles.push(handle);

            (Some(Arc::new(dag)), NetworkModel::Asynchronous)
        } else {
            let consensus_handles = Self::spawn_consensus(
                name.clone(),
                rx_executor_network,
                worker_cache.clone(),
                committee.clone(),
                store,
                parameters.clone(),
                execution_state,
                tx_shutdown.subscribe_n(3),
                rx_new_certificates,
                tx_committed_certificates.clone(),
                tx_consensus_round_updates,
                registry,
            )
            .await?;

            handles.extend(consensus_handles);

            (None, NetworkModel::PartiallySynchronous)
        };

        // Spawn the primary.
        let primary_handles = Primary::spawn(
            name.clone(),
            keypair,
            network_keypair,
            committee.clone(),
            worker_cache.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            tx_new_certificates,
            rx_committed_certificates,
            rx_consensus_round_updates,
            dag,
            network_model,
            tx_shutdown,
            tx_committed_certificates,
            registry,
            Some(tx_executor_network),
        );
        handles.extend(primary_handles);

        Ok(handles)
    }

    /// Spawn the consensus core and the client executing transactions.
    async fn spawn_consensus<State>(
        name: PublicKey,
        rx_executor_network: oneshot::Receiver<anemo::Network>,
        worker_cache: SharedWorkerCache,
        committee: SharedCommittee,
        store: &NodeStorage,
        parameters: Parameters,
        execution_state: State,
        mut shutdown_receivers: Vec<ConditionalBroadcastReceiver>,
        rx_new_certificates: metered_channel::Receiver<Certificate>,
        tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<Round>,
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        PublicKey: VerifyingKey,
        State: ExecutionState + Send + Sync + 'static,
    {
        let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
        let channel_metrics = ChannelMetrics::new(registry);

        let (tx_sequence, rx_sequence) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &channel_metrics.tx_sequence);

        // Check for any sub-dags that have been sent by consensus but were not processed by the executor.
        let restored_consensus_output = get_restored_consensus_output(
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            &execution_state,
        )
        .await?;

        let num_sub_dags = restored_consensus_output.len() as u64;
        if num_sub_dags > 0 {
            info!(
                "Consensus output on its way to the executor was restored for {num_sub_dags} sub-dags",
            );
        }
        consensus_metrics
            .recovered_consensus_output
            .inc_by(num_sub_dags as u64);

        // Spawn the consensus core who only sequences transactions.
        let ordering_engine = Bullshark::new(
            (**committee.load()).clone(),
            store.consensus_store.clone(),
            parameters.gc_depth,
            consensus_metrics.clone(),
        );
        let consensus_handles = Consensus::spawn(
            (**committee.load()).clone(),
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            shutdown_receivers.pop().unwrap(),
            rx_new_certificates,
            tx_committed_certificates,
            tx_consensus_round_updates,
            tx_sequence,
            ordering_engine,
            consensus_metrics.clone(),
            parameters.gc_depth,
        );

        // Spawn the client executing the transactions. It can also synchronize with the
        // subscriber handler if it missed some transactions.
        let executor_handles = Executor::spawn(
            name,
            rx_executor_network,
            worker_cache,
            (**committee.load()).clone(),
            execution_state,
            shutdown_receivers,
            rx_sequence,
            registry,
            restored_consensus_output,
        )?;

        Ok(executor_handles
            .into_iter()
            .chain(std::iter::once(consensus_handles))
            .collect())
    }

    /// Spawn a specified number of workers.
    pub fn spawn_workers(
        // The public key of this authority.
        primary_name: PublicKey,
        // The ids & keypairs of the workers to spawn.
        ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
        // The committee information.
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
        // The node's storage,
        store: &NodeStorage,
        // The configuration parameters.
        parameters: Parameters,
        // The transaction validator defining Tx acceptance,
        tx_validator: impl TransactionValidator,
        // The prometheus metrics Registry
        registry: &Registry,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        let metrics = initialise_metrics(registry);

        for (id, keypair) in ids_and_keypairs {
            let worker_handles = Worker::spawn(
                primary_name.clone(),
                keypair,
                id,
                committee.clone(),
                worker_cache.clone(),
                parameters.clone(),
                tx_validator.clone(),
                store.batch_store.clone(),
                metrics.clone(),
            );
            handles.extend(worker_handles);
        }
        handles
    }
}
