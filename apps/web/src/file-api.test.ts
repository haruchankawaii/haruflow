import { afterEach, expect, test } from "bun:test";
import { ApiError } from "./api";
import { fileRequest } from "./file-api";

const originalFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = originalFetch;
});

test("raw uploads preserve the body and same-origin request protection", async () => {
  let observed: RequestInit | undefined;
  let observedUrl: string | undefined;
  globalThis.fetch = (async (
    url: string | URL | Request,
    options?: RequestInit,
  ) => {
    observed = options;
    observedUrl = String(url);
    return new Response("{}", { status: 201 });
  }) as typeof fetch;
  const body = new Blob(["%PDF-1.7"], { type: "application/pdf" });
  await fileRequest("/documents/invoice/example/attachments", {
    method: "POST",
    headers: new Headers({ "Content-Type": "application/pdf" }),
    body,
  });
  expect(observedUrl).toBe("/api/documents/invoice/example/attachments");
  expect(observed?.credentials).toBe("same-origin");
  expect(observed?.body).toBe(body);
  expect(new Headers(observed?.headers).get("X-Haruflow-Request")).toBe("1");
  expect(new Headers(observed?.headers).get("Content-Type")).toBe(
    "application/pdf",
  );
});

test("file API surfaces structured errors and handles non-JSON failures", async () => {
  globalThis.fetch = (async () =>
    new Response(
      JSON.stringify({
        error: {
          code: "attachment_limit",
          message: "Batas lampiran tercapai.",
        },
      }),
      { status: 409 },
    )) as typeof fetch;
  try {
    await fileRequest("/attachments/example/download");
    throw new Error("Expected error");
  } catch (e) {
    expect(e).toBeInstanceOf(ApiError);
    expect((e as ApiError).code).toBe("attachment_limit");
    expect((e as ApiError).message).toBe("Batas lampiran tercapai.");
  }
  globalThis.fetch = (async () =>
    new Response("Bad gateway", { status: 502 })) as typeof fetch;
  await expect(fileRequest("/attachments/example/download")).rejects.toThrow(
    "Permintaan belum berhasil",
  );
  globalThis.fetch = (async () => {
    throw new TypeError("Network error");
  }) as typeof fetch;
  await expect(fileRequest("/attachments/example/download")).rejects.toThrow(
    "Tidak dapat terhubung",
  );
});
