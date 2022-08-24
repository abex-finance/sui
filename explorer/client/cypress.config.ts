// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'cypress';

import { createLocalnetTasks } from './cypress/localnet';

export default defineConfig({
    e2e: {
        baseUrl: 'http://localhost:8080',
        async setupNodeEvents(on) {
            // TODO: This approach only invokes the faucet once, even on incremental runs. I tried
            // to make this a `before:spec` call, but that doesn't work when running tests interactively.
            // I think we'll probably need a task dedicated to invoking faucet, or something similar so that
            // we can use new key-pairs and avoid running out of gas for long interactive runs.
            on('task', await createLocalnetTasks());
        },
    },
    component: {
        devServer: {
            framework: 'create-react-app',
            bundler: 'webpack',
        },
    },
});
