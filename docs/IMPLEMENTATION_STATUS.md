# Status implementasi Community

Community v0.1 development, upgrade tahap pertama, 18 Oktober 2026. Aplikasi dapat dibangun dan dijalankan; seluruh rencana Community belum selesai.

## Sudah tersedia

- Rust/Axum modular monolith, PostgreSQL/SQLx, frontend React/TypeScript yang dibangun Bun dan disajikan oleh Rust.
- Setup perusahaan/admin, autentikasi lokal, role, pengguna, password reset oleh admin, rotasi password/sesi, dan audit append-only. Pemulihan admin melalui perintah lokal memakai input password tersembunyi dan mencabut seluruh sesi admin tersebut.
- Profil perusahaan, kontak, alamat, nomor pajak, dan awalan nomor pesanan/invoice dapat diubah oleh admin. Perubahan awalan mempertahankan nomor historis dan urutan nomor; zona waktu dapat diubah sebelum jurnal pertama diposting.
- CRUD pelanggan serta produk barang/jasa; penghapusan ditolak bila data terpakai. Barang menyimpan kuantitas, biaya rata-rata, dan nilai persediaan; jasa tidak memiliki saldo stok. Jenis produk dikunci setelah dipakai dan disimpan sebagai snapshot pada baris pesanan.
- Penerimaan stok awal, adjustment bertanda positif/negatif, kartu stok, dan pengiriman dengan penguncian stok serta biaya historis.
- Sales order draft/confirm/cancel, pengiriman penuh, invoice, pembayaran sebagian/penuh, dan reversal berurutan pembayaran → invoice → pengiriman. Pesanan jasa dapat langsung diinvoiskan setelah konfirmasi; pesanan campuran hanya mengirim dan menghitung biaya persediaan untuk barang.
- Bagan akun dasar serta akun tambahan, jurnal otomatis/manual, neraca saldo, dan buku besar dengan filter tanggal serta saldo awal/akhir.
- Penutupan periode berdasarkan zona waktu perusahaan; jurnal dan ledger posted tidak dapat diubah lewat aplikasi.
- Dashboard dengan data nyata, pencarian dan pagination pada UI, CSV, audit, serta tampilan invoice untuk pencetakan browser/PDF.
- Dockerfile multi-stage dan Compose aplikasi + PostgreSQL; migrasi eksplisit, health probe, konfigurasi acak, dan script backup/restore.
- GitHub Actions untuk Rust formatting/Clippy/tests, frontend build/tests, build image, dan uji transaksi serta backup/restore pada instalasi container sementara.

## Verifikasi

- 26 pengujian Rust lokal lulus, termasuk integrasi PostgreSQL untuk alur bisnis, identitas, profil perusahaan, pemulihan admin, produk jasa, pesanan campuran, dan reversal.
- 12 tes frontend lulus, mencakup presisi decimal, pembulatan nilai baris, format nominal, keamanan CSV, dan perilaku barang/jasa.
- Build frontend, Cargo build, serta Clippy dengan warnings-as-errors lulus.
- Uji migrasi versi 5 → 7 mempertahankan stok, riwayat transaksi, dan ledger. Upgrade database preview dilakukan setelah backup; jumlah catatan dan saldo sebelum/sesudah tetap sama.
- Alur setup → pelanggan → produk → stok awal → order → pengiriman → invoice → pembayaran sebagian/lunas telah dijalankan lewat browser pada database preview terpisah.
- QA browser lulus untuk perubahan kontak/awalan dokumen, pemisahan jasa dari penerimaan stok, dan pesanan jasa → konfirmasi → invoice langsung → pembayaran lunas. Invoice menampilkan kontak perusahaan; nomor dokumen lama dan stok barang tetap sama.
- Script backup diperiksa untuk publikasi arsip lengkap, tidak menimpa file yang sudah ada, kegagalan dump, dan pembersihan file sementara milik proses sendiri.
- Hasil CI terbaru dapat diperiksa di [GitHub Actions](https://github.com/haruchankawaii/haruflow/actions). Status setiap run melekat pada commit yang diuji.

## Perbedaan dari rencana dan pekerjaan berikutnya

- Satu perusahaan, satu gudang logis, dan mata uang IDR tetap. Zona waktu awal Asia/Jakarta dapat diubah sebelum jurnal pertama; setelah itu terkunci untuk menjaga tanggal laporan. Multi-company, banyak mata uang, dan banyak gudang belum tersedia.
- Penerimaan persediaan mengkredit modal awal, sehingga ditujukan untuk saldo awal dan penambahan yang memang memakai akun tersebut. Purchasing/utang supplier belum diimplementasikan.
- Saldo awal kas dapat diposting melalui jurnal manual; proses pembukaan saldo awal per-periode khusus yang menjamin satu kali posting belum tersedia.
- Semua posting memakai waktu transaksi saat ini. Backdating, pembukaan kembali periode, dan koreksi jurnal manual langsung melalui tombol reversal belum tersedia.
- Belum ada lampiran, impor CSV, email, jobs/outbox, SSO, API plugin, serta OpenAPI/client generation. Baseline ini belum memiliki kebutuhan kirim event ke sistem luar.
- API listing master data belum memiliki pagination server. Jurnal/pergerakan dibatasi pada riwayat terbaru; buku besar memberi penanda bila tampilan terpotong setelah 5.000 baris dan tetap menghitung saldo dari seluruh rentang.
- Belum ada rilis image pada registry, installer end-user, atau deployment publik. Compose membangun image dari source lokal.
- Uji performa berskala produksi, pilot pengguna bisnis, latihan pemulihan berkala, dan release review masih diperlukan sebelum menyatakan versi stabil atau SLA.

Prioritas berikutnya: proses saldo awal terstruktur dengan pemeriksaan dan satu kali posting → koreksi jurnal manual/penerimaan stok yang terhubung → purchasing/utang supplier → retur dan pengiriman sebagian → impor data serta lampiran → laporan keuangan lengkap. Tahap pertama ini melengkapi fondasi; purchasing dan fitur lanjutan tetap menjadi pekerjaan berikutnya.
