// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SignatureScheme, SuiAddress } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';
import type { Account } from '_src/background/keyring/Account';

type MethodToPayloads = {
    create: {
        args: { password: string; importedEntropy?: string };
        return: void;
    };
    getEntropy: {
        args: string | undefined;
        return: string;
    };
    unlock: {
        args: { password: string };
        return: never;
    };
    walletStatusUpdate: {
        args: never;
        return: {
            isLocked: boolean;
            isInitialized: boolean;
            accounts: ReturnType<Account['toJSON']>[];
            activeAddress: string | null;
        };
    };
    lock: {
        args: never;
        return: never;
    };
    clear: {
        args: never;
        return: never;
    };
    appStatusUpdate: {
        args: { active: boolean };
        return: never;
    };
    setLockTimeout: {
        args: { timeout: number };
        return: never;
    };
    signData: {
        args: { data: string; address: SuiAddress };
        return: {
            signatureScheme: SignatureScheme;
            signature: string;
            pubKey: string;
        };
    };
};

export interface KeyringPayload<Method extends keyof MethodToPayloads>
    extends BasePayload {
    type: 'keyring';
    method: Method;
    args?: MethodToPayloads[Method]['args'];
    return?: MethodToPayloads[Method]['return'];
}

export function isKeyringPayload<Method extends keyof MethodToPayloads>(
    payload: Payload,
    method: Method
): payload is KeyringPayload<Method> {
    return (
        isBasePayload(payload) &&
        payload.type === 'keyring' &&
        'method' in payload &&
        payload['method'] === method
    );
}
