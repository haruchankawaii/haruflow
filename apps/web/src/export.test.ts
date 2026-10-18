import { expect, test } from 'bun:test';
import { csvCell } from './export';

test('CSV quotes, newlines and formula injection are escaped', () => {
  expect(csvCell('Toko "Haru"\nJakarta')).toBe('"Toko ""Haru""\nJakarta"');
  expect(csvCell('=HYPERLINK("https://example.com")')).toBe('"\'=HYPERLINK(""https://example.com"")"');
  expect(csvCell(null)).toBe('""');
});
