// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
pub use metrics::*;

use arc_swap::ArcSwap;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{AuthorityName, ObjectRef, TransactionDigest};
use sui_types::committee::{Committee, EpochId, StakeUnit};
use sui_types::quorum_driver_types::{QuorumDriverError, QuorumDriverResult};
use tap::TapFallible;
use tokio::time::{sleep_until, Instant};

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::Instrument;
use tracing::{debug, error, info, warn};

use crate::authority::authority_notify_read::{NotifyRead, Registration};
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use mysten_metrics::spawn_monitored_task;
use std::fmt::Write;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{QuorumDriverResponse, VerifiedCertificate, VerifiedTransaction};

#[cfg(test)]
mod tests;

const TASK_QUEUE_SIZE: usize = 10000;
const EFFECTS_QUEUE_SIZE: usize = 1000;
const TX_MAX_RETRY_TIMES: u8 = 10;

#[derive(Clone)]
pub struct QuorumDriverTask {
    pub transaction: VerifiedTransaction,
    pub tx_cert: Option<VerifiedCertificate>,
    pub retry_times: u8,
    pub next_retry_after: Instant,
}

impl Debug for QuorumDriverTask {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        write!(writer, "tx_digest={:?} ", self.transaction.digest())?;
        write!(writer, "has_tx_cert={} ", self.tx_cert.is_some())?;
        write!(writer, "retry_times={} ", self.retry_times)?;
        write!(writer, "next_retry_after={:?} ", self.next_retry_after)?;
        write!(f, "{}", writer)
    }
}

pub struct QuorumDriver<A> {
    validators: ArcSwap<AuthorityAggregator<A>>,
    task_sender: Sender<QuorumDriverTask>,
    effects_subscribe_sender: tokio::sync::broadcast::Sender<QuorumDriverResponse>,
    notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    metrics: Arc<QuorumDriverMetrics>,
    max_retry_times: u8,
}

impl<A> QuorumDriver<A> {
    pub(crate) fn new(
        validators: Arc<AuthorityAggregator<A>>,
        task_sender: Sender<QuorumDriverTask>,
        effects_subscribe_sender: tokio::sync::broadcast::Sender<QuorumDriverResponse>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        metrics: Arc<QuorumDriverMetrics>,
        max_retry_times: u8,
    ) -> Self {
        Self {
            validators: ArcSwap::from(validators),
            task_sender,
            effects_subscribe_sender,
            notifier,
            metrics,
            max_retry_times,
        }
    }

    pub fn authority_aggregator(&self) -> &ArcSwap<AuthorityAggregator<A>> {
        &self.validators
    }

    pub fn clone_committee(&self) -> Committee {
        self.validators.load().committee.clone()
    }

    pub fn current_epoch(&self) -> EpochId {
        self.validators.load().committee.epoch
    }

    async fn enqueue_task(&self, task: QuorumDriverTask) -> SuiResult<()> {
        self.task_sender
            .send(task.clone())
            .await
            .tap_err(|e| debug!(?task, "Failed to enqueue task: {:?}", e))
            .tap_ok(|_| {
                debug!(?task, "Enqueued task.");
                self.metrics.current_requests_in_flight.inc();
                self.metrics.total_enqueued.inc();
            })
            .map_err(|e| SuiError::QuorumDriverCommunicationError {
                error: e.to_string(),
            })
    }

    /// Enqueue the task again if it hasn't maxed out the total retry attempts.
    /// If it has, notify failure.
    /// Enqueuing happens only after the `next_retry_after`, if not, wait until that instant
    async fn enqueue_again_maybe(
        &self,
        transaction: VerifiedTransaction,
        tx_cert: Option<VerifiedCertificate>,
        old_retry_times: u8,
    ) -> SuiResult<()> {
        if old_retry_times >= self.max_retry_times {
            // max out the retry times, notify failure
            self.metrics.total_err_responses.inc();
            info!(tx_digest=?transaction.digest(), "Failed to reach finality after attempting for {} times", old_retry_times+1);
            self.notify(
                transaction.digest(),
                &Err(QuorumDriverError::FailedAfterMaximumAttempts {
                    total_attempts: old_retry_times + 1,
                }),
            );
            return Ok(());
        }
        let next_retry_after =
            Instant::now() + Duration::from_millis(200 * u64::pow(2, old_retry_times.into()));
        sleep_until(next_retry_after).await;
        self.enqueue_task(QuorumDriverTask {
            transaction,
            tx_cert,
            retry_times: old_retry_times + 1,
            next_retry_after,
        })
        .await
    }

    pub fn notify(&self, tx_digest: &TransactionDigest, response: &QuorumDriverResult) {
        // TODO: add metrics for error type
        self.notifier.notify(tx_digest, response);
    }
}

impl<A> QuorumDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub async fn submit_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        let tx_digest = transaction.digest();
        debug!(?tx_digest, "Received transaction execution request.");
        self.metrics.total_requests.inc();

        let ticket = self.notifier.register_one(tx_digest);
        self.enqueue_task(QuorumDriverTask {
            transaction,
            tx_cert: None,
            retry_times: 0,
            next_retry_after: Instant::now(),
        })
        .await?;
        Ok(ticket)
    }

    // Used when the it is called in a compoent holding the notifier, and a ticket is
    // already obtained prior to calling this function, for instance, TransactionOrchestrator
    pub async fn submit_transaction_no_ticket(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<()> {
        let tx_digest = transaction.digest();
        debug!(
            ?tx_digest,
            "Received transaction execution request, no ticket."
        );
        self.metrics.total_requests.inc();

        self.enqueue_task(QuorumDriverTask {
            transaction,
            tx_cert: None,
            retry_times: 0,
            next_retry_after: Instant::now(),
        })
        .await
    }

    pub async fn process_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<VerifiedCertificate> {
        let tx_digest = *transaction.digest();
        let result = self
            .validators
            .load()
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("quorum_driver_process_tx", ?tx_digest))
            .await;

        match result {
            Err(SuiError::QuorumFailedToProcessTransaction {
                good_stake,
                errors: _errors,
                conflicting_tx_digests,
            }) if !conflicting_tx_digests.is_empty() => {
                self.metrics
                    .total_err_process_tx_responses_with_nonzero_conflicting_transactions
                    .inc();
                debug!(
                    ?tx_digest,
                    ?good_stake,
                    "Observed {} conflicting transactions: {:?}",
                    conflicting_tx_digests.len(),
                    conflicting_tx_digests
                );
                let attempt_result = self
                    .attempt_conflicting_transactions_maybe(
                        good_stake,
                        &conflicting_tx_digests,
                        &tx_digest,
                    )
                    .await;
                match attempt_result {
                    Err(err) => {
                        debug!(
                            ?tx_digest,
                            "Encountered error in attempt_conflicting_transactions_maybe: {:?}",
                            err
                        );
                    }
                    Ok(None) => {
                        debug!(?tx_digest, "Did not retry any conflicting transactions");
                    }
                    Ok(Some((retried_tx_digest, success))) => {
                        self.metrics
                            .total_attempts_retrying_conflicting_transaction
                            .inc();
                        debug!(
                            ?tx_digest,
                            ?retried_tx_digest,
                            "Retried conflicting transaction success: {}",
                            success
                        );
                        if success {
                            self.metrics
                                .total_successful_attempts_retrying_conflicting_transaction
                                .inc();
                        }
                        return Err(
                            SuiError::QuorumFailedToProcessTransactionWithConflictingTransactions {
                                conflicting_txes: conflicting_tx_digests,
                                retried_tx_digest: Some(retried_tx_digest),
                                retried_tx_success: Some(success),
                            },
                        );
                    }
                }
                Err(
                    SuiError::QuorumFailedToProcessTransactionWithConflictingTransactions {
                        conflicting_txes: conflicting_tx_digests,
                        retried_tx_digest: None,
                        retried_tx_success: None,
                    },
                )
            }
            // TODO: we are particularly interested in what other errors could be returned
            // and use that to shape the retry strategy
            other => other,
        }
    }

    pub async fn process_certificate(
        &self,
        certificate: VerifiedCertificate,
    ) -> SuiResult<QuorumDriverResponse> {
        let effects = self
            .validators
            .load()
            .process_certificate(certificate.clone().into_inner())
            .instrument(tracing::debug_span!("process_cert", tx_digest = ?certificate.digest()))
            .await?;
        let tx_digest = *certificate.digest();
        let response = QuorumDriverResponse {
            tx_cert: certificate,
            effects_cert: effects,
        };
        // On fullnode we expect the send to always succeed because TransactionOrchestrator should be subscribing
        // to this queue all the time. However the if QuorumDriver is used elsewhere log may be noisy.
        if let Err(err) = self.effects_subscribe_sender.send(response.clone()) {
            warn!(?tx_digest, "No subscriber found for effects: {}", err);
        }
        Ok(response)
    }

    pub async fn update_validators(
        &self,
        new_validators: Arc<AuthorityAggregator<A>>,
    ) -> SuiResult {
        self.validators.store(new_validators);
        Ok(())
    }

    // TODO currently this function is not epoch-boundary-safe. We need to make it so.
    /// Returns Ok(None) if the no conflicting transaction was retried.
    /// Returns Ok(Some((tx_digest, true))) if one conflicting transaction was retried and succeeded,
    /// Some((tx_digest, false)) otherwise.
    /// Returns Error on unexpected errors.
    #[allow(clippy::type_complexity)]
    async fn attempt_conflicting_transactions_maybe(
        &self,
        good_stake: StakeUnit,
        conflicting_tx_digests: &BTreeMap<
            TransactionDigest,
            (Vec<(AuthorityName, ObjectRef)>, StakeUnit),
        >,
        original_tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<(TransactionDigest, bool)>> {
        let validity = self.validators.load().committee.validity_threshold();

        let mut conflicting_tx_digests = Vec::from_iter(conflicting_tx_digests.iter());
        conflicting_tx_digests.sort_by(|lhs, rhs| rhs.1 .1.cmp(&lhs.1 .1));
        if conflicting_tx_digests.is_empty() {
            error!("This path in unreachable with an empty conflicting_tx_digests.");
            return Ok(None);
        }

        // we checked emptiness above, safe to unwrap.
        let (tx_digest, (validators, total_stake)) = conflicting_tx_digests.get(0).unwrap();

        if good_stake >= validity && *total_stake >= validity {
            warn!(
                ?tx_digest,
                ?original_tx_digest,
                original_tx_stake = good_stake,
                tx_stake = *total_stake,
                "Equivocation detected: {:?}",
                validators
            );
            self.metrics.total_equivocation_detected.inc();
            return Ok(None);
        }

        // if we have >= f+1 good stake on the current transaction, no point in retrying conflicting ones
        if good_stake >= validity {
            return Ok(None);
        }

        // To be more conservative and try not to actually cause full equivocation,
        // we only retry a transaction when at least f+1 validators claims this tx locks objects
        if *total_stake < validity {
            return Ok(None);
        }

        info!(
            ?tx_digest,
            ?total_stake,
            ?original_tx_digest,
            "retrying conflicting tx."
        );
        let is_tx_executed = self
            .attempt_one_conflicting_transaction(
                tx_digest,
                original_tx_digest,
                validators
                    .iter()
                    .map(|(name, _obj_ref)| *name)
                    .collect::<BTreeSet<_>>(),
            )
            .await?;

        Ok(Some((**tx_digest, is_tx_executed)))
    }

    /// Returns Some(true) if the conflicting transaction is executed successfully
    /// (or already executed), or Some(false) if it did not.
    async fn attempt_one_conflicting_transaction(
        &self,
        tx_digest: &&TransactionDigest,
        original_tx_digest: &TransactionDigest,
        validators: BTreeSet<AuthorityName>,
    ) -> SuiResult<bool> {
        let (signed_transaction, certified_transaction) = self
            .validators
            .load()
            .handle_transaction_info_request_from_some_validators(
                tx_digest,
                &validators,
                Some(Duration::from_secs(10)),
            )
            .await?;

        // If we happen to find that a validator returns TransactionCertificate:
        if let Some(certified_transaction) = certified_transaction {
            self.metrics
                .total_times_conflicting_transaction_already_finalized_when_retrying
                .inc();
            // We still want to ask validators to execute this certificate in case this certificate is not
            // known to the rest of them (e.g. when *this* validator is bad).
            let result = self
                .validators
                .load()
                .process_certificate(certified_transaction.into_inner())
                .await
                .tap_ok(|_resp| {
                    debug!(
                        ?tx_digest,
                        ?original_tx_digest,
                        "Retry conflicting transaction certificate succeeded."
                    );
                })
                .tap_err(|err| {
                    debug!(
                        ?tx_digest,
                        ?original_tx_digest,
                        "Retry conflicting transaction certificate got an error: {:?}",
                        err
                    );
                });
            // We only try it once.
            return Ok(result.is_ok());
        }

        if let Some(signed_transaction) = signed_transaction {
            let verified_transaction = signed_transaction.into_unsigned();
            // Now ask validators to execute this transaction.
            let result = self
                .validators
                .load()
                .execute_transaction(&verified_transaction)
                .await
                .tap_ok(|_resp| {
                    debug!(
                        ?tx_digest,
                        ?original_tx_digest,
                        "Retry conflicting transaction succeeded."
                    );
                })
                .tap_err(|err| {
                    debug!(
                        ?tx_digest,
                        ?original_tx_digest,
                        "Retry conflicting transaction got an error: {:?}",
                        err
                    );
                });
            // We only try it once
            return Ok(result.is_ok());
        }

        // This is unreachable.
        let err_str = "handle_transaction_info_request_from_some_validators shouldn't return empty SignedTransaction and empty CertifiedTransaction";
        error!(err_str);
        Err(SuiError::from(err_str))
    }
}

pub struct QuorumDriverHandler<A> {
    quorum_driver: Arc<QuorumDriver<A>>,
    effects_subscriber: tokio::sync::broadcast::Receiver<QuorumDriverResponse>,
    quorum_driver_metrics: Arc<QuorumDriverMetrics>,
    _processor_handle: JoinHandle<()>,
}

impl<A> QuorumDriverHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub(crate) fn new_with_notify_read(
        validators: Arc<AuthorityAggregator<A>>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        metrics: Arc<QuorumDriverMetrics>,
    ) -> Self {
        Self::new_impl(validators, notifier, metrics)
    }

    pub fn new(validators: Arc<AuthorityAggregator<A>>, metrics: Arc<QuorumDriverMetrics>) -> Self {
        Self::new_impl(
            validators,
            Arc::new(NotifyRead::<TransactionDigest, QuorumDriverResult>::new()),
            metrics,
        )
    }

    /// Used in tests when smaller number of retries is desired
    pub fn new_with_max_retry_times(
        validators: Arc<AuthorityAggregator<A>>,
        metrics: Arc<QuorumDriverMetrics>,
        max_retry_times: u8,
    ) -> Self {
        Self::new_impl_with_max_retry_times(
            validators,
            Arc::new(NotifyRead::<TransactionDigest, QuorumDriverResult>::new()),
            metrics,
            max_retry_times,
        )
    }

    fn new_impl(
        validators: Arc<AuthorityAggregator<A>>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        metrics: Arc<QuorumDriverMetrics>,
    ) -> Self {
        Self::new_impl_with_max_retry_times(validators, notifier, metrics, TX_MAX_RETRY_TIMES)
    }

    fn new_impl_with_max_retry_times(
        validators: Arc<AuthorityAggregator<A>>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        metrics: Arc<QuorumDriverMetrics>,
        max_retry_times: u8,
    ) -> Self {
        let (task_tx, task_rx) = mpsc::channel::<QuorumDriverTask>(TASK_QUEUE_SIZE);
        let (subscriber_tx, subscriber_rx) =
            tokio::sync::broadcast::channel::<_>(EFFECTS_QUEUE_SIZE);
        let quorum_driver = Arc::new(QuorumDriver::new(
            validators,
            task_tx,
            subscriber_tx,
            notifier,
            metrics.clone(),
            max_retry_times,
        ));
        let metrics_clone = metrics.clone();
        let handle = {
            let quorum_driver_clone = quorum_driver.clone();
            spawn_monitored_task!(Self::task_queue_processor(
                quorum_driver_clone,
                task_rx,
                metrics_clone
            ))
        };
        Self {
            quorum_driver,
            _processor_handle: handle,
            effects_subscriber: subscriber_rx,
            quorum_driver_metrics: metrics,
        }
    }

    // Used when the it is called in a compoent holding the notifier, and a ticket is
    // already obtained prior to calling this function, for instance, TransactionOrchestrator
    pub async fn submit_transaction_no_ticket(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<()> {
        self.quorum_driver
            .submit_transaction_no_ticket(transaction)
            .await
    }

    pub async fn submit_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        self.quorum_driver.submit_transaction(transaction).await
    }

    /// Create a new QuorumDriverHandler based on the same AuthorityAggregator.
    /// Note: the new QuorumDriverHandler will have a new ArcSwap<AuthorityAggregator>
    /// that is NOT tied to the original one. So if there are multiple QuorumDriver(Handler)
    /// then all of them need to do reconfigs on their own.
    pub fn clone_new(&self) -> Self {
        let (task_sender, task_rx) = mpsc::channel::<QuorumDriverTask>(TASK_QUEUE_SIZE);
        let (effects_subscribe_sender, subscriber_rx) =
            tokio::sync::broadcast::channel::<_>(EFFECTS_QUEUE_SIZE);
        let validators = ArcSwap::new(self.quorum_driver.authority_aggregator().load_full());
        let quorum_driver = Arc::new(QuorumDriver {
            validators,
            task_sender,
            effects_subscribe_sender,
            notifier: Arc::new(NotifyRead::new()),
            metrics: self.quorum_driver_metrics.clone(),
            max_retry_times: self.quorum_driver.max_retry_times,
        });
        let metrics = self.quorum_driver_metrics.clone();
        let handle = {
            let quorum_driver_copy = quorum_driver.clone();
            spawn_monitored_task!(Self::task_queue_processor(
                quorum_driver_copy,
                task_rx,
                metrics,
            ))
        };
        Self {
            quorum_driver,
            _processor_handle: handle,
            effects_subscriber: subscriber_rx,
            quorum_driver_metrics: self.quorum_driver_metrics.clone(),
        }
    }

    pub fn clone_quorum_driver(&self) -> Arc<QuorumDriver<A>> {
        self.quorum_driver.clone()
    }

    pub fn subscribe_to_effects(&self) -> tokio::sync::broadcast::Receiver<QuorumDriverResponse> {
        self.effects_subscriber.resubscribe()
    }

    /// Process a QuorumDriverTask.
    /// The function has no return value - the corresponding actions of task result
    /// are performed in this call.
    async fn process_task(
        quorum_driver: Arc<QuorumDriver<A>>,
        task: QuorumDriverTask,
        metrics: Arc<QuorumDriverMetrics>,
    ) {
        debug!(?task, "Quorum Driver processing task");
        let QuorumDriverTask {
            transaction,
            tx_cert,
            retry_times: old_retry_times,
            ..
        } = task;
        let tx_digest = *transaction.digest();

        let tx_cert = match tx_cert {
            None => match quorum_driver.process_transaction(transaction.clone()).await {
                Ok(tx_cert) => {
                    debug!(?tx_digest, "Transaction processing succeeded");
                    tx_cert
                }
                Err(err) => {
                    if let Some(qd_error) = convert_to_quorum_driver_error_if_nonretryable(
                        err,
                        &tx_digest,
                        "forming tx cert",
                    ) {
                        // If non-retryable failure, this task reaches terminal state for now, notify waiter.
                        metrics.total_err_responses.inc();
                        quorum_driver.notify(&tx_digest, &Err(qd_error));
                        return;
                    } else {
                        // re-enqueue if retryable
                        spawn_monitored_task!(quorum_driver.enqueue_again_maybe(
                            transaction.clone(),
                            None,
                            old_retry_times
                        ));
                        return;
                    }
                }
            },
            Some(tx_cert) => tx_cert,
        };

        let response = match quorum_driver.process_certificate(tx_cert.clone()).await {
            Ok(QuorumDriverResponse {
                tx_cert,
                effects_cert,
            }) => {
                debug!(?tx_digest, "Certificate processing succeeded");
                QuorumDriverResponse {
                    tx_cert,
                    effects_cert,
                }
            }
            Err(err) => {
                // Note: so far there is no known error in effects-cert forming phase
                // that is considered permanent failure. So we always retry.
                debug!(?tx_digest, "Failed to get effects certificate: {}", err);
                spawn_monitored_task!(quorum_driver.enqueue_again_maybe(
                    transaction.clone(),
                    Some(tx_cert),
                    old_retry_times
                ));
                return;
            }
        };

        metrics.total_ok_responses.inc();
        quorum_driver.notify(&tx_digest, &Ok(response));
    }

    async fn task_queue_processor(
        quorum_driver: Arc<QuorumDriver<A>>,
        mut task_receiver: Receiver<QuorumDriverTask>,
        metrics: Arc<QuorumDriverMetrics>,
    ) {
        while let Some(task) = task_receiver.recv().await {
            // TODO check reconfig process here

            debug!(?task, "Dequeued task");
            if Instant::now()
                .checked_duration_since(task.next_retry_after)
                .is_none()
            {
                // Not ready for next attempt yet, re-enqueue
                let _ = quorum_driver.enqueue_task(task).await;
                continue;
            }
            metrics.current_requests_in_flight.dec();
            let qd = quorum_driver.clone();
            let metrics_clone = metrics.clone();
            spawn_monitored_task!(QuorumDriverHandler::process_task(qd, task, metrics_clone));
        }
    }
}

// TODO: categorize all possible SuiErrors
fn convert_to_quorum_driver_error_if_nonretryable(
    err: SuiError,
    tx_digest: &TransactionDigest,
    action: &'static str,
) -> Option<QuorumDriverError> {
    match &err {
        // TODO: rewrite the equivocation detection code to make it more deterministic
        SuiError::QuorumFailedToProcessTransactionWithConflictingTransactions {
            conflicting_txes,
            retried_tx_digest,
            retried_tx_success,
        } => {
            debug!(?tx_digest, "Got unretryable error when {action}: {err}");
            Some(QuorumDriverError::ObjectsDoubleUsed {
                conflicting_txes: conflicting_txes.clone(),
                retried_tx: *retried_tx_digest,
                retried_tx_success: *retried_tx_success,
            })
        }
        _ => {
            debug!(?tx_digest, "Got retryable error when {action}: {err}");
            None
        }
    }
}
