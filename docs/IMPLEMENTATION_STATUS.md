# Status implementasi Community

Community v0.1 development, 20 Oktober 2026. Pembayaran supplier, umur utang dan chart pembayaran, retur pelanggan/supplier, pemenuhan serta penagihan sebagian, dan laporan keuangan kini dilengkapi impor master CSV serta lampiran bukti transaksi. Hasil pengujian serta status CI dicatat per commit; seluruh rencana Community belum selesai.

## Alur operasional yang tersedia

- Setup perusahaan/admin, autentikasi lokal, role, pengelolaan pengguna, pemulihan admin lokal, rotasi password/sesi, serta audit append-only.
- Profil dan kontak perusahaan, nomor pajak, awalan nomor dokumen, serta zona waktu perusahaan. Nomor historis dipertahankan; zona waktu terkunci setelah jurnal pertama.
- Pelanggan, supplier berversi, produk barang/jasa, saldo stok, biaya rata-rata, nilai persediaan, kartu stok, dan adjustment. Jenis produk yang sudah dipakai dilindungi.
- Sales order draf → konfirmasi → pengiriman satu atau beberapa kali → satu atau beberapa invoice → pembayaran sebagian/penuh. Jasa dapat diinvoiskan tanpa pengiriman stok. Detail pesanan menunjukkan jumlah dipesan, dikirim, ditagih, dan sisa per baris. Lihat [pemenuhan penjualan](./SALES_FULFILLMENT.md).
- PO barang draf → konfirmasi → penerimaan sebagian/penuh → tagihan sebagian/penuh → pembayaran supplier sebagian/penuh. Satu PO dapat memiliki beberapa penerimaan, dan satu penerimaan dapat memiliki beberapa tagihan. Biaya mengikuti snapshot sumber; total alokasi mempertahankan pembulatan dokumen asal. Lihat [panduan pembelian](./PURCHASING.md).
- Pembayaran supplier melalui akun Kas 1100, nomor SP, referensi, jurnal, riwayat, idempotensi, pemeriksaan versi, saldo kas, periode terbuka, dan pembalikan beralasan. Sisa utang memperhitungkan kredit retur dan pembayaran bersih.
- Chart umur utang memakai lima kelompok: belum terlambat, 1–30, 31–60, 61–90, dan lebih dari 90 hari. Jatuh tempo hari ini belum terlambat. Chart pembayaran/refund/koreksi menyediakan rentang 3/6/12 bulan, rincian tagihan sumber, serta CSV dari seluruh sisa utang.
- Retur pelanggan atas barang/jasa dari invoice dan retur barang ke supplier dari tagihan, sebagian/penuh. Kredit memakai nilai historis, refund langsung memerlukan konfirmasi dana aktual, dan pembalikan mempertahankan semua dokumen/jurnal. Retur pelanggan memulihkan biaya pengiriman historis; retur supplier melepas biaya rata-rata saat ini dan mencatat selisih pada akun sistem. Lihat [panduan retur](./RETURNS.md) dan [rincian retur penjualan](./SALES_RETURNS.md).
- Koreksi pembayaran → invoice/tagihan → pengiriman/penerimaan mengikuti dependensi dokumen. Retur aktif harus dibalik lebih dahulu. Pembalikan stok memerlukan mutasi terakhir dan snapshot yang cocok; riwayat mutasi berikutnya tetap dapat memblokir koreksi meskipun mutasi tersebut telah dibalik.

## Impor dan bukti transaksi

- Impor master pelanggan, supplier, dan produk melalui CSV UTF-8, template koma/titik koma, preview wajib, kesalahan per baris/kolom, identitas duplikat, serta commit atomik yang memeriksa ulang data terbaru. Impor tidak mengubah stok, kas, atau jurnal. Hak tulis mengikuti master dan hanya pembuat preview dapat menyimpannya. Lihat [panduan impor](./IMPORTS.md).
- Lampiran PDF/PNG/JPEG pada sepuluh jenis dokumen/pembayaran, maksimum 5 MiB per file dan 20 file/50 MiB aktif per dokumen. Isi file serta SHA-256 berada dalam PostgreSQL, dengan unduhan setelah login, hak tulis per jenis dokumen, arsip/pemulihan beralasan, versi, dan riwayat. Lampiran tidak mengubah transaksi keuangan. Lihat [panduan lampiran](./ATTACHMENTS.md).

## Akuntansi dan laporan

- Bagan akun dasar/tambahan, jurnal otomatis/manual, neraca saldo, buku besar bertanggal, saldo awal/akhir, serta pembalikan jurnal manual. Akun kontrol stok, piutang, kewajiban pembelian, dan selisih retur dilindungi dari posting manual yang tidak sesuai.
- Saldo awal perusahaan melalui draf berversi dan posting atomik stok/jurnal/audit. Pembalikan sebelum transaksi operasional memulihkan snapshot; pembukaan bukan reset tahunan. Lihat [saldo awal](./OPENING_BALANCES.md).
- Penutupan periode memakai zona waktu perusahaan. Dokumen posted, mutasi, dan jurnal mempertahankan riwayat; koreksi membuat catatan lawan.
- **Laba rugi** berdasarkan seluruh akun pendapatan/beban selama periode. **Neraca** memakai saldo kumulatif sampai tanggal akhir dengan laba akumulasi. **Arus kas** metode langsung merekonsiliasi akun Kas 1100, dengan aktivitas operasi/investasi/pendanaan/belum terklasifikasi.
- Laporan menyediakan chart, tanggal inklusif menurut kalender perusahaan, CSV, tautan buku besar/jurnal, serta klasifikasi arus kas berversi dengan alasan dan riwayat. Klasifikasi mengubah metadata laporan tanpa mengubah jurnal. Lihat [panduan laporan keuangan](./FINANCIAL_REPORTS.md).
- Dashboard pendapatan/HPP serta mutasi kas 3/6/12 bulan berasal dari jurnal dalam satu snapshot. Pembalikan mengikuti bulan posting, termasuk nominal negatif dan bulan kosong. Posisi invoice memakai saldo aktif setelah kredit retur.

## Distribusi dan operasi

Rust/Axum modular monolith, PostgreSQL/SQLx, dan React/TypeScript yang dibangun Bun serta disajikan Rust. Dockerfile multi-stage dan Compose menyertakan migrasi eksplisit, health probe, konfigurasi acak, serta backup/restore. GitHub Actions menjalankan Rust formatting/Clippy/tests, frontend tests/build, build image, alur transaksi, pemulihan backup, dan pemulihan admin lokal. Lihat [operasi](./OPERATIONS.md).

## Bukti verifikasi

- Frontend: **58 tes, 302 assertions lulus**; TypeScript dan build produksi Vite lulus. Cakupan mencakup decimal/pembulatan, alokasi sebagian, sisa sen terakhir, preview kredit/refund, kalender, chart, CSV, versi/retry, rute retur, UTF-8/ukuran file impor, batas/izin lampiran, label riwayat, serta request file dan penanganan error.
- **131 pengujian Rust lulus**, termasuk migrasi data lama dan seluruh alur baru. Tambahan cakupan impor/lampiran memeriksa parser CSV, batas file/request, NUL/UTF-8, duplikasi dan perubahan master bersamaan, kepemilikan preview, commit atomik, unggahan bersamaan, batas jumlah/total ukuran, autentikasi unduhan, serta arsip/pemulihan dan immutability. Cargo build dan Clippy dengan warnings-as-errors lulus.
- Uji integrasi meliputi konkurensi/idempotensi, role, periode tertutup, kas/stok tidak cukup, perubahan satuan, batas uang, alokasi berlebih, lima batas umur utang menurut kalender perusahaan, kredit/refund, pembalikan persis, audit/immutability, serta laporan di luar batas daftar dokumen.
- Kedua database preview **migrasi 12 → 18** setelah backup. Perbandingan seluruh data historis sebelum/sesudah mempertahankan produk, stok, nilai, biaya rata-rata, supplier, PO, penerimaan, invoice/tagihan, mutasi, jurnal, baris jurnal, dan pembalikan. Kolom/relasi baru dibackfill tanpa membuat transaksi stok atau jurnal operasional baru.
- Smoke pada instalasi lokal sementara lulus untuk pembayaran supplier sebagian/lunas beserta pembalikannya; lima kelompok umur utang; retur pelanggan/supplier termasuk refund dan pembalikan; penerimaan/pengiriman/invoice/tagihan sebagian dengan pembulatan; serta ketiga laporan dan klasifikasinya.
- Smoke terbaru lulus setelah **backup → perubahan data → restore → verifikasi**, termasuk impor master dan isi lampiran. Hasil akhir: **50 jurnal, 25 mutasi, 4 produk, 2 pelanggan, 2 supplier, 3 pesanan penjualan, 3 PO, dan 7 tagihan supplier**. Stok barang utama 14/Rp760.000, kas Rp270.000, utang Rp180.000, penampung nol, laba kotor Rp70.000, dan neraca saldo Rp1.130.000 per sisi tetap cocok. Server pengujian sementara dihentikan setelah verifikasi.
- Smoke impor mencakup template, UTF-8 BOM, CRLF, alamat berkutip beberapa baris, pemisah titik koma, tiga batch tersimpan, preview tidak valid tanpa master parsial, serta retry commit. Smoke lampiran mencakup seluruh 10 jenis dokumen, unggahan PDF lebih dari 256 KiB, PNG arsip, retry, arsip/pemulihan, 11 file beserta 14 event, autentikasi unduhan, dan kecocokan byte/SHA-256 sesudah restore. Impor serta file yang ditambahkan setelah backup hilang sebagaimana mestinya ketika backup dipulihkan.
- Kedua database preview **migrasi 18 → 20** setelah backup. Jumlah baris dan hash isi seluruh tabel historis dibandingkan sebelum/sesudah migrasi dan tetap sama; kedua migrasi hanya menambah penyimpanan impor/lampiran.
- QA browser impor lulus pada demo terpisah: satu baris pelanggan tanpa nama menghasilkan kesalahan baris/kolom dan tidak menyediakan simpan; file valid dengan nama berkoma dan alamat beberapa baris menambahkan dua pelanggan setelah konfirmasi. Master dari baris valid pada file yang ditolak tetap tidak dibuat. Hasil dan waktu simpan muncul pada riwayat.
- QA browser lampiran lulus: unggah PNG pada PB-000002 → unduh → arsip dengan alasan → tampilkan arsip → pulihkan dengan alasan. SHA-256 unduhan cocok dengan file sumber, versi menjadi 3, dan ketiga event berlabel Indonesia tetap terlihat. Setelah seluruh QA impor/lampiran, kas demo Rp90.000, persediaan Rp680.000, utang Rp120.000, dan 16 jurnal tetap sama.
- QA browser pada perusahaan demo: pembayaran SP-000001 Rp60.000 mengurangi utang PB-000002 dari Rp180.000 menjadi Rp120.000. Retur satu unit memberi kredit Rp60.000, lalu pembalikannya memulihkan stok/biaya persis dan menyimpan riwayat. Hasil akhir demo: stok 13/Rp680.000, kas Rp90.000, utang Rp120.000, ekuitas Rp650.000.
- QA laporan browser: neraca menunjukkan aset Rp770.000 = liabilitas Rp120.000 + ekuitas Rp650.000. Arus kas menunjukkan operasi neto −Rp60.000, saldo awal pembukuan neto Rp150.000, dan saldo akhir Rp90.000. Tautan sumber membuka jurnal pembayaran dengan debit utang/kredit kas Rp60.000.
- Uji browser tahap sebelumnya tetap menjadi bukti untuk setup, barang/jasa, penjualan, pembayaran pelanggan, profil perusahaan, saldo awal/pembalikan, jurnal manual/pembalikan, supplier/PO, penerimaan/pembalikan, dan tagihan pengganti. HTML memakai `Cache-Control: no-store` untuk pemilihan bundle terbaru setelah upgrade.
- Status CI tiap commit dapat diperiksa pada [GitHub Actions](https://github.com/haruchankawaii/haruflow/actions). Kelulusan lokal tidak menggantikan hasil job container di CI.

## Batas saat ini dan pekerjaan berikutnya

- Satu perusahaan, satu gudang logis, dan IDR. Multi-company, banyak gudang, banyak mata uang, HR/payroll, manufaktur, dan POS belum tersedia.
- Pembelian belum mendukung jasa, selisih harga invoice dari penerimaan, pajak/ongkos tambahan, uang muka, atau retur komersial langsung dari penerimaan yang belum ditagih. Retur tidak membuka kembali jumlah pemenuhan atau penagihan pesanan asal.
- Refund dicatat langsung bersama retur; refund tertunda, saldo kredit lintas dokumen, serta pemindahan kredit belum tersedia. Aplikasi mencatat perpindahan kas aktual dan tidak melakukan transfer bank otomatis.
- Arus kas mencakup akun Kas 1100. Rekening bank/aset tambahan belum otomatis menjadi kas/setara kas. Klasifikasi berlaku per jurnal; pembagian satu jurnal ke beberapa aktivitas serta tutup buku tahunan otomatis belum tersedia.
- Saldo awal hanya sebelum transaksi operasional. Pembukaan piutang/utang per pelanggan/supplier/dokumen belum tersedia. Backdating dan pembukaan kembali periode belum tersedia. Mutasi lama sebelum migrasi snapshot tidak dapat ditebak biaya sebelumnya; satuan pengiriman lama dibackfill dari katalog saat upgrade.
- Impor dokumen transaksi, email, jobs/outbox, SSO, API plugin, OpenAPI/client generation, dan pagination API master data belum tersedia. Lampiran belum mendukung penghapusan permanen, retensi otomatis, storage eksternal, atau pembacaan nota otomatis. Daftar operasional dibatasi pada dokumen terbaru; laporan/saldo menghitung seluruh data. Buku besar menandai rincian yang terpotong setelah 5.000 jurnal.
- Belum ada rilis image registry, installer pengguna, atau deployment publik. Pilot bisnis, uji performa, latihan pemulihan berkala, dan release review masih diperlukan untuk kesiapan rilis stabil.

Prioritas selanjutnya: saldo awal piutang/utang per pihak, penguatan alur kredit/refund serta uang muka, lalu persiapan pilot/rilis. Rincian prioritas ada pada [rencana Community](./COMMUNITY_PLAN.md).
