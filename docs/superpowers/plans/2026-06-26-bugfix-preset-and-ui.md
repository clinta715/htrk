# Bug Fix Plan: Preset Discovery, MCP Server, and UI Navigation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all bugs found in the code review covering preset discovery data correctness, MCP server reliability, thread safety, performance, and UI navigation.

**Architecture:** Fixes are organized in 5 phases of increasing risk. Phase 1 is independent one-liners. Phase 2 fixes the preset scanner's data integrity. Phase 3 unifies the dual-copy architecture. Phase 4 adds async scanning + persistence. Phase 5 is polish.

**Tech Stack:** Rust, egui, clack-extensions, serde_json, RwLock/Arc concurrency

---

## File Structure

| File | Responsibility | Changes |
|------|---------------|---------|
| `src/mcp/http.rs` | HTTP SSE transport | Fix session cleanup inversion |
| `src/audio/plugins/preset_discovery.rs` | CLAP preset scanning | Fix finalize/location bug, symlink guard, empty-filetype guard |
| `src/audio/plugins/preset_library.rs` | Preset cache | Fix page_size=0, add unique_plugin_keys |
| `src/app.rs` | Main app | Unify PresetLibrary to Arc, wire rescan, wire persistence, eliminate double-probe |
| `src/mcp/preset_tools.rs` | MCP preset tools | Add pagination, fix comment, background scan, dropped lock optimization |
| `src/mcp/protocol.rs` | ToolContext | Change preset_library to Arc<RwLock<PresetLibrary>> on HtrkApp |
| `src/mcp/server.rs` | MCP server | Share Arc<RwLock<PresetLibrary>> with app |
| `src/actions/keyboard.rs` | Keyboard input | Fix Ctrl+Arrow dead code, page_size clamp |
| `src/ui/pattern_grid.rs` | Pattern grid | Drop unused _col_vis param from sub_column_rect |

---

## Phase 1: Quick Independent Fixes

### Task 1: Fix HTTP session cleanup inversion

**Files:**
- Modify: `src/mcp/http.rs:112`

**Bug:** `sessions.retain(|_, tx| tx.send(String::new()).is_ok() == false)` keeps dead sessions (send fails) and kills live ones (send succeeds → returns true → `== false` is false → removed). It also spams empty SSE events to live clients as a side effect of the probe.

- [ ] **Step 1: Fix the cleanup logic**

Replace `src/mcp/http.rs:112`:

```rust
                            sessions.retain(|_, tx| tx.send(String::new()).is_ok() == false);
```

With:

```rust
                            sessions.retain(|_, tx| !tx.is_closed());
```

`mpsc::Sender::is_closed()` returns `true` when the receiver has been dropped — no side effects, no spurious messages.

- [ ] **Step 2: Run existing tests**

Run: `cargo test --test mcp_integration`
Expected: 6 passed, 0 failed

- [ ] **Step 3: Commit**

```bash
git add src/mcp/http.rs
git commit -m "fix: HTTP SSE session cleanup was inverted — kept dead sessions, killed live ones

retain() kept entries where send() fails (dead) and removed entries
where send() succeeds (live). Also spammed empty SSE events as a
side-effect of the probe. Use is_closed() instead."
```

---

### Task 2: Fix step_sub_column_forward/backward dead-stop

**Files:**
- Modify: `src/app.rs:995-1007`

**Bug:** `step_sub_column_forward` does nothing when the cursor is on `EffectParamLow` (the last sub-column). `handle_text_input` wraps to the first sub-column and advances a row, but the arrow key handler silently no-ops. The old `move_cursor_right()` moved to the next channel. Users holding ArrowRight see the cursor freeze.

- [ ] **Step 1: Write the failing test**

Add to `src/app.rs` test module (or `src/actions/keyboard.rs` tests):

```rust
    #[test]
    fn test_step_sub_column_wraps_at_end() {
        let mut app = test_app();
        app.core.new_song();
        app.core.cursor.sub_column = SubColumn::EffectParamLow;
        app.core.cursor.row = 0;
        app.core.cursor.channel = 0;

        app.step_sub_column_forward();

        // Should wrap to Note and advance to next row
        assert_eq!(app.core.cursor.sub_column, SubColumn::Note);
        assert_eq!(app.core.cursor.row, 1, "should advance to next row on wrap");
    }

    #[test]
    fn test_step_sub_column_backward_wraps_at_start() {
        let mut app = test_app();
        app.core.new_song();
        app.core.cursor.sub_column = SubColumn::Note;
        app.core.cursor.row = 5;
        app.core.cursor.channel = 0;

        app.step_sub_column_backward();

        // Should wrap to EffectParamLow and go back a row
        assert_eq!(app.core.cursor.sub_column, SubColumn::EffectParamLow);
        assert_eq!(app.core.cursor.row, 4, "should go back a row on wrap");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test step_sub_column_wraps -- --nocapture`
Expected: FAIL — cursor stays at EffectParamLow, row doesn't advance

- [ ] **Step 3: Fix step_sub_column_forward and step_sub_column_backward**

Replace `src/app.rs:995-1007`:

```rust
    pub(crate) fn step_sub_column_forward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(next) = self.core.cursor.sub_column.next_visible(col_vis) {
            self.core.cursor.sub_column = next;
        } else {
            // Wrap to first visible sub-column and advance a row
            let first_sub = Self::first_visible_sub_column(col_vis);
            self.core.cursor.sub_column = first_sub;
            self.advance_cursor_down(1);
        }
    }

    pub(crate) fn step_sub_column_backward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(prev) = self.core.cursor.sub_column.prev_visible(col_vis) {
            self.core.cursor.sub_column = prev;
        } else {
            // Wrap to last visible sub-column and go back a row
            let last_sub = Self::last_visible_sub_column(col_vis);
            self.core.cursor.sub_column = last_sub;
            self.advance_cursor_up(1);
        }
    }
```

- [ ] **Step 4: Add last_visible_sub_column helper**

Add after `first_visible_sub_column` in `src/app.rs`:

```rust
    pub(crate) fn last_visible_sub_column(col_vis: crate::ui::pattern_grid::ColumnVisibility) -> crate::ui::pattern_grid::SubColumn {
        use crate::ui::pattern_grid::SubColumn;
        if col_vis.effect { return SubColumn::EffectParamLow; }
        if col_vis.volume { return SubColumn::VolumeOnes; }
        if col_vis.instrument { return SubColumn::InstrumentOnes; }
        if col_vis.note { return SubColumn::Note; }
        SubColumn::Note
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test step_sub_column_wraps -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "fix: step_sub_column_forward/backward now wraps at boundaries

ArrowRight on EffectParamLow wraps to Note and advances a row (matching
text entry behavior). ArrowLeft on Note wraps to the last visible
sub-column and goes back a row. Previously these silently dead-stopped."
```

---

### Task 3: Remove dead Ctrl+ArrowUp/Down octave code + fix misleading comment

**Files:**
- Modify: `src/actions/keyboard.rs:374-377,390-393`
- Modify: `src/mcp/preset_tools.rs:2`

**Bug 1:** `Ctrl+ArrowUp/Down` in the main key handler (`keyboard.rs:374-377`) checks `modifiers.ctrl` but this code is unreachable because `if modifiers.ctrl { return; }` at line 364 returns before the main handler block.

**Bug 2:** `preset_tools.rs:2` comment says "preset.scan is the only mutation tool that requires main-thread dispatch" but it's factually wrong — preset.scan runs on the MCP thread, not the main thread.

- [ ] **Step 1: Remove the dead Ctrl+ArrowUp/Down octave arms**

In `src/actions/keyboard.rs`, inside the `ArrowDown` arm (around line 380), remove the `if modifiers.ctrl { ... }` block:

```rust
                        egui::Key::ArrowDown => {
                            if !any_dialog_open && is_pattern {
                                if modifiers.ctrl {
                                    if app.current_octave > 0 {
                                        app.current_octave -= 1;
                                    }
                                } else if modifiers.shift {
```

Change to:

```rust
                        egui::Key::ArrowDown => {
                            if !any_dialog_open && is_pattern {
                                if modifiers.shift {
```

Do the same for `ArrowUp` (around line 396): remove the `if modifiers.ctrl { ... }` block.

- [ ] **Step 2: Fix the misleading comment in preset_tools.rs**

Replace `src/mcp/preset_tools.rs:1-2`:

```rust
// MCP tools for browsing discovered CLAP presets.
// Read-only: preset.scan is the only mutation tool that requires main-thread dispatch.
```

With:

```rust
// MCP tools for browsing discovered CLAP presets.
// All tools run on the MCP thread. preset.scan writes to preset_library
// via RwLock (same pattern as plugin.scan). See AGENTS.md §21.
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/actions/keyboard.rs src/mcp/preset_tools.rs
git commit -m "cleanup: remove unreachable Ctrl+ArrowUp/Down octave code, fix misleading comment"
```

---

## Phase 2: Preset Scanner Correctness

### Task 4: Fix finalize() losing location_path for multi-preset files

**Files:**
- Modify: `src/audio/plugins/preset_discovery.rs`

**Bug:** `begin_preset` calls `self.finalize(None, "")` to flush the previous preset, but this hardcodes `location_path=None` and `location_kind=""`. So every preset except the last one per `get_metadata` call gets the wrong location. Since `cache_key()` folds `location_path` into the dedup key, presets from different files collide and silently drop.

**Fix:** Store the current location on the receiver before calling `get_metadata`, and have `finalize()` read it instead of receiving it as a parameter.

- [ ] **Step 1: Write the failing test**

Add to `src/audio/plugins/preset_discovery.rs` tests:

```rust
    #[test]
    fn test_finalize_preserves_location() {
        // Simulate the receiver pattern: begin_preset called multiple
        // times within a single get_metadata call (multi-preset file).
        let mut receiver = ScanReceiver::new(
            "/test/plugin.clap".into(),
            "com.test".into(),
            "Test".into(),
            "provider".into(),
            "Provider".into(),
        );
        receiver.set_location(Some("/presets/file.fxp".into()), "file");

        // First preset
        receiver.begin_preset(Some(c"Preset A"), Some(c"key_a")).unwrap();
        receiver.add_feature(c"bass");

        // Second preset — triggers finalize() for Preset A
        receiver.begin_preset(Some(c"Preset B"), Some(c"key_b")).unwrap();
        receiver.add_feature(c"lead");

        // Finalize Preset B
        receiver.finalize();

        assert_eq!(receiver.entries.len(), 2);
        assert_eq!(receiver.entries[0].name, "Preset A");
        assert_eq!(
            receiver.entries[0].location_path.as_deref(),
            Some("/presets/file.fxp"),
            "Preset A should have the file location"
        );
        assert_eq!(receiver.entries[0].location_kind, "file");
        assert_eq!(receiver.entries[1].name, "Preset B");
        assert_eq!(
            receiver.entries[1].location_path.as_deref(),
            Some("/presets/file.fxp"),
            "Preset B should have the file location"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_finalize_preserves_location -- --nocapture`
Expected: FAIL — `set_location` and parameterless `finalize()` don't exist yet

- [ ] **Step 3: Refactor ScanReceiver to track location internally**

In `src/audio/plugins/preset_discovery.rs`, add location fields to `ScanReceiver` struct (after `cur_modification_time`):

```rust
    cur_creation_time: Option<i64>,
    cur_modification_time: Option<i64>,
    cur_location_path: Option<String>,
    cur_location_kind: String,
```

In `ScanReceiver::new`, initialize them:

```rust
            cur_creation_time: None,
            cur_modification_time: None,
            cur_location_path: None,
            cur_location_kind: String::new(),
```

Add `set_location` method:

```rust
    fn set_location(&mut self, path: Option<String>, kind: &str) {
        self.cur_location_path = path;
        self.cur_location_kind = kind.to_string();
    }
```

Change `finalize` to be parameterless — it reads from `cur_location_*`:

```rust
    fn finalize(&mut self) {
        let name = match self.cur_name.take() {
            Some(n) => n,
            None => return,
        };
        let load_key = self.cur_load_key.take().unwrap_or_default();
        self.entries.push(PresetEntry {
            plugin_path: self.plugin_path.clone(),
            plugin_id: self.plugin_id.clone(),
            plugin_name: self.plugin_name.clone(),
            provider_id: self.provider_id.clone(),
            provider_name: self.provider_name.clone(),
            name,
            load_key,
            location_path: self.cur_location_path.clone(),
            location_kind: self.cur_location_kind.clone(),
            flags: self.cur_flags,
            features: std::mem::take(&mut self.cur_features),
            description: self.cur_description.take(),
            creators: std::mem::take(&mut self.cur_creators),
            compatible_plugin_ids: std::mem::take(&mut self.cur_plugin_ids),
            soundpack_id: self.cur_soundpack_id.take(),
            extra_info: std::mem::take(&mut self.cur_extra_info),
            creation_time: self.cur_creation_time.take(),
            modification_time: self.cur_modification_time.take(),
        });
    }
```

In `begin_preset`, change `self.finalize(None, "")` to `self.finalize()`:

```rust
    fn begin_preset(
        &mut self,
        name: Option<&CStr>,
        load_key: Option<&CStr>,
    ) -> Result<(), clack_host::prelude::HostError> {
        self.finalize();
        self.cur_name = name.map(|s| s.to_string_lossy().into_owned());
        self.cur_load_key = load_key.map(|s| s.to_string_lossy().into_owned());
        Ok(())
    }
```

- [ ] **Step 4: Update the scan loop to use set_location before get_metadata**

In `scan_plugin_presets`, update the location loop (around line 318-369). Replace the entire `for loc in &locations` block:

```rust
        for loc in &locations {
            let mut receiver = ScanReceiver::new(
                path_str.clone(),
                plugin_id.clone(),
                plugin_name.clone(),
                pid.clone(),
                pname.clone(),
            );

            match &loc.kind {
                LocationKind::Plugin => {
                    receiver.set_location(None, "plugin");
                    provider.get_metadata(Location::Plugin, &mut receiver);
                    receiver.finalize();
                }
                LocationKind::File(loc_path) => {
                    let dir = PathBuf::from(loc_path);
                    if dir.is_dir() {
                        let matching = find_matching_files(&dir, &filetypes);
                        for file_path in &matching {
                            let fp = file_path.to_string_lossy();
                            let Ok(cpath) = std::ffi::CString::new(fp.as_ref()) else {
                                continue;
                            };
                            receiver.set_location(
                                Some(file_path.to_string_lossy().to_string()),
                                "file",
                            );
                            provider.get_metadata(
                                Location::File { path: cpath.as_c_str() },
                                &mut receiver,
                            );
                            receiver.finalize();
                        }
                    } else if dir.is_file() || dir.exists() {
                        let fp = dir.to_string_lossy();
                        if let Ok(cpath) = std::ffi::CString::new(fp.as_ref()) {
                            receiver.set_location(
                                Some(dir.to_string_lossy().to_string()),
                                "file",
                            );
                            provider.get_metadata(
                                Location::File { path: cpath.as_c_str() },
                                &mut receiver,
                            );
                            receiver.finalize();
                        }
                    }
                }
            }

            all_presets.extend(receiver.entries);
        }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test test_finalize_preserves_location -- --nocapture`
Expected: PASS

- [ ] **Step 6: Run the Surge XT integration test to verify real-world correctness**

Run: `cargo test test_scan_surge_xt_presets -- --nocapture`
Expected: PASS — presets found, location_path populated for all presets

- [ ] **Step 7: Commit**

```bash
git add src/audio/plugins/preset_discovery.rs
git commit -m "fix: finalize() was dropping location_path for all but last preset per file

Multi-preset files lost location info because begin_preset called
finalize(None, '') for the previous preset. Since cache_key() folds
in location_path, presets from different files collided and silently
dropped. Now the receiver tracks location internally via set_location()."
```

---

### Task 5: Fix walk_dir symlink infinite recursion + add depth limit

**Files:**
- Modify: `src/audio/plugins/preset_discovery.rs:437-451`

**Bug:** `walk_dir` uses `path.is_dir()` which follows symlinks. A symlink pointing at an ancestor directory causes infinite recursion → stack overflow. No depth limit means deep trees can also overflow.

- [ ] **Step 1: Replace walk_dir with symlink-safe version**

Replace `walk_dir` in `src/audio/plugins/preset_discovery.rs`:

```rust
fn walk_dir(dir: &Path, results: &mut Vec<PathBuf>, filetypes: &[FileTypeOwned]) {
    walk_dir_recursive(dir, results, filetypes, 0);
}

fn walk_dir_recursive(
    dir: &Path,
    results: &mut Vec<PathBuf>,
    filetypes: &[FileTypeOwned],
    depth: u32,
) {
    if depth > 16 {
        eprintln!("[preset_discovery] Max directory depth reached at {}", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("[preset_discovery] Cannot read directory: {}", dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Use symlink_metadata to avoid following symlinks
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue; // Skip symlinks entirely
        }
        if meta.is_dir() {
            walk_dir_recursive(&path, results, filetypes, depth + 1);
        } else if meta.is_file() {
            if filetypes.is_empty() || matches_filetype(&path, filetypes) {
                results.push(path);
            }
        }
    }
}
```

- [ ] **Step 2: Add empty-filetype guard to find_matching_files**

Replace `find_matching_files`:

```rust
fn find_matching_files(
    dir: &Path,
    filetypes: &[FileTypeOwned],
) -> Vec<PathBuf> {
    // If the provider declared no file types, we can't filter — skip
    // scanning entirely to avoid probing every file in the tree.
    if filetypes.is_empty() {
        eprintln!(
            "[preset_discovery] Provider declared no file types; skipping directory scan of {}",
            dir.display()
        );
        return Vec::new();
    }
    let mut results = Vec::new();
    walk_dir(dir, &mut results, filetypes);
    results
}
```

- [ ] **Step 3: Run the Surge XT integration test**

Run: `cargo test test_scan_surge_xt_presets -- --nocapture`
Expected: PASS — still finds presets (Surge XT declares .fxp filetype)

- [ ] **Step 4: Commit**

```bash
git add src/audio/plugins/preset_discovery.rs
git commit -m "fix: walk_dir now skips symlinks, has depth limit, guards empty filetypes

- symlink_metadata prevents infinite recursion on cyclic symlinks
- depth cap of 16 prevents stack overflow on deep trees
- empty filetype list skips directory scan instead of probing every file"
```

---

## Phase 3: Architecture Unification

### Task 6: Unify PresetLibrary to shared Arc<RwLock<>>

**Files:**
- Modify: `src/app.rs` — change `preset_library: PresetLibrary` to `preset_library: Arc<RwLock<PresetLibrary>>`
- Modify: `src/app.rs` — update `rescan_presets()` to write to the shared Arc (no mirror needed)
- Modify: `src/mcp/preset_tools.rs` — `cmd_preset_scan` now updates the shared Arc (which is the same one the app uses)

**Bug:** The app has its own `PresetLibrary` (a value), and the MCP server has a separate `Arc<RwLock<PresetLibrary>>`. MCP `preset.scan` only updates the MCP copy. The app's UI copy goes stale.

**Fix:** Make `HtrkApp.preset_library` an `Arc<RwLock<PresetLibrary>>` and share the SAME Arc with the MCP server at construction time. Then both the UI and MCP tools read/write the same object.

- [ ] **Step 1: Change the field type in HtrkApp**

In `src/app.rs`, change:

```rust
    pub(crate) preset_library: PresetLibrary,
```

To:

```rust
    pub(crate) preset_library: Arc<RwLock<PresetLibrary>>,
```

- [ ] **Step 2: Update both constructors**

In `from_config_for_tests`:

```rust
            preset_library: Arc::new(RwLock::new(crate::audio::plugins::PresetLibrary::new())),
```

In `from_config`, the field initialization (change `PresetLibrary::new()` to):

```rust
            preset_library: Arc::new(RwLock::new(PresetLibrary::new())),
```

Then create the Arc BEFORE the MCP server block so it can be shared:

```rust
        let preset_library = Arc::new(RwLock::new(PresetLibrary::new()));
```

And use `preset_library.clone()` both for the app field and for the MCP server.

- [ ] **Step 3: Update the MCP server construction to share the Arc**

In `from_config`, inside the `mcp_server` block, the server currently creates its own `PresetLibrary::new()`. After unification, pass the shared Arc:

In `src/mcp/server.rs`, `McpServer::start()` currently creates its own `preset_library`. Instead, accept it as a parameter or assign after creation.

The simplest approach: after `McpServer::start()` returns, swap in the shared Arc:

```rust
                let server = crate::mcp::McpServer::start(mcp_port, http_port);
                // ... existing library/plugin setup ...
                // Share the preset library Arc with the server
                server.preset_library = preset_library.clone();
```

Wait — `server.preset_library` is a field on `McpServer`. We need to make it `pub` (it already is). But the server thread was already spawned with the old (empty) Arc. We need to pass it to `start()` instead.

Better approach: add `preset_library` as a parameter to `McpServer::start()`:

In `src/mcp/server.rs`, change `start()` signature to accept `preset_library: Arc<RwLock<PresetLibrary>>` and use it instead of creating a new one. Update the HTTP server call similarly.

- [ ] **Step 4: Update rescan_presets() to use the shared Arc**

Replace the body of `rescan_presets()`:

```rust
    pub fn rescan_presets(&mut self) -> String {
        use crate::audio::plugins::preset_discovery;

        let paths: Vec<std::path::PathBuf> = self
            .plugin_library
            .list_descriptors()
            .iter()
            .map(|d| d.path.clone())
            .collect();

        eprintln!("[presets] Scanning {} plugin(s) for presets...", paths.len());
        let start = std::time::Instant::now();

        let (entries, errors) = preset_discovery::scan_plugins_for_presets(&paths);

        let elapsed = start.elapsed();
        if let Ok(mut lib) = self.preset_library.write() {
            lib.clear();
            lib.add_presets(entries);
            lib.set_last_scan_time(std::time::SystemTime::now());
        }

        eprintln!(
            "[presets] Done: {} preset(s), {} error(s) in {:.1}s",
            self.preset_library.read().map(|l| l.preset_count()).unwrap_or(0),
            errors.len(),
            elapsed.as_secs_f32()
        );

        format!(
            "Preset scan complete: {} error(s) in {:.1}s",
            errors.len(),
            elapsed.as_secs_f32()
        )
    }
```

No more mirror step — the Arc is shared.

- [ ] **Step 5: Update cmd_preset_scan to use ctx.preset_library (now the same Arc)**

No code change needed in `preset_tools.rs` — `ctx.preset_library` is already the same Arc now.

- [ ] **Step 6: Update all call sites that read preset_library**

Search for `self.preset_library` and `app.preset_library` and update them to use `.read()` / `.write()`:

- In `from_config_for_tests`: `preset_library: Arc::new(RwLock::new(...))`
- Any UI code that reads `self.preset_library` needs `.read()`.

- [ ] **Step 7: Run full test suite**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "fix: unify PresetLibrary to shared Arc<RwLock<>> — eliminates app/MCP divergence

The app and MCP server now share the same Arc<RwLock<PresetLibrary>>.
preset.scan (MCP thread) and rescan_presets (main thread) both update
the same object. No more stale UI copy. No more mirror clone step."
```

---

### Task 7: Eliminate double-probe in rescan_plugins()

**Files:**
- Modify: `src/app.rs:460-486`

**Bug:** `rescan_plugins()` probes each `.clap` file twice — once for `self.plugin_library` and once again for the MCP mirror. Each probe is a dlopen (~10ms), so this doubles scan time.

- [ ] **Step 1: Fix the mirror to clone descriptors instead of re-probing**

In `src/app.rs`, replace the MCP mirror section (around line 469-478):

```rust
        if let Some(ref mcp) = self.mcp_server {
            if let Ok(mut mcp_lib) = mcp.plugin_library.write() {
                mcp_lib.set_scan_roots(scan_roots.clone());
                mcp_lib.clear_cache();
                for path in &scan.clap_files {
                    if let Ok(d) = crate::audio::plugins::clap_plugin::extract_descriptor_for_browser(path) {
                        mcp_lib.add_descriptor(d);
                    }
                }
            }
        }
```

With:

```rust
        if let Some(ref mcp) = self.mcp_server {
            if let Ok(mut mcp_lib) = mcp.plugin_library.write() {
                mcp_lib.set_scan_roots(scan_roots.clone());
                mcp_lib.clear_cache();
                for d in self.plugin_library.list_descriptors() {
                    mcp_lib.add_descriptor(d.clone());
                }
            }
        }
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "perf: rescan_plugins clones descriptors instead of re-dlopening for MCP mirror

Eliminates a second dlopen + factory call per plugin. Halves scan time."
```

---

### Task 8: Wire rescan_presets() into update()

**Files:**
- Modify: `src/app.rs:3045-3048`

**Bug:** `rescan_presets()` is never called. The `preset_scan_done` flag is set to `false` but never checked. The preset library is always empty from the UI's perspective.

- [ ] **Step 1: Add the preset scan gate after the plugin scan gate**

In `src/app.rs`, after the plugin scan block (around line 3048), add:

```rust
        if !self.plugin_scan_done {
            let _ = self.rescan_plugins();
            self.plugin_scan_done = true;
        }

        if !self.preset_scan_done && self.plugin_scan_done {
            // Try to load from cache first; fall back to full scan.
            let cache_path = AppConfig::config_dir().join("preset_cache.json");
            if let Ok(cached) = crate::audio::plugins::PresetLibrary::load_from_file(&cache_path) {
                eprintln!("[presets] Loaded {} preset(s) from cache", cached.preset_count());
                if let Ok(mut lib) = self.preset_library.write() {
                    *lib = cached;
                }
            } else {
                let _ = self.rescan_presets();
                // Save to cache for next startup
                if let Ok(lib) = self.preset_library.read() {
                    let _ = lib.save_to_file(&cache_path);
                }
            }
            self.preset_scan_done = true;
        }
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "fix: wire rescan_presets into update() with cache load/save

Preset library is now populated at startup. Loads from disk cache
(preset_cache.json) if available; falls back to full scan and saves
the result for next time."
```

---

## Phase 4: Performance

### Task 9: Make preset.scan non-blocking via background thread

**Files:**
- Modify: `src/mcp/preset_tools.rs:70-116`
- Modify: `src/mcp/server.rs` — add scan status tracking

**Bug:** `preset.scan` blocks the sole MCP server thread for ~28s, starving all other MCP tools.

**Fix:** Spawn a background thread for the scan, return an immediate "scan_started" response, and have `preset.status` report scanning progress.

- [ ] **Step 1: Add scan state to McpServer**

In `src/mcp/server.rs`, add to `McpServer`:

```rust
    pub preset_scan_in_progress: Arc<std::sync::atomic::AtomicBool>,
```

Initialize it in `start()`:

```rust
    let preset_scan_in_progress = Arc::new(std::sync::atomic::AtomicBool::new(false));
```

Pass it to `ToolContext`:

```rust
    pub preset_scan_in_progress: Arc<std::sync::atomic::AtomicBool>,
```

In `src/mcp/protocol.rs`, add to `ToolContext`:

```rust
    pub preset_scan_in_progress: Arc<std::sync::atomic::AtomicBool>,
```

- [ ] **Step 2: Rewrite cmd_preset_scan to spawn a thread**

In `src/mcp/preset_tools.rs`, replace `cmd_preset_scan`:

```rust
pub fn cmd_preset_scan(_params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    // Check if a scan is already running
    if ctx.preset_scan_in_progress.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(json!({
            "status": "already_scanning",
            "message": "A preset scan is already in progress"
        }));
    }

    // Collect plugin paths under a read lock (released immediately)
    let plugin_paths: Vec<std::path::PathBuf> = {
        let plib = ctx
            .plugin_library
            .read()
            .map_err(|e| format!("Plugin library lock poisoned: {e}"))?;
        plib.list_descriptors()
            .iter()
            .map(|d| d.path.clone())
            .collect()
    };

    if plugin_paths.is_empty() {
        return Ok(json!({
            "status": "no_plugins",
            "message": "No plugins discovered yet — run plugin.scan first"
        }));
    }

    // Mark scan as in-progress and spawn a background thread
    ctx.preset_scan_in_progress.store(true, std::sync::atomic::Ordering::Relaxed);

    let preset_lib = ctx.preset_library.clone();
    let scan_flag = ctx.preset_scan_in_progress.clone();
    let plugin_count = plugin_paths.len();

    std::thread::Builder::new()
        .name("htrk-preset-scan".into())
        .spawn(move || {
            let (entries, errors) =
                crate::audio::plugins::preset_discovery::scan_plugins_for_presets(&plugin_paths);

            if let Ok(mut lib) = preset_lib.write() {
                lib.clear();
                lib.add_presets(entries);
                lib.set_last_scan_time(std::time::SystemTime::now());
            }

            eprintln!(
                "[preset_discovery] Background scan done: {} preset(s), {} error(s) from {} plugin(s)",
                preset_lib.read().map(|l| l.preset_count()).unwrap_or(0),
                errors.len(),
                plugin_count
            );

            scan_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| format!("Failed to spawn scan thread: {e}"))?;

    Ok(json!({
        "status": "scan_started",
        "plugins_to_scan": plugin_count,
        "message": "Scan running in background. Use preset.status to check progress."
    }))
}
```

- [ ] **Step 3: Update cmd_preset_status to report scan state**

In `cmd_preset_status`, add the scanning flag:

```rust
    let scanning = ctx.preset_scan_in_progress.load(std::sync::atomic::Ordering::Relaxed);
```

And include it in the response:

```rust
    Ok(json!({
        "total_presets": total,
        "unique_plugins": plugin_names.len(),
        "last_scan": scan_time,
        "plugins": plugin_names,
        "scanning": scanning,
    }))
```

- [ ] **Step 4: Update all ToolContext construction sites**

Update every place that creates a `ToolContext` to include `preset_scan_in_progress`:
- `src/mcp/server.rs` — inside the thread closure
- `src/mcp/http.rs` — `handle_post`
- `src/mcp/plugin_tools.rs` — tests
- `src/mcp/preset_tools.rs` — tests

- [ ] **Step 5: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "perf: preset.scan runs in background thread, no longer blocks MCP server

Returns immediately with 'scan_started'. Use preset.status to check
progress (scanning: true/false). Other MCP tools remain responsive
during the ~28s scan."
```

---

### Task 10: Wire atomic preset cache persistence

**Files:**
- Modify: `src/audio/plugins/preset_library.rs` — make save_to_file atomic
- Already wired in Task 8 (load) — add save after rescan_presets and on exit

- [ ] **Step 1: Make save_to_file atomic**

In `src/audio/plugins/preset_library.rs`, replace `save_to_file`:

```rust
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let json = self.to_json()?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json)
            .map_err(|e| format!("Cannot write preset cache: {e}"))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("Cannot rename preset cache: {e}"))?;
        Ok(())
    }
```

- [ ] **Step 2: Add cache save to on_exit**

In `src/app.rs`, find the `on_exit` or equivalent lifecycle method. Search for `fn on_exit`:

Add before the method returns:

```rust
        // Save preset cache for fast startup
        let cache_path = AppConfig::config_dir().join("preset_cache.json");
        if let Ok(lib) = self.preset_library.read() {
            if lib.preset_count() > 0 {
                let _ = lib.save_to_file(&cache_path);
            }
        }
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/audio/plugins/preset_library.rs src/app.rs
git commit -m "perf: atomic preset cache save + load on exit/startup

save_to_file writes to .tmp then renames (atomic). Cache is loaded
on startup (Task 8) and saved on exit. Eliminates 28s cold rescan."
```

---

## Phase 5: Polish

### Task 11: Add pagination to cmd_preset_list_by_plugin

**Files:**
- Modify: `src/mcp/preset_tools.rs:46-66`
- Modify: `src/mcp/tools.rs` — update tool definition

- [ ] **Step 1: Add page/page_size params to the handler**

Replace `cmd_preset_list_by_plugin` in `src/mcp/preset_tools.rs`:

```rust
pub fn cmd_preset_list_by_plugin(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let plugin_path = params
        .get("plugin_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_path'")?;
    let plugin_id = params
        .get("plugin_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_id'")?;
    let page = params.get("page").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let page_size = params
        .get("page_size")
        .and_then(|v| v.as_i64())
        .unwrap_or(50) as usize;

    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    let mut presets: Vec<_> = lib.list_plugin_presets(plugin_path, plugin_id);
    let total = presets.len();
    let total_pages = if page_size == 0 {
        1
    } else {
        (total + page_size - 1) / page_size
    };
    let start = page * page_size;
    let page_presets: Vec<_> = presets
        .drain(start..)
        .take(page_size)
        .collect();
    let result: Vec<serde_json::Value> = page_presets
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "total_pages": total_pages,
        "presets": result
    }))
}
```

- [ ] **Step 2: Update the tool definition in tools.rs**

Find the `preset.list_by_plugin` tool_def and add page/page_size properties.

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/mcp/preset_tools.rs src/mcp/tools.rs
git commit -m "fix: add pagination to preset.list_by_plugin

Previously returned all presets in one unbounded response. Now
respects page/page_size like preset.list."
```

---

### Task 12: Fix page_size=0 edge case + drop read lock during serialization

**Files:**
- Modify: `src/audio/plugins/preset_library.rs:159-170`
- Modify: `src/mcp/preset_tools.rs:20-26`

- [ ] **Step 1: Clamp page_size to minimum 1 in search()**

In `src/audio/plugins/preset_library.rs`, in the `search` method, after computing `total_results`:

```rust
        let page_size = page_size.max(1);
```

Add this before the `total_pages` calculation.

- [ ] **Step 2: Drop read lock early in cmd_preset_list**

In `src/mcp/preset_tools.rs`, `cmd_preset_list`:

```rust
    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    let results = lib.search(query, filter_plugin, page, page_size);
    drop(lib); // Release lock before serialization
    serde_json::to_value(&results).map_err(|e| format!("Serialization error: {e}"))
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/audio/plugins/preset_library.rs src/mcp/preset_tools.rs
git commit -m "fix: clamp page_size to min 1, drop read lock before serialization"
```

---

### Task 13: Clean up format_iso8601 + remove dead branch

**Files:**
- Modify: `src/mcp/preset_tools.rs:150-192`

- [ ] **Step 1: Replace hand-rolled date with chrono**

The project already depends on `chrono`. Replace `format_iso8601` and `is_leap` in `preset_tools.rs`:

Replace the two functions with:

```rust
fn format_iso8601(unix_secs: u64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(unix_secs as i64, 0)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add src/mcp/preset_tools.rs
git commit -m "cleanup: replace hand-rolled format_iso8601 with chrono"
```

---

### Task 14: Add skipped_no_factory count to preset.scan response

**Files:**
- Modify: `src/audio/plugins/preset_discovery.rs` — return count of NoPresetDiscoveryFactory
- Modify: `src/mcp/preset_tools.rs` — include in response

- [ ] **Step 1: Change scan_plugins_for_presets to return 3 values**

Change the return type from `(Vec<PresetEntry>, Vec<(PathBuf, String)>)` to:

```rust
pub struct ScanSummary {
    pub presets: Vec<PresetEntry>,
    pub errors: Vec<(PathBuf, String)>,
    pub skipped_no_factory: usize,
}
```

And update `scan_plugins_for_presets`:

```rust
pub fn scan_plugins_for_presets(plugin_paths: &[PathBuf]) -> ScanSummary {
    let mut presets = Vec::new();
    let mut errors = Vec::new();
    let mut skipped = 0usize;

    for path in plugin_paths {
        match scan_plugin_presets(path) {
            Ok(entries) => {
                presets.extend(entries);
            }
            Err(PresetScanError::NoPresetDiscoveryFactory) => {
                skipped += 1;
            }
            Err(e) => {
                errors.push((path.clone(), e.to_string()));
            }
        }
    }

    ScanSummary { presets, errors, skipped_no_factory: skipped }
}
```

- [ ] **Step 2: Update all callers**

- `src/app.rs` `rescan_presets()`:
```rust
        let summary = preset_discovery::scan_plugins_for_presets(&paths);
        // use summary.presets, summary.errors, summary.skipped_no_factory
```

- `src/mcp/preset_tools.rs` `cmd_preset_scan`: use the summary fields.

- [ ] **Step 3: Update the Surge XT test** (it calls `scan_plugin_presets` directly, not the bulk function, so it's fine)

- [ ] **Step 4: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "fix: report skipped_no_factory count in preset scan results"
```

---

## Final Verification

- [ ] **Run full test suite**: `cargo test`
- [ ] **Run Surge XT integration test**: `cargo test test_scan_surge_xt_presets -- --nocapture`
- [ ] **Run all CLAP preset scan test**: `cargo test test_scan_all_clap_presets -- --nocapture`
- [ ] **Verify sub-column navigation**: `cargo test step_sub_column -- --nocapture`
- [ ] **Verify finalize location fix**: `cargo test test_finalize_preserves_location -- --nocapture`
- [ ] **Bump version**: `0.20.0` → `0.21.0`
- [ ] **Update AGENTS.md** if needed
- [ ] **Commit and push**
