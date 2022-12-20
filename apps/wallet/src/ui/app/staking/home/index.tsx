// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Route, Routes } from 'react-router-dom';

import { FEATURES } from '../../experimentation/features';
import StakePage from '../stake';
import { ValidatorDetail } from '../validator-detail';
import { Validators } from '../validators';

export function Staking() {
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <Routes>
            <Route path="/*" element={<Validators />} />
            <Route path="/validator-details" element={<ValidatorDetail />} />
            {stakingEnabled ? (
                <Route path="/new" element={<StakePage />} />
            ) : null}
        </Routes>
    );
}
