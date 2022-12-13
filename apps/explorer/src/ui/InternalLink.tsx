// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '../utils/stringUtils';

import { Link, type LinkProps } from '~/ui/Link';

type BaseProps = Omit<LinkProps, 'children'>;

export interface AddressLinkProps extends BaseProps {
    address: string;
    noTruncate?: boolean;
}

export interface ObjectLinkProps extends BaseProps {
    objectId: string;
    noTruncate?: boolean;
}

export function AddressLink({
    address,
    noTruncate,
    ...props
}: AddressLinkProps) {
    const truncatedAddress = noTruncate ? address : formatAddress(address);

    return (
        <Link
            variant="mono"
            to={`/address/${encodeURIComponent(address)}`}
            {...props}
        >
            {truncatedAddress}
        </Link>
    );
}

export function ObjectLink({
    objectId,
    noTruncate,
    ...props
}: ObjectLinkProps) {
    const truncatedObjectId = noTruncate ? objectId : formatAddress(objectId);
    return (
        <Link
            variant="mono"
            to={`/object/${encodeURIComponent(objectId)}`}
            {...props}
        >
            {truncatedObjectId}
        </Link>
    );
}
