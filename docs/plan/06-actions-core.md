# 06 — ระบบ Action และ action กลาง

fastlane มี action ~400 ตัว ซึ่งเป็นเหตุผลหลักที่คนยังใช้มัน ส่วนนี้คือหัวใจของงาน

## Trait

```rust
pub trait Action: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> ArgSchema;                 // ใช้ validate + `shlane action show`
    fn is_supported(&self, platform: Platform) -> bool;
    fn run(&self, ctx: &mut LaneContext, args: &Args) -> Result<ActionOutput>;
    fn dry_run(&self, ctx: &LaneContext, args: &Args) -> Result<String> { ... }
}

pub struct ActionOutput(pub HashMap<String, Value>);   // ไหลเข้า ctx.outputs[step_id]
```

## Registry

- ทะเบียน static ตอน compile สำหรับ built-in
- ทะเบียน dynamic ตอน runtime สำหรับ plugin (ดู [09](09-plugins.md))
- `shlane action list` / `shlane action show <name>` อ่านจากทะเบียนนี้
- ชื่อ action ใช้ `snake_case` ตามแบบ fastlane เพื่อให้คนย้ายมาคุ้นมือ

## กฎที่ทุก action ต้องทำตาม

1. **ต้องรองรับ `--dry-run`** — พิมพ์คำสั่งจริงที่จะรัน โดยไม่รัน
2. **ต้องประกาศ binary ที่ต้องใช้** — ถ้าไม่มี `gradle`/`xcodebuild` ต้อง error ด้วย exit code 5 พร้อมบอกวิธีติดตั้ง ไม่ใช่ปล่อย "command not found"
3. **ต้อง mask secret ใน log** — ค่าที่ schema ทำเครื่องหมาย `sensitive: true` จะถูกลงทะเบียนใน `SecretRegistry` อัตโนมัติ
4. **ต้อง idempotent เท่าที่ทำได้** — รันซ้ำแล้วไม่พัง
5. **ต้องคืน output ที่มีประโยชน์** — เช่น `build_ios` คืน `ipa`, `dsym`, `build_number`
6. **ต้องมี test อย่างน้อย 1 ตัว** (ดู [13](13-testing-and-quality.md))

## Action กลาง (ไม่ผูก platform) — M3

### Shell / process
| action | แทนของ fastlane | หมายเหตุ |
|---|---|---|
| `sh` | `sh` | เหมือน `run:` แต่เรียกจาก script ได้ |
| `ensure_env_vars` | `ensure_env_vars` | fail เร็วถ้า secret ไม่ครบ |
| `which_tool` | — | เช็คว่ามี binary + เวอร์ชันขั้นต่ำ |

### Git
| action | แทนของ |
|---|---|
| `git_status_clean` | `ensure_git_status_clean` |
| `git_branch` | `git_branch` |
| `git_commit` | `git_commit` |
| `git_tag` | `add_git_tag` |
| `git_push` | `push_to_git_remote`, `push_git_tags` |
| `git_pull` | `git_pull` |
| `changelog_from_commits` | `changelog_from_git_commits` |
| `last_git_tag` | `last_git_tag` |

### Version
| action | แทนของ |
|---|---|
| `bump_version` | `increment_version_number` (iOS) / `increment_version_code` (Android) รวมเป็นตัวเดียวที่รู้จัก platform |
| `read_version` | `get_version_number`, `get_build_number` |

### แจ้งเตือน
| action | แทนของ |
|---|---|
| `notify_slack` | `slack` |
| `notify_discord` | plugin |
| `notify_teams` | plugin |
| `http_request` | — (escape hatch สำหรับ webhook อื่น) |

### ไฟล์และ artifact
| action | แทนของ |
|---|---|
| `zip` / `unzip` | `zip` |
| `copy_artifacts` | `copy_artifacts` |
| `clean_build_artifacts` | `clean_build_artifacts` |
| `download` | `download` |
| `template_render` | `erb` (ใช้ template engine ง่ายๆ แทน ERB) |

## ลำดับความสำคัญ

จัดลำดับตาม "ถ้าไม่มีตัวนี้ ก็ลบ Gemfile ไม่ได้":

1. **P0** — `sh`, `git_*`, `bump_version`, `notify_slack`, `ensure_env_vars`
2. **P1** — `changelog_from_commits`, `zip`, `copy_artifacts`, `http_request`
3. **P2** — ที่เหลือ

ทุกอย่างที่ไม่อยู่ใน P0–P2 ให้ใช้ `run:` ไปก่อน แล้วค่อยดูว่ามีคนขอ action จริงไหม
