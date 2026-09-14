# 11 — การใช้งานบน CI

จุดขายหลักของ shlane เหนือ fastlane อยู่ตรงนี้: ไม่ต้อง `bundle install`

## ตรวจจับ CI

```rhai
is_ci()        // true เมื่อเจอ CI, GITHUB_ACTIONS, GITLAB_CI, BITRISE_IO, CIRCLECI, JENKINS_URL, BUILDKITE
ci_provider()  // "github" | "gitlab" | "bitrise" | ...
```

พฤติกรรมที่เปลี่ยนอัตโนมัติเมื่ออยู่บน CI:
- ปิด spinner และสี (ยกเว้น provider ที่รองรับ ANSI)
- `ui_confirm()` ไม่รอ input — ใช้ default หรือ error ตาม flag
- เปิด `--verbose` โดยปริยายเมื่อ step ล้มเหลว (พิมพ์ log เต็มของ step นั้นซ้ำ)

## `setup_ci` (แทนของ fastlane)

```yaml
- action: setup_ci
  with:
    keychain_name: shlane_tmp
    timeout: 3600
```

ทำ: สร้าง temp keychain, unlock, ตั้งเป็น default, และลงทะเบียน cleanup ให้ลบทิ้งตอนจบ **ไม่ว่า lane จะสำเร็จหรือล้มเหลว**

## Report

| รูปแบบ | flag | ใช้กับ |
|---|---|---|
| JUnit XML | `--report junit:./reports/shlane.xml` | ทุก CI ที่อ่าน test report |
| JSON | `--report json:./reports/shlane.json` | เครื่องมือภายใน |
| Markdown summary | `--report md:$GITHUB_STEP_SUMMARY` | GitHub Actions job summary |

## Annotations ของ GitHub Actions

เมื่อ `ci_provider() == "github"` ให้ปล่อย workflow command:

```
::group::build_ios
::error file=shlane.yaml,line=42::step 'testflight' ล้มเหลว: invalid API key
::endgroup::
```

ทำให้ error ไปปรากฏบนไฟล์ในหน้า PR ตรงๆ — สิ่งที่ fastlane ไม่ทำให้

## GitHub Action wrapper

สร้าง repo แยก `prongbang/shlane-action`:

```yaml
- uses: prongbang/shlane-action@v1
  with:
    version: "0.5.0"      # หรือ "latest"
    lane: beta
    params: "target=production"
```

- ดาวน์โหลด binary ตาม platform, ตรวจ checksum, cache ด้วย `@actions/tool-cache`
- ใช้เวลา setup ~1–2 วินาที เทียบกับ `bundle install` ที่ 30–120 วินาที — **เป็นตัวเลขที่ควรโฆษณา**

## ตัวอย่าง workflow

```yaml
jobs:
  beta:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: prongbang/shlane-action@v1
        with: { version: "0.5.0" }
      - run: shlane validate
      - run: shlane run beta target=production --report junit:reports/shlane.xml
        env:
          ASC_KEY_ID: ${{ secrets.ASC_KEY_ID }}
          ASC_ISSUER_ID: ${{ secrets.ASC_ISSUER_ID }}
          ASC_KEY_P8: ${{ secrets.ASC_KEY_P8 }}
      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: reports, path: reports/ }
```

## Caching

`shlane` เองไม่ควรทำ cache แต่ควร **บอก CI ได้ว่าอะไรควร cache**:

```
shlane cache-paths --json
# → ["~/.gradle/caches", "~/Library/Developer/Xcode/DerivedData", ".shlane/plugins"]
```

## ข้อกำหนดเรื่องความทนทาน

- ทุก network action ต้อง retry แบบ exponential backoff (CI network ไม่เสถียรเป็นปกติ)
- มี timeout ต่อ step (ดู [03](03-config-schema.md)) เพื่อไม่ให้ job ค้างจนชน limit ของ CI
- รับสัญญาณ SIGTERM ที่ CI ส่งตอนใกล้ timeout → รัน `error` hook → ออกอย่างสะอาด
