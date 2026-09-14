# 12 — การย้ายจาก fastlane

คนจะไม่ย้ายถ้าต้องเขียนใหม่ทั้งหมด — เส้นทางการย้ายสำคัญไม่แพ้ตัว feature

## กลยุทธ์: ย้ายทีละ lane ไม่ใช่ทีเดียวทั้งหมด

ระหว่างเปลี่ยนผ่าน ให้ทั้งสองอย่างอยู่ร่วมกันได้:

```yaml
lanes:
  test:
    steps:
      - action: test_android        # ย้ายมาแล้ว

  beta:
    steps:
      - run: bundle exec fastlane beta   # ยังไม่ได้ย้าย — เรียก fastlane เดิมไปก่อน
```

ลำดับที่แนะนำ: `test` → `build` → `bump version/changelog` → `distribute` (ยากสุด ย้ายท้ายสุด)

## `shlane migrate`

```
shlane migrate --fastfile fastlane/Fastfile --out shlane.yaml
```

เป็นเครื่องมือ **best-effort** ไม่ใช่ตัวแปลสมบูรณ์ — Fastfile คือ Ruby ที่รันโค้ดอะไรก็ได้

ทำได้:
- ดึง `lane :name do |options| ... end` เป็น lane
- ดึง `platform :ios do ... end` เป็น `platform: ios`
- ดึง `private_lane` เป็น `private: true`
- ดึง `desc "..."` เป็น `description`
- แปลง action ที่อยู่ในตารางด้านล่าง พร้อม argument
- แปลง `sh "..."` เป็น `run:`
- แปลง `options[:key]` เป็น `${params.key}`

ทำไม่ได้ (ต้องใส่ `# TODO: ย้ายด้วยมือ` ไว้ให้):
- เงื่อนไข/ลูป Ruby, การเรียก method ที่ผู้ใช้เขียนเอง
- `lane_context[SharedValues::X]` ที่ซับซ้อน
- plugin ที่ไม่มีตัวเทียบ
- โค้ด Ruby ใน `Fastfile` ที่อยู่นอก lane

output ต้องมี **รายงานสรุป**: แปลงได้กี่ lane, กี่ action, อะไรที่ต้องทำเอง

## ตารางแปลง action (ย่อ)

| fastlane | shlane | เอกสาร |
|---|---|---|
| `sh` | `run:` | [03](03-config-schema.md) |
| `gym` / `build_app` | `build_ios` | [07](07-actions-ios.md) |
| `scan` / `run_tests` | `test_ios` | [07](07-actions-ios.md) |
| `pilot` / `upload_to_testflight` | `testflight` | [07](07-actions-ios.md) |
| `match` | `codesign_sync` | [07](07-actions-ios.md) |
| `gradle` | `gradle` / `build_android` | [08](08-actions-android.md) |
| `supply` | `play_store` | [08](08-actions-android.md) |
| `firebase_app_distribution` | `firebase_distribution` | [08](08-actions-android.md) |
| `increment_build_number` | `bump_version` | [06](06-actions-core.md) |
| `git_commit` / `add_git_tag` / `push_to_git_remote` | `git_commit` / `git_tag` / `git_push` | [06](06-actions-core.md) |
| `changelog_from_git_commits` | `changelog_from_commits` | [06](06-actions-core.md) |
| `slack` | `notify_slack` | [06](06-actions-core.md) |
| `ensure_git_status_clean` | `git_status_clean` | [06](06-actions-core.md) |
| `setup_ci` | `setup_ci` | [11](11-ci-integration.md) |
| `Appfile` | `ios:` block | [07](07-actions-ios.md) |
| `Matchfile` | `codesign:` block | [07](07-actions-ios.md) |
| `.env` ของ fastlane | `env_files:` | [10](10-secrets-and-env.md) |

ตารางเต็มต้องอยู่ใน `docs/migration.md` และอัปเดตทุกครั้งที่เพิ่ม action

## เอกสารที่ต้องเขียนคู่กัน

1. **"ย้ายจาก fastlane ใน 15 นาที"** — คู่มือสั้น เน้นโปรเจกต์ทั่วไป
2. **ตารางเทียบ action แบบเต็ม** — ให้ค้นหาได้ว่า action ที่ใช้อยู่มีตัวแทนไหม
3. **"อะไรที่ shlane ยังทำไม่ได้"** — ซื่อสัตย์ตั้งแต่แรก ดีกว่าให้คนย้ายมาแล้วติด

## ตัวชี้วัด

ถือว่าเส้นทางการย้ายใช้ได้เมื่อ: โปรเจกต์ตัวอย่างที่มี Fastfile ~100 บรรทัด ย้ายเสร็จภายใน 1 ชั่วโมงโดยคนที่ไม่เคยใช้ shlane มาก่อน
