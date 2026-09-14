# แผนพัฒนา shlane ให้แทนที่ fastlane

เอกสารชุดนี้คือแผนงานสำหรับพัฒนา `shlane` จากสถานะปัจจุบัน (prototype ~200 บรรทัด)
ไปเป็นเครื่องมือ automation ที่ใช้แทน [fastlane](https://fastlane.tools) ได้จริงในงาน mobile CI/CD

## สถานะปัจจุบัน (v0.1.0)

| หัวข้อ | สถานะ |
|---|---|
| โค้ด | `src/main.rs` ไฟล์เดียว 199 บรรทัด |
| ความสามารถ | `shlane run <lane> [k=v]`, YAML lanes (before/steps/script/after), Rhai builtins 4 ตัว |
| Actions | ยังไม่มี (มีแต่ `run:` ที่เรียก `sh -c`) |
| Tests | ไม่มี |
| CI | ไม่มี |
| เอกสาร | ไม่มี (README.md ยังไม่ถูกสร้าง ทั้งที่ Cargo.toml อ้างถึง) |

## สารบัญ

| ไฟล์ | เนื้อหา |
|---|---|
| [00-goals-and-scope.md](00-goals-and-scope.md) | เป้าหมาย ขอบเขต สิ่งที่ไม่ทำ นิยามคำว่า "แทนได้" |
| [01-gap-analysis.md](01-gap-analysis.md) | เทียบ feature fastlane กับ shlane ทีละข้อ |
| [02-architecture.md](02-architecture.md) | โครงสร้างโมดูล, error handling, lane context |
| [03-config-schema.md](03-config-schema.md) | สเปก `shlane.yaml` v1 |
| [04-cli-ux.md](04-cli-ux.md) | คำสั่ง CLI, flags, exit codes, รูปแบบ output |
| [05-scripting-rhai.md](05-scripting-rhai.md) | API ของ Rhai ที่ต้องมี |
| [06-actions-core.md](06-actions-core.md) | ระบบ action + action กลางที่ไม่ผูกกับ platform |
| [07-actions-ios.md](07-actions-ios.md) | แทน gym / scan / match / pilot / deliver |
| [08-actions-android.md](08-actions-android.md) | แทน gradle / supply / firebase distribution |
| [09-plugins.md](09-plugins.md) | ระบบ plugin (แทน fastlane plugins) |
| [10-secrets-and-env.md](10-secrets-and-env.md) | env, .env, secrets, การ mask ใน log |
| [11-ci-integration.md](11-ci-integration.md) | การใช้งานบน CI, report, GitHub Action |
| [12-migration-from-fastlane.md](12-migration-from-fastlane.md) | เครื่องมือและคู่มือย้ายจาก Fastfile |
| [13-testing-and-quality.md](13-testing-and-quality.md) | กลยุทธ์ทดสอบและคุณภาพโค้ด |
| [14-release-and-distribution.md](14-release-and-distribution.md) | การ build/แจกจ่าย binary |
| [15-roadmap.md](15-roadmap.md) | Milestone M0–M7, ลำดับงาน, ความเสี่ยง |

## วิธีใช้เอกสารชุดนี้

- อ่าน `00` และ `01` ก่อน เพื่อเข้าใจว่าทำไมและแค่ไหนถึงพอ
- `02`–`05` คือรากฐานที่ต้องทำก่อน action ใดๆ (เปลี่ยนทีหลังแพง)
- `06`–`09` คือเนื้องานหลักที่ทำให้ "แทน fastlane ได้"
- `15` คือลำดับการลงมือจริง — ถ้าจะเริ่มพรุ่งนี้ เริ่มที่ M0
