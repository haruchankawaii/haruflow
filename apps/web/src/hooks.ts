import { useCallback, useEffect, useState } from 'react';
import { api, errorMessage } from './api';

export function useResource<T>(path: string, revision = 0) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const reload = useCallback(() => setRetry(value => value + 1), []);
  useEffect(() => {
    const controller = new AbortController();
    setLoading(true); setError('');
    api<T>(path, { signal: controller.signal }).then(setData).catch(error => { if (!controller.signal.aborted) setError(errorMessage(error)); }).finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
  }, [path, revision, retry]);
  return { data, loading, error, reload };
}

export function usePagination<T>(items: T[], resetKey = '', pageSize = 20) {
  const [page, setPage] = useState(1);
  useEffect(() => setPage(1), [resetKey]);
  const pages = Math.max(1, Math.ceil(items.length / pageSize));
  const current = Math.min(page, pages);
  return { items: items.slice((current - 1) * pageSize, current * pageSize), current, pages, total: items.length, go: setPage };
}
