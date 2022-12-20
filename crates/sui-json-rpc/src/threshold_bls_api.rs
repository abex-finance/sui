// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::ThresholdBlsApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto_tbls::{mocked_dkg, tbls::ThresholdBls, types::ThresholdBls12381MinSig};
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_core_types::value::MoveStructLayout;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::SuiTBlsSignObjectCommitmentType::{ConsensusCommitted, FastPathCommitted};
use sui_json_rpc_types::{
    SuiCertifiedTransactionEffects, SuiTBlsSignObjectCommitmentType,
    SuiTBlsSignRandomnessObjectResponse,
};
use sui_open_rpc::Module;
use sui_types::base_types::{EpochId, ObjectID};
use sui_types::crypto::{construct_tbls_randomness_object_message, AuthoritySignInfoTrait};
use sui_types::messages::ConsensusTransactionKey;
use sui_types::object::Owner::Shared;
use sui_types::object::{ObjectRead, Owner};

pub struct ThresholdBlsApiImpl {
    state: Arc<AuthorityState>,
}

impl ThresholdBlsApiImpl {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }

    fn is_randomness_object(&self, _layout: &MoveStructLayout) -> bool {
        // TODO: complete.
        true
    }
    /// Return true if the given object is alive and committed according to my local view of (or in
    /// other words, that its creation was committed).
    ///
    /// We define "live and committed" to be:
    /// - a shared object that exists locally.
    /// - an owned object that exists locally and was last modified in previous epochs.
    /// - an owned object that exists locally and its previous transaction was processed by the
    ///   consensus.
    async fn is_object_alive_and_committed(&self, object_id: ObjectID) -> RpcResult<bool> {
        let obj_read = self
            .state
            .get_object_read(&object_id)
            .await
            .map_err(|e| anyhow!(e))?;
        let ObjectRead::Exists(_obj_ref, obj, layout) = obj_read else {
            return Ok(false); };

        if let Some(layout) = layout {
            if !self.is_randomness_object(&layout) {
                Err(anyhow!("Not a Randomness object"))?
            }
        }

        // a shared object that exists locally.
        if let Shared {
            initial_shared_version: _,
        } = obj.owner
        {
            return Ok(true);
        }

        // TODO: if the object was created/modified in previous epoch -> return true since if it
        //       was not committed, it would have been reverted on epoch change.
        //       - how to check in which epoch a tx digest was committed?
        //       - can we get it from the tx?

        // If the object was created/modified in the current epoch, check if that previous_tx
        // was committed.
        //
        // Note that the object may have been created earlier and even if the last transaction
        // has not been committed, previous ones may have. Since we deal here with non shared
        // objects, erring on the safe side is ok as it merely causes some delay for the user.
        // TODO: get the first version of the object and check its digest instead.


        // TODO: uncomment after tests pass
        // let was_processed = self.state.epoch_store().is_consensus_message_processed(
        //     &ConsensusTransactionKey::Certificate(obj.previous_transaction),
        // );
        // Ok(was_processed.map_err(|e| anyhow!(e))?)
        Ok(true)
    }

    async fn verify_effects_cert(
        &self,
        object_id: ObjectID,
        curr_epoch: EpochId,
        effects_cert: &SuiCertifiedTransactionEffects,
    ) -> RpcResult<bool> {
        if effects_cert.auth_sign_info.epoch != curr_epoch {
            Err(anyhow!("Inconsistent epochs"))?
        }
        // Check the certificate.
        let committee = self
            .state
            .committee_store()
            .get_committee(&curr_epoch)
            .map_err(|e| anyhow!(e))?
            .ok_or(anyhow!("Committee not available"))?; // Should never happen?

        // TODO: convert SuiTransactionEffects to TransactionEffects before the next line
        //
        // effects_cert
        //     .auth_sign_info
        //     .verify(&effects_cert.effects, &committee)
        //     .map_err(|e| anyhow!(e))?;

        // Check that the object was indeed in the effects.
        effects_cert
            .effects
            .created
            .iter()
            .chain(effects_cert.effects.mutated.iter())
            .find(|owned_obj_ref| owned_obj_ref.reference.object_id == object_id)
            .ok_or(anyhow!(
                "Object was not created/mutated in the provided transaction effects certificate"
            ))?;

        // TODO: Check it is the right type if it exists locally. However, since the tbls threshold
        // is f+1, it won't be hermetic.

        Ok(true)
    }
}

#[async_trait]
impl ThresholdBlsApiServer for ThresholdBlsApiImpl {
    /// Currently this is an insecure implementation since we do not have the DKG yet.
    /// All the checks below are done with the local view of the node. Later on those checks will be
    /// done by each of the validators (using their local view) when they are requested to sign
    /// on a randomness object.
    async fn tbls_sign_randomness_object(
        &self,
        object_id: ObjectID,
        object_creation_epoch: SuiTBlsSignObjectCommitmentType,
    ) -> RpcResult<SuiTBlsSignRandomnessObjectResponse> {
        let curr_epoch = self.state.epoch();
        match object_creation_epoch {
            ConsensusCommitted => {
                if !self.is_object_alive_and_committed(object_id).await? {
                    Err(anyhow!("Non committed object"))?
                }
            }
            FastPathCommitted(effects_cert) => {
                if !self
                    .verify_effects_cert(object_id, curr_epoch, &effects_cert)
                    .await?
                {
                    Err(anyhow!("Invalid effects certificate"))?
                }
            }
        };

        // Construct the message to be signed, as done in the Move code of the Randomness object.
        let msg = construct_tbls_randomness_object_message(curr_epoch, &object_id);

        // Sign the message using the mocked DKG keys.
        let (sk, _pk) = mocked_dkg::generate_full_key_pair(curr_epoch);
        let signature = ThresholdBls12381MinSig::sign(&sk, msg.as_slice());
        Ok(SuiTBlsSignRandomnessObjectResponse { signature })
    }
}

impl SuiRpcModule for ThresholdBlsApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::ThresholdBlsApiOpenRpc::module_doc()
    }
}
