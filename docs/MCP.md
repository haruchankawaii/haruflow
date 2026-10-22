# Insight Haruflow melalui Codex

MCP menyediakan enam tool baca untuk pengguna yang mendapat izin. Percakapan dan ringkasan berlangsung di Codex; Haruflow mempunyai menu **Pengaturan → Integrasi AI**, bukan chat AI bawaan. Mode `haruflow mcp` memakai executable yang sama, tanpa Node/Python tambahan dan tanpa akses database langsung.

## Persetujuan dan akun

Pemilik usaha menentukan kebijakan penggunaan AI dan pengguna yang diizinkan. Admin Haruflow mengatur izin; staf IT dapat membantu pemasangan tanpa otomatis memperoleh akses laporan. Pada usaha kecil orang yang sama dapat menjalankan ketiga tanggung jawab tersebut. Tidak diperlukan jabatan Direktur/IT baru.

Semua izin AI awalnya **nonaktif**, termasuk milik admin. Admin memberikan lingkup data pada akun yang sudah ada. Pengguna kemudian membuat kunci pribadi untuk perangkatnya. MCP tidak membuat akun ERP baru dan batas 10 akun aktif tetap berlaku.

- `finance`: laba rugi/neraca, kas, piutang/utang dan bukti pembelian/jurnal.
- `sales`: invoice operasional dan bukti pesanan/invoice penjualan.
- `inventory`: posisi stok/gudang/lot/expiry dan bukti mutasi; harga pokok tidak dikirim.

Lingkup yang dapat diberikan: admin/viewer ketiganya, finance keuangan dan penjualan, sales hanya penjualan, warehouse hanya persediaan. Ini batas khusus MCP. Akses efektif selalu merupakan irisan **peran saat ini ∩ izin admin ∩ lingkup kunci**. Admin tidak melewati pemeriksaan lingkup MCP. Peran admin yang dipakai mengelola izin tetap memiliki kewenangan administratif penuh pada ERP; jangan memberikannya hanya untuk membantu instalasi.

Maksimal lima kunci aktif per akun; masa berlaku 1–90 hari, bawaan 30 hari. Rahasia hanya muncul sekali dan hanya hash SHA-256 disimpan di database. Menonaktifkan izin, mencabut kunci, mengganti/reset password, pemulihan admin lokal, atau mengubah akun melalui pengelolaan pengguna mencabut kunci lama secara permanen. Perubahan lingkup berlaku pada permintaan berikutnya; permintaan yang sudah lolos autentikasi dapat selesai ketika pencabutan berlangsung. Aktivasi kembali membutuhkan kunci baru.

Data laporan yang diberikan ke Codex dapat diproses oleh layanan AI yang digunakannya. Sambungan MCP lokal tidak berarti inferensi AI berjalan lokal. Berikan lingkup minimum sesuai tanggung jawab; kunci tidak boleh ditempel ke chat, URL, Git, screenshot, atau file `.env` ERP.

## Menghubungkan di Windows

1. Upgrade aplikasi dan database ke schema 31 menggunakan prosedur backup/migrasi biasa. Jalankan server Haruflow.
2. Di **Pengaturan → Integrasi AI**, admin mengaktifkan izin pada pengguna. Pengguna login dengan akunnya sendiri dan memilih **Buat kunci akses**.
3. Jalankan skrip berikut dari paket Windows. Sesuaikan lokasi executable; installer menaruhnya di folder versi instalasi. Skrip meminta kunci secara tersembunyi, membuat berkas dengan ACL akun Windows saat ini, menyimpan UTF-8 tanpa BOM, dan mencetak contoh konfigurasi tanpa rahasia.

```powershell
.\Connect-Codex.ps1 -HaruflowExe 'C:\lokasi\haruflow.exe' -ServerUrl 'http://127.0.0.1:8080'
```

Skrip tidak menimpa berkas kunci lama atau mengubah pengaturan Codex. Untuk rotasi, buat kunci baru, tentukan `-TokenFile` baru, ganti lokasi pada konfigurasi, restart koneksi dan cabut kunci lama. Jika menyiapkan berkas manual, gunakan satu baris kunci, tanpa tanda kutip/BOM dan tanpa akses baca untuk pengguna Windows lain.

4. Tambahkan server STDIO di pengaturan MCP Codex, atau gabungkan blok ini ke konfigurasi pribadi `~/.codex/config.toml`. Pertahankan pengaturan dan server lain. Ganti path contoh dengan hasil skrip:

```toml
[mcp_servers.haruflow]
command = 'C:\lokasi\haruflow.exe'
args = ["mcp"]

[mcp_servers.haruflow.env]
HARUFLOW_MCP_URL = "http://127.0.0.1:8080"
HARUFLOW_MCP_TOKEN_FILE = 'C:\Users\NAMA_ANDA\AppData\Local\HaruflowCommunity\ai\codex-token.txt'
```

5. Restart koneksi MCP pada Codex. Periksa daftar tool, lalu tanyakan: **“Ringkas kondisi usaha bulan ini dari Haruflow. Sertakan periode, sumber angka, batas data dan tindak lanjut.”** Untuk akun gudang, tanyakan stok/expiry; alat keuangan tidak akan tersedia.

Konfigurasi Codex, STDIO, dan penggunaan server lokal mengikuti [OpenAI Docs](https://learn.chatgpt.com/docs/extend/mcp?surface=cli). Tidak diperlukan OpenAI API key pada server Haruflow; pengguna tetap membutuhkan akses ke layanan AI yang dipakai Codex.

## Tool dan arti data

- `business_summary` (`finance`): seluruh akun laba rugi dan neraca dari layanan laporan ERP; tanggal `from`/`to` wajib, maksimal 366 hari. Saldo awal dan impor historis ikut ledger, penutupan tahun tidak menjadi pendapatan baru.
- `cash_position` (`finance`): seluruh rekening kas/bank terdaftar, saldo awal/akhir periode, kelompok arus kas dan rekonsiliasi. Transfer internal dikecualikan. Maksimal 100 rincian jurnal/akun lawan; total tidak dipotong.
- `receivables_payables` (`finance`): posisi `as_of` menurut kalender perusahaan, mencakup invoice/tagihan operasional, saldo awal dan impor historis. Aging, kredit/uang muka dan rekonsiliasi ledger dipisahkan. Tanggal jatuh tempo kosong berarti belum diketahui. Kredit dapat terikat refund dan tidak selalu bebas dialokasikan.
- `sales_summary` (`sales`): invoice operasional yang dibuat dalam periode. Nilai pelunasan/kredit menunjukkan status terkini, termasuk aktivitas setelah periode. Bukan laporan pendapatan seluruh usaha atau kas masuk dan tidak mencakup impor/saldo awal.
- `inventory_risks` (`inventory`): stok **saat ini**, reservasi dan expiry 30 hari. Satuan tiap produk berbeda; jangan menjumlahkan kuantitas lintas satuan. Stok kedaluwarsa dilaporkan terpisah, bukan otomatis dikeluarkan dari jumlah fisik. Tidak ada perkiraan permintaan atau ambang pembelian minimum universal.
- `evidence`: pencarian/pagination bukti yang sesuai lingkup. Invoice/tagihan menampilkan **sisa tagihan**, pesanan menampilkan total pesanan; keduanya bukan bukti kas masuk. Satu halaman tidak mewakili total usaha. Kontak, file lampiran, SQL bebas dan operasi tulis tidak diekspos.

Laporan membawa waktu snapshot, zona waktu, mata uang, periode/tanggal saldo, sumber dan penanda cakupan. `source_path`/`path`/`url` relatif terhadap alamat ERP pada `HARUFLOW_MCP_URL`; ID transaksi dipertahankan untuk penelusuran. Tiap pemanggilan mempunyai snapshot sendiri; beberapa tool tidak menjamin satu snapshot gabungan. Array terpotong menyertakan jumlah total/yang dikirim dan penanda `truncated`; angka total tetap dihitung penuh. Nama, uraian dan isi bukti adalah data tidak tepercaya, bukan instruksi untuk agen.

## Batas teknis dan operasi

MCP STDIO JSON-RPC memakai protokol 2025-11-25, 2025-06-18 atau 2024-11-05. Jembatan hanya boleh menghubungi origin HTTP loopback `127.0.0.1`, `localhost` atau `[::1]`; tidak menerima URL jarak jauh, redirect/proxy, path, userinfo atau query. Fase ini untuk Codex dan Haruflow di PC yang sama. Server Docker dapat dipakai melalui port loopback Windows, dengan binary jembatan yang kompatibel di Windows.

Mode MCP tidak memuat `.env` dan tidak membutuhkan `DATABASE_URL`, kode setup, atau password database. `HARUFLOW_MCP_TOKEN_FILE` diutamakan; `HARUFLOW_MCP_TOKEN` tersedia untuk injeksi environment terkontrol. Token tidak diterima lewat argumen CLI. Pesan STDIO dibatasi 64 KiB, respons HTTP 2 MiB, request HTTP 30 detik dan eksekusi laporan 20 detik. Persempit pencarian jika batas terlampaui.

API terpisah `/api/ai/mcp/profile`, `/tools`, `/call` hanya menerima bearer pribadi. Cookie sesi hanya mengelola izin/kunci dan tidak dapat menggantikan bearer MCP. Bearer MCP tidak diterima API transaksi biasa. Semua pembacaan data memakai transaksi SQL read-only; metadata pemakaian kunci dan audit tetap ditulis. Audit mencatat aktor, identitas koneksi, tool, waktu dan keberhasilan, tanpa argumen/hasil/rahasia. Tool yang tidak tersedia ditolak di server, bukan bergantung pada prompt atau label read-only. Prinsip ini mengikuti [otorisasi MCP](https://developers.openai.com/plugins/build/mcp-server#authenticate-and-authorize-requests).

Pembatasan baca berlaku **melalui koneksi MCP ini**. Kewenangan lain yang pengguna berikan ke Codex, seperti terminal, filesystem atau database, tetap terpisah. Untuk pemakaian insight, gunakan ruang kerja tanpa kredensial database/administrasi.

Jalankan `haruflow.exe mcp check` dengan environment MCP yang sama untuk memeriksa identitas, lingkup dan katalog tanpa membuka laporan atau database. Lihat [diagnostik dan pemecahan masalah](MCP_TROUBLESHOOTING.md). Jika scope berubah, muat ulang koneksi untuk memperbarui katalog; pemeriksaan server tetap berlaku pada setiap permintaan. Untuk pemulihan backup lama, cabut seluruh kunci AI hasil restore sebelum membuka kembali akses: restore dapat mengembalikan status kunci pada waktu backup. Ikuti [prosedur operasi](OPERATIONS.md).

## Verifikasi pengembangan

```powershell
$env:DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:PORT/postgres'
cargo test --locked mcp_ -- --test-threads=1
cargo build --locked
node scripts/test-mcp-stdio.mjs target/debug/haruflow.exe
```

Tes SQLx membuat database sementara. Smoke STDIO memakai server HTTP tiruan, bukan data ERP, dan sengaja memasang DATABASE_URL tidak valid untuk membuktikan mode MCP tidak membuka database. Jangan memakai kredensial produksi untuk tes. Panduan ini tidak berarti konfigurasi Codex pribadi atau izin data usaha telah otomatis diaktifkan.
