# Sample Library MCP API — Roadmap

## Overview

Phase 1 (directory tree browsing + name-pattern matching) is **implemented**. This document covers future phases to expand the sample library's search, indexing, and intelligent loading capabilities via the MCP API.

## Current State (Phase 1 — Complete)

### What's Built

- `SampleLibrary` in `src/mcp/library.rs` — per-session, in-memory directory browser with lazy WAV header caching
- `library: Arc<RwLock<SampleLibrary>>` on `McpServer`, passed to all `ToolContext` instances
- `library_roots: Vec<String>` in `AppConfig` persists root paths across sessions
- Filename heuristic parser: splits on `_`, ` `, `.` (not `-`), detects notes `[A-G][#b-]?[0-9]+`, BPM `\d+bpm`, category tokens
- WAV header reader: extracts duration, sample_rate, bit_depth, channels (no PCM data)
- Per-file and per-directory caching, cleared on `set_roots()`

### MCP Tools (Phase 1)

| Tool | Type | Description |
|------|------|-------------|
| `sample_library.configure` | Read-only | Set library root directories |
| `sample_library.list_dir` | Read-only | Browse directory with WAV metadata + name heuristics, paginated |
| `sample_library.search` | Read-only | Filename/category substring search across cached entries |
| `sample_library.import` | Mutation | Load WAV into module with optional `target_slot`, `name`, `set_note` |

### Limitations of Phase 1

- **No persistent index** — cold cache on every app restart, WAV headers re-read on first browse
- **No background scanning** — library is populated lazily as the user browses
- **Naive search** — substring match on cached entries only; uncached directories are invisible to search
- **No content analysis** — no spectral features, key detection, or transient analysis
- **No tagging** — metadata comes only from filename heuristics, not user curation
- **No multi-sample auto-mapping** — `import` loads one sample at a time

---

## Phase 2 — Background Incremental Index (~3 days)

### Goal

Build a complete index of the library in the background, enabling fast search across all roots without requiring the user to browse first.

### Architecture

| Thread | Responsibility |
|--------|---------------|
| **MCP server thread** (existing) | Handle queries, directory listings — read-only, fast |
| **Background indexer thread** (new) | Walk directories, extract metadata, populate the index |
| **Main thread** (existing) | Only for `sample_library.import` mutations |

### New MCP Tools

```
sample_library.start_scan(?roots: [...]) → { scan_id, status: "started" }
sample_library.scan_status(scan_id) → { state: "scanning"|"complete", progress: { files_scanned, total, current_path } }
sample_library.pause_scan(scan_id)
sample_library.resume_scan(scan_id)
```

### Indexing Strategy

- **Incremental**: skip files whose `modified` timestamp hasn't changed since last index
- **Resumable**: scan state checkpointed periodically
- **Non-blocking**: MCP server continues handling queries during a scan
- **Background thread**: spawned by `start_scan`, communicates progress via shared atomic state

### Index Structure

```rust
pub struct SampleLibrary {
    pub roots: Vec<PathBuf>,
    cache: HashMap<PathBuf, LibraryEntry>,       // Phase 1: per-file cache
    dir_cache: HashMap<PathBuf, Vec<PathBuf>>,   // Phase 1: per-directory child list
    // Phase 2 additions:
    index_state: IndexState,                      // idle | scanning { scan_id, progress } | ready
    scan_queue: Vec<PathBuf>,                     // pending paths to scan
}

enum IndexState {
    Idle,
    Scanning { scan_id: String, files_scanned: usize, total_estimate: usize },
    Ready { last_scan: SystemTime, file_count: usize },
}
```

### WAV Header Reading (existing, reused)

`read_wav_header(path)` from Phase 1 reads only the RIFF/fmt/data chunks (~1ms per file). The background thread calls this for each WAV file and populates the cache.

---

## Phase 3 — Advanced Search & Filtering (~2 days)

### Goal

Rich query interface once the index is built, supporting musical context filters.

### Enhanced `sample_library.search`

```
sample_library.search({
    query: "kick 808",              // full-text filename search
    category: "drums",              // from name heuristic or user tags
    root_note: "C",                 // note name or "any"
    bpm_min: 100, bpm_max: 120,    // tempo range
    duration_min: 0.05,            // seconds
    duration_max: 2.0,
    sample_rate: 44100,             // exact match or null
    bit_depth: 16,                  // exact match or null
    tags: ["acoustic", "dry"],      // user tags (Phase 4)
    root_paths: ["D:\\Samples\\Drums"],  // scope to certain roots
    sort: "duration",               // name, duration, sample_rate, bpm, date
    order: "asc",
    page: 0, page_size: 50
})
```

### Sort Options

- `name` — alphabetical
- `duration` — shortest/longest first
- `date` — most recently modified
- `bpm` — tempo order
- `category` — grouped by category

---

## Phase 4 — Persistent SQLite Index (~2-3 days)

### Goal

Index survives app restarts. No full rescan needed on launch.

### Architecture

- SQLite database stored alongside `AppConfig` (e.g., `~/.htrk/sample_library.db`)
- Schema:

```sql
CREATE TABLE samples (
    path TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    modified INTEGER NOT NULL,        -- unix timestamp
    file_size INTEGER NOT NULL,
    duration REAL,
    sample_rate INTEGER,
    bit_depth INTEGER,
    channels INTEGER,
    category TEXT,
    root_note TEXT,
    bpm REAL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE tags (
    path TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (path, tag),
    FOREIGN KEY (path) REFERENCES samples(path) ON DELETE CASCADE
);

CREATE TABLE scan_history (
    scan_id TEXT PRIMARY KEY,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    files_scanned INTEGER DEFAULT 0,
    files_added INTEGER DEFAULT 0,
    errors INTEGER DEFAULT 0
);
```

### New MCP Tools

```
sample_library.init_db(db_path) → { initialized: true }
sample_library.ingest(paths: [...]) → { added, updated, skipped }
sample_library.refresh() → { rescanned: N }
sample_library.stats() → { total_files, total_duration, by_category: {...}, by_root: {...} }
```

### Dependency

Add `rusqlite` (SQLite bindings, public domain).

### Startup Behavior

On app launch: open DB, check for roots whose filesystem `modified` has changed, queue those for incremental rescan. Search works immediately from persisted index.

---

## Phase 5 — Tagging & Curation (~1-2 days)

### Goal

User-curated metadata alongside filename heuristics.

### New MCP Tools

```
sample_library.tag(path: "...", tags: ["kick", "acoustic", "dry"])
sample_library.untag(path: "...", tags: ["dry"])
sample_library.get_tags(path: "...") → ["kick", "acoustic"]
sample_library.list_tags() → [{ tag: "kick", count: 342 }, { tag: "pad", count: 87 }, ...]
```

### Integration

- Tags stored in SQLite `tags` table (Phase 4) or in-memory `HashMap<PathBuf, Vec<String>>` (if Phase 4 not yet done)
- Search results include `tags` field
- `list_dir` results include `tags` field
- Tags augment (not replace) filename heuristic categories

---

## Phase 6 — Multi-Sample Auto-Mapping (~2 days)

### Goal

Load multiple samples and automatically distribute them across an instrument's note map.

### New MCP Tool

```
sample_library.import_multi({
    selections: [
        { path: "...", root_note: "C2" },
        { path: "...", root_note: "C3" },
        { path: "...", root_note: "C4" },
    ],
    target_instrument: 3,
    mapping: {
        mode: "chromatic" | "ranges" | "single",
        overlap: 0,                // overlap zone in semitones
        fade: "linear" | "equal_power",
    }
})
```

### Mapping Modes

- **chromatic**: each sample maps to its root_note, fills gaps with nearest sample (pitch-shifted)
- **ranges**: each sample covers a key range centered on its root_note
- **single**: all samples mapped to the same key (layered/multi-sample for velocity)

### Integration

Uses existing `MapNoteToSampleCommand` and `MapRangeCommand` in the undo system. Creates an `Instrument` with populated `sample_map` and `note_map`.

---

## Phase 7 — Context-Aware Suggestions (~2-3 days)

### Goal

"Intelligent" sample suggestions based on the current musical context.

### New MCP Tool

```
sample_library.suggest({
    context: {
        key_scale: "Am",               // current key/scale
        bpm: 140,
        channel_type: "bass",          // bass, drums, lead, pad, fx
        instrument_slot: 3,
        exclude_recent: true,          // avoid recently used samples
    }
}) → [{
    path: "...",
    name: "...",
    relevance_score: 0.87,
    reason: "Bass sample in Am — matches current key; no bass instrument currently mapped"
}, ...]
```

### Relevance Scoring

Factors (weighted):
- **Key match** (0.3): sample's detected root_note is in the current scale
- **Tempo match** (0.2): sample's BPM is within ±5% of song BPM
- **Category fit** (0.3): category matches the requested `channel_type`
- **Variety** (0.1): not recently used in this session
- **Duration fit** (0.1): duration is reasonable for the channel type

### Context Source

The MCP server reads `PlaybackSnapshot` + `ChannelsSnapshot` (already available in `ToolContext`) to determine:
- Current BPM
- What instruments/samples are currently mapped to each channel
- What notes are being played

---

## Phase 8 — Content-Based Analysis (~3-5 days, experimental)

### Goal

Extract audio features for similarity search and automatic categorization.

### Features

- **Spectral centroid** → bright vs dark
- **RMS energy** → loud vs quiet
- **Transient density** → percussive vs sustained
- **Zero-crossing rate** → noisy vs tonal
- **Fundamental frequency** → pitch detection (for root_note auto-detection)

### New MCP Tools

```
sample_library.analyze(path: "...") → { spectral_centroid, rms, transient_density, zcr, f0_estimate }
sample_library.find_similar(path: "...", limit: 10) → [{ path, similarity_score }]
```

### Implementation

- Feature extraction in the background indexer thread (Phase 2)
- Features stored in SQLite (Phase 4) alongside metadata
- Similarity via cosine distance on feature vectors
- Requires PCM data reading (not just headers) — slower than Phase 1-3 but runs in background

---

## Summary Timeline

| Phase | Feature | Effort | Depends On |
|-------|---------|--------|------------|
| 1 ✅ | Directory tree + name matching | Done | — |
| 2 | Background incremental index | ~3 days | Phase 1 |
| 3 | Advanced search & filtering | ~2 days | Phase 2 |
| 4 | Persistent SQLite index | ~2-3 days | Phase 2 |
| 5 | Tagging & curation | ~1-2 days | Phase 4 (or Phase 1 in-memory) |
| 6 | Multi-sample auto-mapping | ~2 days | Phase 1 |
| 7 | Context-aware suggestions | ~2-3 days | Phases 2-3 |
| 8 | Content-based analysis | ~3-5 days | Phases 2-4 |
| **Total** | | **~15-20 days** |

## Design Principles

1. **All operations via MCP** — the library is controlled entirely through JSON-RPC tools, enabling both UI and external automation
2. **Progressive enhancement** — Phase 1 works without an index; each phase adds capability without breaking earlier tools
3. **Read-only on MCP thread** — search, browse, and query run on the MCP server thread without blocking the main/audio threads
4. **Import is the only mutation** — loading samples into the module goes through the existing main-thread dispatch queue
5. **Filename heuristics + user curation** — automatic metadata from filenames is augmented (not replaced) by user tags and content analysis
6. **Graceful degradation** — if SQLite is unavailable, fall back to in-memory cache; if content analysis fails, fall back to filename-only metadata
