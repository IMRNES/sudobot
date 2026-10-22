# Task Plan: Audit & Fix Role Distribution Bug

## Masalah
Role lama tidak dihapus saat role diberikan ulang ke user yang sama, menyebabkan akumulasi role yang tidak diinginkan.

## Temuan Audit (berbasis bukti produksi imrnes)

**Konfigurasi produksi (guild 1450861658061803624):**
- `stack_roles = false` (mode replacing: satu anggota = satu rung, rungs lama dicabut)
- 21 reward roles (level 0 → 2000)
- 92 anggota, 28 punya profil leveling (level 0–14)

**Akumulasi terkonfirmasi (1 kasus):**
- `698168586673455155` (level 3) memakai `['Apprentice', 'Neophyte']`
  - Harusnya hanya `Apprentice` (mode replacing)
  - Neophyte (level 0) adalah rung lama yang seharusnya dicabut saat naik ke level 3

**Tidak ada badge sama sekali (53 kasus):**
- 53 dari 80 anggota manusia tidak memakai badge ladder sama sekali
- Termasuk 28 anggota yang sudah punya profil leveling di level 1–14
- Hal ini menunjukkan `grant_starting_role` pada `handle_member_join` tidak konvergen untuk anggota yang bergabung

**Root cause:**
- `handle_member_join` di `apps/bot/src/events.rs` hanya **menambahkan** role (level-0 rung, auto roles, re-jail) tanpa mencabut rung yang sudah tidak sesuai level
- Path level-up sudah benar (`plan_role_changes` mengembalikan grant + revoke), tapi join path tidak pernah memanggilnya
- Startup reconcile hanya berjalan sekali per deploy, tidak setiap anggota rejoint

## Perbaikan

**File yang diubah:** `apps/bot/src/events.rs`

1. Ganti `grant_starting_role` dengan `reconcile_level_roles_on_join`
2. Fungsi baru memanggil `plan_role_changes` (sama seperti path level-up)
3. Revoke dulu, baru grant (untuk mencegah sementara memakai dua rungs)
4. Best effort: gagal Discord API tidak membatalkan join

**Verifikasi:**
- `cargo check --workspace` ✓
- `cargo clippy --workspace --all-targets` ✓ (nol warning)
- `cargo fmt --all` ✓
- `cargo test --workspace` ✓ (294 iam + 48 bot, 0 gagal)

## Langkah Selanjutnya
- Deploy ke produksi (imrnes) melalui CI/CD deploy.yml
- Verifikasi anggota `698168586673455155` tidak lagi memakai Neophyte setelah rejoint
