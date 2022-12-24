// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;

use anyhow::{anyhow, ensure};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;

use sui_adapter::adapter::resolve_and_type_check;
use sui_adapter::execution_mode::ExecutionMode;
use sui_json::{resolve_move_function_args, SuiJsonCallArg, SuiJsonValue};
use sui_json_rpc_types::GetRawObjectDataResponse;
use sui_json_rpc_types::SuiObjectInfo;
use sui_json_rpc_types::{RPCTransactionRequestParams, SuiData, SuiTypeTag};
use sui_types::base_types::{ObjectID, ObjectRef, ObjectType, SuiAddress};
use sui_types::coin::{Coin, LockedCoin};
use sui_types::error::SuiError;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, InputObjectKind, MoveCall, ObjectArg, SingleTransactionKind, TransactionData,
    TransactionKind, TransferObject,
};

use sui_types::governance::{
    ADD_DELEGATION_LOCKED_COIN_FUN_NAME, ADD_DELEGATION_MUL_COIN_FUN_NAME,
    SWITCH_DELEGATION_FUN_NAME, WITHDRAW_DELEGATION_FUN_NAME,
};
use sui_types::move_package::MovePackage;
use sui_types::object::{Object, Owner};
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::{
    coin, fp_ensure, parse_sui_struct_tag, SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

#[async_trait]
pub trait DataReader {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error>;

    async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error>;
}

#[derive(Clone)]
pub struct TransactionBuilder(pub Arc<dyn DataReader + Sync + Send>);

impl TransactionBuilder {
    async fn select_gas(
        &self,
        signer: SuiAddress,
        input_gas: Option<ObjectID>,
        budget: u64,
        input_objects: Vec<ObjectID>,
    ) -> Result<ObjectRef, anyhow::Error> {
        if let Some(gas) = input_gas {
            self.get_object_ref(gas).await
        } else {
            let objs = self.0.get_objects_owned_by_address(signer).await?;
            let gas_objs = objs
                .iter()
                .filter(|obj| obj.type_ == GasCoin::type_().to_string());

            for obj in gas_objs {
                let response = self.0.get_object(obj.object_id).await?;
                let obj = response.object()?;
                let gas: GasCoin = bcs::from_bytes(
                    &obj.data
                        .try_as_move()
                        .ok_or_else(|| anyhow!("Cannot parse move object to gas object"))?
                        .bcs_bytes,
                )?;
                if !input_objects.contains(&obj.id()) && gas.value() >= budget {
                    return Ok(obj.reference.to_object_ref());
                }
            }
            Err(anyhow!("Cannot find gas coin for signer address [{signer}] with amount sufficient for the budget [{budget}]."))
        }
    }

    pub async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> anyhow::Result<TransactionData> {
        let single_transfer = self.single_transfer_object(object_id, recipient).await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![object_id])
            .await?;
        Ok(TransactionData::new(
            TransactionKind::Single(single_transfer),
            signer,
            gas,
            gas_budget,
        ))
    }

    async fn single_transfer_object(
        &self,
        object_id: ObjectID,
        recipient: SuiAddress,
    ) -> Result<SingleTransactionKind, anyhow::Error> {
        Ok(SingleTransactionKind::TransferObject(TransferObject {
            recipient,
            object_ref: self.get_object_ref(object_id).await?,
        }))
    }

    pub async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> anyhow::Result<TransactionData> {
        let object = self.get_object_ref(sui_object_id).await?;
        Ok(TransactionData::new_transfer_sui(
            recipient, signer, amount, object, gas_budget,
        ))
    }

    pub async fn pay(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        if let Some(gas) = gas {
            if input_coins.contains(&gas) {
                return Err(anyhow!("Gas coin is in input coins of Pay transaction, use PaySui transaction instead!"));
            }
        }

        let handles: Vec<_> = input_coins
            .iter()
            .map(|id| self.get_object_ref(*id))
            .collect();
        let coin_refs = join_all(handles)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<ObjectRef>>>()?;
        let gas = self
            .select_gas(signer, gas, gas_budget, input_coins)
            .await?;
        let data =
            TransactionData::new_pay(signer, coin_refs, recipients, amounts, gas, gas_budget);
        Ok(data)
    }

    pub async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        fp_ensure!(!input_coins.is_empty(), SuiError::EmptyInputCoins.into());

        let handles: Vec<_> = input_coins
            .into_iter()
            .map(|id| self.get_object_ref(id))
            .collect();
        let coin_refs = join_all(handles)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<ObjectRef>>>()?;
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object_ref = coin_refs[0];
        Ok(TransactionData::new_pay_sui(
            signer,
            coin_refs,
            recipients,
            amounts,
            gas_object_ref,
            gas_budget,
        ))
    }

    pub async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        fp_ensure!(!input_coins.is_empty(), SuiError::EmptyInputCoins.into());

        let handles: Vec<_> = input_coins
            .into_iter()
            .map(|id| self.get_object_ref(id))
            .collect();

        let coin_refs = join_all(handles)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<ObjectRef>>>()?;
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object_ref = coin_refs[0];
        Ok(TransactionData::new_pay_all_sui(
            signer,
            coin_refs,
            recipient,
            gas_object_ref,
            gas_budget,
        ))
    }

    pub async fn move_call<Mode: ExecutionMode>(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<SuiTypeTag>,
        call_args: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let single_move_call = self
            .single_move_call::<Mode>(package_object_id, module, function, type_args, call_args)
            .await?;
        let input_objects = single_move_call
            .input_objects()?
            .iter()
            .flat_map(|obj| match obj {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();

        let gas = self
            .select_gas(signer, gas, gas_budget, input_objects)
            .await?;

        Ok(TransactionData::new(
            TransactionKind::Single(single_move_call),
            signer,
            gas,
            gas_budget,
        ))
    }

    async fn single_move_call<Mode: ExecutionMode>(
        &self,
        package_object_id: ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<SuiTypeTag>,
        call_args: Vec<SuiJsonValue>,
    ) -> anyhow::Result<SingleTransactionKind> {
        let package_ref = self.get_object_ref(package_object_id).await?;
        let module = Identifier::from_str(module)?;
        let function = Identifier::from_str(function)?;

        let type_args = type_args
            .into_iter()
            .map(|ty| ty.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let call_args = self
            .resolve_and_checks_json_args::<Mode>(
                package_object_id,
                &module,
                &function,
                &type_args,
                call_args,
            )
            .await?;

        Ok(SingleTransactionKind::Call(MoveCall {
            package: package_ref,
            module,
            function,
            type_arguments: type_args,
            arguments: call_args,
        }))
    }

    async fn get_object_arg(
        &self,
        id: ObjectID,
        objects: &mut BTreeMap<ObjectID, Object>,
    ) -> Result<ObjectArg, anyhow::Error> {
        let response = self.0.get_object(id).await?;
        let obj: Object = response.into_object()?.try_into()?;
        let obj_ref = obj.compute_object_reference();
        let owner = obj.owner;
        objects.insert(id, obj);
        Ok(match owner {
            Owner::Shared {
                initial_shared_version,
            } => ObjectArg::SharedObject {
                id,
                initial_shared_version,
            },
            Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                ObjectArg::ImmOrOwnedObject(obj_ref)
            }
        })
    }

    async fn resolve_and_checks_json_args<Mode: ExecutionMode>(
        &self,
        package_id: ObjectID,
        module: &Identifier,
        function: &Identifier,
        type_args: &[TypeTag],
        json_args: Vec<SuiJsonValue>,
    ) -> Result<Vec<CallArg>, anyhow::Error> {
        let package = self.0.get_object(package_id).await?.into_object()?;
        let package = package
            .data
            .try_as_package()
            .cloned()
            .ok_or_else(|| anyhow!("Object [{}] is not a move package.", package_id))?;
        let package: MovePackage = MovePackage::new(package.id, &package.module_map)?;

        let json_args = resolve_move_function_args(
            &package,
            module.clone(),
            function.clone(),
            type_args,
            json_args,
            Mode::allow_arbitrary_function_calls(),
        )?;
        let mut args = Vec::new();
        let mut objects = BTreeMap::new();
        for arg in json_args {
            args.push(match arg {
                SuiJsonCallArg::Object(id) => {
                    CallArg::Object(self.get_object_arg(id, &mut objects).await?)
                }
                SuiJsonCallArg::Pure(p) => CallArg::Pure(p),
                SuiJsonCallArg::ObjVec(v) => {
                    let mut object_ids = vec![];
                    for id in v {
                        object_ids.push(self.get_object_arg(id, &mut objects).await?);
                    }
                    CallArg::ObjVec(object_ids)
                }
            })
        }
        let compiled_module = package.deserialize_module(module)?;

        // TODO set the Mode from outside?
        resolve_and_type_check::<Mode>(
            &objects,
            &compiled_module,
            function,
            type_args,
            args.clone(),
            false,
        )?;

        Ok(args)
    }

    pub async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Vec<u8>>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let gas = self.select_gas(sender, gas, gas_budget, vec![]).await?;
        Ok(TransactionData::new_module(
            sender,
            gas,
            compiled_modules,
            gas_budget,
        ))
    }

    // TODO: consolidate this with Pay transactions
    pub async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let coin = self.0.get_object(coin_object_id).await?.into_object()?;
        let coin_object_ref = coin.reference.to_object_ref();
        let coin: Object = coin.try_into()?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![coin_object_id])
            .await?;

        Ok(TransactionData::new_move_call(
            signer,
            self.get_object_ref(SUI_FRAMEWORK_OBJECT_ID).await?,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_VEC_FUNC_NAME.to_owned(),
            type_args,
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_object_ref)),
                CallArg::Pure(bcs::to_bytes(&split_amounts)?),
            ],
            gas_budget,
        ))
    }

    // TODO: consolidate this with Pay transactions
    pub async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let coin = self.0.get_object(coin_object_id).await?.into_object()?;
        let coin_object_ref = coin.reference.to_object_ref();
        let coin: Object = coin.try_into()?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![coin_object_id])
            .await?;

        Ok(TransactionData::new_move_call(
            signer,
            self.get_object_ref(SUI_FRAMEWORK_OBJECT_ID).await?,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_N_FUNC_NAME.to_owned(),
            type_args,
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_object_ref)),
                CallArg::Pure(bcs::to_bytes(&split_count)?),
            ],
            gas_budget,
        ))
    }

    // TODO: consolidate this with Pay transactions
    pub async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let coin = self.0.get_object(primary_coin).await?.into_object()?;
        let primary_coin_ref = coin.reference.to_object_ref();
        let coin_to_merge_ref = self.get_object_ref(coin_to_merge).await?;
        let coin: Object = coin.try_into()?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![primary_coin, coin_to_merge])
            .await?;

        Ok(TransactionData::new_move_call(
            signer,
            self.get_object_ref(SUI_FRAMEWORK_OBJECT_ID).await?,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_JOIN_FUNC_NAME.to_owned(),
            type_args,
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(primary_coin_ref)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_merge_ref)),
            ],
            gas_budget,
        ))
    }

    pub async fn batch_transaction<Mode: ExecutionMode>(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        fp_ensure!(
            !single_transaction_params.is_empty(),
            SuiError::InvalidBatchTransaction {
                error: "Batch Transaction cannot be empty".to_owned(),
            }
            .into()
        );
        let mut tx_kinds = Vec::new();
        for param in single_transaction_params {
            let single_tx = match param {
                RPCTransactionRequestParams::TransferObjectRequestParams(param) => {
                    self.single_transfer_object(param.object_id, param.recipient)
                        .await?
                }
                RPCTransactionRequestParams::MoveCallRequestParams(param) => {
                    self.single_move_call::<Mode>(
                        param.package_object_id,
                        &param.module,
                        &param.function,
                        param.type_arguments,
                        param.arguments,
                    )
                    .await?
                }
            };
            tx_kinds.push(single_tx);
        }

        let all_inputs = tx_kinds
            .iter()
            .map(|tx| tx.input_objects())
            .collect::<Result<Vec<_>, _>>()?;
        let inputs = all_inputs
            .iter()
            .flatten()
            .flat_map(|obj| match obj {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();

        let gas = self.select_gas(signer, gas, gas_budget, inputs).await?;

        Ok(TransactionData::new(
            TransactionKind::Batch(tx_kinds),
            signer,
            gas,
            gas_budget,
        ))
    }

    pub async fn request_add_delegation(
        &self,
        signer: SuiAddress,
        mut coins: Vec<ObjectID>,
        amount: Option<u64>,
        validator: SuiAddress,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let gas = self
            .select_gas(signer, gas, gas_budget, coins.clone())
            .await?;

        let mut obj_vec = vec![];
        let coin = coins
            .pop()
            .ok_or_else(|| anyhow!("Coins input should contain at lease one coin object."))?;
        let (oref, coin_type) = self.get_object_ref_and_type(coin).await?;
        obj_vec.push(ObjectArg::ImmOrOwnedObject(oref));

        let ObjectType::Struct(type_) = &coin_type else{
            return Err(anyhow!("Provided object [{coin}] is not a move object."))
        };
        ensure!(
            Coin::is_coin(type_) || LockedCoin::is_locked_coin(type_),
            "Expecting either Coin<T> or LockedCoin<T> as input coin objects. Received [{type_}]"
        );

        for coin in coins {
            let (oref, type_) = self.get_object_ref_and_type(coin).await?;
            ensure!(
                type_ == coin_type,
                "All coins should be the same type, expecting {coin_type}, got {type_}."
            );
            obj_vec.push(ObjectArg::ImmOrOwnedObject(oref))
        }

        let function = if Coin::is_coin(type_) {
            ADD_DELEGATION_MUL_COIN_FUN_NAME
        } else {
            ADD_DELEGATION_LOCKED_COIN_FUN_NAME
        }
        .to_owned();

        Ok(TransactionData::new_move_call(
            signer,
            self.get_object_ref(SUI_FRAMEWORK_OBJECT_ID).await?,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            function,
            vec![],
            gas,
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                }),
                CallArg::ObjVec(obj_vec),
                CallArg::Pure(bcs::to_bytes(&amount)?),
                CallArg::Pure(bcs::to_bytes(&validator)?),
            ],
            gas_budget,
        ))
    }

    pub async fn request_withdraw_delegation(
        &self,
        signer: SuiAddress,
        delegation: ObjectID,
        staked_sui: ObjectID,
        principal_withdraw_amount: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let delegation = self.get_object_ref(delegation).await?;
        let staked_sui = self.get_object_ref(staked_sui).await?;
        let gas = self.select_gas(signer, gas, gas_budget, vec![]).await?;

        Ok(TransactionData::new_move_call(
            signer,
            self.get_object_ref(SUI_FRAMEWORK_OBJECT_ID).await?,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            WITHDRAW_DELEGATION_FUN_NAME.to_owned(),
            vec![],
            gas,
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(delegation)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(staked_sui)),
                CallArg::Pure(bcs::to_bytes(&principal_withdraw_amount)?),
            ],
            gas_budget,
        ))
    }

    pub async fn request_switch_delegation(
        &self,
        signer: SuiAddress,
        delegation: ObjectID,
        staked_sui: ObjectID,
        new_validator_address: SuiAddress,
        switch_pool_token_amount: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let delegation = self.get_object_ref(delegation).await?;
        let staked_sui = self.get_object_ref(staked_sui).await?;
        let gas = self.select_gas(signer, gas, gas_budget, vec![]).await?;

        Ok(TransactionData::new_move_call(
            signer,
            self.get_object_ref(SUI_FRAMEWORK_OBJECT_ID).await?,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            SWITCH_DELEGATION_FUN_NAME.to_owned(),
            vec![],
            gas,
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(delegation)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(staked_sui)),
                CallArg::Pure(bcs::to_bytes(&new_validator_address)?),
                CallArg::Pure(bcs::to_bytes(&switch_pool_token_amount)?),
            ],
            gas_budget,
        ))
    }

    // TODO: we should add retrial to reduce the transaction building error rate
    async fn get_object_ref(&self, object_id: ObjectID) -> anyhow::Result<ObjectRef> {
        self.get_object_ref_and_type(object_id)
            .await
            .map(|(oref, _)| oref)
    }

    async fn get_object_ref_and_type(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<(ObjectRef, ObjectType)> {
        let object = self.0.get_object(object_id).await?.into_object()?;

        let object_type = object
            .data
            .type_()
            .map(parse_sui_struct_tag)
            .transpose()?
            .map_or(ObjectType::Package, ObjectType::Struct);

        Ok((object.reference.to_object_ref(), object_type))
    }
}
