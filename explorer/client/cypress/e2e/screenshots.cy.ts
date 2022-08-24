// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:3000');
Cypress.config('viewportHeight', 980);
Cypress.config('viewportWidth', 1440);

describe('Screenshots', () => {
    describe('Package Details', () => {
        it('takes a screenshot', () => {
            cy.task('publishPackage', true).then((transaction) => {
                cy.visit(
                    `/objects/${transaction.effects.created?.[0]?.reference.objectId}`
                );
                cy.wait(1000);
                cy.screenshot('package-details');
            });
        });
    });

    describe('Object Details', () => {
        it('takes a screenshot', () => {
            cy.task('mintNft').then(([transaction]) => {
                cy.visit(
                    `/objects/${transaction.effects.created?.[0]?.reference.objectId}`
                );
                cy.wait(1000);
                cy.screenshot('object-details');
            });
        });
    });

    describe('Address Details', () => {
        it('takes a screenshot', () => {
            cy.task('mintNft', 4).then(([transaction]) => {
                cy.visit(`/addresses/${transaction.certificate.data.sender}`);
                cy.wait(1000);
                cy.screenshot('address-details');

                // Expand coin aggregation:
                cy.get(
                    '#groupCollection > div > div > div:first-child'
                ).click();
                cy.screenshot('address-details-expanded');
            });
        });
    });

    describe('Transaction Details', () => {
        describe('publish', () => {
            it('takes a screenshot', () => {
                cy.task('publishPackage').then((transaction) => {
                    cy.visit(
                        `/transactions/${encodeURIComponent(
                            transaction.certificate.transactionDigest
                        )}`
                    );
                    cy.wait(1000);
                    cy.screenshot('transaction-details-publish');
                });
            });
        });

        describe('events', () => {
            it('takes a screenshot', () => {
                cy.task('mintNft').then(([transaction]) => {
                    cy.visit(
                        `/transactions/${encodeURIComponent(
                            transaction.certificate.transactionDigest
                        )}`
                    );
                    cy.contains('Events').click()
                    cy.screenshot('transaction-details-events');
                });
            });
        });
    });

    describe('Home Page', () => {
        it('takes a screenshot', () => {
            cy.visit('/home-other');
            cy.wait(4000);
            cy.screenshot('home-page');
        });
    });
});
