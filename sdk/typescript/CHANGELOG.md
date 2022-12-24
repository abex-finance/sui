# @mysten/sui.js

## 0.21.0

### Minor Changes

- 4fb12ac6d: - removes `transfer` function from framework Coin
  - renames `newTransferTx` function from framework Coin to `newPayTransaction`. Also it's now a public method and without the need of signer so a dapp can use it
  - fixes edge cases with pay txs
- bb14ffdc5: Remove ImmediateReturn and WaitForTxCert from ExecuteTransactionRequestType
- 7d0f25b61: Add devInspectTransaction, which is similar to dryRunTransaction, but lets you call any Move function(including non-entry function) with arbitrary values.

## 0.20.0

### Minor Changes

- ea71d8216: Use intent signing if sui version > 0.18

### Patch Changes

- f93b59f3a: Fixed usage of named export for CommonJS module

## 0.19.0

### Minor Changes

- 6c1f81228: Remove signature from trasaction digest hash
- 519e11551: Allow keypairs to be exported
- b03bfaec2: Add getTransactionAuthSigners endpoint

### Patch Changes

- b8257cecb: add missing int types
- f9be28a42: Fix bug in Coin.isCoin
- 24987df35: Regex change for account index for supporting multiple accounts

## 0.18.0

### Minor Changes

- 66021884e: Send serialized signature with new executeTransactionSerializedSig endpoint
- 7a67d61e2: Unify TxnSerializer interface
- 2a0b8e85d: Add base58 encoding for TransactionDigest

### Patch Changes

- 45293b6ff: Replace `getCoinDenominationInfo` with `getCoinMetadata`
- 7a67d61e2: Add method in SignerWithProvider for calculating transaction digest

## 0.17.1

### Patch Changes

- 623505886: Fix callArg serialization bug in LocalTxnSerializer

## 0.17.0

### Minor Changes

- a9602e533: Remove deprecated events API
- db22728c1: \* adds dryRunTransaction support
  - adds getGasCostEstimation to the signer-with-provider that estimates the gas cost for a transaction
- 3b510d0fc: adds coin transfer method to framework that uses pay and paySui

## 0.16.0

### Minor Changes

- 01989d3d5: Remove usage of Buffer within SDK
- 5e20e6569: Event query pagination and merge all getEvents\* methods

### Patch Changes

- Updated dependencies [1a0968636]
  - @mysten/bcs@0.5.0

## 0.15.0

### Minor Changes

- c27933292: Update the type of the `endpoint` field in JsonRpcProvider from string to object

### Patch Changes

- c27933292: Add util function for faucet
- 90898d366: Support passing utf8 and ascii string
- c27933292: Add constants for default API endpoints
- Updated dependencies [1591726e8]
- Updated dependencies [1591726e8]
  - @mysten/bcs@0.4.0

## 0.14.0

### Minor Changes

- 8b4bea5e2: Remove gateway related APIs
- e45b188a8: Introduce PaySui and PayAllSui native transaction types to TS SDK.

### Patch Changes

- e86f8bc5e: Add `getRpcApiVersion` to Provider interface
- b4a8ee9bf: Support passing a vector of objects in LocalTxnBuilder
- ef3571dc8: Fix gas selection bug for a vector of objects
- cccfe9315: Add deserialization util method to LocalTxnDataSerializer
- 2dc594ef7: Introduce getCoinDenominationInfo, which returns denomination info of a coin, now only supporting SUI coin.
- 4f0c611ff: Protocol change to add 'initial shared version' to shared object references.

## 0.13.0

### Minor Changes

- 1d036d459: Transactions query pagination and merge all getTransactions\* methods
- b11b69262: Add gas selection to LocalTxnSerializer
- b11b69262: Deprecate Gateway related APIs
- b11b69262: Add rpcAPIVersion to JsonRpcProvider to support multiple RPC API Versions

## 0.12.0

### Minor Changes

- e0b173b9e: Standardize Ed25519KeyPair key derivation with SLIP10
- 059ede517: Flip the default value of `skipDataValidation` to true in order to mitigate the impact of breaking changes on applications. When there's a mismatch between the Typescript definitions and RPC response, the SDK now log a console warning instead of throwing an error.
- 03e6b552b: Add util function to get coin balances
- 4575c0a02: Fix type definition of SuiMoveNormalizedType
- ccf7f148d: Added generic signAndExecuteTransaction method to the SDK, which can be used with any supported type of transaction.

### Patch Changes

- e0b173b9e: Support Pay Transaction type in local transaction serializer

## 0.11.0

### Minor Changes

- d343b67e: Re-release packages

### Patch Changes

- Updated dependencies [d343b67e]
  - @mysten/bcs@0.3.0

## 0.11.0-pre

### Minor Changes

- 5de312c9: Add support for subscribing to events on RPC using "subscribeEvent".
- 5de312c9: Add support for Secp256k1 keypairs.

### Patch Changes

- c5e4851b: Updated build process from TSDX to tsup.
- a0fdb52e: Updated publish transactions to accept ArrayLike instead of Iterable.
- e2aa08e9: Fix missing built files for packages.
- Updated dependencies [c5e4851b]
- Updated dependencies [e2aa08e9]
  - @mysten/bcs@0.2.1
