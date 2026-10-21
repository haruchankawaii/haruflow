import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fingerprint, validateManifest } from './backup-manifest.mjs';

test('backup fingerprints detect corruption and reject incompatible restore targets', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'haruflow-backup-proof-'));
  try {
    const path = join(dir, 'proof.backup');
    await writeFile(path, 'PGDMP-disposable-proof');
    const actual = await fingerprint(path);
    const manifest = { format: 2, ...actual, schema_version: 21, postgres_major: 18,
      application: { version: '0.1.0', commit: 'a'.repeat(40) } };
    validateManifest(manifest, actual, { schema_version: 22, postgres_major: 18 });
    assert.throws(() => validateManifest(manifest, actual, { schema_version: 20, postgres_major: 18 }), /lebih baru/);
    assert.throws(() => validateManifest(manifest, actual, { schema_version: 22, postgres_major: 14 }), /mayor/);
    await writeFile(path, 'PGDMP-disposable-corruption');
    assert.throws(() => validateManifest(manifest, { ...actual, sha256: 'b'.repeat(64) }), /SHA-256/);
    assert.notDeepEqual(await fingerprint(path), actual);
    for (const changed of [{ format: 3 }, { schema_version: -1 }, { postgres_major: '18' }, { bytes: 0 },
      { application: { version: '0.1.0', commit: 'secret-or-invalid-commit' } }]) {
      assert.throws(() => validateManifest({ ...manifest, ...changed }, actual));
    }
  } finally { await unlink(join(dir, 'proof.backup')).catch(error => { if (error.code !== 'ENOENT') throw error; }); await rmdir(dir); }
});
