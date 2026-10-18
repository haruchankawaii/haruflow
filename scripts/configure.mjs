import { randomBytes } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const password = randomBytes(24).toString('hex');
const setupToken = randomBytes(32).toString('hex');
const content = `DB_PASSWORD=${password}
DATABASE_URL=postgres://haruflow:${password}@127.0.0.1:5432/haruflow
HARUFLOW_SETUP_TOKEN=${setupToken}
HARUFLOW_BIND=127.0.0.1:8080
HARUFLOW_PORT=8080
HARUFLOW_STATIC_DIR=apps/web/dist
HARUFLOW_SECURE_COOKIES=false
RUST_LOG=haruflow=info,tower_http=info
`;
try {
  await writeFile(new URL('../.env', import.meta.url), content, { flag: 'wx', mode: 0o600 });
  console.log(`Konfigurasi lokal dibuat di ${root}.env. Simpan setup token dari file tersebut untuk membuat perusahaan pertama.`);
} catch (error) {
  if (error.code === 'EEXIST') {
    console.error('.env sudah ada. Konfigurasi dan secrets yang ada tidak diubah.');
  } else {
    console.error('Tidak dapat membuat konfigurasi:', error.message);
  }
  process.exitCode = 1;
}
