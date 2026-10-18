# Status implementasi Community

Baseline v0.1 development, 18 Oktober 2026. Repository kini berisi aplikasi yang dapat dibangun dan dijalankan, bukan hanya proposal arsitektur.

## Sudah tersedia

- Rust/Axum modular monolith, PostgreSQL/SQLx, frontend React/TypeScript yang dibangun Bun dan disajikan oleh Rust.
- Setup perusahaan/admin, autentikasi lokal, role, pengguna, password reset oleh admin, rotasi password/sesi, dan audit append-only.
- CRUD pelanggan dan produk; penghapusan ditolak bila data terpakai. Produk menyimpan kuantitas, biaya rata-rata, dan nilai persediaan.
- Penerimaan stok awal, adjustment bertanda positif/negatif, kartu stok, dan pengiriman dengan penguncian stok serta biaya historis.
- Sales order draft/confirm/cancel, pengiriman penuh, invoice, pembayaran sebagian/penuh, dan reversal berurutan pembayaran → invoice → pengiriman.
- Bagan akun dasar serta akun tambahan, jurnal otomatis/manual, neraca saldo, dan buku besar dengan filter tanggal serta saldo awal/akhir.
- Penutupan periode berdasarkan zona waktu perusahaan; jurnal dan ledger posted tidak dapat diubah lewat aplikasi.
- Dashboard dengan data nyata, pencarian dan pagination pada UI, CSV, audit, serta tampilan invoice untuk pencetakan browser/PDF.
- Dockerfile multi-stage dan Compose aplikasi + PostgreSQL; migrasi eksplisit, health probe, konfigurasi acak, dan script backup/restore.
- GitHub Actions untuk Rust formatting/Clippy/tests, frontend build/tests, build image, dan uji transaksi serta backup/restore pada instalasi container sementara.

## Verifikasi

- 16 pengujian Rust lokal lulus, termasuk dua pengujian integrasi PostgreSQL yang mencakup rangkaian skenario bisnis serta identitas.
- Enam tes frontend lulus untuk presisi decimal, pembulatan nilai baris, format nominal, dan keamanan CSV.
- Build frontend, Cargo build, serta Clippy dengan warnings-as-errors lulus.
- Alur setup → pelanggan → produk → stok awal → order → pengiriman → invoice → pembayaran sebagian/lunas telah dijalankan lewat browser pada database preview terpisah.
- Script backup diperiksa untuk publikasi arsip lengkap, tidak menimpa file yang sudah ada, kegagalan dump, dan pembersihan file sementara milik proses sendiri.
- Hasil CI terbaru dapat diperiksa di [GitHub Actions](https://github.com/haruchankawaii/haruflow/actions). Status setiap run melekat pada commit yang diuji.

## Perbedaan dari rencana dan pekerjaan berikutnya

- Produk versi ini seluruhnya produk stok. Produk jasa/nonstok belum tersedia.
- Satu perusahaan, satu gudang logis, IDR, dan zona waktu awal Asia/Jakarta. Pengaturan multi-company/mata uang/gudang belum tersedia.
- Penerimaan persediaan mengkredit modal awal, sehingga ditujukan untuk saldo awal dan penambahan yang memang memakai akun tersebut. Purchasing/utang supplier belum diimplementasikan.
- Saldo awal kas dapat diposting melalui jurnal manual; proses pembukaan saldo awal per-periode khusus yang menjamin satu kali posting belum tersedia.
- Semua posting memakai waktu transaksi saat ini. Backdating, pembukaan kembali periode, dan koreksi jurnal manual langsung melalui tombol reversal belum tersedia.
- Belum ada lampiran, impor CSV, email, jobs/outbox, SSO, API plugin, serta OpenAPI/client generation. Baseline ini belum memiliki kebutuhan kirim event ke sistem luar.
- API listing master data belum memiliki pagination server. Jurnal/pergerakan dibatasi pada riwayat terbaru; buku besar memberi penanda bila tampilan terpotong setelah 5.000 baris dan tetap menghitung saldo dari seluruh rentang.
- Belum ada rilis image pada registry, installer end-user, atau deployment publik. Compose membangun image dari source lokal.
- Uji performa berskala produksi, pilot pengguna bisnis, latihan pemulihan berkala, dan release review masih diperlukan sebelum menyatakan versi stabil atau SLA.

Prioritas berikutnya: purchasing/utang → retur dan pengiriman sebagian → proses pembukaan saldo awal yang lebih terstruktur → impor data serta lampiran → laporan keuangan lengkap. Tetapkan kebutuhan dari pilot sebelum memperluas modul.
