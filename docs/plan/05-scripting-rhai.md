# 05 — Rhai Scripting API

ปัจจุบันมี builtin 4 ตัว: `param`, `env`, `run`, `print` (`src/main.rs:167-199`)

## ปัญหาของ API ปัจจุบัน

| ปัญหา | ผล |
|---|---|
| `run()` คืน `i32` อย่างเดียว (`src/main.rs:180`) | เอา stdout ไปใช้ต่อไม่ได้ ซึ่งเป็น use case หลักของการเขียน script |
| `run()` ไม่หยุดเมื่อ fail | script รันต่อทั้งที่คำสั่งก่อนหน้าพัง |
| `param()` คืน `""` เมื่อไม่มี key (`src/main.rs:171`) | พิมพ์ชื่อ param ผิดแล้วไม่มีใครรู้ |
| `env()` อ่านจาก process env (`src/main.rs:175`) | ผูกกับ global state ที่จะเลิกใช้ (ดู [02](02-architecture.md)) |
| ไม่มีทางเซ็ตค่ากลับไปให้ step ถัดไป | script เป็น dead end |
| ไม่มี limit | script วนลูปไม่รู้จบทำให้ CI ค้าง |

## API เป้าหมาย

### การรันคำสั่ง

```rhai
let r = run("git rev-parse HEAD");   // fail แล้ว throw (ยกเลิก lane)
r.stdout      // String
r.stderr      // String
r.code        // int
r.success     // bool

let r = try_run("which gradle");     // ไม่ throw ให้เช็ค r.success เอง
let out = capture("git log -1 --pretty=%s");   // คืน stdout ที่ trim แล้ว
```

### พารามิเตอร์และ env

```rhai
param("target")              // throw ถ้าไม่มีและไม่มี default
param_or("target", "dev")
has_param("target")
env("APP_ENV")               // อ่านจาก LaneContext ไม่ใช่ process env
set_env("BUILD_NUMBER", n)   // มีผลกับ step ถัดไปใน lane เดียวกัน
```

### ส่งค่าระหว่าง step (แทน `lane_context` ของ fastlane)

```rhai
set_output("ipa_path", "build/MyApp.ipa");
output("build", "ipa")       // อ่านผลของ step id = build
```

### เรียก action และ lane (ยังไม่ทำ — รอ M3)

```rhai
action("build_ios", #{ scheme: "MyApp", configuration: "Release" });
call_lane("notify", #{ channel: "#releases" });
```

นี่คือกุญแจสำคัญ: ทำให้ Rhai เป็น escape hatch เต็มรูปแบบ — อะไรที่ YAML ทำไม่ได้ (เงื่อนไขซับซ้อน, loop) เขียน Rhai แล้วยังเรียก action เดิมได้

### UI / logging

```rhai
ui_message("...");  ui_success("...");  ui_error("...");  ui_important("...");
ui_confirm("จะ deploy จริงไหม?")   // บน CI ให้คืน true อัตโนมัติหรือ error ตาม flag
```

`print()` ปัจจุบัน override built-in ของ Rhai (`src/main.rs:196`) — เก็บไว้ได้แต่ route ผ่านระบบ logging เดียวกัน

### Utility

```rhai
file_exists(p); read_file(p); write_file(p, s);
json_parse(s); json_stringify(v); yaml_parse(s);
semver_bump("1.2.3", "minor");    // "1.3.0"
now_iso(); git_sha(); git_branch();
```

## ความปลอดภัยและขอบเขต

ตั้งค่า Engine ตอนสร้าง (`src/main.rs:87`):

```rust
engine.set_max_operations(10_000_000);
engine.set_max_expr_depths(64, 32);
engine.set_max_string_size(10 * 1024 * 1024);
engine.set_max_array_size(100_000);
engine.disable_symbol("eval");
```

และมี timeout รวมของ script ผ่าน `on_progress`

## Error reporting

ปัจจุบัน error จาก script ถูก `eprintln!` แล้วรันต่อ (`src/main.rs:125-127`) — ต้องเปลี่ยนเป็น:
- script error = lane fail (ยกเว้น step นั้นตั้ง `continue_on_error: true`)
- ข้อความ error ต้องมีเลขบรรทัดของ script และ map กลับไปยังบรรทัดใน `shlane.yaml`

## ทางเลือกในอนาคต

รองรับ `script_file: ./scripts/release.rhai` เพื่อให้ script ยาวๆ ออกจาก YAML ไปอยู่ในไฟล์ที่ editor ช่วย highlight ได้
