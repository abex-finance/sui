// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    bcs,
    Ed25519Keypair,
    RawSigner,
    JsonRpcProvider,
    LocalTxnDataSerializer,
} from '@mysten/sui.js';
import fs from 'fs/promises';
import fetch from 'node-fetch';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'path';

import type { Keypair } from '@mysten/sui.js';

const TEST_NFTS = [
    'https://i.imgur.com/lWZIo5I.png',
    'https://i.imgur.com/3O3E7GI.png',
    'https://i.imgur.com/egSWiB3.png',
    'https://i.imgur.com/HJo669X.png',
];

const BASICS_EXAMPLE = path.join(
    __dirname,
    '../../../sui_programmability/examples/basics/'
);

const BASICS_BYTECODE = path.join(
    BASICS_EXAMPLE,
    './build/Basics/bytecode_modules'
);

const execAsync = promisify(execFile);

async function faucet(keypair: Keypair) {
    const res = await fetch('http://localhost:9123/faucet', {
        method: 'POST',
        headers: {
            'content-type': 'application/json',
        },
        body: JSON.stringify({
            recipient: keypair.getPublicKey().toSuiAddress(),
        }),
    });

    const data = await res.json();
    if (!res.ok || !data.ok) {
        throw new Error('Unable to invoke local faucet.');
    }
}

export async function createLocalnetTasks() {
    const keypair = Ed25519Keypair.generate();
    const provider = new JsonRpcProvider('http://localhost:5001');
    const signer = new RawSigner(
        keypair,
        provider,
        new LocalTxnDataSerializer(provider)
    );

    await faucet(keypair);
    // This fresh keypair should only have coin objects from faucet:
    const [gasObject] = await provider.getObjectsOwnedByAddress(
        keypair.getPublicKey().toSuiAddress()
    );

    async function publishPackage(publishModule = false) {
        await execAsync('sui', ['move', 'build', '--path', BASICS_EXAMPLE]);

        let compiledModules: number[][] = [];
        if (!publishModule) {
            const file = await fs.readFile(
                path.join(BASICS_BYTECODE, './counter.mv')
            );
            compiledModules.push([...file]);
        } else {
            const files = await fs.readdir(BASICS_BYTECODE, {
                withFileTypes: true,
            });

            for (const file of files.slice(0, 3)) {
                if (file.isFile()) {
                    const contents = await fs.readFile(
                        path.join(BASICS_BYTECODE, file.name)
                    );
                    compiledModules.push([...contents]);
                }
            }
        }

        return signer.publish({
            compiledModules,
            gasBudget: 30000,
            gasPayment: gasObject.objectId,
        });
    }

    async function mintNft(count: number = 1) {
        const transactions = [];
        for (let i = 0; i < count; i++) {
            const tx = await signer.executeMoveCall({
                packageObjectId: '0x2',
                module: 'devnet_nft',
                function: 'mint',
                typeArguments: [],
                arguments: [
                    { Pure: bcs.ser(bcs.STRING, 'Example NFT').toBytes() },
                    {
                        Pure: bcs
                            .ser(
                                bcs.STRING,
                                'This is an example NFT, which was minted to demonstrate the explorer'
                            )
                            .toBytes(),
                    },
                    {
                        Pure: bcs
                            .ser(bcs.STRING, TEST_NFTS[i % TEST_NFTS.length])
                            .toBytes(),
                    },
                ],
                gasPayment: gasObject.objectId,
                gasBudget: 30000,
            });
            transactions.push(tx);
        }
        return transactions;
    }

    return {
        publishPackage,
        mintNft,
    };
}
