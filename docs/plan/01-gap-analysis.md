# 01 — Gap Analysis: fastlane vs shlane

เทียบความสามารถ fastlane กับสถานะ `shlane` วันนี้ (`src/main.rs` 199 บรรทัด)

## ตารางเทียบ

| ความสามารถ fastlane | shlane วันนี้ | ช่องว่าง | แผนที่รับผิดชอบ |
|---|---|---|---|
| Fastfile + lane | ✅ `lanes:` ใน YAML | — | [03](03-config-schema.md) |
| `before_all` / `after_all` | ⚠️ มี `before`/`after` แต่เป็นระดับ lane เท่านั้น | ไม่มีระดับ global | [03](03-config-schema.md) |
| `error` block | ❌ | ไม่มี hook ตอน fail เลย | [03](03-config-schema.md) |
| เรียก lane จาก lane อื่น | ❌ | ต้องเขียนซ้ำ | [03](03-config-schema.md) |
| `platform :ios do ... end` | ❌ | ไม่มีการจัดกลุ่ม lane | [03](03-config-schema.md) |
| Private lane | ❌ | lane ทุกตัวเรียกจากภายนอกได้หมด | [03](03-config-schema.md) |
| `options[:key]` + default | ⚠️ มี `param()` แต่ไม่มี default / required / type | พารามิเตอร์ผิดแล้วรันไปเงียบๆ (คืน `""`) | [03](03-config-schema.md) |
| `fastlane lanes` / `list` | ❌ | ดูไม่ได้ว่ามี lane อะไร (เห็นเฉพาะตอนพิมพ์ชื่อผิด) | [04](04-cli-ux.md) |
| `fastlane init` | ❌ | — | [04](04-cli-ux.md) |
| Actions ~400 ตัว | ❌ 0 ตัว | ช่องว่างใหญ่สุด | [06](06-actions-core.md), [07](07-actions-ios.md), [08](08-actions-android.md) |
| `lane_context[SharedValues::...]` | ❌ | ส่งค่าระหว่าง step ไม่ได้ | [02](02-architecture.md) |
| ผลลัพธ์ของคำสั่ง (stdout) | ❌ `run()` คืนแค่ exit code (`src/main.rs:180`) | เอา output ไปใช้ต่อไม่ได้ | [05](05-scripting-rhai.md) |
| `.env` / `--env` | ⚠️ มี `env:` ใน YAML แต่ไม่อ่านไฟล์ `.env` | เก็บ secret ใน YAML ที่ commit ไม่ได้ | [10](10-secrets-and-env.md) |
| ซ่อน secret ใน log | ❌ | เสี่ยง secret หลุดใน CI log | [10](10-secrets-and-env.md) |
| Plugins (`fastlane add_plugin`) | ❌ | ขยายไม่ได้ | [09](09-plugins.md) |
| `is_ci`, `setup_ci` | ❌ | ต้องจัดการ keychain เองบน CI | [11](11-ci-integration.md) |
| Report (JUnit/JSON) | ❌ | CI อ่านผลไม่ได้ | [11](11-ci-integration.md) |
| Appfile / Matchfile | ❌ | — | [07](07-actions-ios.md) |
| `fastlane_version` / เช็คเวอร์ชัน | ❌ | ไม่มี `version:` ใน config | [03](03-config-schema.md) |
| สรุปเวลาแต่ละ action ตอนจบ | ❌ | ไม่รู้ว่า step ไหนช้า | [04](04-cli-ux.md) |

## ปัญหาเชิงเทคนิคของโค้ดปัจจุบันที่ต้องแก้ก่อน

| ปัญหา | ตำแหน่ง | ผลกระทบ |
|---|---|---|
| `expect()` ทุกจุดที่อ่านไฟล์/parse | `src/main.rs:75-76`, `159` | error message ไม่บอกอะไรผู้ใช้ + `panic = "abort"` ทำให้ไม่มี backtrace |
| หา config เฉพาะ `shlane.yaml` ใน cwd | `src/main.rs:75` | รันจาก subdirectory ของ repo ไม่ได้ |
| `env::set_var` แก้ env ของทั้งโปรเซส | `src/main.rs:81` | เป็น `unsafe` ใน Rust 2024 และรั่วข้าม lane |
| `run()` ใน Rhai ไม่หยุดเมื่อคำสั่งล้มเหลว | `src/main.rs:180-193` | script รันต่อทั้งที่ command พังไปแล้ว |
| Interpolation `${k}` ทำเฉพาะ `before`/`steps`/`after` | `src/main.rs:65-72` | ค่าที่ไม่มีใน params จะเหลือ `${k}` ดิบส่งเข้า shell |
| ไม่มีการ escape ค่า param ก่อนต่อเป็น shell string | `src/main.rs:114` | ค่าที่มี `;` หรือ `` ` `` กลายเป็น command injection |
| ลำดับ execute คือ before → steps → script → after | `src/main.rs:102-136` | ผู้ใช้คุมลำดับไม่ได้ (ดู `example/shlane.yaml` lane `deploy` ที่ `steps` อ้าง `${target}` ก่อน `script` จะเซ็ต) |
| ไม่มี test | ทั้งโปรเจกต์ | refactor แล้วไม่รู้ว่าพังไหม |

## สรุป

ช่องว่างแบ่งเป็น 3 ชั้น ต้องทำตามลำดับ:

1. **ชั้นฐาน** (M0–M2): error handling, โครงสร้างโมดูล, config schema, CLI, context — ถ้าไม่ทำก่อน action ทุกตัวจะต้องเขียนใหม่
2. **ชั้น action** (M3–M5): งานหนักสุด แต่ทำทีละตัวได้ ปล่อย value ได้เรื่อยๆ
3. **ชั้นระบบนิเวศ** (M6–M7): plugin, migration tool, การแจกจ่าย
