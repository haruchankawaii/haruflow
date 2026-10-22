# Latihan penerimaan Community di Windows

Penerimaan untuk lingkup pengembangan Community memakai **simulasi transaksi, instalasi, backup/restore dan percakapan Codex**. Simulasi cukup untuk penyelesaian tugas ini; pilot usaha nyata tidak menjadi prasyarat. Hasil aktual dicatat pada [status implementasi](IMPLEMENTATION_STATUS.md). Panduan bergambar ada di [GUIDE](GUIDE.md).

Dokumen ini juga menyediakan formulir opsional bila suatu usaha nanti menerapkannya. Nama/jenis usaha, volume dan data nyata belum diberikan; jangan menganggap formulir kosong sebagai bukti persetujuan pemilik atau pilot nyata yang sudah selesai.

## Persiapan operator

- Catat nama/jenis usaha, Windows/CPU/RAM, PostgreSQL, commit/schema dan lokasi backup. Mulai dengan satu perusahaan IDR dan paling banyak 10 akun aktif sesuai tugas.
- Sepakati tanggal mulai, saldo awal fisik/kas/piutang/utang dan cara menghindari impor transaksi yang sudah termasuk saldo awal.
- Pilih rekening/gudang seperlunya. Gunakan gudang utama bila hanya satu lokasi. Tetapkan unit dasar; lot/seri hanya pada produk yang memang memerlukannya.
- Pulihkan backup ke database uji kosong dan cocokkan saldo serta bukti sebelum memakai data nyata.

## Skenario penerimaan simulasi

- Beli barang/jasa, terima sebagian, tagih dengan selisih/ongkir/diskon/pajak yang disetujui, bayar sebagian melalui rekening yang benar, lalu koreksi/retur.
- Jual barang/jasa, kirim/tagih sebagian, terima DP/pembayaran, alokasikan kredit dan refund; cocokkan stok, kas dan sisa tagihan.
- Transfer antar rekening dan gudang; cocokkan mutasi bank tanpa jurnal ganda; uji lot/seri/reservasi bila digunakan.
- Impor salinan kecil transaksi lama, periksa tanggal, aging, buku pembantu dan akun kontrol; hentikan bila saldo awal menjadi ganda.
- Review laba rugi/neraca/arus kas; lakukan tutup tahun di database uji, pembalikan dan koreksi, pastikan laporan tetap cocok.
- Restart server, ulangi permintaan yang terputus, backup → perubahan → restore. Pastikan satu efek transaksi dan byte bukti tetap sama.

## Pembagian tugas latihan

Dalam latihan, operator penjualan/pembelian mengisi transaksi, petugas kas memeriksa saldo, petugas gudang memeriksa barang, dan admin/IT memeriksa akses serta pemulihan. Satu orang boleh menjalankan semua peran pada fixture simulasi; hasil setiap pemeriksaan tetap dicatat. Gunakan perusahaan/port/database latihan. Pada deployment usaha nyata, pemilik menunjuk penanggung jawab dan memutuskan penerimaan data usahanya sendiri.

### Skenario minimal dengan angka pembanding

Lakukan dengan tanggal dalam periode terbuka, satuan dasar pcs, tanpa pajak/diskon/ongkir, satu kas, satu gudang, dan produk uji baru yang tidak memiliki transaksi lain. Nilai rupiah berikut adalah angka latihan, bukan saldo awal usaha:

1. Pemilik mencatat kas awal Rp1.000.000 dan stok/piutang/utang produk uji nol. Kas awal harus diposting lewat saldo awal resmi agar neraca seimbang.
2. Operator membeli dan menerima 10 pcs dengan harga Rp20.000, membuat tagihan supplier Rp200.000, lalu membayar Rp100.000 melalui kas. Gudang memeriksa stok 10 pcs/nilai Rp200.000, finance memeriksa kas Rp900.000 dan utang Rp100.000.
3. Operator menjual, mengirim, dan menagih 4 pcs dengan harga Rp35.000, lalu menerima Rp100.000. Kriteria lulus: stok 6 pcs/nilai Rp120.000, kas Rp1.000.000, piutang Rp40.000, utang Rp100.000, penjualan Rp140.000, HPP Rp80.000 dan laba kotor Rp60.000. Laba bersih sama bila tidak ada biaya/transaksi lain pada perusahaan latihan. Seluruh jurnal harus seimbang.
4. Finance menagih sisa piutang Rp40.000 dan melunasi utang Rp100.000. Kriteria lulus: kas Rp940.000, stok 6 pcs/nilai Rp120.000, piutang dan utang nol; laba tidak bertambah hanya karena pelunasan. Cocokkan laporan, buku pembantu dan transaksi sumber, bukan hanya kartu dashboard.
5. Admin memberi finance hanya lingkup AI yang dibutuhkan dan gudang hanya persediaan. Pengguna bertanya melalui Codex tentang saldo pada langkah 3 atau 4. Jawaban harus menyebut periode/sumber, cocok dengan angka di ERP, dan tidak menyimpulkan sebab yang tidak ada buktinya. Kunci gudang harus ditolak saat meminta laporan keuangan; pencabutan kunci harus membuat permintaan berikutnya gagal. Percakapan dapat mengirim data ke penyedia model Codex, jadi pemilik menyetujui data yang dibagikan.
6. Operator menjalankan [backup Windows](WINDOWS_BACKUP.md), menyimpan salinan di perangkat terpisah dan memverifikasinya. IT memulihkan ke database kosong lain, mencabut kunci MCP hasil restore sebelum aplikasi hidup, lalu membuktikan saldo/langkah 4 dan byte/SHA256 lampiran contoh sama. Kunci MCP lama harus tetap ditolak setelah pemulihan. Catat durasi, titik data terakhir dan lokasi salinan.
7. Operator mencoba sekali alur koreksi/retur/pembayaran terputus di perusahaan latihan terpisah, mengikuti aturan dokumen terkait. Kriteria lulus: tidak ada jurnal atau mutasi ganda, laporan akhir sesuai peristiwa yang benar, dan tindakan tercatat dalam riwayat. Jangan mengubah urutan skenario angka di atas tanpa menyesuaikan nilai pembanding.

Masalah selisih saldo, akses lintas lingkup, transaksi ganda atau gagal pemulihan menghentikan penerimaan skenario yang bersangkutan sampai diperbaiki dan diuji ulang. Untuk tugas Community ini hasil simulasi yang tercatat merupakan batas penerimaan. Jika nantinya diterapkan pada usaha nyata, salinan kecil data yang disetujui pemilik dan pendampingan satu siklus pembelian–penjualan–penagihan dapat dipakai sebagai latihan deployment terpisah. Hasil simulasi tidak dilabeli sebagai pilot nyata.

## Formulir opsional untuk deployment usaha nyata

Nama usaha: ____; pemilik/operator: ____; tanggal: ____; build/schema: ____.

Jumlah transaksi uji: ____; saldo stok/kas/piutang/utang sebelum dan sesudah: ____.

Backup yang dipulihkan dan hasil pencocokan: ____; masalah tersisa dan penanggung jawab: ____.

Keputusan pemilik (diterima/perlu perbaikan), batas pemakaian, dan persetujuan mulai operasional: ____.

Persetujuan berbagi data ke AI/lingkup/pengguna: ____; pemeriksaan kunci lama setelah restore: ____; pemilik backup dan jadwal/perangkat cadangan: ____.

Status formulir usaha nyata: **belum diisi**. Tidak ada persetujuan pemilik, jadwal backup PC usaha atau hasil data usaha nyata yang dianggap sudah diberikan oleh dokumen ini. Formulir ini bukan pekerjaan tertunda yang menghalangi penerimaan pengembangan Community berbasis simulasi.

## Uji sintetis pengembang

`node scripts/community-readiness.mjs` membuat database `haruflow_readiness_<acak>` sendiri, menjalankan pembelian campuran, rekening/rekonsiliasi, stok lokasi, impor historis, tutup tahun, penghentian server di tengah transaksi, retry/restart, serta backup/restore. Hanya proses/database buatannya yang dihentikan/dihapus. Konfigurasi `HARUFLOW_READINESS_BINARY`, `HARUFLOW_READINESS_POSTGRES_URL`, `HARUFLOW_READINESS_PSQL`, `HARUFLOW_READINESS_PG_DUMP`, `HARUFLOW_READINESS_PG_RESTORE` menunjuk build/alat uji. Bukti disimpan di output/readiness/ tanpa kredensial.
