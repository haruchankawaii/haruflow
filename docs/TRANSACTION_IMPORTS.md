# Impor transaksi historis

Menu Impor transaksi tersedia untuk Admin/Finance. Unduh template sesuai jenis: customer_invoices, supplier_bills, customer_payments, supplier_payments, journals. Gunakan CSV UTF-8 koma/titik koma, maksimal 256 KiB dan 1.000 baris. Preview disimpan per pembuat dan memuat kesalahan setiap baris. Review semua efek sebelum konfirmasi; commit memvalidasi ulang dan membatalkan seluruh batch bila satu baris gagal.

Dokumen memerlukan reference unik, party_id dari master yang ada, document_date, due_date, total, counter_account, description. Invoice mengisi piutang; tagihan mengisi utang. Counter-account harus sesuai maksud migrasi; akun kas/kontrol tidak dapat dipakai sebagai penyeimbang sembarang. Periksa dengan pencatatan sebelumnya agar saldo awal dan dokumen historis tidak dihitung ganda.

Pembayaran memerlukan reference, document_reference, party_id yang sama, payment_date, amount, cash_account_code terdaftar. Tagihan harus sudah diimpor pada batch sebelumnya. Tidak boleh membayar melebihi sisa. Pembayaran supplier memeriksa saldo kas historis serta saldo setelahnya agar kas masa depan tidak dipakai membiayai tanggal lama.

Jurnal CSV memuat satu entri debit/kredit dua akun per reference: posting_date, description, debit_account, credit_account, amount. Untuk jurnal majemuk, pecah menjadi pasangan seimbang dengan reference berbeda atau gunakan jurnal bertanggal di Akuntansi. Impor transaksi tidak membuat pesanan, mutasi stok, penerimaan, atau pengiriman fiktif. Stok awal diinput melalui saldo awal persediaan yang sesuai.

Dokumen historis muncul pada Saldo pihak, aging, tautan rincian dan target alokasi kredit pihak. Posisi per tanggal memakai peristiwa finansial pada tanggalnya. Dokumen yang dibalik tidak menghapus riwayat lama; pembayaran dibalik dahulu, lalu alokasi dilepas, baru dokumen dapat dibalik. Pembalikan dicatat pada hari operasional terbuka dengan rekening sumber, bukan mengedit transaksi asli. Batas akun, role, idempotensi, versi dan periode juga berlaku.
