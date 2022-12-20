// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type GetObjectDataResponse } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from './useRpc';

export function useGetObject(
    objectId: string
): UseQueryResult<GetObjectDataResponse, unknown> {
    const rpc = useRpc();
    const response = useQuery(
        ['object', objectId],
        async () => {
            await new Promise(resolve => setTimeout(resolve, 2000));
            return rpc.getObject(objectId);
        },
        { enabled: !!objectId }
    );

    return response;
}
