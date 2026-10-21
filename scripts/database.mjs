import { spawn } from 'node:child_process';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, link, mkdir, open, readFile, unlink } from 'node:fs/promises';
import { once } from 'node:events';
import { finished, pipeline } from 'node:stream/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fingerprint, validateManifest } from './backup-manifest.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const compose = ['compose', '--env-file', '.env', '-f', 'deploy/community/compose.yml'];
const [mode, requestedPath, confirmation] = process.argv.slice(2);

function docker(args, options = {}) {
  const child = spawn('docker', [...compose, ...args], { cwd: root, ...options });
  const done = new Promise((accept, reject) => {
    child.once('error', reject);
    child.once('exit', code => code === 0 ? accept() : reject(new Error(`Docker selesai dengan kode ${code}.`)));
  });
  // Attach a rejection handler immediately; callers still await the original promise.
  done.catch(() => {});
  return { child, done };
}

async function capture(args) {
  const { child, done } = docker(args, { stdio: ['ignore', 'pipe', 'inherit'] });
  let content = '';
  for await (const chunk of child.stdout) {
    content += chunk.toString('utf8');
    if (content.length > 65536) { child.kill(); throw new Error('Metadata operasional terlalu besar.'); }
  }
  await done;
  return content.trim();
}

async function identity() {
  const application = JSON.parse(await capture(['run', '--rm', '--no-deps', 'app', 'version']));
  const psql = ['exec', '-T', 'postgres', 'psql', '-U', 'haruflow', '-d', 'haruflow', '-Atq', '-v', 'ON_ERROR_STOP=1', '-c'];
  // A restore target can be an empty, freshly created database. PostgreSQL
  // resolves table names even in an unused CASE branch, so check separately.
  const migrated = await capture([...psql, "SELECT to_regclass('public._sqlx_migrations') IS NOT NULL;"]);
  const schema = migrated === 't' ? 'COALESCE((SELECT max(version) FROM _sqlx_migrations WHERE success),0)' : '0';
  const database = JSON.parse(await capture([...psql,
    `SELECT json_build_object('schema_version',${schema},'postgres_version',current_setting('server_version'));`]));
  return { application, ...database, postgres_major: Number.parseInt(database.postgres_version, 10) };
}

async function inspect(path, target) {
  const actual = await fingerprint(path);
  let metadata;
  try { metadata = JSON.parse(await readFile(`${path}.json`, 'utf8')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (metadata?.format === 2) validateManifest(metadata, actual, target);
  else if (metadata && metadata.format !== 1) throw new Error('Format manifest backup belum didukung.');
  else console.warn('Backup lama tanpa checksum format 2: integritas akan diperiksa melalui pg_restore --list; identitas versi perlu diperiksa operator.');
  // Inspect archive structure before stopping the app or changing its database.
  const { child, done } = docker(['exec', '-T', 'postgres', 'pg_restore', '--list'], { stdio: ['pipe', 'ignore', 'inherit'] });
  const streamed = pipeline(createReadStream(path), child.stdin).catch(error => {
    // pg_restore --list may exit after reading only the archive TOC. A clean
    // exit is checked separately; it need not consume all proof-file bytes.
    if (!['EPIPE', 'ERR_STREAM_PREMATURE_CLOSE'].includes(error.code)) throw error;
  });
  await Promise.all([streamed, done]);
  return metadata;
}

async function backup(path) {
  const build = await identity();
  const partial = `${path}.partial`;
  const metadataPartial = `${partial}.json`;
  const metadataPath = `${path}.json`;
  let ownsPartial = false;
  let ownsMetadataPartial = false;
  let publishedArchive = false;
  let publishedMetadata = false;
  let completed = false;
  let output;
  let outputDone;
  let child;
  let dumpDone;
  let streamDone;

  try {
    // Wait for exclusive creation before spawning pg_dump. An existing partial
    // archive belongs to another run and must never be removed by this one.
    output = createWriteStream(partial, { flags: 'wx', mode: 0o600 });
    outputDone = finished(output, { cleanup: true });
    outputDone.catch(() => {});
    await once(output, 'open');
    ownsPartial = true;
    ({ child, done: dumpDone } = docker(
      ['exec', '-T', 'postgres', 'pg_dump', '-U', 'haruflow', '-d', 'haruflow', '-Fc'],
      { stdio: ['ignore', 'pipe', 'inherit'] },
    ));
    streamDone = pipeline(child.stdout, output);
    await Promise.all([streamDone, dumpDone]);

    const metadata = await open(metadataPartial, 'wx', 0o600);
    ownsMetadataPartial = true;
    try {
      await metadata.writeFile(JSON.stringify({ format: 2, created_at: new Date().toISOString(), ...build,
        ...await fingerprint(partial), database: 'haruflow', archive: path }, null, 2));
    } finally {
      await metadata.close();
    }

    // Hard links publish completed files exclusively. Unlike rename on Unix,
    // link fails when the destination exists instead of replacing it.
    await link(metadataPartial, metadataPath);
    publishedMetadata = true;
    await link(partial, path);
    publishedArchive = true;
    completed = true;
  } finally {
    if (!completed) {
      child?.kill();
      output?.destroy();
      // Wait for our streams and child to close before removing owned files,
      // including on Windows where open files cannot always be unlinked.
      await Promise.allSettled([streamDone, dumpDone, outputDone].filter(Boolean));
    }
    const cleanup = [];
    if (ownsPartial) cleanup.push(unlink(partial));
    if (ownsMetadataPartial) cleanup.push(unlink(metadataPartial));
    if (!completed && publishedArchive) cleanup.push(unlink(path));
    if (!completed && publishedMetadata) cleanup.push(unlink(metadataPath));
    const results = await Promise.allSettled(cleanup);
    for (const result of results) {
      if (result.status === 'rejected' && result.reason.code !== 'ENOENT') {
        console.error(`Tidak dapat membersihkan file backup sementara: ${result.reason.message}`);
      }
    }
  }
}

try {
  await access(resolve(root, '.env'));
  if (mode === 'backup') {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const path = resolve(root, requestedPath || `backups/haruflow-${timestamp}.backup`);
    await mkdir(dirname(path), { recursive: true });
    await backup(path);
    console.log(`Backup selesai: ${path}\nCadangkan .env secara terpisah dengan enkripsi dan catat versi image aplikasi.`);
  } else if (mode === 'restore' && requestedPath && confirmation === '--confirm-restore') {
    const path = resolve(root, requestedPath);
    await access(path);
    const target = await identity();
    const metadata = await inspect(path, { schema_version: target.application.schema_version, postgres_major: target.postgres_major });
    await docker(['stop', 'app'], { stdio: 'inherit' }).done;
    const { child, done } = docker(['exec', '-T', 'postgres', 'pg_restore', '-U', 'haruflow', '-d', 'haruflow', '--clean', '--if-exists', '--single-transaction', '--exit-on-error'], { stdio: ['pipe', 'inherit', 'inherit'] });
    await Promise.all([pipeline(createReadStream(path), child.stdin), done]);
    console.log('Restore selesai. Aplikasi tetap berhenti: pastikan versi image sesuai backup, jalankan migrate bila diperlukan, lalu jalankan docker compose --env-file .env -f deploy/community/compose.yml up -d app dan verifikasi saldo.');
    if (metadata?.schema_version < target.application.schema_version) console.log('Backup memakai schema lebih lama; jalankan migrasi eksplisit sebelum aplikasi dinyalakan.');
  } else if (mode === 'verify' && requestedPath) {
    const target = await identity();
    await inspect(resolve(root, requestedPath), { schema_version: target.application.schema_version, postgres_major: target.postgres_major });
    console.log('Ukuran, checksum bila tersedia, identitas schema, dan struktur arsip backup lolos pemeriksaan. Database tidak diubah.');
  } else {
    throw new Error('Gunakan: node scripts/database.mjs backup [file.backup]\nAtau: node scripts/database.mjs verify file.backup\nAtau: node scripts/database.mjs restore file.backup --confirm-restore\nRestore mengganti isi database. Gunakan instalasi tujuan yang benar dan backup terlebih dahulu.');
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
