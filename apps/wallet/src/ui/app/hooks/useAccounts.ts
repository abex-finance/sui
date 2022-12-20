import { useMemo, useSyncExternalStore } from 'react';

import { SyncedStore } from '../helpers/SyncedStore';

import type { Account } from '_src/background/keyring/Account';

type AccountType = ReturnType<Account['toJSON']>;

export const accountsStore = new SyncedStore<AccountType[] | null>(null);

export function useAccounts(addressFilter: string): AccountType | null;
export function useAccounts(addressesFilter?: string[]): AccountType[];
export function useAccounts(addressesFilters?: string | string[]) {
    const accounts = useSyncExternalStore(
        accountsStore.subscribe,
        accountsStore.getSnapshot
    );

    return useMemo(() => {
        if (!accounts) {
            return null;
        }
        if (typeof addressesFilters === 'string') {
            return (
                accounts?.find(
                    (anAccount) => anAccount.address === addressesFilters
                ) || null
            );
        }
        if (!addressesFilters) {
            return accounts;
        }
        return accounts.filter((anAccount) =>
            addressesFilters.includes(anAccount.address)
        );
    }, [accounts, addressesFilters]);
}
