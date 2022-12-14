// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import nacl from 'tweetnacl';
import { describe, it, expect } from 'vitest';
import { Base64DataBuffer, Ed25519Keypair } from '../../../src';

const VALID_SECRET_KEY =
  'QiWNzaFM8RHGAriXG4zIQ+keRsqQUVHAJ0SmsBfmkxY=';
const INVALID_SECRET_KEY =
  'QiWNzaFM8RHGAriXG4zIQ+keRsqQUVHAJ0SmsBfmkxbMYjMuNLstXNafYO+7KjbLkWx+tFgwHqNmNsTbsBK9iA==';
const TEST_MNEMONIC =
  'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss';

describe('ed25519-keypair', () => {
  it('new keypair', () => {
    const keypair = new Ed25519Keypair();
    expect(keypair.getPublicKey().toBytes().length).toBe(32);
    expect(2).toEqual(2);
  });

  it('create keypair from secret key', () => {
    const secretKey = fromB64(VALID_SECRET_KEY);
    const keypair = Ed25519Keypair.fromSecretKey(secretKey);
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'zGIzLjS7LVzWn2Dvuyo2y5FsfrRYMB6jZjbE27ASvYg='
    );
  });

  it('creating keypair from invalid secret key throws error', () => {
    const secretKey = fromB64(INVALID_SECRET_KEY);
    expect(() => {
      Ed25519Keypair.fromSecretKey(secretKey);
    }).toThrow('Wrong seed size. Expected 32 bytes, got 64.');
  });

  it('signature of data is valid', () => {
    const keypair = new Ed25519Keypair();
    const signData = new Base64DataBuffer(
      new TextEncoder().encode('hello world')
    );
    const signature = keypair.signData(signData);
    const isValid = nacl.sign.detached.verify(
      signData.getData(),
      signature.getData(),
      keypair.getPublicKey().toBytes()
    );
    expect(isValid).toBeTruthy();
  });

  it('derive ed25519 keypair from path and mnemonics', () => {
    // Test case generated against rust: /sui/crates/sui/src/unit_tests/keytool_tests.rs#L149
    const keypair = Ed25519Keypair.deriveKeypair(TEST_MNEMONIC);
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'aFstb5h4TddjJJryHJL1iMob6AxAqYxVv3yRt05aweI='
    );
    expect(keypair.getPublicKey().toSuiAddress()).toEqual(
      '1a4623343cd42be47d67314fce0ad042f3c82685'
    );
  });

  it('incorrect coin type node for ed25519 derivation path', () => {
    expect(() => {
      Ed25519Keypair.deriveKeypair(`m/44'/0'/0'/0'/0'`, TEST_MNEMONIC);
    }).toThrow('Invalid derivation path');
  });

  it('incorrect purpose node for ed25519 derivation path', () => {
    expect(() => {
      Ed25519Keypair.deriveKeypair(`m/54'/784'/0'/0'/0'`, TEST_MNEMONIC);
    }).toThrow('Invalid derivation path');
  });

  it('invalid mnemonics to derive ed25519 keypair', () => {
    expect(() => {
      Ed25519Keypair.deriveKeypair('aaa');
    }).toThrow('Invalid mnemonic');
  });
});
