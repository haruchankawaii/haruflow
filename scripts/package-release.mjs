import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, copyFile, writeFile, readFile, chmod, access, lstat } from 'node:fs/promises';
import { resolve, dirname, join, basename, isAbsolute, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
function git(...args) {
  const result = spawnSync('git', ['-c', `safe.directory=${resolve(root).replaceAll('\\', '/')}`, ...args], { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, 'Cannot inspect source revision.'); return result.stdout.trim();
}
const sha = git('rev-parse', 'HEAD'); assert.match(sha, /^[a-f0-9]{40}$/);
assert.equal(git('status', '--porcelain', '--untracked-files=normal'), '', 'Commit tested changes before packaging a reproducible candidate.');
const binary = resolve(root, process.env.HARUFLOW_RELEASE_BINARY || `target/release/haruflow${process.platform === 'win32' ? '.exe' : ''}`);
await access(binary);
const result = spawnSync(binary, ['version'], { cwd: root, encoding: 'utf8', windowsHide: true });
assert.equal(result.status, 0, 'Candidate executable must provide build identity.');
const build = JSON.parse(result.stdout.trim()); assert.equal(build.commit, sha, 'Rebuild with HARUFLOW_BUILD_COMMIT equal to the packaged Git revision.');
const label = `haruflow-community-${process.platform}-${process.arch}-${sha.slice(0, 12)}`;
const destination = resolve(root, 'output/releases', label);
await mkdir(dirname(destination), { recursive: true });
await mkdir(destination); // Exclusive directory creation prevents mixing candidates.
const sourceRoot = join(destination, 'source'); await mkdir(sourceRoot);
for (const name of git('ls-files').split(/\r?\n/).filter(Boolean)) {
  assert.ok(!isAbsolute(name) && !name.split(/[\\/]/).includes('..'), 'Tracked source path must stay within candidate.');
  assert.ok(!(await lstat(join(root, name))).isSymbolicLink(), 'Source package must not follow external symlinks.');
  const target = join(sourceRoot, name); await mkdir(dirname(target), { recursive: true }); await copyFile(join(root, name), target);
}
const executable = join(destination, basename(binary)); await copyFile(binary, executable);
if (process.platform !== 'win32') await chmod(executable, 0o755);
const nativeCommand = `${process.platform === 'win32' ? '.\\' : './'}${basename(binary)}`;
const nativePermissions = process.platform === 'win32' ? '' : `After downloading the Actions artifact, run chmod +x ${nativeCommand} to restore executable permission. `;
async function copyTree(source, target) {
  const { readdir } = await import('node:fs/promises'); await mkdir(target, { recursive: true });
  for (const entry of await readdir(source, { withFileTypes: true })) {
    assert.ok(!entry.isSymbolicLink(), 'Build output must not contain symlinks.');
    if (entry.isDirectory()) await copyTree(join(source, entry.name), join(target, entry.name));
    else await copyFile(join(source, entry.name), join(target, entry.name));
  }
}
await copyTree(join(root, 'apps/web/dist'), join(destination, 'web'));
await copyFile(join(root, 'LICENSE'), join(destination, 'LICENSE'));
if (process.platform === 'win32') await copyFile(join(root, 'scripts/windows/Install-Haruflow.ps1'), join(destination, 'Install-Haruflow.ps1'));
await writeFile(join(destination, 'BUILD.json'), JSON.stringify({ ...build, platform: process.platform, architecture: process.arch, source: 'source/', channel: 'candidate' }, null, 2));
await writeFile(join(destination, 'START-HERE.md'), `# Haruflow Community candidate\n\nBuild: ${sha}. This is a candidate, not evidence that a business pilot is complete.\n\n## Docker Compose\n\nThe complete corresponding source is in source/. From that folder follow README.md and docs/OPERATIONS.md: configure, build app, start PostgreSQL, migrate, start app. No registry publication is required.\n\n## Native ${process.platform}/${process.arch}\n\nRequires PostgreSQL 14+ and a separately configured database. Set DATABASE_URL, HARUFLOW_SETUP_TOKEN (32+ characters), HARUFLOW_BIND=127.0.0.1:8080, HARUFLOW_STATIC_DIR=web, and HARUFLOW_SECURE_COOKIES=false for loopback HTTP only. ${nativePermissions}From this folder run ${nativeCommand} migrate, then ${nativeCommand} serve. Use HTTPS and secure cookies for network access.\n\nNever copy production credentials into this package. Back up before upgrade. Verify SHA256SUMS and consult source/docs/OPERATIONS.md before restore. Community remains AGPL-3.0-only; corresponding source and license are included.\n`);
if (process.platform === 'win32') await writeFile(join(destination, 'INSTALASI-WINDOWS.md'), `# Haruflow Community untuk usaha kecil\n\nSatu perusahaan, IDR, maksimal 10 akun aktif. Pasang PostgreSQL 14+ dan siapkan database kosong khusus Haruflow. Ekstrak paket lengkap sebelum menjalankan installer.\n\nJalankan dari PowerShell: ./Install-Haruflow.ps1 -PostgresBin "C:/Program Files/PostgreSQL/16/bin"\n\nSesuaikan folder PostgreSQL yang terpasang. Installer meminta koneksi database secara tersembunyi. Ikuti kebijakan eksekusi PowerShell Windows Anda; paket ini belum ditandatangani. Instalasi per pengguna tidak memerlukan service Windows. Buka pintasan Haruflow Community atau Start-Haruflow.ps1 dari folder instalasi. Baca Setup-pertama.txt untuk kode setup. Stop-Haruflow.ps1 menghentikan server instalasi ini. PostgreSQL tetap harus berjalan.\n\nInstaller memverifikasi checksum, menyimpan versi, dan membuat backup sebelum migrasi database lama. Arsip backup installer memakai metadata haruflow-windows-pre-upgrade-v1, terpisah dari manifest alat database.mjs. Untuk restore gunakan pg_restore ke database kosong dan verifikasi data sebelum mengganti koneksi. Lihat source/docs/WINDOWS.md dan source/docs/OPERATIONS.md.\n\nPaket ini adalah kandidat uji. Uji penerimaan dengan data usaha dan persetujuan pemilik tetap diperlukan sebelum produksi.\n`);
const { readdir } = await import('node:fs/promises');
async function files(folder) {
  const items = [];
  for (const entry of await readdir(folder, { withFileTypes: true })) {
    const path = join(folder, entry.name); if (entry.isDirectory()) items.push(...await files(path)); else items.push(path);
  }
  return items.sort();
}
const checksums = [];
for (const path of await files(destination)) checksums.push(`${createHash('sha256').update(await readFile(path)).digest('hex')}  ${relative(destination, path).replaceAll('\\', '/')}`);
await writeFile(join(destination, 'SHA256SUMS'), `${checksums.join('\n')}\n`);
console.log(destination);
