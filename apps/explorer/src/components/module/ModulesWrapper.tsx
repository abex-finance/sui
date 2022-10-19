// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, memo } from 'react';
import { useSearchParams } from 'react-router-dom';

import Pagination from '../../components/pagination/Pagination';
import ModuleView from './ModuleView';

import styles from './ModuleView.module.css';

interface Props {
    id?: string;
    title: string;
    modules: [moduleName: string, code: string][];
}

const MODULES_PER_PAGE = 3;
// TODO: Include Pagination for now use viewMore and viewLess
function ModuleViewWrapper({ id, title, modules }: Props) {
    const [searchParams] = useSearchParams();
    const [modulesPageNumber, setModulesPageNumber] = useState(1);
    const totalModulesCount = modules.length;

    useEffect(() => {
        if (searchParams.get('module')) {
            const moduleIndex = modules.findIndex(([moduleName]) => {
                return moduleName === searchParams.get('module');
            });

            setModulesPageNumber(
                Math.floor(moduleIndex / MODULES_PER_PAGE) + 1
            );
        }
    }, [searchParams, modules]);

    const stats = {
        stats_text: 'total modules',
        count: totalModulesCount,
    };

    return (
        <div className={styles.modulewraper}>
            <h3 className={styles.title}>{title}</h3>
            <div className={styles.module}>
                {modules
                    .filter(
                        (_, index) =>
                            index >=
                                (modulesPageNumber - 1) * MODULES_PER_PAGE &&
                            index < modulesPageNumber * MODULES_PER_PAGE
                    )
                    .map(([name, code], idx) => (
                        <ModuleView key={idx} id={id} name={name} code={code} />
                    ))}
            </div>
            {totalModulesCount > MODULES_PER_PAGE && (
                <Pagination
                    totalItems={totalModulesCount}
                    itemsPerPage={MODULES_PER_PAGE}
                    currentPage={modulesPageNumber}
                    onPagiChangeFn={setModulesPageNumber}
                    stats={stats}
                />
            )}
        </div>
    );
}

export default memo(ModuleViewWrapper);
