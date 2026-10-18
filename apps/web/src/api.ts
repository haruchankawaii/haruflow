export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) { super(message); }
}

export async function api<T>(path: string, options: { method?: string; body?: unknown; signal?: AbortSignal } = {}): Promise<T> {
  const method = options.method ?? 'GET';
  let response: Response;
  try {
    response = await fetch(`/api${path}`, {
      method, credentials: 'same-origin', signal: options.signal,
      headers: { 'Accept': 'application/json', ...(method !== 'GET' ? { 'Content-Type': 'application/json', 'X-Haruflow-Request': '1' } : {}) },
      ...(options.body !== undefined ? { body: JSON.stringify(options.body) } : {}),
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') throw error;
    throw new ApiError(0, 'connection_error', 'Tidak dapat terhubung ke server. Periksa koneksi lalu coba lagi.');
  }
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    if (response.status === 401 && !path.startsWith('/auth/') && !path.startsWith('/setup')) window.dispatchEvent(new Event('haruflow:unauthorized'));
    throw new ApiError(response.status, data?.error?.code ?? 'request_failed', data?.error?.message ?? 'Permintaan belum berhasil. Silakan coba lagi.');
  }
  return data as T;
}

export const errorMessage = (error: unknown) => error instanceof Error ? error.message : 'Terjadi kesalahan. Silakan coba lagi.';
