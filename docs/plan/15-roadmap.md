# 15 — Roadmap

ลำดับการลงมือจริง แต่ละ milestone ต้อง **ปล่อยของที่ใช้ได้** ไม่ใช่แค่ refactor ค้าง

## M0 — ฐานราก (จำเป็นก่อนทุกอย่าง) ✅ เสร็จแล้ว

| งาน | เอกสาร |
|---|---|
| เขียน `README.md` และเพิ่มไฟล์ `LICENSE` (Apache-2.0) | [14](14-release-and-distribution.md) |
| เขียน characterization test ของพฤติกรรมปัจจุบัน | [13](13-testing-and-quality.md) |
| แตก `main.rs` เป็นโมดูลตามโครงใน [02](02-architecture.md) | [02](02-architecture.md) |
| เปลี่ยน `expect()` → `Result` + `ShlaneError`, ลบ `panic = "abort"` | [02](02-architecture.md) |
| ตั้ง CI: fmt, clippy (`-D warnings`), test matrix | [13](13-testing-and-quality.md) |
| แก้ command injection ใน interpolation | [03](03-config-schema.md) |

**เสร็จเมื่อ:** พฤติกรรมเดิมทุกอย่างยังทำงาน มี test ครอบ, CI เขียว, error ทุกตัวอ่านรู้เรื่อง

## M1 — Config และ CLI v1 ✅ เสร็จแล้ว

- schema v1 ตาม [03](03-config-schema.md): `version`, `params` แบบมี type/required/default, `description`, `private`, `platform`, `if`, `id`, `workdir`, `timeout`, `retry`, `continue_on_error`
- step แบบ `lane:` (เรียก lane อื่น) และ `error` hook
- หา config แบบไล่ขึ้น directory tree
- คำสั่ง `list`, `validate`, `init`, `completions`
- exit codes ตาม [04](04-cli-ux.md), ตารางสรุปตอนจบ, `--dry-run`

**เสร็จเมื่อ:** เขียน pipeline จริงด้วย `run:` ล้วนๆ แล้วใช้งานแทน shell script ได้

> ยังเหลือจาก M1: การจัดการ Ctrl-C (ต้องมี signal handler), `--json`, `--verbose/-q` และสี
> — ย้ายไปรวมกับงาน logging ใน M2

## M2 — Runtime และ scripting ✅ เสร็จแล้ว

- `LaneContext` เต็มรูปแบบ, ลบ `env::set_var` ทั้งหมด
- `.env` / `env_files` / ลำดับความสำคัญของ env ([10](10-secrets-and-env.md))
- `SecretRegistry` + การ mask ทุกช่องทาง
- Rhai API ใหม่ตาม [05](05-scripting-rhai.md): `run()` คืน struct, `set_output()`, `call_lane()`, limits
- logging ด้วย `tracing`, `--json`, `--verbose/-q`

**เสร็จเมื่อ:** test พิสูจน์ได้ว่า secret ไม่หลุดใน log ทุกรูปแบบ และส่งค่าระหว่าง step ได้

> หมายเหตุจากการ implement: `call_lane()` และ `action()` ใน Rhai ยังไม่ได้ทำ เพราะต้องเรียก
> executor ซ้อนเข้าไปจาก builtin ซึ่งต้องรื้อ ownership — ทำพร้อม action registry ใน M3
> ส่วน logging ใช้ `ui` module ของตัวเองแทน `tracing` (CLI ต้องการ event stream ที่นิ่ง
> มากกว่า subscriber stack)

## M3 — Action framework + action กลาง ✅ เสร็จแล้ว

- `trait Action` + registry + `shlane action list/show`
- action P0 ตาม [06](06-actions-core.md): `sh`, `git_*`, `bump_version`, `read_version`, `notify_slack`, `ensure_env_vars`, `http_request`

**เสร็จเมื่อ:** lane "bump version + commit + tag + push + แจ้ง Slack" ทำได้โดยไม่ต้องเขียน shell เลย

> ทำแล้ว 13 action และ `action()` ใน Rhai ด้วย ส่วน `call_lane()` ยังไม่ทำ (ต้องเรียก
> executor ซ้อน) — ใช้ step `lane:` แทน
>
> เปลี่ยนจากแผน: ใช้ `ureq` แทน `reqwest` (CLI แบบ blocking ไม่ต้องแบก async runtime)
> ผลคือ MSRV ขยับ 1.74 → 1.85 และ binary 3.6 → 5.4 MB

## M4 — Android ✅ เสร็จแล้ว

- `gradle`, `build_android`, `test_android`, `sign_android` ([08](08-actions-android.md))
- `play_store` (Publishing API v3)
- `firebase_distribution`
- report JUnit ([11](11-ci-integration.md))

**เสร็จเมื่อ:** โปรเจกต์ Android จริงลบ `Gemfile` ทิ้งได้

> ทำแล้ว: `gradle`, `build_android`, `test_android`, `sign_android`, `play_store`,
> `firebase_distribution` และ `--report junit|json|md`
>
> ต่างจากแผน: `firebase_distribution` ใช้วิธี wrap `firebase` CLI (ทางเลือก 1 ในเอกสาร)
> ไม่ใช่ REST เพราะ upload endpoint คืน long-running operation ที่ต้อง poll และเทสกับ
> ของจริงไม่ได้ — REST ยังเป็นงานในอนาคต
>
> ยังไม่ได้ verify: `play_store` เทสเฉพาะรูปร่าง request (unit test) การคุยกับ Google
> จริงต้องมี service account — เป็นงาน e2e

## M5 — iOS ✅ เสร็จแล้ว (บางส่วน)

- `build_ios`, `test_ios` + parse `.xcresult`
- `keychain`, `setup_ci`
- App Store Connect API (JWT) + `testflight`
- code signing ทางเลือก C (API key + `-allowProvisioningUpdates`) ตาม [07](07-actions-ios.md)

**เสร็จเมื่อ:** โปรเจกต์ iOS จริงขึ้น TestFlight ได้จาก CI

> ทำแล้ว: `build_ios` (พร้อมสร้าง ExportOptions.plist), `test_ios`, `keychain`,
> `testflight` (ผ่าน altool), `asc_request` (ES256 JWT ด้วย ring)
>
> **ยังไม่ได้ทำ:**
> - `codesign_sync` / match — ทำแค่ทางเลือก C (API key + `-allowProvisioningUpdates`)
>   ทีมที่ใช้ match อยู่ยังย้ายมาไม่ได้ นี่คือ blocker ที่เหลือจริงๆ
> - แปลง `.xcresult` เป็น JUnit — ต้อง parse output ของ `xcresulttool` ซึ่งเช็ค shape
>   ไม่ได้ถ้าไม่มี Xcode การเดาแล้วเขียนไปจะได้ของที่ดูเหมือนเสร็จแต่ใช้ไม่ได้
>
> **ยังไม่ได้ verify:** ทุก action ต้องมี macOS + Xcode — unit test คลุมการประกอบคำสั่ง,
> plist และ JWT claims แต่ไม่ได้ทดสอบกับของจริง

## M6 — ระบบนิเวศ ⚠️ เสร็จบางส่วน

- plugin แบบ external executable + Rhai module ([09](09-plugins.md))
- `shlane migrate` + `docs/migration.md` ([12](12-migration-from-fastlane.md))
- `codesign_sync` ที่อ่าน match repo เดิมได้ (ทางเลือก A)
- GitHub Action wrapper + Homebrew tap ([14](14-release-and-distribution.md))

> ทำแล้ว: plugin แบบ external executable (`path:` เท่านั้น) + lockfile SHA-256 +
> `plugin list/lock/verify`, และ `shlane migrate`
>
> **ยังไม่ทำ:**
> - ดึง plugin จาก git host (`source: github:...`) — ต้องมี installer ที่ตรวจ checksum ก่อน
> - Rhai module plugin (ทางเลือก B)
> - ~~`codesign_sync` ที่อ่าน match repo เดิม~~ ✅ ทำแล้ว (read-only)
> - ~~GitHub Action wrapper~~ ✅ ทำแล้ว (`action.yml` + `install.sh`) / Homebrew tap ยังไม่ทำ

## M7 — 1.0 ⚠️ เสร็จบางส่วน

> ทำแล้ว: CI detection + GitHub annotations, `shlane env`, `install.sh` (ตรวจ checksum),
> `action.yml`, release workflow (macOS arm64/x86-64, Linux x86-64/arm64/musl)
>
> **ยังไม่ทำ:** `appstore` (deliver — อัปโหลด metadata ขึ้น App Store), เอกสารบนเว็บ,
> Homebrew tap, การแช่ schema v1 อย่างเป็นทางการ
>
> **ไม่มี Windows binary:** runner ยังเรียก `sh` ตรงๆ อยู่ — บอกตรงๆ ดีกว่าปล่อย binary
> ที่พังตั้งแต่ step แรก

- `appstore` (upload metadata + submit for review)
- เอกสารครบบนเว็บ
- แช่ schema v1, สัญญาเรื่อง backward compatibility
- e2e nightly ทั้งสอง platform เขียวติดต่อกัน 2 สัปดาห์

## ลำดับความสำคัญถ้ามีเวลาจำกัด

ถ้าทำได้แค่ 3 อย่าง: **M0 → M1 → M4**
เพราะได้เครื่องมือที่ใช้แทน shell script ได้จริงบน Android โดยไม่ต้องแตะความซับซ้อนของ Apple

## ความเสี่ยงหลัก

| ความเสี่ยง | ผลกระทบ | การรับมือ |
|---|---|---|
| **ขอบเขตบานปลายไปไล่ทำ action ให้ครบ 400 ตัว** | ไม่มีวันเสร็จ | ยึด P0/P1/P2 ใน [06](06-actions-core.md) และปล่อยให้ `run:` + plugin รับส่วนที่เหลือ |
| **`match` คือกำแพงที่ทีมใหญ่ข้ามไม่ได้** | ย้ายมาไม่ได้จริง | ทำทางเลือก C ก่อนเพื่อได้ value เร็ว แล้วทำ A ใน M6 |
| **Apple/Google เปลี่ยน API** | action พังเงียบๆ | e2e nightly + แยกชั้น HTTP ให้แก้จุดเดียว |
| **maintainer คนเดียว** | bus factor = 1 | ทำ plugin ให้ดีตั้งแต่ต้น เพื่อให้ชุมชนเติมส่วนที่ขาดได้เอง |
| **fastlane "ก็ใช้ได้อยู่แล้ว"** | ไม่มีคนย้าย | โฟกัสจุดที่เจ็บจริง: เวลา setup บน CI และ error message ที่อ่านรู้เรื่อง |
| **ทดสอบ iOS ต้องมี Apple account** | เทสไม่ได้ | แยก `build_command()` ออกมาเทสแบบ pure ([13](13-testing-and-quality.md)) |

## ตัวชี้วัดความสำเร็จ

| ตัวชี้วัด | เป้า |
|---|---|
| เวลา setup บน CI (เทียบ `bundle install`) | < 5 วินาที (จาก 30–120 วินาที) |
| เวลาเริ่มทำงานของ `shlane run` | < 100 ms |
| ขนาด binary | < 15 MB |
| เวลาที่ใช้ย้าย Fastfile 100 บรรทัด | < 1 ชั่วโมง |
| โปรเจกต์จริงที่ลบ `Gemfile` ได้ | อย่างน้อย 1 ตัวต่อ platform ก่อน 1.0 |
