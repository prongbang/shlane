# 13 — การทดสอบและคุณภาพโค้ด

ตอนนี้โปรเจกต์ **ไม่มี test เลย** ซึ่งเป็นปัญหาที่ต้องแก้ก่อน refactor ใน M0 ไม่ใช่หลังจากนั้น

## ชั้นของการทดสอบ

| ชั้น | ทดสอบอะไร | รันเมื่อไร | เครื่องมือ |
|---|---|---|---|
| **Unit** | parse config, interpolation, การประกอบ argument ของ action, การ mask secret | ทุก commit | `cargo test` |
| **Integration (CLI)** | รัน binary จริงกับ `shlane.yaml` ตัวอย่าง | ทุก commit | `assert_cmd` + `tempfile` |
| **Snapshot** | รูปแบบ output ของ `list`, `validate`, ตารางสรุป, error message | ทุก commit | `insta` |
| **Contract** | action ที่ยิง HTTP (Play Store, App Store Connect) | ทุก commit | `wiremock` / mock server |
| **E2E** | build จริงบนโปรเจกต์ตัวอย่าง | nightly + ก่อน release | macOS/Linux runner |

## สิ่งที่ต้องมี test ตั้งแต่ M0 (ก่อนแตะโค้ดเดิม)

เขียน characterization test ของพฤติกรรมปัจจุบันก่อน เพื่อให้ refactor แล้วรู้ว่าพังตรงไหน:

- [ ] lane ที่มี `before`/`steps`/`script`/`after` ครบ รันตามลำดับที่คาด
- [ ] lane ที่ไม่มีอยู่ → พิมพ์รายชื่อ lane ที่มี (`src/main.rs:139-149`)
- [ ] `${key}` ถูกแทนค่าใน `run:` (`src/main.rs:65-72`)
- [ ] คำสั่งที่ exit ไม่ใช่ 0 ทำให้โปรแกรมหยุด (`src/main.rs:161-164`)
- [ ] YAML ผิดรูปแบบ → ไม่ panic แบบไม่มีข้อความ

## Fixtures

```
tests/
  fixtures/
    minimal.yaml           # lane เดียว step เดียว
    full.yaml              # ใช้ทุก field ในสเปก (ดู 03)
    invalid_syntax.yaml
    invalid_lane_ref.yaml
    cyclic_lanes.yaml
    secrets.yaml           # ตรวจว่า secret ไม่หลุด
  cli/
    run.rs  list.rs  validate.rs  init.rs
  snapshots/
```

## เทส action โดยไม่ต้องมี Xcode/Gradle

แยก action เป็น 2 ส่วนเสมอ:

```rust
// ส่วนที่ทดสอบได้ (pure): args → คำสั่งที่จะรัน
fn build_command(args: &BuildIosArgs) -> Vec<String>

// ส่วนที่ทดสอบไม่ได้: เรียก build_command() แล้ว spawn
fn run(...)
```

ทดสอบ `build_command()` อย่างละเอียด — นี่คือจุดที่ bug ส่วนใหญ่อยู่ (argument ผิด, quote ผิด, order ผิด) ไม่ใช่ที่การ spawn

สำหรับ e2e: เตรียม `examples/ios-sample/` และ `examples/android-sample/` เป็นโปรเจกต์เปล่าที่ build ได้จริง

## คุณภาพโค้ด

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check          # license + advisory
cargo test --all-features
```

- **MSRV**: กำหนดและใส่ใน `Cargo.toml` (`rust-version`) แล้วเทสใน CI
- **ห้าม `unwrap()`/`expect()` ในโค้ด production** — บังคับด้วย clippy lint `unwrap_used`, `expect_used` (ยกเว้นใน test)
- **ห้าม `panic = "abort"`** ใน profile release (ดู [02](02-architecture.md))

## CI workflow ของตัว shlane เอง

```yaml
jobs:
  check:       # fmt, clippy, deny — ubuntu
  test:        # matrix: ubuntu, macos, windows × stable, MSRV
  e2e-android: # ubuntu + Android SDK — nightly
  e2e-ios:     # macos-14 — nightly
```

## เป้าหมายความครอบคลุม

- config + interpolation + secret masking: **> 90%**
- runtime/executor: **> 80%**
- actions: ทุกตัวต้องมี test ของ `build_command()` อย่างน้อย 1 happy path + 1 error case
- ไม่ตั้งเป้าตัวเลขรวมทั้งโปรเจกต์ เพราะจะไปไล่เทสส่วนที่เป็น glue โดยไม่ได้ประโยชน์
