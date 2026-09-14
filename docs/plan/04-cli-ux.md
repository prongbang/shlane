# 04 — CLI และ UX

ปัจจุบันมีคำสั่งเดียว: `shlane run <name> [params...]` (`src/main.rs:17-27`)

## ชุดคำสั่งเป้าหมาย

| คำสั่ง | หน้าที่ | Milestone |
|---|---|---|
| `shlane run <lane> [k=v ...]` | รัน lane (มีแล้ว) | — |
| `shlane list` / `shlane lanes` | แสดง lane ทั้งหมด + description + params (แทน `fastlane lanes`) | M1 |
| `shlane validate` | ตรวจ config โดยไม่รัน | M1 |
| `shlane init` | สร้าง `shlane.yaml` ตั้งต้น (ตรวจว่าเป็นโปรเจกต์ iOS/Android/Flutter แล้วใส่ lane ให้) | M1 |
| `shlane action list` | แสดง action ที่ใช้ได้ | M3 |
| `shlane action show <name>` | แสดง argument ของ action ตัวนั้น | M3 |
| `shlane env` | แสดง env ที่ resolve แล้ว (mask secret) | M2 |
| `shlane plugin add/list/remove` | จัดการ plugin | M6 |
| `shlane migrate` | แปลง Fastfile → shlane.yaml | M6 |
| `shlane completions <shell>` | shell completion | M1 |

## Global flags

| flag | ความหมาย |
|---|---|
| `-f, --file <path>` | ระบุไฟล์ config |
| `-C, --cwd <dir>` | เปลี่ยน working directory ตั้งต้น |
| `--dry-run` | แสดงสิ่งที่จะทำ ไม่รันจริง |
| `-v, --verbose` / `-vv` | เพิ่มระดับ log |
| `-q, --quiet` | เฉพาะ error |
| `--json` | output เป็น JSON บรรทัดละ event (ให้ tool อื่นอ่าน) |
| `--no-color` | ปิดสี (auto-detect ผ่าน `NO_COLOR` และ tty) |
| `--env <profile>` | เลือก `.env.<profile>` (ดู [10](10-secrets-and-env.md)) |
| `--param k=v` | ทางเลือกที่ชัดเจนกว่า trailing args |

## Exit codes

ปัจจุบัน `exit(1)` ทันทีเมื่อ command fail (`src/main.rs:163`) และ lane ไม่เจอก็ยัง exit 0 (`src/main.rs:139-149`) — ต้องแก้

| code | ความหมาย |
|---|---|
| 0 | สำเร็จ |
| 1 | lane ล้มเหลว (step ใด step หนึ่ง fail) |
| 2 | config ผิด / validation ไม่ผ่าน |
| 3 | ไม่พบ lane หรือไม่พบไฟล์ config |
| 4 | พารามิเตอร์ไม่ถูกต้อง |
| 5 | ไม่พบเครื่องมือที่ต้องใช้ (xcodebuild, gradle, ...) |
| 130 | ถูก interrupt (Ctrl-C) |

## รูปแบบ output

```
shlane 0.5.0 · lane: beta · platform: ios

  ✔ ตรวจ git สะอาด                                     0.2s
  ✔ build_ios (scheme=MyApp)                          2m 14s
  ⠋ testflight (ipa=build/MyApp.ipa)
```

ตอนจบต้องมีตารางสรุปแบบ fastlane:

```
สรุป
  #  step                     ผล      เวลา
  1  ensure_git_status_clean  ✔       0.2s
  2  build_ios                ✔    2m 14s
  3  testflight               ✘      45.1s

  ล้มเหลวที่ step 3 (testflight): invalid API key
  รวม 2m 59s
```

ข้อกำหนด:
- log สดต้อง stream ออกมาแบบ real-time (ห้าม buffer จนจบ) — CI ที่ timeout เพราะไม่มี output คือปัญหาจริง
- spinner ต้องปิดอัตโนมัติเมื่อไม่ใช่ tty
- error สุดท้ายต้องบอก **lane, step index, ชื่อ step, บรรทัดใน YAML, คำสั่งจริงที่รัน**
- `--json` ปล่อย event: `lane_started`, `step_started`, `step_output`, `step_finished`, `lane_finished`

## Interrupt handling

Ctrl-C ต้องฆ่า child process ที่กำลังรันก่อน แล้วรัน `error` hook แล้วจึงออกด้วย 130 — ปัจจุบันไม่มีการจัดการเลย ทำให้ `xcodebuild` ค้างเป็น zombie ได้
