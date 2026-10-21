# Pilot Community: usaha kecil di Windows

Status: fixture sintetis dapat diuji otomatis; penerimaan oleh usaha nyata belum dilakukan. Nama usaha, jenis usaha, volume, dan data nyata belum diberikan. Jangan menandai pilot diterima hanya dari CI atau screenshot demo.

## Persiapan operator

- Catat nama/jenis usaha, Windows/CPU/RAM, PostgreSQL, commit/schema dan lokasi backup. Mulai dengan satu perusahaan IDR dan paling banyak 10 akun aktif sesuai tugas.
- Sepakati tanggal mulai, saldo awal fisik/kas/piutang/utang dan cara menghindari impor transaksi yang sudah termasuk saldo awal.
- Pilih rekening/gudang seperlunya. Gunakan gudang utama bila hanya satu lokasi. Tetapkan unit dasar; lot/seri hanya pada produk yang memang memerlukannya.
- Pulihkan backup ke database uji kosong dan cocokkan saldo serta bukti sebelum memakai data nyata.

## Uji penerimaan bersama pemilik

- Beli barang/jasa, terima sebagian, tagih dengan selisih/ongkir/diskon/pajak yang disetujui, bayar sebagian melalui rekening yang benar, lalu koreksi/retur.
- Jual barang/jasa, kirim/tagih sebagian, terima DP/pembayaran, alokasikan kredit dan refund; cocokkan stok, kas dan sisa tagihan.
- Transfer antar rekening dan gudang; cocokkan mutasi bank tanpa jurnal ganda; uji lot/seri/reservasi bila digunakan.
- Impor salinan kecil transaksi lama, periksa tanggal, aging, buku pembantu dan akun kontrol; hentikan bila saldo awal menjadi ganda.
- Review laba rugi/neraca/arus kas; lakukan tutup tahun di database uji, pembalikan dan koreksi, pastikan laporan tetap cocok.
- Restart server, ulangi permintaan yang terputus, backup → perubahan → restore. Pastikan satu efek transaksi dan byte bukti tetap sama.

## Catatan penerimaan yang harus diisi manusia

Nama usaha: ____; pemilik/operator: ____; tanggal: ____; build/schema: ____.

Jumlah transaksi uji: ____; saldo stok/kas/piutang/utang sebelum dan sesudah: ____.

Backup yang dipulihkan dan hasil pencocokan: ____; masalah tersisa dan penanggung jawab: ____.

Keputusan pemilik (diterima/perlu perbaikan), batas pemakaian, dan persetujuan mulai operasional: ____.

## Uji sintetis pengembang

`node scripts/community-readiness.mjs` membuat database `haruflow_readiness_<acak>` sendiri, menjalankan pembelian campuran, rekening/rekonsiliasi, stok lokasi, impor historis, tutup tahun, penghentian server di tengah transaksi, retry/restart, serta backup/restore. Hanya proses/database buatannya yang dihentikan/dihapus. Konfigurasi `HARUFLOW_READINESS_BINARY`, `HARUFLOW_READINESS_POSTGRES_URL`, `HARUFLOW_READINESS_PSQL`, `HARUFLOW_READINESS_PG_DUMP`, `HARUFLOW_READINESS_PG_RESTORE` menunjuk build/alat uji. Bukti disimpan di output/readiness/ tanpa kredensial.
