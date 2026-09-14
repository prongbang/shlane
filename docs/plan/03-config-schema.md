# 03 — สเปก `shlane.yaml` v1

## หลักการ

- ทุก field มี default ที่สมเหตุสมผล — ไฟล์ที่สั้นที่สุดที่ใช้งานได้ต้องสั้นจริง
- schema ตรวจได้ก่อนรันด้วย `shlane validate`
- รองรับไฟล์ปัจจุบัน (`example/shlane.yaml`) ต่อไปได้ ไม่ทำ breaking change โดยไม่จำเป็น

## ตัวอย่างเต็ม

```yaml
version: 1                      # ใหม่ — ไว้ migrate schema ในอนาคต
min_shlane: "0.5.0"             # ใหม่ — แทน fastlane_version

env:
  APP_ENV: production
env_files:                      # ใหม่ — ดู 10
  - .env
  - .env.${SHLANE_PROFILE}

include:                        # ใหม่ — แตกไฟล์ย่อยได้
  - lanes/ios.yaml
  - lanes/android.yaml

script: |-                      # shared Rhai script (มีอยู่แล้ว)
  fn greet(name) { print("Hello, " + name); }

before_all:                     # ใหม่ — ระดับ global
  - run: git rev-parse --short HEAD
after_all:
  - action: notify_slack
    with: { text: "เสร็จแล้ว" }
error:                          # ใหม่ — รันเมื่อ lane ใดก็ตาม fail
  - action: notify_slack
    with: { text: "พังที่ ${error.step}" }

lanes:
  beta:
    description: "build + ขึ้น TestFlight"   # ใหม่ — โชว์ใน `shlane list`
    platform: ios                            # ใหม่ — จัดกลุ่ม
    private: false                           # ใหม่
    params:                                  # ใหม่ — ประกาศพารามิเตอร์
      target:
        type: string                         # string | int | bool | enum
        required: true
        values: [staging, production]
        description: "ปลายทางที่จะ deploy"
      notes:
        type: string
        default: "no notes"
    steps:
      - name: "ตรวจ git สะอาด"               # ใหม่ — ชื่อที่อ่านออกใน log
        action: ensure_git_status_clean

      - id: build                            # ใหม่ — เก็บผลไว้อ้างต่อ
        action: build_ios
        with:
          scheme: MyApp
          configuration: Release

      - name: upload
        action: testflight
        with:
          ipa: ${steps.build.ipa}            # ใหม่ — อ้างผลของ step ก่อนหน้า
        if: ${params.target} == "production"  # ใหม่ — เงื่อนไข
        retry: 2                             # ใหม่
        timeout: 20m                         # ใหม่

      - run: ./scripts/cleanup.sh            # รูปแบบเดิม ยังใช้ได้
        workdir: ./ios                       # ใหม่
        continue_on_error: true              # ใหม่
        env: { FOO: bar }                    # ใหม่ — env เฉพาะ step

      - script: |                            # script เป็น step ได้แล้ว
          print("done " + param("target"));

      - lane: notify                         # ใหม่ — เรียก lane อื่น
        with: { channel: "#releases" }

  notify:
    private: true
    steps:
      - action: notify_slack
        with: { channel: ${params.channel} }
```

## ชนิดของ step

step หนึ่งตัวต้องมีเพียง key เดียวจาก 4 อย่างนี้:

| key | ความหมาย |
|---|---|
| `run:` | คำสั่ง shell |
| `action:` | เรียก built-in action หรือ plugin (ดู [06](06-actions-core.md)) |
| `script:` | Rhai inline |
| `lane:` | เรียก lane อื่นในไฟล์เดียวกัน |

field ร่วมของทุก step: `name`, `id`, `if`, `env`, `workdir`, `timeout`, `retry`, `continue_on_error`

## Interpolation

ขยายจาก `${key}` ปัจจุบัน (`src/main.rs:65-72`) เป็น namespace:

| รูปแบบ | มาจาก |
|---|---|
| `${params.x}` | พารามิเตอร์ของ lane |
| `${env.X}` | environment |
| `${steps.<id>.<field>}` | ผลของ step ก่อนหน้า (`stdout`, `code`, หรือ output เฉพาะของ action) |
| `${shlane.lane}`, `${shlane.version}` | ข้อมูลของ runtime |

กฎสำคัญ 2 ข้อที่ต่างจากปัจจุบัน:
1. **อ้างตัวแปรที่ไม่มีจริง = error** ไม่ใช่ปล่อย `${x}` ดิบไปให้ shell (`src/main.rs:69`)
2. **ค่าที่แทนเข้าไปใน `run:` ต้อง escape** — ปัจจุบันต่อสตริงตรงๆ ทำให้ `target="a; rm -rf /"` รันได้จริง

รองรับ `${key}` แบบเดิม (ไม่มี namespace) ต่อไป โดย resolve ตามลำดับ params → env และเตือน deprecated

## Validation ที่ `shlane validate` ต้องจับได้

- YAML syntax ผิด (พร้อมเลข บรรทัด)
- `lane:` อ้างถึง lane ที่ไม่มี / เรียกวนเป็นวงจร
- `action:` ชื่อไม่มีในทะเบียน
- `with:` ขาด argument ที่ action บังคับ / มี key เกิน
- step ที่ไม่มี `run`/`action`/`script`/`lane` หรือมีมากกว่าหนึ่ง
- `${...}` ที่อ้างถึงสิ่งที่ไม่มีทาง resolve ได้ (เช่น `steps.x` ที่ไม่มี step id นั้น หรืออยู่หลังจุดที่อ้าง)
- `params.type` กับ `default` ไม่ตรงชนิด
- `min_shlane` สูงกว่าเวอร์ชันที่ติดตั้ง

## การหาไฟล์ config

ปัจจุบันอ่าน `shlane.yaml` ใน cwd อย่างเดียว (`src/main.rs:75`) เปลี่ยนเป็น:

1. `--file <path>` ถ้าระบุ
2. `$SHLANE_CONFIG`
3. ไล่หาขึ้นไปจาก cwd จนถึง root: `shlane.yaml` → `shlane.yml` → `.shlane/shlane.yaml`
4. ไม่เจอ → error พร้อมแนะนำ `shlane init`

workdir ตั้งต้นของทุก step = โฟลเดอร์ที่มีไฟล์ config ไม่ใช่ cwd (รันจาก subdirectory ไหนก็ได้ผลเหมือนกัน)
