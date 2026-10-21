# Kas dan bank Community

Menu **Kas & bank** mengelola rekening IDR dalam satu perusahaan. Kas `1100` otomatis terdaftar pada upgrade; migrasi tidak mengubah jurnal atau saldo lama. Pengguna finance dan admin dapat menambah rekening, transfer, serta melakukan rekonsiliasi. Pengguna lain dapat melihat saldo dan riwayat transfer.

## Rekening dan transaksi

Tambahkan rekening dengan kode akun aset baru, nama, jenis kas/bank, serta identitas bank opsional. Kode yang sudah digunakan ditolak agar sebuah akun lama tidak berubah maknanya. Saldo rekening baru nol. Gunakan transfer dari rekening lain atau transaksi sumber untuk mencatat dana nyata. Seluruh rekening memakai IDR.

Pembayaran invoice, pembayaran supplier, uang muka, refund, dan refund langsung pada retur mendukung pilihan akun kas/bank. Permintaan API lama tanpa `cash_account_code` tetap memakai `1100`. Pilihan rekening tidak mengubah nilai tagihan maupun alokasi kredit. Pembalikan memakai rekening dari jurnal asli, termasuk transaksi lama.

Transfer meminta rekening sumber dan tujuan berbeda, nominal positif, alasan, dan referensi opsional. Layar tinjauan menunjukkan kas keluar pada sumber, kas masuk pada tujuan, dan perubahan total kas nol. Konfirmasi setelah transfer bank fisik benar-benar selesai. Server memeriksa saldo sumber di dalam transaksi yang memegang kunci pembukuan bersama; dua permintaan bersamaan tidak bisa membelanjakan saldo yang sama. Retry dengan kunci idempotensi yang sama menghasilkan satu posting.

Pembalikan transfer adalah jurnal baru dengan alasan; jurnal asli tetap tersimpan. Rekening tujuan semula harus cukup untuk mengembalikan dana. Periode pembukuan saat ini harus terbuka. Transfer internal dan pembalikannya tidak menambah arus kas eksternal pada laporan dan grafik dashboard. Laporan arus kas menjumlahkan semua rekening kas/bank terdaftar.

## Mutasi bank CSV

Pilih rekening di tab **Rekonsiliasi bank**, lalu **Impor mutasi CSV**. Isi referensi unik mutasi, periode, saldo awal dan saldo akhir sesuai laporan bank. File atau teks CSV maksimal 256 KiB / 1.000 transaksi; pemisah dapat koma atau titik koma. Header wajib:

```csv
date,reference,description,amount
2026-10-01,BANK-001,Setoran,1000000.00
2026-10-01,BANK-002,Pembayaran supplier,-250000.00
```

Tanggal memakai `YYYY-MM-DD`. Nominal masuk positif dan keluar negatif; gunakan titik untuk desimal, tanpa pemisah ribuan. Referensi per transaksi harus unik dalam file. Semua tanggal harus berada dalam periode, dan saldo awal ditambah jumlah mutasi harus sama dengan saldo akhir. CSV bertanda kutip mendukung koma dan kutip ganda yang di-escape. Periode impor pada rekening yang sama tidak boleh tumpang tindih.

Pratinjau menampilkan transaksi dan SHA-256 tanpa menulis database. Konfirmasi mengimpor baris bukti bank; **impor tidak membuat jurnal penerimaan atau pembayaran**, sehingga saldo tidak terhitung dua kali. Referensi/file yang sudah diimpor ditolak. Kunci idempotensi melindungi retry konfirmasi.

## Pencocokan dan pemeriksaan

Pilih **Cocokkan** pada baris mutasi, pilih baris jurnal yang benar, lalu isi alasan. Kandidat harus memiliki rekening dan nominal bertanda yang sama, serta belum digunakan baris bank lain. Tanggal bank dapat berbeda dari tanggal pembukuan; operator tetap memeriksa referensi. Tidak ada pencocokan otomatis maupun koneksi ke bank.

**Lepas cocok** membutuhkan alasan dan menyimpan riwayat. Versi baris mencegah dua pengguna menimpa perubahan satu sama lain. Pencocokan dan pelepasannya tidak mengubah jurnal maupun saldo. Pembalikan dokumen sumber tetap berupa baris jurnal baru dan harus diperiksa terhadap mutasi bank yang sesuai.

Status periode **Cocok** hanya muncul bila saldo awal dan akhir bank sama dengan buku besar pada tanggal perusahaan, semua baris bank sudah dicocokkan, dan tidak ada baris buku pada periode itu yang tertinggal. Jumlah cocok saja tidak cukup bila saldo berbeda. Jika transaksi bank belum ada dalam buku, catat melalui dokumen sumber yang benar sebelum mencocokkan.

Riwayat transfer utama menampilkan 500 transfer terakhir dan membaginya ke halaman. Dokumen bank dan peristiwa pencocokan tetap tersimpan untuk audit. Periksa file dan periode dengan teliti sebelum konfirmasi karena bukti impor bersifat permanen.
