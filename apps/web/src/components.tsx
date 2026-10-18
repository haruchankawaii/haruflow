import { useEffect, useId, useRef, type FormEvent, type ReactNode } from 'react';
import { ArrowDownLeft, ArrowUpRight, Check, ChevronLeft, ChevronRight, CircleAlert, Inbox, LoaderCircle, Search, X } from 'lucide-react';
import { statusNames } from './format';

export function Brand({ dark = false }: { dark?: boolean }) {
  return <div className={`brand ${dark ? 'brand-dark' : ''}`}><span className="brand-symbol" aria-hidden="true"><svg viewBox="0 0 36 36"><path d="M7 6h7v12c4-7 9-7 16-7v7c-7 0-11 1-16 12H7z" fill="currentColor" /><circle cx="27" cy="29" r="3.5" fill="currentColor" /></svg></span><span>haruflow<span className="brand-dot">.</span></span></div>;
}

export function Loading({ label = 'Memuat ruang kerja…' }: { label?: string }) {
  return <div className="loading-state" role="status"><LoaderCircle className="spin" size={24} /><span>{label}</span></div>;
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return <div className="error-state" role="alert"><CircleAlert size={24} /><h3>Data belum dapat dimuat</h3><p>{message}</p>{onRetry && <button className="button secondary" onClick={onRetry}>Coba lagi</button>}</div>;
}

export function EmptyState({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  return <div className="empty-state"><div className="empty-icon"><Inbox size={26} strokeWidth={1.5} /></div><h3>{title}</h3><p>{description}</p>{action}</div>;
}

export function PageHeader({ eyebrow, title, description, actions }: { eyebrow: string; title: string; description: string; actions?: ReactNode }) {
  return <div className="page-heading"><div><div className="eyebrow">{eyebrow}</div><h1>{title}</h1><p>{description}</p></div><div className="heading-actions">{actions}</div></div>;
}

export function Status({ status }: { status: string }) {
  return <span className={`status status-${status}`}><span />{statusNames[status] ?? status}</span>;
}

export function SearchInput({ value, onChange, placeholder = 'Cari…' }: { value: string; onChange: (value: string) => void; placeholder?: string }) {
  return <label className="search-field"><Search size={17} /><input aria-label={placeholder} value={value} onChange={event => onChange(event.target.value)} placeholder={placeholder} />{value && <button type="button" aria-label="Hapus pencarian" onClick={() => onChange('')}><X size={15} /></button>}</label>;
}

export function FormField({ label, hint, children, className = '' }: { label: string; hint?: string; children: ReactNode; className?: string }) {
  return <label className={`form-field ${className}`}><span>{label}</span>{children}{hint && <small>{hint}</small>}</label>;
}

export function SubmitButton({ busy, children }: { busy: boolean; children: ReactNode }) {
  return <button className="button primary" type="submit" disabled={busy}>{busy ? <LoaderCircle className="spin" size={17} /> : <Check size={17} />}{busy ? 'Menyimpan…' : children}</button>;
}

export function Modal({ title, description, onClose, children, wide = false }: { title: string; description?: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    const handleCancel = (event: Event) => { event.preventDefault(); closeRef.current(); };
    element?.addEventListener('cancel', handleCancel);
    return () => { element?.removeEventListener('cancel', handleCancel); element?.close(); };
  }, []);
  return <dialog ref={dialog} className={`modal ${wide ? 'modal-wide' : ''}`} aria-labelledby={titleId} onClick={event => { if (event.target === dialog.current) { const rect = dialog.current.getBoundingClientRect(); if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) onClose(); } }}><div className="modal-heading"><div><h2 id={titleId}>{title}</h2>{description && <p>{description}</p>}</div><button className="icon-button" aria-label="Tutup dialog" onClick={onClose}><X size={20} /></button></div>{children}</dialog>;
}

export function FormError({ error }: { error: string }) { return error ? <div className="form-error" role="alert"><CircleAlert size={17} /><span>{error}</span></div> : null; }

export function FormActions({ onClose, busy, label = 'Simpan' }: { onClose: () => void; busy: boolean; label?: string }) {
  return <div className="form-actions"><button type="button" className="button secondary" disabled={busy} onClick={onClose}>Batal</button><SubmitButton busy={busy}>{label}</SubmitButton></div>;
}

export function MovementIcon({ incoming }: { incoming: boolean }) {
  return <span className={`movement-icon ${incoming ? 'incoming' : 'outgoing'}`}>{incoming ? <ArrowDownLeft size={17} /> : <ArrowUpRight size={17} />}</span>;
}

export type FormHandler = (event: FormEvent<HTMLFormElement>) => void;

export function Pagination({ current, pages, total, go }: { current: number; pages: number; total: number; go: (page: number) => void }) {
  return <div className="pagination"><span>{total} data · Halaman {current} dari {pages}</span><div><button className="icon-button" disabled={current <= 1} onClick={() => go(current - 1)} aria-label="Halaman sebelumnya"><ChevronLeft size={16} /></button><button className="icon-button" disabled={current >= pages} onClick={() => go(current + 1)} aria-label="Halaman berikutnya"><ChevronRight size={16} /></button></div></div>;
}
