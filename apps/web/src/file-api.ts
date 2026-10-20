import { ApiError } from "./api";

export async function fileRequest(path: string, options: RequestInit = {}) {
  const headers = new Headers(options.headers);
  if (options.method === "POST") headers.set("X-Haruflow-Request", "1");
  let response: Response;
  try {
    response = await fetch(`/api${path}`, {
      ...options,
      credentials: "same-origin",
      headers,
    });
  } catch {
    throw new ApiError(
      0,
      "connection_error",
      "Tidak dapat terhubung ke server. Periksa koneksi lalu coba lagi.",
    );
  }
  if (!response.ok) {
    if (response.status === 401)
      window.dispatchEvent(new Event("haruflow:unauthorized"));
    const data = await response.json().catch(() => null);
    throw new ApiError(
      response.status,
      data?.error?.code ?? "request_failed",
      data?.error?.message ?? "Permintaan belum berhasil. Silakan coba lagi.",
    );
  }
  return response;
}

export async function downloadFile(path: string, filename: string) {
  const response = await fileRequest(path);
  const url = URL.createObjectURL(await response.blob());
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
