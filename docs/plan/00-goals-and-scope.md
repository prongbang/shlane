# 00 — เป้าหมายและขอบเขต

## ทำไมต้องมีตัวแทน fastlane

| ปัญหาของ fastlane | ผลกระทบจริง |
|---|---|
| ต้องมี Ruby + Bundler + gem ครบ | ต้องจัดการ rbenv/rvm, Ruby version ชนกันระหว่างเครื่อง dev กับ CI |
| `bundle install` ทุก CI run | เสียเวลา 30–120 วินาทีต่อ job ถ้าไม่ cache |
| Dependency conflict | gem ของ plugin ชนกันเองเป็นเรื่องปกติ |
| Cold start ช้า | `fastlane` เริ่มทำงานจริงหลัง ~3–8 วินาที |
| Fastfile เป็น Ruby DSL | อ่านง่ายตอนสั้น แต่พังยากตอนยาว ไม่มี schema ตรวจก่อนรัน |

เป้าของ `shlane`: **binary เดียว ไม่มี runtime dependency, เริ่มทำงานทันที, config มี schema ตรวจได้**

## เป้าหมาย (Goals)

1. **Single static binary** — ดาวน์โหลดแล้วรันได้เลย ไม่ต้องติดตั้งอะไรเพิ่ม
2. **ครอบคลุม 80% ของ workflow ที่ใช้จริง** — build, test, sign, bump version, upload TestFlight/Play Store, แจ้งเตือน Slack
3. **Config ก่อน code** — YAML เป็นหลัก, Rhai เข้ามาเมื่อ YAML ไม่พอ
4. **ตรวจได้ก่อนรัน** — `shlane validate` บอก error ของ config โดยไม่ต้องรันจริง
5. **เส้นทางย้ายที่ชัดเจน** — มีเครื่องมือและตารางแปลงจาก Fastfile
6. **เร็ว** — เวลาตั้งแต่เรียกคำสั่งถึงเริ่ม step แรก < 100ms

## สิ่งที่ไม่ทำ (Non-goals)

- **ไม่ทำ action ครบ 400 ตัวแบบ fastlane** — เลือกเฉพาะที่มีคนใช้จริง ที่เหลือใช้ `run:` หรือ plugin
- **ไม่รัน Fastfile (Ruby) โดยตรง** — แปลงได้แบบ best-effort เท่านั้น ไม่ฝัง Ruby interpreter
- **ไม่ทำ GUI / web dashboard**
- **ไม่ทำตัวเป็น CI server** — shlane ถูกเรียกโดย CI ไม่ใช่มาแทน CI
- **ไม่รองรับ Windows สำหรับ action ฝั่ง iOS** (ข้อจำกัดของ Xcode เอง) — แต่ core ต้องรันได้บน Windows

## นิยาม "แทน fastlane ได้" (Acceptance Criteria)

ถือว่าสำเร็จเมื่อโปรเจกต์จริงหนึ่งตัวทำสิ่งเหล่านี้ได้ครบ **โดยลบ `Gemfile` และ `fastlane/` ออกได้**

- [ ] `shlane run beta` build iOS app, เซ็นด้วย certificate จาก CI, อัปโหลดขึ้น TestFlight ได้
- [ ] `shlane run beta` ฝั่ง Android build AAB, เซ็น, อัปโหลดขึ้น Play Store internal track ได้
- [ ] `shlane run test` รัน unit test + ออก JUnit report ให้ CI อ่านได้
- [ ] bump version/build number แล้ว commit + tag + push ได้
- [ ] อ่าน secret จาก environment ของ CI ได้ และไม่มี secret หลุดใน log
- [ ] lane ที่ fail ทำให้ CI job แดง ด้วย exit code ที่ถูกต้อง และมี error message ที่บอกได้ว่าพังที่ step ไหน
- [ ] เวลารวมของ pipeline ไม่ช้ากว่า fastlane เดิม

## หลักการออกแบบ

1. **Explicit > implicit** — ไม่มี magic global state แบบ `lane_context` ที่ไม่รู้ว่าใครเซ็ต (ดู [02](02-architecture.md))
2. **Fail fast, fail loud** — ผิดตรงไหนบอกบรรทัดนั้นใน YAML
3. **ทุก action ต้องรองรับ `--dry-run`** — พิมพ์สิ่งที่จะทำโดยไม่ทำจริง
4. **Escape hatch เสมอ** — ถ้า action ไม่รองรับ option ที่ต้องการ ต้องยิง raw command ได้
