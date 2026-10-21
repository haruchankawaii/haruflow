import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';

export async function fingerprint(path) {
  const before = await stat(path);
  if (!before.isFile() || before.size < 5) throw new Error('Arsip backup kosong atau bukan file.');
  const hash = createHash('sha256');
  let bytes = 0;
  for await (const chunk of createReadStream(path)) { hash.update(chunk); bytes += chunk.length; }
  const after = await stat(path);
  if (before.size !== after.size || bytes !== after.size || before.mtimeMs !== after.mtimeMs)
    throw new Error('Arsip berubah saat diperiksa; ulangi dengan file backup yang tidak sedang ditulis.');
  return { bytes, sha256: hash.digest('hex') };
}

export function validateManifest(manifest, actual, target) {
  if (manifest.format !== 2) throw new Error('Format manifest backup belum didukung.');
  if (!Number.isSafeInteger(manifest.bytes) || manifest.bytes <= 0 || !/^[a-f0-9]{64}$/.test(manifest.sha256 ?? ''))
    throw new Error('Ukuran atau checksum manifest backup tidak valid.');
  if (manifest.bytes !== actual.bytes || manifest.sha256 !== actual.sha256)
    throw new Error('Ukuran/SHA-256 arsip berbeda dari manifest; restore dibatalkan sebelum aplikasi dihentikan.');
  if (!Number.isSafeInteger(manifest.schema_version) || manifest.schema_version < 1 ||
      !Number.isSafeInteger(manifest.postgres_major) || manifest.postgres_major < 14 ||
      typeof manifest.application?.version !== 'string' ||
      !(manifest.application.commit === 'unknown' || /^[a-f0-9]{40}$/i.test(manifest.application?.commit ?? '')))
    throw new Error('Identitas aplikasi/schema/PostgreSQL pada manifest tidak valid.');
  if (target && manifest.schema_version > target.schema_version)
    throw new Error('Schema backup lebih baru dari aplikasi tujuan. Gunakan versi aplikasi yang sesuai.');
  if (target && manifest.postgres_major !== target.postgres_major)
    throw new Error('Versi mayor PostgreSQL berbeda. Gunakan prosedur migrasi PostgreSQL terpisah.');
}
