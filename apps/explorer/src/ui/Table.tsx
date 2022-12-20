// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useReactTable,
    getCoreRowModel,
    flexRender,
    type RowData,
    type TableOptions,
} from '@tanstack/react-table';

import { Placeholder } from './Placeholder';

interface TableProps<TData extends RowData>
    extends Omit<TableOptions<TData>, 'getCoreRowModel'> {
    isLoading?: boolean;
    loadingPlaceholders?: number;
}

const DEFAULT_LOADING_PLACEHOLDERS = 10;

export function Table<TData extends RowData>({
    isLoading,
    loadingPlaceholders,
    ...props
}: TableProps<TData>) {
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
                {isLoading
                    ? Array.from(
                          {
                              length:
                                  loadingPlaceholders ||
                                  DEFAULT_LOADING_PLACEHOLDERS,
                          },
                          (_, i) => (
                              <tr key={i}>
                                  {table
                                      .getVisibleFlatColumns()
                                      .map((column) => (
                                          <td
                                              key={column.id}
                                              className="px-0 py-2 text-bodySmall font-medium leading-none text-steel-darker"
                                          >
                                              <div className="pr-2">
                                                  <Placeholder />
                                              </div>
                                          </td>
                                      ))}
                              </tr>
                          )
                      )
                    : table.getRowModel().rows.map((row) => (
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
