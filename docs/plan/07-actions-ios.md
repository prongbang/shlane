# 07 — Action ฝั่ง iOS

ส่วนที่ยากที่สุดของการแทน fastlane เพราะ code signing ของ Apple ซับซ้อนจริง

## ตารางแปลง

| fastlane | shlane | ความยาก | Priority |
|---|---|---|---|
| `gym` / `build_app` | `build_ios` | สูง | P0 |
| `scan` / `run_tests` | `test_ios` | กลาง | P0 |
| `pilot` / `upload_to_testflight` | `testflight` | กลาง | P0 |
| `match` | `codesign_sync` | สูงมาก | P1 |
| `sigh` / `get_provisioning_profile` | `provisioning_profile` | สูง | P1 |
| `cert` / `get_certificates` | `certificate` | สูง | P1 |
| `deliver` / `upload_to_app_store` | `appstore` | สูงมาก | P2 |
| `produce` | — | — | ไม่ทำ |
| `snapshot` | — | — | ไม่ทำ (ใช้ `run:` เรียก xcodebuild เอง) |
| `frameit` | — | — | ไม่ทำ |
| `pem`, `precheck`, `spaceship` | — | — | ไม่ทำ |
| `setup_ci` | `setup_ci` (ดู [11](11-ci-integration.md)) | กลาง | P0 |
| `create_keychain`, `unlock_keychain`, `delete_keychain` | `keychain` | กลาง | P0 |
| `update_project_team`, `update_code_signing_settings` | `xcode_settings` | กลาง | P2 |
| Appfile | `ios:` block ใน `shlane.yaml` | ต่ำ | P1 |
| Matchfile | `codesign:` block ใน `shlane.yaml` | ต่ำ | P1 |

## `build_ios` (แทน gym)

เป็น wrapper ของ `xcodebuild archive` + `xcodebuild -exportArchive`

```yaml
- id: build
  action: build_ios
  with:
    workspace: MyApp.xcworkspace     # หรือ project:
    scheme: MyApp
    configuration: Release
    export_method: app-store         # app-store | ad-hoc | development | enterprise
    output_dir: ./build
    destination: "generic/platform=iOS"
    xcargs: "-allowProvisioningUpdates"
    clean: true
    silent: false
```

output: `ipa`, `dsym`, `archive`, `app_path`

งานที่ต้องทำจริง:
- สร้าง `ExportOptions.plist` จาก argument (นี่คือสิ่งที่ gym ทำให้แล้วคนไม่รู้ตัว)
- parse output ของ xcodebuild ที่ยาวมาก — ต้องมีโหมดสรุป (แบบ xcpretty) ไม่งั้น log จมทะเล
- แยก error ของ compile ออกจาก error ของ signing ให้ได้ เพราะสองอย่างนี้แก้คนละทาง
- รองรับ `xcresult` bundle สำหรับดึงผล

## `test_ios` (แทน scan)

```yaml
- action: test_ios
  with:
    scheme: MyAppTests
    devices: ["iPhone 15"]
    result_bundle: ./build/test.xcresult
    output: [junit, json]
    code_coverage: true
```

ต้อง parse `.xcresult` ด้วย `xcrun xcresulttool get --format json` แล้วแปลงเป็น JUnit XML ([11](11-ci-integration.md))

## Code signing — ตัดสินใจเชิงกลยุทธ์

`match` คือเหตุผลอันดับหนึ่งที่ทีมยังติดกับ fastlane มันเก็บ certificate/profile ที่เข้ารหัสไว้ใน git repo แล้วซิงก์ลงทุกเครื่อง

มี 3 ทางเลือก:

| ทางเลือก | ข้อดี | ข้อเสีย |
|---|---|---|
| **A. เข้ากันได้กับ match repo เดิม** | ทีมย้ายมาได้โดยไม่ต้องออก certificate ใหม่ | ต้อง reverse-engineer รูปแบบการเข้ารหัสของ match (OpenSSL AES-256-CBC) และโครงสร้างโฟลเดอร์ให้ตรงเป๊ะ |
| **B. ทำระบบใหม่ของตัวเอง** | ออกแบบได้สะอาด ใช้ age/sops ที่ปลอดภัยกว่า | ทีมต้อง migrate certificate ซึ่งเจ็บปวด |
| **C. รองรับเฉพาะ App Store Connect API + `-allowProvisioningUpdates`** | ง่ายสุด ไม่ต้องเก็บ secret เอง Apple จัดการให้ | ไม่รองรับ enterprise/ad-hoc บางกรณี และต้องมี API key |

**ข้อเสนอ: ทำ C ก่อน (M5) → A ทีหลัง (M6+)**
เพราะ C ครอบคลุม CI สมัยใหม่ส่วนใหญ่ และให้ value เร็วสุด ส่วน A เป็นตัวชี้ขาดว่าทีมใหญ่จะย้ายมาได้ไหม

## App Store Connect API

ทั้ง `testflight` และ `appstore` ต้องใช้ JWT ที่เซ็นด้วย ES256 จาก `.p8` key

```yaml
ios:
  api_key:
    key_id: ${env.ASC_KEY_ID}
    issuer_id: ${env.ASC_ISSUER_ID}
    key_content: ${env.ASC_KEY_P8}     # base64 — ต้องถูก mask ใน log เสมอ
```

- ใช้ `jsonwebtoken` + `p256`/`ring` สำหรับเซ็น
- token อายุ 20 นาที ต้อง refresh อัตโนมัติสำหรับการอัปโหลดที่ใช้เวลานาน
- การอัปโหลด binary จริงยังต้องใช้ `xcrun altool` / `iTMSTransporter` ในเฟสแรก — เขียน uploader เองทีหลังถ้าจำเป็น

## ความเสี่ยง

- **Apple เปลี่ยน API/พฤติกรรมบ่อย** — ต้องมี integration test ที่รันจริงบน macOS runner อย่างน้อยสัปดาห์ละครั้ง
- **ทดสอบยาก** — ต้องมี Apple Developer account จริง แยก test เป็น 2 ชั้น: unit test ของการประกอบ argument (รันทุก PR) กับ e2e (รันตามตาราง)
- **macOS runner แพง** — จำกัดจำนวน e2e
