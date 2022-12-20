// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from '@mysten/sui.js';

import type {
    SignaturePubkeyPair,
    Keypair,
    SuiAddress,
    Base64DataBuffer,
} from '@mysten/sui.js';

export type AccountType = 'derived' | 'imported';

export class Account {
    #keypair: Keypair;
    public readonly derivationPath: string | null;
    public readonly address: SuiAddress;

    constructor(
        type: 'derived',
        options: { derivationPath: string; keypair: Keypair }
    );
    constructor(type: 'imported', options: { keypair: Keypair });
    constructor(
        public readonly type: AccountType,
        options: { derivationPath?: string; keypair: Keypair }
    ) {
        this.derivationPath = options.derivationPath || null;
        this.#keypair = options.keypair;
        this.address = normalizeSuiAddress(
            this.#keypair.getPublicKey().toSuiAddress()
        );
    }

    exportKeypair() {
        return this.#keypair.export();
    }

    async sign(data: Base64DataBuffer): Promise<SignaturePubkeyPair> {
        return {
            signatureScheme: this.#keypair.getKeyScheme(),
            signature: this.#keypair.signData(data),
            pubKey: this.#keypair.getPublicKey(),
        };
    }

    toJSON() {
        return {
            type: this.type,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }
}
