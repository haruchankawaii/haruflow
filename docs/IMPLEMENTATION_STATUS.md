# Status implementasi Community

Community v0.1 development, upgrade tahap ketiga, 18 Oktober 2026. Koreksi jurnal/penerimaan dan chart dashboard melengkapi saldo awal, profil perusahaan, serta produk jasa; seluruh rencana Community belum selesai.

## Sudah tersedia

- Rust/Axum modular monolith, PostgreSQL/SQLx, frontend React/TypeScript yang dibangun Bun dan disajikan oleh Rust.
- Setup perusahaan/admin, autentikasi lokal, role, pengguna, password reset oleh admin, rotasi password/sesi, dan audit append-only. Pemulihan admin melalui perintah lokal memakai input password tersembunyi dan mencabut seluruh sesi admin tersebut.
- Profil perusahaan, kontak, alamat, nomor pajak, dan awalan nomor pesanan/invoice dapat diubah oleh admin. Perubahan awalan mempertahankan nomor historis dan urutan nomor; zona waktu dapat diubah sebelum jurnal pertama diposting.
- CRUD pelanggan serta produk barang/jasa; penghapusan ditolak bila data terpakai. Barang menyimpan kuantitas, biaya rata-rata, dan nilai persediaan; jasa tidak memiliki saldo stok. Jenis produk dikunci setelah dipakai dan disimpan sebagai snapshot pada baris pesanan.
- Penerimaan stok awal, adjustment bertanda positif/negatif, kartu stok, dan pengiriman dengan penguncian stok serta biaya historis.
- Pembalikan penerimaan baru yang memiliki snapshot posisi sebelumnya dan belum diikuti mutasi produk lain. Jumlah, nilai, dan biaya rata-rata dipulihkan persis; dokumen asal, alasan, serta jurnal pembalik tetap terhubung. Urutan mutasi baru menggunakan nomor monoton agar waktu awal transaksi tidak keliru menentukan urutan.
- Sales order draft/confirm/cancel, pengiriman penuh, invoice, pembayaran sebagian/penuh, dan reversal berurutan pembayaran → invoice → pengiriman. Pesanan jasa dapat langsung diinvoiskan setelah konfirmasi; pesanan campuran hanya mengirim dan menghitung biaya persediaan untuk barang.
- Bagan akun dasar serta akun tambahan, jurnal otomatis/manual, neraca saldo, dan buku besar dengan filter tanggal serta saldo awal/akhir.
- Pembalikan jurnal manual untuk Finance/Admin, dengan debit/kredit setiap baris dibalik persis, perlindungan posting ganda, alasan, dan tautan riwayat. Jurnal otomatis tetap dikoreksi melalui dokumen asal. Lihat [panduan koreksi dan chart](./CORRECTIONS_AND_CHARTS.md).
- Saldo awal perusahaan melalui draf berversi, pemeriksaan keseimbangan, dan posting atomik stok/jurnal/audit. Persediaan otomatis ke akun kontrol 1300; saldo akun lain memakai aset, liabilitas, atau ekuitas. Pembalikan sebelum transaksi operasional mengembalikan dokumen ke draf dengan snapshot historis yang tetap tersimpan. Lihat [panduan saldo awal](./OPENING_BALANCES.md).
- Penutupan periode berdasarkan zona waktu perusahaan; jurnal dan ledger posted tidak dapat diubah lewat aplikasi.
- Dashboard dengan chart pendapatan/HPP dan mutasi kas berperiode 3/6/12 bulan, bulan kosong bernilai nol, nilai negatif, serta rincian nominal. Chart pembayaran invoice menunjukkan posisi aktif seluruh waktu. Data berasal dari jurnal dan invoice dalam satu snapshot; pembalikan mengikuti bulan posting di zona waktu perusahaan. Pencarian/pagination UI, CSV, audit, dan cetak invoice tetap tersedia.
- Dockerfile multi-stage dan Compose aplikasi + PostgreSQL; migrasi eksplisit, health probe, konfigurasi acak, dan script backup/restore.
- GitHub Actions untuk Rust formatting/Clippy/tests, frontend build/tests, build image, dan uji transaksi serta backup/restore pada instalasi container sementara.

## Verifikasi

- 44 pengujian Rust lokal lulus: 43 dalam suite penuh, ditambah regresi cache HTML setelah QA. Cakupan baru termasuk inversi jurnal per baris, pemulihan biaya rata-rata persis, snapshot penerimaan lama, urutan mutasi meskipun timestamp transaksi lebih awal, retry/konkurensi, periode tertutup, angka chart, bulan kosong, koreksi negatif, dan batas bulan lokal.
- 22 tes frontend lulus, mencakup presisi decimal, pembulatan, keamanan CSV, barang/jasa, saldo awal, serta skala chart positif/negatif, nominal besar, dan tanggal kalender.
- Build frontend, Cargo build, serta Clippy dengan warnings-as-errors lulus.
- Uji migrasi versi 5 → 9 mempertahankan stok, riwayat transaksi, dan ledger. Kedua database preview versi 8 → 9 di-upgrade setelah backup; jumlah catatan, stok, nilai, dan biaya rata-rata sebelum/sesudah tetap sama.
- Alur setup → pelanggan → produk → stok awal → order → pengiriman → invoice → pembayaran sebagian/lunas telah dijalankan lewat browser pada database preview terpisah.
- QA browser lulus untuk perubahan kontak/awalan dokumen, pemisahan jasa dari penerimaan stok, dan pesanan jasa → konfirmasi → invoice langsung → pembayaran lunas. Invoice menampilkan kontak perusahaan; nomor dokumen lama dan stok barang tetap sama.
- QA browser saldo awal menggunakan perusahaan demo terpisah: draf belum seimbang tidak dapat diposting dan belum mengubah neraca saldo; posting kas Rp100.000 serta stok Rp500.000 menghasilkan debit/kredit Rp600.000. Pembalikan mengembalikan stok dan akun ke nol tanpa menghapus riwayat.
- QA browser koreksi lulus: penerimaan uji Rp140.000 dibalik dan stok kembali ke 10 unit senilai Rp500.000 dengan biaya rata-rata Rp50.000. Jurnal kas uji Rp1.000.000 dibalik; kas kembali Rp150.000 dan neraca saldo Rp650.000 per sisi. Riwayat serta tautan asal/pembalik tetap terlihat.
- QA chart lulus untuk pilihan 3/6/12 bulan, rincian bulanan, fokus nominal, dan posisi invoice. Pada preview lama, pendapatan Rp260.000 dan HPP Rp100.000 menghasilkan laba kotor Rp160.000; chart memakai angka yang sama dengan jurnal. HTML aplikasi memakai `Cache-Control: no-store` agar browser tidak menyimpan halaman pemilih bundle lama setelah upgrade.
- Uji container mencakup pembukaan → pembalikan → perbaikan draf → posting ulang → penjualan barang/jasa → backup/restore → pemulihan admin. Riwayat dua posting, satu posting aktif, alasan pembalikan, stok, dan neraca saldo tetap cocok sesudah restore.
- Uji container tambahan mencakup pembatalan/penggantian penerimaan, pembalikan jurnal manual, dan angka chart sesudah restore: 13 jurnal, 7 mutasi, stok 11, persediaan Rp580.000, kas Rp270.000, laba kotor Rp70.000, dan neraca saldo Rp950.000 per sisi.
- Script backup diperiksa untuk publikasi arsip lengkap, tidak menimpa file yang sudah ada, kegagalan dump, dan pembersihan file sementara milik proses sendiri.
- Hasil CI terbaru dapat diperiksa di [GitHub Actions](https://github.com/haruchankawaii/haruflow/actions). Status setiap run melekat pada commit yang diuji.

## Perbedaan dari rencana dan pekerjaan berikutnya

- Satu perusahaan, satu gudang logis, dan mata uang IDR tetap. Zona waktu awal Asia/Jakarta dapat diubah sebelum jurnal pertama; setelah itu terkunci untuk menjaga tanggal laporan. Multi-company, banyak mata uang, dan banyak gudang belum tersedia.
- Penerimaan persediaan mengkredit modal awal, sehingga ditujukan untuk saldo awal dan penambahan yang memang memakai akun tersebut. Purchasing/utang supplier belum diimplementasikan.
- Saldo awal hanya satu konteks pembukaan perusahaan sebelum jurnal atau mutasi stok operasional, bukan reset tahunan. Pembukaan piutang per pelanggan/tagihan belum tersedia; akun kontrol piutang tidak dapat diisi langsung. Instalasi yang sudah bertransaksi tidak dapat menambahkan pembukaan baru, dan saldo awal tidak dapat dibalik setelah transaksi lain berjalan.
- Semua posting memakai waktu transaksi saat ini. Backdating dan pembukaan kembali periode belum tersedia. Penerimaan sebelum migrasi 9 tidak memiliki snapshot biaya dan tidak dapat dibalik melalui tombol baru. Penerimaan yang sudah diikuti mutasi lain tetap perlu diperiksa dan dikoreksi melalui penyesuaian stok.
- Belum ada lampiran, impor CSV, email, jobs/outbox, SSO, API plugin, serta OpenAPI/client generation. Baseline ini belum memiliki kebutuhan kirim event ke sistem luar.
- API listing master data belum memiliki pagination server. Jurnal/pergerakan dibatasi pada riwayat terbaru; buku besar memberi penanda bila tampilan terpotong setelah 5.000 baris dan tetap menghitung saldo dari seluruh rentang.
- Belum ada rilis image pada registry, installer end-user, atau deployment publik. Compose membangun image dari source lokal.
- Uji performa berskala produksi, pilot pengguna bisnis, latihan pemulihan berkala, dan release review masih diperlukan sebelum menyatakan versi stabil atau SLA.

Prioritas berikutnya: purchasing/utang supplier → retur dan pengiriman sebagian → impor data serta lampiran → laporan keuangan lengkap. Tiga tahap upgrade ini melengkapi fondasi; purchasing dan fitur lanjutan tetap menjadi pekerjaan berikutnya.
