# Membaca laporan keuangan

Buka **Akuntansi → Laporan keuangan**. Semua pengguna yang sudah masuk dapat membaca laporan, membuka sumber angkanya, dan mengekspor CSV. Administrator dan Finance juga dapat memberi klasifikasi arus kas dengan alasan.

Laporan memakai jurnal yang telah diposting, termasuk jurnal manual, saldo awal, transaksi operasional, retur, dan jurnal pembalik. Draf pesanan yang belum menghasilkan jurnal tidak dihitung. Nominal menggunakan IDR dan ketelitian dua desimal; chart membantu membandingkan angka, sedangkan nominal pada laporan tetap menjadi rujukan.

## Memilih tanggal

Isi **Dari tanggal** dan **Sampai tanggal (inklusif)**, lalu pilih **Tampilkan**. Rentang awal mengikuti bulan berjalan sampai hari ini. Tanggal memakai kalender `YYYY-MM-DD`, tahun 0001–9999, dan zona waktu perusahaan yang ditampilkan di atas laporan. Tanggal awal tidak boleh melewati tanggal akhir.

Tanggal yang dipakai adalah **tanggal posting jurnal**, bukan tanggal pesanan, tanggal invoice supplier, atau jatuh tempo. Misalnya, invoice supplier bertanggal September yang baru dicatat pada Oktober masuk ke jurnal Oktober. Pembalikan yang diposting pada Oktober juga masuk ke Oktober; transaksi September tetap berada pada riwayat September.

Semua bagian laporan dihitung dari satu snapshot database. Nama perusahaan, rentang, zona waktu, dan waktu snapshot ditampilkan agar laporan dapat dikenali saat diekspor. Periode akuntansi yang telah ditutup tetap dapat dibaca.

## Laba rugi: hasil selama periode

Tab **Laba rugi** menjumlahkan seluruh akun kelompok Pendapatan dan Beban pada rentang terpilih, termasuk akun tambahan yang dibuat pengguna. Laba atau rugi bersih adalah **total pendapatan dikurangi total beban**.

Pembelian persediaan belum otomatis menjadi beban: nilainya lebih dahulu masuk persediaan. HPP muncul melalui jurnal pengiriman. Penerimaan PO dan tagihan supplier juga mempunyai jurnal aset/liabilitas tersendiri. Membayar utang supplier mengurangi kas dan utang; pembayaran tersebut tidak dibebankan sekali lagi pada laba rugi.

Retur pelanggan mengurangi pendapatan dan membalik biaya historis barang yang dikembalikan. Selisih nilai retur pembelian, bila ada, ikut masuk melalui akun beban **Selisih nilai retur pembelian**. Koreksi dapat menghasilkan pendapatan atau beban bersih negatif pada suatu periode; tanda minus tetap ditampilkan.

Klik nama akun untuk membuka rincian buku besarnya sesuai rentang laporan, lalu klik jurnal untuk memeriksa debit dan kredit sumbernya.

## Neraca: posisi sampai tanggal akhir

Tab **Neraca** memakai akumulasi seluruh jurnal sampai tanggal akhir, termasuk transaksi sebelum tanggal awal filter. Karena itu, mengubah tanggal awal saja mengubah laba rugi dan arus kas, tetapi tidak mengubah neraca untuk tanggal akhir yang sama.

Neraca menampilkan:

- **Aset:** seluruh akun kelompok aset, termasuk kas, piutang, persediaan, dan akun aset tambahan.
- **Liabilitas:** seluruh akun kewajiban, termasuk barang diterima belum ditagih, utang supplier, dan akun tambahan.
- **Ekuitas tercatat:** saldo akun kelompok ekuitas.
- **Laba/rugi akumulasi:** saldo bersih akun pendapatan dikurangi beban sampai tanggal neraca. Buka rinciannya untuk melihat akun pembentuknya.

Pemeriksaan laporan adalah **aset = liabilitas + ekuitas tercatat + laba/rugi akumulasi**. Selisih ditampilkan bila persamaan tidak cocok. Akun pendapatan memakai saldo normal kredit, sedangkan beban memakai saldo normal debit.

Laba akumulasi dihitung dari saldo akun pendapatan dan beban yang masih ada pada jurnal. Bila pengguna sudah membuat jurnal pemindahan laba ke ekuitas, saldo akun tersebut ikut berubah sehingga nilai yang sudah dipindahkan tidak dihitung lagi sebagai laba akumulasi. Tampilan ini tidak membuat jurnal tutup buku atau membagi laba otomatis menurut tahun buku.

Rincian akun neraca memuat seluruh jurnal sampai tanggal akhir. Saldo buku besar diberi label **debit − kredit**, sehingga akun liabilitas atau ekuitas bersaldo kredit biasanya bernilai negatif di rincian tersebut. Nominal pada neraca mengikuti saldo normal kelompok akun.

## Arus kas: pergerakan akun Kas 1100

Tab **Arus kas** menggunakan metode langsung dalam cakupan **akun Kas `1100`**. Akun aset lain, termasuk rekening bank tambahan yang dibuat sendiri, belum otomatis termasuk kas atau setara kas pada laporan ini.

Laporan memperlihatkan saldo sebelum tanggal awal, seluruh debit kas sebagai kas masuk, seluruh kredit kas sebagai kas keluar, perubahan neto, serta saldo akhir sampai tanggal akhir. Pemeriksaannya adalah **saldo awal + kas masuk − kas keluar = saldo akhir**. Transaksi tanpa akun Kas 1100 tidak menambah arus kas.

Aktivitas dibagi menjadi Operasi, Investasi, Pendanaan, dan **Belum terklasifikasi / saldo awal**. Pemetaan otomatis menggunakan akun lawan jurnal:

- Akun pendapatan, beban, piutang `1200`, persediaan `1300`, dan akun kontrol kewajiban pembelian masuk Operasi.
- Akun ekuitas masuk Pendanaan, kecuali jurnal saldo awal pembukuan.
- Saldo awal pembukuan, jurnal yang mencampurkan kelompok berbeda, akun aset/liabilitas tambahan yang sifatnya belum jelas, dan jurnal berisi debit sekaligus kredit Kas 1100 masuk kelompok belum terklasifikasi.

Nama atau nomor akun tambahan tidak cukup untuk menyimpulkan apakah suatu transaksi merupakan pembelian peralatan, pinjaman, atau kegiatan lain. Karena itu, arus investasi atas akun tersebut perlu diklasifikasikan secara manual. Kelompok Investasi dapat bernilai nol ketika belum ada jurnal yang diklasifikasikan ke dalamnya. Seluruh pergerakan yang belum terklasifikasi tetap dihitung dalam rekonsiliasi kas.

Pada **Sumber pergerakan kas**, pilih aktivitas untuk menyaring rincian dan gunakan **Lihat jurnal** untuk memeriksa sumbernya. Jurnal dengan kas masuk sekaligus keluar mempertahankan nilai bruto kedua sisi serta penjelasan klasifikasinya; nilai netonya tetap sesuai jurnal.

## Mengubah klasifikasi arus kas

Administrator atau Finance dapat memilih **Klasifikasi** pada jurnal sumber, memilih Operasi/Investasi/Pendanaan, mengisi alasan 3–1.000 karakter, lalu menyimpan. Pilih **Otomatis / belum terklasifikasi** untuk melepaskan pilihan manual dan memakai pemetaan sistem kembali.

Klasifikasi berlaku untuk **seluruh pergerakan kas pada jurnal tersebut**; pembagian satu jurnal ke beberapa aktivitas belum tersedia. Setiap perubahan menyimpan versi, alasan, pelaku, dan waktu. Riwayatnya dapat dibuka pada baris jurnal. Permintaan dengan versi lama ditolak agar perubahan pengguna lain tidak tertimpa.

Klasifikasi merupakan keterangan pelaporan dan tidak mengubah nominal, akun, tanggal, atau jurnal yang telah diposting. Karena itu, klasifikasi jurnal lama tetap dapat diperbaiki ketika periode keuangannya telah ditutup. Penyajian kelompok arus kas historis dapat berubah setelah klasifikasi diperbaiki; waktu snapshot dan riwayat klasifikasi membantu menjelaskan perbedaannya.

## CSV dan pemeriksaan rincian

**CSV laporan** mengekspor ketiga laporan untuk rentang yang sedang ditampilkan, beserta dasar tanggal, zona waktu, waktu snapshot, total, rekonsiliasi, sumber arus kas, dan riwayat klasifikasi. Nominal disimpan sebagai nilai desimal, bukan pembulatan chart. Ekspor tidak dibatasi oleh 500 dokumen terbaru pada daftar operasional.

Rincian buku besar dimuat ketika dibuka. Bila ada posting baru setelah laporan diambil, muat ulang laporan dan rincian untuk mencocokkan snapshot terbaru. Buku besar menampilkan maksimal 5.000 jurnal pertama pada rentangnya dan memberi pemberitahuan bila rincian terpotong; saldo awal/akhir serta total laporan tetap mencakup seluruh jurnal.

Panduan ini menjelaskan perilaku Haruflow Community. Laporan belum merupakan pernyataan kepatuhan terhadap standar pelaporan atau perpajakan tertentu; kebutuhan format resmi, pengelompokan tambahan, dan kebijakan akuntansi usaha perlu ditentukan sesuai penggunaannya.
