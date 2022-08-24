// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiTransactionResponse } from '@mysten/sui.js';

declare global {
    namespace Cypress {
        interface Chainable {
            task(name: 'publishPackage', arg?: boolean): Chainable<SuiTransactionResponse>;
            task(name: 'mintNft', arg?: number): Chainable<SuiTransactionResponse[]>;
        }
    }
}
