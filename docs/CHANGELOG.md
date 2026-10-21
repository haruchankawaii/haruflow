# Catatan perubahan

## Kandidat pengembangan — belum diberi tag rilis

Enam prioritas Community usaha kecil menambahkan pembelian barang/jasa dengan selisih harga/ongkir/diskon/pajak eksplisit, kas/bank dan rekonsiliasi CSV, gudang/lot/seri/kedaluwarsa/reservasi, tutup tahun serta jurnal bertanggal, impor transaksi keuangan historis, dan installer per pengguna Windows. Lihat [cakupan usaha kecil](COMMUNITY_SMALL_BUSINESS.md). Satu perusahaan, IDR dan 10 akun aktif tetap berlaku; gudang dapat lebih dari satu.

Upgrade paket ini membutuhkan migrasi 0026–0030 setelah backup dengan aplikasi berhenti. Stok lama dipetakan ke gudang utama, snapshot tagihan lama dibackfill dari nilai aslinya, dan posted_at jurnal lama mengikuti created_at aslinya. Nominal serta kolom historis asal tidak diubah. Installer memerlukan PostgreSQL terpasang; pilot usaha nyata dan rilis stabil tetap memerlukan penerimaan pemilik.

Paket saldo pihak menambahkan pembukaan piutang/utang dan kredit lama, uang muka pelanggan/supplier, alokasi antar-tagihan pihak yang sama, kredit retur tersimpan, refund tertunda/sebagian, buku pembantu, dan aging per tanggal. Kas aktual dipisahkan dari pemakaian kredit. Dokumen lama mempertahankan jurnal dan riwayatnya tanpa memposting ulang penerimaan/pembayaran.

Paket sebelumnya mencakup tanggal/tempo invoice, sumber dan pembalikan alokasi, lampiran saldo pihak, pencarian riwayat server dan ekspor sesuai filter, readiness schema, identitas build, manifest checksum backup, runner pengukuran, serta workflow artefak kandidat Linux/Windows.

Upgrade membutuhkan migrasi 0022–0025 setelah backup dengan aplikasi berhenti. Nominal/jurnal historis dipertahankan; tanggal invoice lama yang tidak diketahui tetap kosong sampai diberi metadata eksplisit. Backup lama dapat dipulihkan lalu dimigrasikan dengan aplikasi yang sesuai. Penurunan versi memerlukan backup dan executable yang cocok.

Hasil tes, QA, upgrade, restore, pengukuran, dan CI yang benar-benar telah dijalankan dicatat pada [status implementasi](./IMPLEMENTATION_STATUS.md). Catatan ini tidak menyatakan pilot nyata, installer otomatis, publikasi registry, atau rilis stabil sudah selesai.
