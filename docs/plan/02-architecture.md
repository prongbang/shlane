# 02 — สถาปัตยกรรม

## โครงสร้างโมดูลเป้าหมาย

แยก `src/main.rs` (199 บรรทัด) ออกเป็น:

```
src/
  main.rs              # entry point บางๆ: parse CLI → เรียก app::run() → map error เป็น exit code
  cli/
    mod.rs             # clap definitions
    commands/          # run.rs, list.rs, init.rs, validate.rs, action.rs, plugin.rs, migrate.rs
  config/
    mod.rs
    model.rs           # struct Config, Lane, Step, Param (serde)
    loader.rs          # หา + อ่านไฟล์, รองรับ include, merge
    validate.rs        # ตรวจ schema, lane ที่อ้างไม่มีจริง, วงจร lane ซ้อน
  runtime/
    mod.rs
    executor.rs        # ลำดับการรัน lane/step, hooks, error handling
    context.rs         # LaneContext: params, env, outputs, dry_run, workdir
    shell.rs           # spawn process, stream output, จับ stdout/stderr, timeout
    interpolate.rs     # ${var} + escaping
  script/
    mod.rs
    engine.rs          # ตั้งค่า Rhai engine (limits, modules)
    builtins.rs        # ลงทะเบียน run/param/env/ui/...
  actions/
    mod.rs
    registry.rs        # trait Action + ทะเบียน
    core/              # sh, git, http, notify, version, file
    ios/
    android/
  plugin/
    mod.rs             # โหลด plugin ภายนอก
  report/
    mod.rs             # สรุปผล, JUnit XML, JSON
  error.rs             # ShlaneError
```

กฎ: `main.rs` ต้องไม่เกิน ~50 บรรทัด และไม่มี business logic

## Error handling

เลิกใช้ `expect()` ทั้งหมด (ปัจจุบันมีที่ `src/main.rs:75`, `76`, `159`)

```rust
// error.rs
#[derive(Debug, thiserror::Error)]
pub enum ShlaneError {
    #[error("ไม่พบไฟล์ config: {0}")]
    ConfigNotFound(PathBuf),
    #[error("config ผิดรูปแบบที่ {path}:{line}: {msg}")]
    ConfigInvalid { path: PathBuf, line: usize, msg: String },
    #[error("ไม่พบ lane '{name}' (lane ที่มี: {available})")]
    LaneNotFound { name: String, available: String },
    #[error("step '{step}' ล้มเหลว (exit code {code})")]
    StepFailed { step: String, code: i32 },
    #[error("script error: {0}")]
    Script(String),
    #[error("action '{action}' ล้มเหลว: {msg}")]
    Action { action: String, msg: String },
}
```

- ใช้ `anyhow::Result` ที่ชั้นบน, `thiserror` ที่ชั้น library
- **ลบ `panic = "abort"` ออกจาก `Cargo.toml`** เพราะบังคับให้ error ทุกอย่างกลายเป็น crash ที่อ่านไม่ออก
- error ทุกตัวต้องบอกได้ว่า "พังที่ lane ไหน step ที่เท่าไร บรรทัดไหนใน YAML"

## LaneContext — แทน global state

ปัญหาปัจจุบัน: `env::set_var` (`src/main.rs:81`) แก้ env ของทั้งโปรเซส — ใน Rust 2024 เป็น `unsafe` และค่ารั่วข้าม lane

```rust
pub struct LaneContext {
    pub lane: String,
    pub params: HashMap<String, Value>,   // จาก CLI + default ใน config
    pub env: HashMap<String, String>,     // ส่งเข้า Command::envs() ไม่แตะ env ของโปรเซส
    pub outputs: HashMap<String, Value>,  // ผลลัพธ์ของ step/action (แทน lane_context ของ fastlane)
    pub workdir: PathBuf,
    pub dry_run: bool,
    pub secrets: SecretRegistry,          // ค่าที่ต้อง mask ใน log (ดู 10)
    pub started_at: Instant,
}
```

- step ที่มี `id:` จะเก็บผลลง `outputs[id]` → อ้างถึงได้ด้วย `${steps.build.stdout}` หรือ `output("build")` ใน Rhai
- `env` เป็น map ที่ส่งเข้า child process เท่านั้น ไม่มีการ mutate global

## ลำดับการทำงานของ lane

ปัจจุบันตายตัวเป็น before → steps → script → after (`src/main.rs:102-136`) ซึ่งทำให้ `example/shlane.yaml` lane `deploy` ทำงานผิดลำดับ

เป้าหมาย: **`steps` เป็นลำดับเดียว** — script เป็น step ชนิดหนึ่ง ไม่ใช่ block แยก

```
global before_all
  └─ lane before
       └─ steps[0..n]   (run | action | script | lane)
            └─ ถ้า fail → lane error hook → global after_all(error) → exit
       └─ lane after
global after_all
```

`before`/`after`/`script` ระดับ lane ยังรองรับต่อเพื่อ backward compat แต่เอกสารแนะนำให้ใช้ `steps` อย่างเดียว

## การประมวลผลของ shell

`shell.rs` ต้องรองรับสิ่งที่ `run_shell_command` ปัจจุบันยังไม่มี (`src/main.rs:152-165`):

| ความสามารถ | เหตุผล |
|---|---|
| จับ stdout/stderr พร้อม stream ออกหน้าจอ | ต้องใช้ผลลัพธ์ต่อ แต่ผู้ใช้ก็อยากเห็น log สด |
| timeout ต่อ step | กัน job ค้างบน CI |
| `workdir` ต่อ step | โปรเจกต์ monorepo |
| เลือก shell (`sh`/`bash`/`pwsh`) | รองรับ Windows |
| ไม่ `exit(1)` จากในฟังก์ชัน | คืน `Result` ขึ้นไปให้ executor ตัดสินใจ (มี error hook ให้รัน) |
| mask secret ก่อนพิมพ์ | ดู [10](10-secrets-and-env.md) |

## Dependencies ที่จะเพิ่ม

| crate | ใช้ทำอะไร |
|---|---|
| `anyhow`, `thiserror` | error |
| `serde_yaml` (มีแล้ว) หรือย้ายไป `serde_yaml_ng` | serde_yaml ถูก deprecate แล้ว — ตัดสินใจใน M0 |
| `tracing` + `tracing-subscriber` | logging เป็นระดับ, ใส่ context ได้ |
| `console` / `owo-colors` | สีและ symbol ใน terminal |
| `which` | หา binary (xcodebuild, gradle) |
| `reqwest` (rustls) | action ที่ยิง HTTP |
| `tempfile`, `assert_cmd`, `insta` | dev-dependencies สำหรับเทส |

หลีกเลี่ยงการดึง async runtime เข้ามาถ้าไม่จำเป็น — งานส่วนใหญ่คือรอ subprocess แบบ blocking
