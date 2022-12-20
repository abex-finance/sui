// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer, isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import { createColumnHelper } from '@tanstack/react-table';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';

import { useGetObject } from '~/hooks/useGetObject';
import { Banner } from '~/ui/Banner';
import { AddressLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Table } from '~/ui/Table';
import { Text } from '~/ui/Text';

const VALIDATORS_OBJECT_ID = '0x05';

const VALDIATOR_NAME = /^[A-Z-_.\s0-9]+$/i;

export type ValidatorMetadata = {
    type: '0x2::validator::ValidatorMetadata';
    fields: {
        name: string | number[];
        net_address: string;
        next_epoch_stake: number;
        pubkey_bytes: string;
        sui_address: string;
    };
};

export type Validator = {
    type: '0x2::validator::Validator';
    fields: {
        delegation: bigint;
        delegation_count: number;
        metadata: ValidatorMetadata;
        pending_delegation: bigint;
        pending_delegation_withdraw: bigint;
        pending_delegator_count: number;
        pending_delegator_withdraw_count: number;
        pending_stake: {
            type: '0x1::option::Option<0x2::balance::Balance<0x2::sui::SUI>>';
            fields: any;
        };
        pending_withdraw: bigint;
        stake_amount: bigint;
    };
};

export const STATE_DEFAULT: ValidatorState = {
    delegation_reward: 0,
    epoch: 0,
    id: { id: '', version: 0 },
    parameters: {
        type: '0x2::sui_system::SystemParameters',
        fields: {
            max_validator_candidate_count: 0,
            min_validator_stake: BigInt(0),
        },
    },
    storage_fund: 0,
    treasury_cap: {
        type: '',
        fields: {},
    },
    validators: {
        type: '0x2::validator_set::ValidatorSet',
        fields: {
            delegation_stake: BigInt(0),
            active_validators: [],
            next_epoch_validators: [],
            pending_removals: '',
            pending_validators: '',
            quorum_stake_threshold: BigInt(0),
            total_validator_stake: BigInt(0),
        },
    },
};

const textDecoder = new TextDecoder();

export type ObjFields = {
    type: string;
    fields: any;
};

export type SystemParams = {
    type: '0x2::sui_system::SystemParameters';
    fields: {
        max_validator_candidate_count: number;
        min_validator_stake: bigint;
    };
};

export type ValidatorState = {
    delegation_reward: number;
    epoch: number;
    id: { id: string; version: number };
    parameters: SystemParams;
    storage_fund: number;
    treasury_cap: ObjFields;
    validators: {
        type: '0x2::validator_set::ValidatorSet';
        fields: {
            delegation_stake: bigint;
            active_validators: Validator[];
            next_epoch_validators: Validator[];
            pending_removals: string;
            pending_validators: string;
            quorum_stake_threshold: bigint;
            total_validator_stake: bigint;
        };
    };
};

function StakeColumn(prop: { stake: bigint; stakePercent: number }) {
    return (
        <div className="flex items-end gap-0.5">
            <Text variant="bodySmall" color="steel-darker">
                {prop.stake.toString()}
            </Text>
            <Text variant="captionSmall" color="steel-dark">
                {prop.stakePercent.toFixed(2)}%
            </Text>
        </div>
    );
}

function getName(name: string | number[]) {
    if (Array.isArray(name)) {
        return String.fromCharCode(...name);
    }

    const decodedName = textDecoder.decode(
        new Base64DataBuffer(name).getData()
    );
    if (!VALDIATOR_NAME.test(decodedName)) {
        return name;
    } else {
        return decodedName;
    }
}

const columnHelper = createColumnHelper<Validator>();

const columns = [
    columnHelper.accessor('fields.metadata.fields.name', {
        header: 'Name',
        cell: (info) => getName(info.getValue()),
    }),
    columnHelper.accessor('fields.metadata.fields.sui_address', {
        header: 'Address',
        cell: (info) => <AddressLink address={info.getValue()} />,
    }),
    columnHelper.accessor('fields.stake_amount', {
        header: 'Stake',
        cell: (info) => (
            <StakeColumn stake={info.getValue()} stakePercent={0} />
        ),
    }),
];

export function TopValidatorsCard({ limit }: { limit?: number }) {
    const { data, isLoading, isError } =
        useGetObject(VALIDATORS_OBJECT_ID);

    const validatorData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const activeValidators =
        validatorData?.validators.fields.active_validators.slice(0, limit);

    if (isError || (!isLoading && !activeValidators?.length)) {
        return (
            <Banner variant="error" fullWidth>
                Validator data could not be loaded
            </Banner>
        );
    }

    return (
        <>
            <Table
                data={activeValidators ?? []}
                columns={columns}
                isLoading={isLoading}
                loadingPlaceholders={5}
            />

            {limit && (
                <div className="mt-3">
                    <Link to="/validators">
                        <div className="flex items-center gap-2">
                            More Validators <ArrowRight fill="currentColor" />
                        </div>
                    </Link>
                </div>
            )}
        </>
    );
}
