# Penerimaan MCP dengan data usaha sintetis

`scripts/mcp-acceptance.mjs` memeriksa binary Haruflow, server HTTP sungguhan, PostgreSQL, dan jembatan STDIO dalam satu alur. Tes tidak menghubungi model AI, mengaktifkan koneksi Codex pribadi, atau mengubah preview/data usaha. Jalankan binary yang sudah dibangun dari perubahan yang ingin diuji.

```powershell
cargo build --locked
node scripts/mcp-acceptance.mjs
```

Binary bawaan adalah `target/debug/haruflow.exe` pada Windows dan `target/debug/haruflow` pada platform lain. PostgreSQL bawaan `postgres://postgres@127.0.0.1:55432/postgres` harus merupakan layanan pengujian lokal; pengguna memerlukan hak membuat dan menghapus database. Skrip membuat nama database acak sendiri, meminta port HTTP kosong, memigrasikan schema 31 dan mengisi data sintetis melalui API ERP. Skrip tidak menerima nama database target yang sudah ada.

Override opsional melalui environment:

- `HARUFLOW_MCP_ACCEPTANCE_BINARY`: binary debug, release atau paket yang diuji.
- `HARUFLOW_MCP_ACCEPTANCE_POSTGRES_URL`: URL PostgreSQL pengujian lokal; gunakan mekanisme environment terlindungi untuk rahasianya, jangan tempelkan ke chat atau commit.
- `HARUFLOW_MCP_ACCEPTANCE_PSQL` dan `HARUFLOW_MCP_ACCEPTANCE_PG_DUMP`: path utilitas PostgreSQL yang kompatibel dengan server.
- `HARUFLOW_MCP_ACCEPTANCE_CHECK=1`: juga jalankan diagnostik CLI `haruflow mcp check`, cocokkan scope/katalog tool dan simpan hasil aman. Gunakan binary yang sudah memuat perintah baru ini; opsi tidak memanggil laporan bisnis atau model.

Untuk proses utilitas PostgreSQL, semua environment berawalan `PG` yang diwarisi dibuang terlebih dahulu, termasuk `PGHOSTADDR`, `PGSERVICE`, `PGSERVICEFILE`, `PGPASSFILE` dan `PGOPTIONS`. Host/port/pengguna/password kemudian diisi dari URL lokal eksplisit dan nama database selalu milik run tersebut. Override PostgreSQL ambient juga dibuang untuk proses migrasi/server SQLx dan jembatan. Environment proses pemanggil tidak diubah. Ini mencegah konfigurasi koneksi lain mengalihkan pembuatan, ekspor atau penghapusan database ke target lain.

Kredensial setup, password pengguna, sesi browser dan kunci MCP dibuat acak dalam memori. Argumen proses tidak membawa kunci/password. Child process mewarisi environment pemanggil dengan override koneksi masing-masing; jangan menjalankan skrip di lingkungan yang membagikan environment proses kepada pengguna tidak tepercaya. Rahasia dan informasi kontak tidak ditulis ke bukti hasil.

## Data dan hasil yang diketahui

Periode saat ini menggunakan bulan dan tanggal perusahaan dalam `Asia/Jakarta`. Semua nilai uang di bawah dalam IDR dan sengaja kecil agar perhitungannya mudah diperiksa; ini bukan rekomendasi nominal untuk usaha.

Saldo awal menempatkan kas 1000, persediaan 10 pcs dengan biaya 20 per pcs, dan piutang 200 terhadap ekuitas 1400. Impor historis tahun 2020 menambah invoice pelanggan 100, pelunasan 40, serta tagihan supplier jasa 150. Saldo awal diposting pada waktu tes; tanggal dokumen lama pada rincian piutang tidak memindahkan tanggal posting saldo awal ke tahun 2020.

Penjualan operasional saat ini terdiri atas barang 2 pcs × 50 dan jasa 80. Pelanggan membayar 60; biaya barang terjual 40 dan pengeluaran kas jasa 30. Transfer 100 dari kas ke rekening bank terdaftar tidak mengubah kas gabungan.

Angka yang harus dipenuhi:

- Pendapatan periode 180, beban 70, laba bersih 110.
- Aset 1610, utang 150 dan neraca seimbang.
- Kas awal periode 40, kas masuk 1060, keluar 30, perubahan 1030, kas akhir gabungan 1070. Kas saldo awal 1000 masuk arus periode tetapi bukan pendapatan.
- Piutang saat ini 380: saldo awal 200 + impor tersisa 60 + invoice barang tersisa 40 + invoice jasa 80. Utang supplier 150. Pada cutoff `2020-03-01`, piutang hanya 60 dan utang 150.
- Empat dokumen piutang; `limit:1` hanya mengirim satu dokumen, sedangkan total tetap lengkap. Due date tidak diketahui berjumlah 200 dan tidak boleh otomatis dinyatakan menunggak.
- Dua invoice operasional berjumlah 180, pelunasan status sekarang 60, sisa 120. Ringkasan operasional tidak memasukkan impor historis.
- Stok saat ini 8 pcs; satu produk lain bersatuan kg tidak memiliki stok tersedia. Data tidak menyediakan ambang minimum, lead time, atau riwayat permintaan untuk menyimpulkan jumlah pembelian.

Sepuluh kelompok pemeriksaan mencakup keenam tool, metadata/sumber, perbedaan ledger dan invoice operasional, arus kas lintas rekening, cutoff AR/AP, stok tanpa biaya, pagination bukti, catatan tidak tepercaya, izin gudang, larangan mutasi, validasi argumen, pencabutan pada sesi STDIO yang masih hidup, serta audit tanpa rahasia. Hash tabel bisnis sebelum dan setelah pembacaan harus tetap sama; metadata penggunaan dan audit boleh bertambah.

## Bukti dan pembersihan

Hasil tersimpan di `output/mcp-acceptance/<nama-database-acak>/`:

- `evidence.json`: identitas build/schema, periode, hasil pemeriksaan, respons keenam tool yang aman, ID sumber sintetis, dan rubrik percakapan.
- `schema-only.backup`: arsip struktur database tanpa baris data, sesi, password atau kunci; ukuran dan SHA-256 ada di bukti JSON. Ini bukan backup fixture bisnis untuk restore.
- `cleanup.json`: bukti bahwa proses milik tes berhenti dan database milik tes dihapus.

Bukti diekspor sebelum database dihapus. Kegagalan pemeriksaan tetap mengekspor bukti gagal dan membersihkan sumber daya milik tes. Jika ekspor bukti gagal, database sintetis dipertahankan agar kegagalan dapat diperiksa; proses tetap dihentikan. Skrip hanya menghentikan handle child process sendiri dan menghapus database acak yang dibuat oleh run tersebut. Interupsi keras OS dapat memerlukan pembersihan manual; selalu cocokkan nama database pada bukti sebelum tindakan manual.

## Penerimaan jawaban Codex yang terpisah

Kelulusan skrip membuktikan transport, otorisasi, dan fakta laporan. Kelulusan ini **belum membuktikan mutu jawaban bahasa alami model**. `model_review.model_invoked` selalu `false` dan status awal `pending`.

`model_review.cases` menyediakan enam pertanyaan beserta fakta wajib dan jawaban yang harus ditolak. Untuk pemeriksaan Codex sungguhan, gunakan instalasi/data sintetis yang setara atau data usaha yang sudah diizinkan, koneksi pribadi sesuai lingkup, dan akun gudang terpisah. Fixture skrip langsung dibersihkan sehingga kunci dan link origin hasil tes tidak menjadi koneksi siap pakai setelah run selesai. Jangan menganggap tanya jawab terhadap file bukti offline sebagai tes koneksi MCP langsung.

Operator mencatat model/client yang digunakan, waktu, lingkup, tool yang dipanggil, jawaban dan sumbernya, lalu menilai setiap kasus. Jawaban harus menyebut periode/zona waktu, membedakan saldo dan aktivitas, menyatakan halaman/cakupan yang terpotong, tidak menganggap penjelasan sebab pasti hanya dari total, tidak mengikuti instruksi dalam uraian jurnal, dan menolak operasi tulis atau data di luar izin. Status review hanya boleh dinaikkan setelah bukti percakapan nyata tersedia. Pilot pemilik usaha tetap merupakan penerimaan terpisah.
