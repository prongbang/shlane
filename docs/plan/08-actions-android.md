# 08 — Action ฝั่ง Android

ง่ายกว่า iOS มาก ควรทำ **ก่อน** iOS เพื่อพิสูจน์สถาปัตยกรรม action ด้วยงานที่คุมได้

## ตารางแปลง

| fastlane | shlane | ความยาก | Priority |
|---|---|---|---|
| `gradle` | `gradle` | ต่ำ | P0 |
| `gradle(task: "assembleRelease")` | `build_android` | ต่ำ | P0 |
| `gradle(task: "bundleRelease")` | `build_android` (`format: aab`) | ต่ำ | P0 |
| `gradle(task: "test")` | `test_android` | ต่ำ | P0 |
| `supply` / `upload_to_play_store` | `play_store` | กลาง | P1 |
| `firebase_app_distribution` (plugin) | `firebase_distribution` | กลาง | P1 |
| `sign_apk` / `zipalign` (plugin) | `sign_android` | กลาง | P1 |
| `get_version_code` (plugin) | `read_version` (ดู [06](06-actions-core.md)) | ต่ำ | P0 |
| `screengrab` | — | — | ไม่ทำ |

## `gradle` — ตัวฐาน

```yaml
- action: gradle
  with:
    task: assembleRelease
    project_dir: ./android
    properties:
      android.injected.version.code: ${steps.ver.code}
    flags: ["--no-daemon", "--stacktrace"]
    wrapper: true        # ใช้ ./gradlew ถ้ามี (default)
```

- ต้องหา `gradlew` โดยไล่ขึ้นจาก `project_dir` และตรวจ execute permission
- parse output ของ gradle เพื่อหาไฟล์ที่ถูกสร้าง (`apk`/`aab` path) แล้วคืนเป็น output — ปัจจุบันคนต้อง hardcode path เอง
- ตั้ง `ORG_GRADLE_PROJECT_*` จาก `properties` แทนการต่อ `-P` ยาวๆ เมื่อค่าเป็น secret (ไม่โผล่ใน process list)

## `build_android`

```yaml
- id: build
  action: build_android
  with:
    format: aab            # apk | aab
    flavor: prod
    build_type: release
    project_dir: ./android
```

output: `aab` / `apk`, `mapping_txt`, `version_code`, `version_name`

## `sign_android`

```yaml
- action: sign_android
  with:
    input: ${steps.build.apk}
    keystore: ${env.ANDROID_KEYSTORE_PATH}      # หรือ keystore_base64
    keystore_password: ${env.KEYSTORE_PASSWORD} # sensitive
    key_alias: upload
    key_password: ${env.KEY_PASSWORD}           # sensitive
```

- ใช้ `apksigner` + `zipalign` จาก Android SDK build-tools (หา path จาก `$ANDROID_HOME`)
- รองรับ keystore แบบ base64 ใน env สำหรับ CI แล้วเขียนลง temp file ที่ลบทิ้งเสมอ (แม้ตอน error)
- ค่า password ทุกตัวต้องเข้า `SecretRegistry` (ดู [10](10-secrets-and-env.md))

## `play_store` (แทน supply)

ใช้ Google Play Developer Publishing API v3 ซึ่งเป็น REST ตรงไปตรงมา:

```yaml
- action: play_store
  with:
    package_name: com.example.app
    aab: ${steps.build.aab}
    track: internal          # internal | alpha | beta | production
    release_status: draft
    rollout: 0.1
    service_account_json: ${env.PLAY_SERVICE_ACCOUNT}   # sensitive
    mapping: ${steps.build.mapping_txt}
    release_notes:
      en-US: ${params.notes}
```

ขั้นตอนของ API: `edits.insert` → `edits.bundles.upload` → `edits.tracks.update` → `edits.commit`
ต้องจัดการ: OAuth2 service account (JWT → access token), resumable upload สำหรับไฟล์ใหญ่, retry เมื่อเจอ 5xx และ rate limit

## `firebase_distribution`

- ทางเลือก 1: wrap `firebase` CLI (เร็ว ทำเสร็จได้ใน 1 วัน) — แต่เพิ่ม dependency ที่ผู้ใช้ต้องติดตั้ง ซึ่งขัดกับเป้าหมาย "binary เดียว"
- ทางเลือก 2: เรียก Firebase App Distribution REST API ตรง (ใช้ service account เดียวกับ Play)

**ข้อเสนอ: ทำทางเลือก 2** และมี `use_cli: true` เป็น fallback

## ทำไมต้องทำ Android ก่อน

1. ทดสอบได้บน Linux runner ที่ถูกและเร็ว
2. ไม่ต้องมี Apple account ในการพัฒนา
3. Play API เป็น REST ธรรมดา → พิสูจน์ชั้น HTTP/auth/retry ที่ iOS จะใช้ซ้ำได้
4. code signing ของ Android คือ "ไฟล์ keystore + password" ซึ่งเข้าใจง่ายกว่า match มาก
