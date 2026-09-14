# 14 — การ Release และแจกจ่าย

"binary เดียว ไม่ต้องติดตั้งอะไร" จะเป็นจุดขายได้ ก็ต่อเมื่อการติดตั้งง่ายจริง

## Target ที่ต้อง build

| target | ใช้กับ |
|---|---|
| `aarch64-apple-darwin` | Mac M1+ (สำคัญสุด — คนทำ mobile ใช้ตัวนี้) |
| `x86_64-apple-darwin` | Mac Intel |
| `x86_64-unknown-linux-gnu` | CI ทั่วไป |
| `aarch64-unknown-linux-gnu` | ARM runner |
| `x86_64-unknown-linux-musl` | container ที่ไม่มี glibc |
| `x86_64-pc-windows-msvc` | Windows (เฉพาะ core + Android) |

ใช้ [`cargo-dist`](https://opensource.axo.dev/cargo-dist/) สร้าง release workflow, installer script, checksum และ release notes ให้ครบในทีเดียว

## ช่องทางติดตั้ง

| ช่องทาง | คำสั่ง | Priority |
|---|---|---|
| Install script | `curl -fsSL https://shlane.dev/install.sh \| sh` | P0 |
| GitHub Releases | ดาวน์โหลด tarball ตรง | P0 |
| Homebrew tap | `brew install prongbang/tap/shlane` | P0 |
| crates.io | `cargo install shlane` | P1 |
| GitHub Action | ดู [11](11-ci-integration.md) | P0 |
| Docker | `ghcr.io/prongbang/shlane` | P2 |
| mise / asdf plugin | — | P2 |

## เงื่อนไขก่อน publish ขึ้น crates.io

`Cargo.toml` ประกาศ `readme = "README.md"` แต่ **ไฟล์นั้นยังไม่มีอยู่จริง** → `cargo publish` จะล้มเหลว

ต้องทำใน M0:
- [ ] สร้าง `README.md`
- [ ] ตรวจ `cargo package --list` ว่าไม่มีไฟล์แปลกปลอม
- [ ] เพิ่ม `LICENSE` (Cargo.toml ระบุ Apache-2.0 แต่ยังไม่มีไฟล์ในรีโป)
- [ ] `cargo publish --dry-run` ผ่าน

## Versioning

- Semantic versioning
- ก่อน 1.0: breaking change ของ schema ได้ แต่ต้องมีบันทึกใน CHANGELOG และ `shlane validate` ต้องเตือนแบบชี้ทางแก้
- `version: 1` ใน `shlane.yaml` (ดู [03](03-config-schema.md)) ทำให้เปลี่ยน schema ในอนาคตได้โดยไม่พังของเก่า
- `min_shlane:` ให้ config บอกได้ว่าต้องใช้เวอร์ชันขั้นต่ำเท่าไร

## ความปลอดภัยของ artifact

- ปล่อย `SHA256SUMS` ทุก release
- เซ็น artifact (minisign หรือ cosign keyless ผ่าน GitHub OIDC)
- install script ต้องตรวจ checksum ก่อนติดตั้ง
- เปิด GitHub artifact attestation

## CHANGELOG

ใช้ Keep a Changelog + conventional commits สร้างอัตโนมัติ
commit แรกของรีโปคือ `feat: initial` ซึ่งเข้ารูปแบบนี้อยู่แล้ว — ตั้ง commitlint ใน CI ต่อได้เลย

## ขนาด binary

`Cargo.toml` ตั้ง `lto = true`, `codegen-units = 1`, `strip = true` ไว้แล้ว ดี
แต่ต้องลบ `panic = "abort"` ออก (ดู [02](02-architecture.md)) — ยอมให้ binary ใหญ่ขึ้นเล็กน้อยเพื่อแลกกับ error message ที่ใช้งานได้จริง
ตั้งเป้า: **< 15 MB** ต่อ target และมี CI job ที่เตือนเมื่อขนาดโตขึ้นเกิน 10% ใน PR เดียว
