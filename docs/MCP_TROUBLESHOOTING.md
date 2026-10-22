# Memeriksa koneksi MCP Haruflow

Panduan ini untuk Codex dan Haruflow di PC Windows yang sama. Penyiapan awal dan arti enam tool dijelaskan di [panduan MCP](MCP.md). Pemeriksaan memakai kunci pribadi yang telah diberikan melalui **Pengaturan → Integrasi AI**; jangan menempelkan kunci ke chat atau argumen terminal.

## Tes koneksi tanpa membuka laporan

Jalankan executable dari paket yang sama dengan server Haruflow. Pakai alamat dan berkas kunci yang sama dengan konfigurasi MCP Codex:

```powershell
$env:HARUFLOW_MCP_URL = 'http://127.0.0.1:8080'
$env:HARUFLOW_MCP_TOKEN_FILE = 'C:\Users\NAMA_ANDA\AppData\Local\HaruflowCommunity\ai\codex-token.txt'
& 'C:\lokasi\haruflow.exe' mcp check
```

Sukses menghasilkan satu objek JSON dengan `connected: true`, `read_only: true`, identitas akun (`id`, `name`, `role`), `scopes`, dan nama `tools` yang tersedia. Identitas akun dan lingkup harus sesuai pengguna yang akan memakai Codex. Diagnostik tidak menjalankan tool laporan, mengirim isi transaksi, menampilkan kunci, memuat `.env`, atau membuka koneksi database. Permintaan profil/katalog tetap melewati autentikasi server dan dapat memperbarui metadata pemakaian kunci.

Kegagalan menghasilkan exit code bukan nol dan pesan singkat di stderr, tanpa objek sukses di stdout. Perintah tidak mengubah konfigurasi Codex, mengaktifkan izin, memperbarui aplikasi, atau menerbitkan kunci baru. `haruflow mcp` tanpa `check` tetap merupakan transport STDIO untuk Codex, bukan perintah tes interaktif.

## Menangani pesan kegagalan

- **Cannot open/read MCP token file:** pastikan path lengkap benar, berkas ada, UTF-8 tanpa BOM, dan akun Windows yang menjalankan Codex boleh membacanya. Jangan mengatasi masalah dengan membuka izin ke semua pengguna. Gunakan `Connect-Codex.ps1` untuk membuat berkas baru dengan ACL akun saat ini.
- **Set HARUFLOW_MCP_TOKEN_FILE …:** environment koneksi belum diberikan. Konfigurasi terminal dan konfigurasi MCP Codex adalah dua environment terpisah; keduanya perlu menunjuk berkas dan alamat yang sama.
- **MCP token must contain … / Invalid MCP token format:** berkas harus berisi satu baris kunci persis, tanpa tanda kutip, spasi tambahan atau BOM. Jangan menampilkan isinya lewat terminal. Untuk berkas yang rusak, buat kunci pengganti dan cabut kunci lama.
- **HARUFLOW_MCP_URL must be …:** gunakan origin HTTP loopback, misalnya `http://127.0.0.1:8080`, tanpa `/api`, nama pengguna/password, query atau fragment. URL LAN/Internet, HTTPS dan alamat singkat seperti `127.1` tidak didukung tahap ini.
- **Haruflow is unavailable …:** jalankan server Haruflow, pastikan healthcheck siap dan port benar. Jika server dalam Docker, port harus diterbitkan ke loopback Windows. Proxy tidak dipakai oleh bridge.
- **MCP access expired, was revoked, or the user is inactive …:** login kembali ke Haruflow dan periksa akun serta izin AI. Kunci yang dicabut/kedaluwarsa tidak dapat diaktifkan kembali; buat kunci pengganti sesuai izin lalu perbarui path berkas di Codex. Reset/ganti password dan perubahan akun juga mencabut kunci lama.
- **does not have permission … / no effective scopes …:** admin memeriksa peran akun dan izin AI; pengguna memeriksa lingkup kunci. Akses merupakan irisan ketiganya. Jangan memberikan peran admin untuk melewati pembatasan.
- **invalid MCP response/profile/tool list …:** pastikan origin menuju server Haruflow yang benar dan server serta bridge berasal dari paket yang kompatibel. Server lama, halaman HTML, hasil proxy/redirect atau katalog tidak sesuai akan ditolak. Diagnostik tidak meneruskan isi respons yang gagal.
- **size limit / timed out …:** untuk pemanggilan laporan, persempit tanggal atau pencarian. Jika pemeriksaan metadata sederhana sudah gagal, periksa log kesehatan server dan gunakan paket yang sesuai.

Setelah tes berhasil, restart koneksi MCP Codex dan cocokkan katalog tool. Perubahan izin tetap diperiksa pada setiap permintaan walaupun klien belum memuat ulang katalog. Untuk uji pertama gunakan pertanyaan dan pencocokan angka pada [panduan MCP](MCP.md); `connected: true` membuktikan koneksi dan katalog, bukan membuktikan kebenaran kesimpulan AI.

## Bantuan tanpa membagikan rahasia

Catat versi aplikasi/schema, waktu kegagalan, nama perintah dan pesan singkat. Sensor identitas akun jika membagikan JSON diagnostik ke pihak lain. Jangan mengirim isi berkas kunci, konfigurasi yang mengandung rahasia, `.env`, password database atau screenshot saat kunci baru muncul.

Jika perangkat hilang, kunci terpapar atau backup database dipulihkan, cabut kunci yang terdampak sebelum memakai koneksi kembali. Prosedur restore dan backup ada di [panduan operasi](OPERATIONS.md).

## Mencabut seluruh kunci sesudah restore

Restore dapat mengembalikan status kunci pada waktu backup. Sebelum server hasil pemulihan dibuka kembali untuk pengguna, operator database lokal dapat menjalankan perintah ini dari folder instalasi dengan konfigurasi database pemulihan yang benar:

```powershell
& 'C:\lokasi\haruflow.exe' admin revoke-ai-tokens 'admin@usaha.example'
```

Perintah mencabut **semua kunci AI**, bukan hanya kunci milik alamat email tersebut. EMAIL harus menunjuk akun admin Haruflow yang sudah ada dan masih aktif sebagai atribusi audit. Kewenangan menjalankannya berasal dari akses langsung operator ke database, seperti pemulihan password admin; mengetahui alamat email admin saja tidak memberikan akses. Tidak perlu server HTTP berjalan atau kunci MCP. Jangan menaruh password database atau kunci pada argumen terminal.

Sesudah pencabutan, pengguna yang masih mendapat izin AI membuat kunci pengganti masing-masing. Perbarui berkas kunci di perangkat mereka, jalankan `mcp check`, lalu sambungkan kembali Codex. Perintah pemulihan ini berbeda dari mode `mcp`/`mcp check`: ia memakai konfigurasi database dan sengaja mengubah status kunci serta mencatat audit. Jangan menjalankannya pada database yang salah.
