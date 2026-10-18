import { spawn } from 'node:child_process';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, mkdir, writeFile } from 'node:fs/promises';
import { pipeline } from 'node:stream/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

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

try {
  await access(resolve(root, '.env'));
  if (mode === 'backup') {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const path = resolve(root, requestedPath || `backups/haruflow-${timestamp}.backup`);
    await mkdir(dirname(path), { recursive: true });
    const { child, done } = docker(['exec', '-T', 'postgres', 'pg_dump', '-U', 'haruflow', '-d', 'haruflow', '-Fc'], { stdio: ['ignore', 'pipe', 'inherit'] });
    await Promise.all([pipeline(child.stdout, createWriteStream(path, { flags: 'wx', mode: 0o600 })), done]);
    await writeFile(`${path}.json`, JSON.stringify({ format: 1, created_at: new Date().toISOString(), application: 'Haruflow Community 0.1.0', database: 'haruflow', archive: path }, null, 2), { flag: 'wx', mode: 0o600 });
    console.log(`Backup selesai: ${path}\nCadangkan .env secara terpisah dengan enkripsi dan catat versi image aplikasi.`);
  } else if (mode === 'restore' && requestedPath && confirmation === '--confirm-restore') {
    const path = resolve(root, requestedPath);
    await access(path);
    await docker(['stop', 'app'], { stdio: 'inherit' }).done;
    const { child, done } = docker(['exec', '-T', 'postgres', 'pg_restore', '-U', 'haruflow', '-d', 'haruflow', '--clean', '--if-exists', '--single-transaction', '--exit-on-error'], { stdio: ['pipe', 'inherit', 'inherit'] });
    await Promise.all([pipeline(createReadStream(path), child.stdin), done]);
    console.log('Restore selesai. Aplikasi tetap berhenti: pastikan versi image sesuai backup, lalu jalankan Compose start app dan verifikasi saldo.');
  } else {
    throw new Error('Gunakan: node scripts/database.mjs backup [file.backup]\nAtau: node scripts/database.mjs restore file.backup --confirm-restore\nRestore mengganti isi database. Gunakan instalasi tujuan yang benar dan backup terlebih dahulu.');
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
