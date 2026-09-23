# Tauri v2 Windows Implementation Research Report

**Date:** 2026-09-23 | **Focus:** Windows 11 tray utility + headless CLI hook pattern

---

## 1. Scaffolding: create-tauri-app + React + Tailwind v4

```bash
# Standard scaffold (stable v2.10.1+)
npm create tauri-app@latest

# Select: React → TypeScript → Vite
# Results in: src-tauri/tauri.conf.json, src/, src-tauri/src/main.rs

# Install Tailwind v4 via @tailwindcss/vite
npm install -D @tailwindcss/vite

# vite.config.ts
import tailwindcss from '@tailwindcss/vite'
export default {
  plugins: [tailwindcss()],
}
```

**Version pinning:** Tauri core ~2.10.1 (stable as of March 2026).

---

## 2. Tray Icon: Feature + Menu + Runtime Updates

**Built-in to core.** Menu via `TrayIconBuilder::new().menu()` + `on_menu_event()`. Items: "Mở", "Lưu tất cả", "Tự lưu", "Thoát".

**Badge:** macOS `set_badge_label()`; Windows use `TrayIcon::set_icon()` at runtime (no native badge).

**Hide on close:** Use `CloseRequested` + `prevent_close()` → `window.hide()`.

---

## 3. Single-Instance Plugin v2 (tauri-plugin-single-instance ~2.4.3)

**Must initialize FIRST** in builder chain before other plugins.

```rust
use tauri_plugin_single_instance::SingleInstanceExt;

.plugin(tauri_plugin_single_instance::init(
  |app, argv, cwd| {
    // argv: Vec<String> of command-line args from second instance
    // cwd: Current working directory of second instance
    // Emit event for JS to handle
    app.emit("single-instance", serde_json::json!({
      "args": argv,
      "cwd": cwd,
    })).ok();
  }
))
```

**Critical:** `argv` + `cwd` passed to callback. **Stdin from second instance cannot be directly passed to callback**—second instance must read stdin, then pass as arg (or file temp write). No cross-instance stdin forwarding in plugin.

---

## 4. Windows GUI Subsystem + Stdin + Headless CLI

Check args **before** `Builder::run()`. Read stdin in headless subcommand before GUI init.

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

if std::env::args().nth(1).as_deref() == Some("hook-save") {
  let mut stdin = String::new();
  std::io::stdin().read_to_string(&mut stdin).ok();
  // Process JSON from stdin
  std::process::exit(0);
}
tauri::Builder::default().run(tauri::generate_context!())
```

**Key:** `windows_subsystem = "windows"` hides console but piped stdin works fine. No console flash if stdin pre-piped by parent.

---

## 5. Spawning Detached Background Process on Windows

**Flags:** Use `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP` (NOT `DETACHED_PROCESS` with it).

```rust
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

let mut child = Command::new("some_process.exe");
child.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
child.spawn().expect("spawn failed");

// Child survives after parent exits
// No console window appears
```

**Why:** `CREATE_NO_WINDOW` alone is insufficient; `CREATE_NEW_PROCESS_GROUP` ensures child detaches. `DETACHED_PROCESS` deprecated for modern use.

---

## 6. Windows Long-Path Manifest in Tauri v2

**Location:** `build.rs` via `WindowsAttributes::app_manifest()`.

```rust
// build.rs
fn main() {
  let manifest = include_str!("./windows-manifest.xml");
  
  let mut windows = tauri_build::WindowsAttributes::new();
  windows = windows.app_manifest(manifest);
  
  let attrs = tauri_build::Attributes::new()
    .windows_attributes(windows);
  tauri_build::try_build(attrs).expect("build failed");
}
```

**Manifest snippet:**
```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <asmv3:application>
    <asmv3:windowsSettings xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">
      <longPathAware>true</longPathAware>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>
```

**Tauri v2 tauri-build:** ~1.3.0+ supports `WindowsAttributes` API directly.

---

## 7. Plugin Names + Versions (as of Sept 2026)

| Plugin | Version | Notes |
|--------|---------|-------|
| dialog | 2.0.0+ | Folder picker, file save |
| notification | 2.0.0+ | Toast messages |
| opener | 2.0.0+ | Open URL in browser |
| autostart | 2.0.0+ | Boot auto-start registration |
| single-instance | 2.4.3+ | Multi-instance coordination |

**Capabilities required:** `src-tauri/capabilities/default.json`
```json
{
  "permissions": [
    "dialog:allow-open_dialog",
    "notification:allow-send",
    "shell:allow-open"
  ]
}
```

---

## 8. Font Bundling: @fontsource-variable in Vite

**Install:**
```bash
npm install @fontsource-variable/inter @fontsource-variable/source-serif-4
```

**src/main.css or src/main.ts:**
```ts
import "@fontsource-variable/inter/index.css";
import "@fontsource-variable/source-serif-4/wght.css";
```

**Tailwind config:**
```js
theme: {
  fontFamily: {
    sans: ['Inter Variable', 'sans-serif'],
    serif: ['Source Serif 4 Variable', 'serif'],
  },
}
```

**Key:** Vite auto-rebases font URLs; bundled into dist, served as 'self', works offline. **Tauri CSP default:** `font-src 'self' data:` blocks CDN.

---

## Sources

- [Tauri v2 Create Project](https://v2.tauri.app/start/create-project/)
- [Tauri v2 Menu & Tray API](https://v2.tauri.app/reference/javascript/api/namespacemenu/)
- [Single-Instance Plugin Docs](https://v2.tauri.app/plugin/single-instance/)
- [Single-Instance Rust Crate](https://docs.rs/crate/tauri-plugin-single-instance/latest)
- [Tauri CLI Plugin](https://v2.tauri.app/plugin/cli/)
- [Process Creation Flags (Windows)](https://learn.microsoft.com/sv-se/windows/win32/ProcThread/process-creation-flags)
- [tauri-build WindowsAttributes](https://docs.rs/tauri-build/latest/tauri_build/struct.WindowsAttributes.html)
- [@fontsource-variable/inter](https://www.npmjs.com/package/@fontsource-variable/inter)
- [@fontsource-variable/source-serif-4](https://www.npmjs.com/package/@fontsource-variable/source-serif-4)
- [Fontsource Vite Guide](https://fontsource.org/docs/guides/vite)

---

## Unresolved Questions

1. **Single-instance stdin forwarding:** Confirmed plugin passes argv+cwd only. For JSON on stdin, second instance must read stdin first, then encode in argv as base64 arg—confirm pattern works end-to-end with hook runner.
2. **Badge on Windows tray:** Confirmed no native API; verify if overlay icon (custom set_icon with badge text) is best UX.
3. **GUI manifest embedding:** `WindowsAttributes::app_manifest()` works, but not yet validated in working build; may require `build.rs` path resolution care.
4. **Stdin + windows_subsystem interaction:** Tested that piped stdin works, but edge case of console flash in certain parent-process contexts not fully explored.

