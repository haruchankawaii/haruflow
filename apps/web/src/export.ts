export function csvCell(value: unknown): string {
  const text = String(value ?? '');
  // Prevent spreadsheet formula execution when opening user supplied values.
  const safe = /^[=+\-@\t\r]/.test(text) ? `'${text}` : text;
  return `"${safe.replaceAll('"', '""')}"`;
}

export function downloadCsv(filename: string, headers: string[], rows: unknown[][]) {
  const csv = '\uFEFF' + [headers, ...rows].map(row => row.map(csvCell).join(',')).join('\r\n');
  const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8;' }));
  const anchor = document.createElement('a'); anchor.href = url; anchor.download = `${filename}.csv`; anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
