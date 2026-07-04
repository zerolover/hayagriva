# Hayagriva FFI Design

## Goal

A small C ABI on top of `hayagriva` for a C++ host to:

- Load and update a BibLaTeX bibliography.
- Pick a citation style and locale from hayagriva's embedded archive.
- Search/browse the bibliography to let a user pick entries to cite.
- Render citations and a bibliography for a document, in citation order, as
  structured (not flattened-to-string) results that preserve rich text
  formatting (italics, superscript, links).

First consumer: a **LibreOffice Impress** extension (citation-insert search
popup, footer bibliography per slide).

## Scope

- Only `StyleClass::InText` styles are exposed (`hayagriva_list_styles`
  filters on the CSL `class` field). `Note`-class styles render full
  formatted reference text as the in-text citation itself, which depends on
  paginated footnotes; Impress has no footnote primitive.
- Citations are `{ "text": string, "sup": bool }`, not formatted runs.
  Bibliography entries use full runs.
- `note_number` is not part of the render output (only meaningful for
  `Note`-class styles, which aren't reachable through this API).
- `hayagriva_render` always takes the complete ordered list of citation
  groups for the whole document, not just newly inserted ones — CSL
  numbering/disambiguation is stateful across the whole document. Slide
  attribution is plugin-side bookkeeping (see "Per-slide bibliography").
- bib/style/locale are independently mutable; the context exposes separate
  setters rather than baking them into construction.

## API surface

```c
typedef struct HayagrivaCtx HayagrivaCtx;

// Lifecycle
HayagrivaCtx* hayagriva_new(char** out_error);
void hayagriva_free(HayagrivaCtx* ctx);

// Mutable state — call any of these again at any time to change it
bool hayagriva_set_bib(HayagrivaCtx* ctx, const char* bib_str, char** out_error);
bool hayagriva_set_style(HayagrivaCtx* ctx, const char* style_name, char** out_error);
bool hayagriva_set_locale(HayagrivaCtx* ctx, const char* locale /* nullable */, char** out_error);

// Queries — JSON results, see schemas below
char* hayagriva_list_styles(void);                 // independent of ctx state
char* hayagriva_list_locales(void);                // independent of ctx state
char* hayagriva_list_entries(HayagrivaCtx* ctx);
char* hayagriva_render(HayagrivaCtx* ctx, const char* citation_groups_json, char** out_error);

// Every char* returned by any function above (including *out_error) must be
// released with this — never with free()/delete.
void hayagriva_free_string(char* s);
```

## String and memory rules

- All strings are UTF-8, NUL-terminated.
  - Strings passed **into** Rust (`bib_str`, `style_name`, `locale`,
    `citation_groups_json`) are borrowed for the call only.
  - Strings **returned** by Rust (`char*` return values, and whatever
    `*out_error` is set to) are heap-allocated (`CString::into_raw`). The
    caller releases them via `hayagriva_free_string` exactly once — never
    `free`/`delete`, the allocators aren't guaranteed to match.
  - `out_error` is only written on failure. Functions signal failure via
    `false`/`NULL`.
- No `catch_unwind` at the boundary. `panic = "abort"` in this crate's
  release profile, so a panic aborts the host process.
- If `CString::new` fails on an embedded NUL byte, the function reports it
  through `out_error` instead of panicking.

## JSON schemas

### `hayagriva_list_styles()`

```json
[
  { "key": "apa", "display_name": "APA Style 7th edition", "default_locale": null },
  { "key": "ieee", "display_name": "IEEE Reference Guide version 11.29.2023", "default_locale": null },
  { "key": "gb-7714-2015-numeric", "display_name": "China National Standard GB/T 7714-2015 (numeric, 中文)", "default_locale": "zh-CN" }
]
```

Sorted by `display_name`. `key` is what you pass to `hayagriva_set_style`.

### `hayagriva_list_locales()`

```json
[
  "en-GB",
  "en-US",
  "zh-CN"
]
```

### `hayagriva_list_entries(ctx)`

```json
[
  {
    "key": "Hinton06",
    "title": "A Fast Learning Algorithm for Deep Belief Nets",
    "authors": ["Geoffrey E. Hinton", "Simon Osindero", "Yee Whye Teh"],
    "year": 2006,
    "container_title": "Neural Computation",
    "volume": "18",
    "issue": null,
    "page_range": "1527-1554"
  }
]
```

Data source for the citation-picker search popup. Filtering happens
client-side on this cached list, not per-keystroke FFI calls.

### `hayagriva_render(ctx, citation_groups_json)`

Input — ordered citation groups for the whole document:

```json
[["Hinton06"], ["Goodfellow2016", "Baevski2019"]]
```

Output:

```json
{
  "citations": [
    { "text": "(Hinton et al., 2006)", "sup": false },
    { "text": "(Goodfellow et al., 2016; Baevski & Auli, 2019)", "sup": false }
  ],
  "bibliography": {
    "items": [
      {
        "key": "Hinton06",
        "prefix_runs": [
          { "text": "[1] ", "italic": false, "font_weight": "normal", "small_caps": false, "underline": false, "vertical_align": "none", "url": null }
        ],
        "runs": [
          { "text": "G. E. Hinton, S. Osindero, and Y. W. Teh, “A Fast Learning Algorithm for Deep Belief Nets,” ", "italic": false, "font_weight": "normal", "small_caps": false, "underline": false, "vertical_align": "none", "url": null },
          { "text": "Neural Computation", "italic": true, "font_weight": "normal", "small_caps": false, "underline": false, "vertical_align": "none", "url": null },
          { "text": ", vol. 18, pp. 1527–1554, 2006.", "italic": false, "font_weight": "normal", "small_caps": false, "underline": false, "vertical_align": "none", "url": null }
        ]
      }
    ]
  }
}
```

- `citations[i]` corresponds by index to `citation_groups_json[i]`.
- `bibliography.items[]` is keyed by `key` — build a lookup, don't rely on
  array order.
- `runs`/`prefix_runs` mirror `Formatting` plus a `url` field.

### Formatting model

| JSON field | Source enum | Values |
|---|---|---|
| `italic` | `FontStyle` | `Normal` / `Italic` |
| `small_caps` | `FontVariant` | `Normal` / `SmallCaps` |
| `font_weight` | `FontWeight` | `normal` / `bold` / `light` |
| `underline` | `TextDecoration` | `None` / `Underline` |
| `vertical_align` | `VerticalAlign` | `none` / `baseline` / `sup` / `sub` |

Plus `url: string | null` from `ElemChild::Link`.

The FFI layer walks `ElemChildren` directly rather than reusing hayagriva's
`Plain`/`VT100`/`Html` writers: `Plain`/`VT100` drop link URLs, and `Html`
doesn't escape text content.

## Per-slide bibliography

The plugin tracks which citation keys were inserted on which slide, flattens
all groups into `citation_groups_json` in insertion order, builds a
`key -> item` map from `bibliography.items` once per render, and looks up
each slide's keys in that map for its footer.

Numbering (`prefix_runs`) is global, not renumbered per slide — a slide
showing `[1]` and `[3]` but not `[2]` is expected.

## CJK / Chinese citations

No special handling needed:

- UTF-8 already carries CJK text.
- GB/T 7714 styles and `zh-CN`/`zh-TW` locales are already in hayagriva's
  archive.
- `Person::is_cjk()` (`src/types/persons.rs`) already handles CJK name
  formatting/sorting.
- Data-side caveats: `.bib` source must be UTF-8 (not GBK/GB2312), and
  Chinese author names should be a single BibLaTeX name token (e.g.
  `author = {张三 and 李四}`), not split Western-style.

## Header file

Hand-written, at `hayagriva-ffi/include/hayagriva.h`, not
`cbindgen`-generated — the surface is small enough to keep in sync by hand.

## Crate layout

Standalone crate `hayagriva-ffi/`, depending on `hayagriva` via a path
dependency, `[lib] crate-type = ["cdylib"]`. No `[workspace]` table, not a
member of the root crate's workspace — its own `Cargo.lock`/`target/`/
`[profile.release]`.

- `cd hayagriva-ffi && cargo build --release` compiles only this crate and
  its dependencies, not the root crate's CLI bins or their dependencies.
- Root crate's `Cargo.toml`/build artifacts are unaffected.
