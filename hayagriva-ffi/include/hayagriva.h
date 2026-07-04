#ifndef HAYAGRIVA_FFI_H
#define HAYAGRIVA_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdbool.h>

/*
 * hayagriva-ffi: a small C ABI over hayagriva for citation formatting.
 *
 * String conventions:
 *   - All strings crossing this boundary are UTF-8, NUL-terminated.
 *   - Strings passed in (bib_str, style_name, locale, citation_groups_json)
 *     are borrowed for the duration of the call; the caller keeps ownership.
 *   - Strings returned (any `char*` return value, and whatever `out_error`
 *     is set to) are heap-allocated by Rust. The caller must release every
 *     one of them with hayagriva_free_string exactly once. Do NOT call
 *     free()/delete on them -- the allocators are not guaranteed to match.
 *   - `out_error` is only written to on failure and is left untouched on
 *     success. Functions returning bool/char* signal failure via
 *     false/NULL respectively.
 *   - This does not guard against Rust panics (no catch_unwind): a bug
 *     triggering a panic aborts the host process rather than surfacing as
 *     an error, since this crate's release profile uses panic = "abort".
 *
 * Only `StyleClass::InText` CSL styles are exposed (see
 * hayagriva_list_styles): this build targets slide-based hosts (no
 * pagination/footnotes), so footnote-class ("Note") styles are excluded.
 */

typedef struct HayagrivaCtx HayagrivaCtx;

/* Lifecycle */

/* Create a new, empty context (no bibliography/style/locale loaded yet). */
HayagrivaCtx* hayagriva_new(char** out_error);

/* Destroy a context created by hayagriva_new. `ctx` may be NULL (no-op). */
void hayagriva_free(HayagrivaCtx* ctx);

/* Mutable state -- call any of these again at any time to change it. */

/* Parse `bib_str` (BibLaTeX source) and replace the context's bibliography.
 * Returns false and sets *out_error on a parse error; the previous
 * bibliography (if any) is left untouched. */
bool hayagriva_set_bib(HayagrivaCtx* ctx, const char* bib_str, char** out_error);

/* Select an embedded CSL style by key, as returned by
 * hayagriva_list_styles's "key" field. */
bool hayagriva_set_style(HayagrivaCtx* ctx, const char* style_name, char** out_error);

/* Select an embedded locale, e.g. "en-US" or "zh-CN". Pass NULL to fall
 * back to the style's own default locale. */
bool hayagriva_set_locale(HayagrivaCtx* ctx, const char* locale, char** out_error);

/* Queries -- all return a heap-allocated UTF-8 JSON string (or NULL on
 * failure) that must be released with hayagriva_free_string. */

/*
 * Returns a JSON array of the embedded in-text CSL styles, sorted by
 * display_name:
 *   [ { "key": "apa", "display_name": "APA Style 7th edition", "default_locale": "en-US" }, ... ]
 * `default_locale` is null if the style has no explicit default locale and
 * therefore falls back to hayagriva's built-in default.
 * Does not depend on context state and cannot fail.
 */
char* hayagriva_list_styles(void);

/*
 * Returns a JSON array of the embedded CSL locales:
 *   [ "en-US", "zh-CN", ... ]
 * Does not depend on context state and cannot fail.
 */
char* hayagriva_list_locales(void);

/*
 * Returns a JSON array describing every entry in the loaded bibliography,
 * independent of the current style:
 *   [ { "key", "title", "authors": [...], "year", "container_title",
 *       "volume", "issue", "page_range" }, ... ]
 * Intended as the data source for a client-side search/picker; returns NULL
 * if no bibliography has been loaded yet.
 */
char* hayagriva_list_entries(const HayagrivaCtx* ctx);

/*
 * Renders citations and the bibliography for `citation_groups_json`, a JSON
 * array of citation groups in document order, e.g.:
 *   [["Hinton06"], ["Goodfellow2016", "Baevski2019"]]
 * (each inner array is a single citation event that may cite multiple keys
 * together). This must be the *complete* ordered list of citation groups
 * seen so far -- CSL numbering/disambiguation is stateful across the whole
 * document, not per call.
 *
 * Returns JSON of the shape:
 *   {
 *     "citations": [ { "text": "...", "sup": bool }, ... ],
 *     "bibliography": {
 *       "items": [
 *         { "key": "...",
 *           "prefix_runs": [ { "text", "italic", "font_weight", "small_caps",
 *                               "underline", "vertical_align", "url" }, ... ],
 *           "runs": [ ... same run shape ... ] },
 *         ...
 *       ]
 *     }
 *   }
 * `citations[i]` corresponds by index to `citation_groups_json[i]`.
 * `bibliography.items[]` should be looked up by "key", not relied on for a
 * particular order.
 *
 * `font_weight` is one of "normal"/"bold"/"light"; `vertical_align` is one
 * of "none"/"baseline"/"sup"/"sub"; `url` is a JSON string or null.
 *
 * Fails (returns NULL and sets *out_error) if no bibliography/style has been
 * set, or if a citation key is not present in the bibliography.
 */
char* hayagriva_render(
    const HayagrivaCtx* ctx,
    const char* citation_groups_json,
    char** out_error
);

/* Release any `char*` returned by the functions above (including whatever
 * an `out_error` parameter was set to). `s` may be NULL (no-op). */
void hayagriva_free_string(char* s);

#ifdef __cplusplus
}
#endif

#endif /* HAYAGRIVA_FFI_H */
