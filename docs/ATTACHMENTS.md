# Lampiran bukti transaksi

Lampiran menghubungkan bukti PDF, PNG, atau JPEG dengan dokumen Haruflow. Contohnya invoice supplier pada tagihan, surat jalan pada penerimaan, dan bukti transfer pada pembayaran. Menambahkan atau mengarsipkan bukti tidak mengubah nominal dokumen, status pembayaran, stok, maupun jurnal.

## Membuka dan menambahkan bukti

Buka rincian dokumen atau pembayaran, lalu bagian **Lampiran**. Pilih file, periksa nama/jenis/ukurannya, dan unggah. File hanya diterima jika jenis yang dinyatakan sesuai dengan isi awal file yang dikenali server. Mengganti ekstensi file tidak mengubah jenisnya.

Jenis yang didukung adalah **PDF, PNG, dan JPEG**. Batasnya **5 MiB per file**, **20 lampiran aktif per dokumen**, dan **50 MiB lampiran aktif per dokumen**. Kuota diperiksa lagi pada server, termasuk ketika beberapa pengguna mengunggah atau memulihkan lampiran bersamaan.

Nama file harus 1–200 karakter, tanpa spasi tepi, pemisah direktori, atau karakter kontrol; nama `.` dan `..` ditolak. Nama dipakai sebagai keterangan unduhan, bukan sebagai path penyimpanan pada server. Isi lampiran disimpan bersama metadata ukuran dan SHA-256. Permintaan unggah ulang yang sama tidak membuat file ganda.

Lampiran tersedia untuk 13 jenis sumber: pesanan penjualan, invoice, pembayaran pelanggan, PO, penerimaan PO, tagihan supplier, pembayaran supplier, jurnal, retur pelanggan, retur supplier, sumber dana pihak, refund aktual, serta rincian saldo pembukaan. Lampiran pada pembayaran/refund menggunakan identitas transaksi tersebut, terpisah dari invoice, tagihan, atau sumber dana induknya. Pada **Saldo pihak**, buka **Bukti sumber dana** atau rincian refund; bukti tagihan lama dapat ditambahkan pada rincian pembukaan.

## Hak akses dan unduhan

Semua pengguna yang sudah masuk dapat membaca lampiran sesuai cakupan satu perusahaan Haruflow saat ini. File tidak mempunyai URL publik tanpa autentikasi. Unduhan dikirim sebagai file lampiran, bukan halaman yang dijalankan atau ditampilkan di dalam aplikasi.

- Administrator dapat menambahkan, mengarsipkan, dan memulihkan seluruh jenis lampiran.
- Sales mengelola lampiran pesanan penjualan.
- Warehouse mengelola lampiran PO dan penerimaan PO.
- Finance mengelola lampiran invoice, pembayaran, tagihan, jurnal, retur, sumber dana pihak, refund aktual, serta rincian pembukaan.

Mengunggah bukti pembayaran tidak mencatat pembayaran secara otomatis. Gunakan alur pembayaran untuk mencatat dana aktual, lalu hubungkan buktinya. Lampiran juga tidak menjalankan transfer bank, membaca isi nota secara otomatis, atau menentukan kebenaran nilai dokumen.

## Arsip, pemulihan, dan riwayat

Jika bukti salah atau sudah diganti, **arsipkan** dengan alasan. File, pengunggah, waktu, dan riwayat tindakannya tetap disimpan. Bukti yang diarsipkan masih dapat diunduh dari riwayat; pengarsipan tidak menghapus isinya secara permanen.

Lampiran arsip dapat **dipulihkan** dengan alasan, selama kuota aktif masih cukup. Perubahan memakai versi sehingga tindakan pengguna lain tidak tertimpa. Konflik versi memerlukan muat ulang dan pemeriksaan keadaan terbaru sebelum mengulang tindakan.

Pengarsipan melepaskan kuota lampiran aktif, tetapi tidak mengurangi ukuran data historis database atau backup. Penghapusan permanen dan retensi otomatis belum disediakan. Dokumen atau jurnal yang sudah dibalik tetap menyimpan bukti serta riwayatnya.

## Backup dan upgrade

Isi file disimpan sebagai `BYTEA` di PostgreSQL. Backup database menggunakan `pg_dump -Fc` ikut menyimpan seluruh lampiran aktif, arsip, dan riwayatnya. Tidak ada volume lampiran atau direktori upload tambahan yang perlu disinkronkan secara terpisah dari backup database.

Setelah restore, periksa daftar dokumen, lampiran aktif/arsip, alasan dan versi, lalu unduh contoh file dan cocokkan ukuran serta SHA-256. Memeriksa nama file saja belum membuktikan bahwa isi bukti ikut pulih.

Ukuran dan waktu backup mengikuti pertumbuhan file yang disimpan. Gunakan prosedur [backup/restore](./OPERATIONS.md) serta versi aplikasi yang memahami migrasi lampiran. File sumber pada komputer pengguna tetap dikelola pengguna.
