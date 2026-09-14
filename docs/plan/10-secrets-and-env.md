# 10 — Environment และ Secrets

## ปัญหาปัจจุบัน

```rust
// src/main.rs:79-83
if let Some(envs) = config.env {
    for (key, value) in envs {
        env::set_var(key, value);      // แก้ env ของทั้งโปรเซส
    }
}
```

1. `env::set_var` เป็น `unsafe` ใน Rust 2024 (ไม่ thread-safe)
2. ค่ารั่วข้าม lane — lane ที่เรียกทีหลังเห็น env ของ lane ก่อนหน้า
3. `example/shlane.yaml` เก็บ `API_KEY: "abc123"` ตรงๆ ในไฟล์ที่ commit → สอนวิธีที่ผิด
4. ไม่มีการ mask ค่าใดๆ ใน log เลย

## ลำดับความสำคัญของ env (สูงสุดชนะ)

```
1. env ของ step        (step.env)
2. --param / ตัวแปรที่ set_env() ใน script
3. env ของ lane        (lane.env)
4. env ของ process     (ที่ CI ฉีดมา)
5. .env.<profile>      (จาก --env หรือ $SHLANE_PROFILE)
6. .env
7. env ของ config      (config.env)
```

env ทั้งหมดอยู่ใน `LaneContext.env` แล้วส่งเข้า child process ด้วย `Command::envs()` — **ไม่แตะ env ของโปรเซสหลักเลย**

## ไฟล์ .env

```yaml
env_files:
  - .env                    # ไม่ commit
  - .env.${SHLANE_PROFILE}  # ไม่ commit
  - .env.defaults           # commit ได้ (ค่าที่ไม่ลับ)
```

- ไฟล์ที่ไม่มีอยู่จริงให้ข้ามเงียบๆ ถ้าอยู่ในรูป `${...}` ที่ resolve ไม่ได้
- `shlane init` ต้องเพิ่ม `.env*` ลง `.gitignore` ให้ (ยกเว้น `.env.defaults`)

## การ mask secret ใน log

`SecretRegistry` เก็บค่าที่ต้องปิดบัง ค่าจะเข้าทะเบียนเมื่อ:

- argument ของ action ที่ schema ระบุ `sensitive: true`
- ตัวแปร env ที่ชื่อเข้าเงื่อนไข: `*_TOKEN`, `*_SECRET`, `*_PASSWORD`, `*_KEY`, `*_CREDENTIALS`
- ประกาศเองใน config:
  ```yaml
  secrets:
    - ${env.MY_CUSTOM_VALUE}
  ```
- plugin ส่ง event `{"type":"secret","value":"..."}` (ดู [09](09-plugins.md))

การ mask ต้องทำ **ทุกช่องทาง**: stdout/stderr ของ child process, ข้อความ error, ตารางสรุป, `--json` output, และรายการคำสั่งที่พิมพ์ตอน `--dry-run`

ข้อควรระวัง: ต้อง mask ทั้งค่าดิบ, ค่าที่ base64 แล้ว และค่าที่ url-encoded แล้ว เพราะ tool ปลายทางมักแปลงก่อน log

## การส่ง secret เข้า subprocess

- **ห้ามใส่ใน command line** — โผล่ใน `ps aux` และใน log ของ CI ที่พิมพ์คำสั่ง
- ใช้ env ของ process ลูก หรือเขียนลง temp file ที่มี permission `0600` และลบทิ้งด้วย RAII guard (ลบแม้ตอน panic/Ctrl-C)

## Credential store (ภายหลัง)

สำหรับเครื่อง dev: เก็บใน macOS Keychain / libsecret ผ่าน `keyring` crate เพื่อไม่ต้องมี `.env` วางบนดิสก์ — เป็นงานหลัง 1.0

## Checklist ที่ต้องผ่าน

- [ ] ไม่มี `env::set_var` เหลือในโค้ด
- [ ] `shlane env` แสดงค่าทั้งหมดโดย secret เป็น `***`
- [ ] มี test ที่พิสูจน์ว่า secret ไม่โผล่ใน stdout, stderr, error message และ JSON output
- [ ] `example/shlane.yaml` เปลี่ยนจาก `API_KEY: "abc123"` เป็นการอ่านจาก env
