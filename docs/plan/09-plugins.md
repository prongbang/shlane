# 09 — ระบบ Plugin

fastlane มี plugin กว่า 300 ตัวที่เป็น Ruby gem — shlane จะไม่มีทางทำ action ครบเท่านี้ด้วยตัวเอง ระบบ plugin จึงจำเป็น

## ทางเลือก

| แบบ | วิธีทำ | ข้อดี | ข้อเสีย |
|---|---|---|---|
| **A. External executable** | plugin คือ binary/script ที่ชื่อ `shlane-<name>` คุยกันด้วย JSON ผ่าน stdin/stdout | เขียนด้วยภาษาอะไรก็ได้, ไม่กระทบ binary หลัก, แยก crash ได้ | ต้อง spawn process, ส่งข้อมูลใหญ่ไม่สะดวก |
| **B. Rhai module** | plugin คือไฟล์ `.rhai` ที่โหลดจาก path หรือ git | ไม่ต้อง compile, เขียนง่ายมาก, sandbox ได้ | ทำได้แค่สิ่งที่ builtin เปิดให้, ช้า |
| **C. WASM** | plugin เป็น `.wasm` component | sandbox แน่น, portable | เครื่องมือยังไม่สุก, เข้าถึง filesystem/network ลำบาก |
| **D. Rust crate + recompile** | ผู้ใช้ build shlane เอง | เร็วสุด | ขัดกับเป้าหมาย "ดาวน์โหลดแล้วใช้เลย" |

**ข้อเสนอ: A + B** (A สำหรับงานหนัก, B สำหรับ glue code) และเก็บ C ไว้พิจารณาหลัง 1.0

## Protocol ของ external plugin (แบบ A)

```
shlane → plugin (stdin, JSON บรรทัดเดียว)
{
  "protocol": 1,
  "op": "run",                   // "describe" | "run" | "dry_run"
  "action": "notify_line",
  "args": { "token": "***", "message": "hi" },
  "context": { "lane": "beta", "workdir": "/repo", "dry_run": false }
}

plugin → shlane (stdout, JSON บรรทัดละ event)
{"type":"log","level":"info","message":"sending..."}
{"type":"secret","value":"xxx"}                 // ขอให้ mask ค่านี้ใน log
{"type":"result","ok":true,"outputs":{"id":"123"}}
```

- `op: "describe"` คืน schema ของ action → ใช้ใน `shlane validate` และ `shlane action show`
- exit code ไม่ใช่ 0 หรือไม่มี `result` event = ล้มเหลว
- stderr ของ plugin ถูก relay เป็น log ระดับ warn

## Manifest

```yaml
# shlane-plugin.yaml
name: line-notify
version: 0.1.0
protocol: 1
executable: ./bin/shlane-line-notify
platforms: [macos, linux]
actions:
  - notify_line
```

## การประกาศใช้ใน shlane.yaml

```yaml
plugins:
  - name: line-notify
    source: github:someone/shlane-line-notify@v0.1.0
  - name: internal-tools
    source: path:./tools/shlane-plugins/internal
```

## คำสั่ง

```
shlane plugin add github:someone/shlane-line-notify@v0.1.0
shlane plugin list
shlane plugin remove line-notify
shlane plugin verify          # เช็ค checksum + protocol version
```

- ติดตั้งลง `.shlane/plugins/` ในโปรเจกต์ (commit `shlane-plugins.lock` ที่มี checksum)
- **ต้องมี lockfile พร้อม SHA-256** — plugin คือโค้ดที่รันด้วยสิทธิ์เต็มบน CI ที่ถือ signing key ของแอป การ resolve แบบ floating version คือช่องโหว่ supply chain

## สถานะ (M6 — ทำแล้ว)

- plugin แบบ external executable (ทางเลือก A) + protocol v1 ครบ
- `path:` และ `source:` (`github:owner/repo@tag`, git URL, ssh)
- `shlane plugin install / list / lock / verify`
- lockfile SHA-256 — tag ที่ถูกย้ายจะถูกปฏิเสธ ไม่ใช่ติดตั้งทับ

- Rhai module plugin (ทางเลือก B) — manifest ใช้ `script:` แทน `executable:` หนึ่ง action
  ต่อหนึ่ง function ได้ builtin เหมือน script ของ lane ยกเว้น `action()` (registry ถือ plugin อยู่
  จะส่ง registry กลับเข้า plugin ไม่ได้)

## ข้อกำหนดด้านความปลอดภัย

1. ไม่ auto-install plugin ตอน `shlane run` — ต้องสั่ง `plugin add` อย่างชัดเจน
2. lockfile ต้องมี checksum และตรวจทุกครั้งก่อนรัน
3. `shlane run` ต้องพิมพ์รายชื่อ plugin ที่โหลดตอนเริ่ม (ให้เห็นใน audit log ของ CI)
4. plugin ไม่ได้รับ secret ทั้งหมดของ context — ได้เฉพาะค่าที่ระบุใน `with:` ของ step นั้น
