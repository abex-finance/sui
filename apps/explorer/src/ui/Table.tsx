// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useReactTable,
    getCoreRowModel,
    flexRender,
    type RowData,
    type TableOptions,
} from '@tanstack/react-table';

interface TableProps<TData extends RowData>
    extends Omit<TableOptions<TData>, 'getCoreRowModel'> {}

export function Table<TData extends RowData>(props: TableProps<TData>) {
    const table = useReactTable({
        ...props,
        getCoreRowModel: getCoreRowModel(),
    });

    return (
        <table className="w-full text-left">
            <thead>
                {table.getHeaderGroups().map((headerGroup) => (
                    <tr key={headerGroup.id}>
                        {headerGroup.headers.map((header) => (
                            <th
                                key={header.id}
                                className="p-0 py-3 text-caption font-semibold uppercase tracking-wider text-steel-dark"
                            >
                                {header.isPlaceholder
                                    ? null
                                    : flexRender(
                                          header.column.columnDef.header,
                                          header.getContext()
                                      )}
                            </th>
                        ))}
                    </tr>
                ))}
            </thead>
            <tbody>
                {table.getRowModel().rows.map((row) => (
                    <tr key={row.id}>
                        {row.getVisibleCells().map((cell) => (
                            <td
                                key={cell.id}
                                className="px-0 py-2 text-bodySmall font-medium leading-none text-steel-darker"
                            >
                                {flexRender(
                                    cell.column.columnDef.cell,
                                    cell.getContext()
                                )}
                            </td>
                        ))}
                    </tr>
                ))}
            </tbody>
            <tfoot>
                {table.getFooterGroups().map((footerGroup) => (
                    <tr key={footerGroup.id}>
                        {footerGroup.headers.map((header) => (
                            <th key={header.id}>
                                {header.isPlaceholder
                                    ? null
                                    : flexRender(
                                          header.column.columnDef.footer,
                                          header.getContext()
                                      )}
                            </th>
                        ))}
                    </tr>
                ))}
            </tfoot>
        </table>
    );
}
