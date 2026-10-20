import { describe, expect, test } from "bun:test";
import {
  CSV_MAX_BYTES,
  csvFileError,
  decodeCsv,
  importKinds,
} from "./imports-model";
describe("CSV input boundaries", () => {
  test("accepts UTF-8 BOM and quoted multiline bytes without rewriting CSV", () => {
    const csv = 'name,email\r\n"Kopi\nPagi",a@example.test';
    expect(decodeCsv(new TextEncoder().encode("\ufeff" + csv).buffer)).toBe(
      csv,
    );
    expect(() => decodeCsv(new Uint8Array([0xff, 0xfe]).buffer)).toThrow(
      "UTF-8",
    );
  });
  test("rejects empty and oversized files but includes exact limit", () => {
    expect(csvFileError({ name: "data.CSV", size: CSV_MAX_BYTES })).toBe("");
    expect(
      csvFileError({ name: "data.csv", size: CSV_MAX_BYTES + 1 }),
    ).toContain("256");
    expect(csvFileError({ name: "data.csv", size: 0 })).not.toBe("");
    expect(csvFileError({ name: "data.xlsx", size: 20 })).toContain(".csv");
  });
  test("only offers master data imports permitted for the role", () => {
    expect(importKinds("sales")).toEqual(["customers"]);
    expect(importKinds("warehouse")).toEqual(["products", "suppliers"]);
    expect(importKinds("viewer")).toEqual([]);
    expect(importKinds("finance")).toEqual([]);
    expect(importKinds("admin")).toHaveLength(3);
  });
});
