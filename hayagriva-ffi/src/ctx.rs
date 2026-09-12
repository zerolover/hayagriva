use biblatex::{ParseErrorKind, Token};
use citationberg::{
    CitationFormat, IndependentStyle, Locale, LocaleCode, Style, StyleCategory, StyleClass,
};
use hayagriva::archive::{ArchivedStyle, locales};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, Library, io,
};
use unicode_width::UnicodeWidthChar;

use crate::json;

const TAB_WIDTH: usize = 4;
const CONTEXT_RADIUS: usize = 40;

fn char_boundary_at_or_before(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    (0..=offset)
        .rev()
        .find(|index| source.is_char_boundary(*index))
        .unwrap_or(0)
}

fn char_display_width(character: char, column: usize) -> usize {
    if character == '\t' {
        TAB_WIDTH - column % TAB_WIDTH
    } else {
        character.width().unwrap_or(0)
    }
}

fn display_width(text: &str, start_column: usize) -> usize {
    let mut column = start_column;
    for character in text.chars() {
        column += char_display_width(character, column);
    }
    column - start_column
}

fn expand_tabs(text: &str, start_column: usize) -> String {
    let mut column = start_column;
    let mut expanded = String::with_capacity(text.len());
    for character in text.chars() {
        if character == '\t' {
            let spaces = char_display_width(character, column);
            expanded.push_str(&" ".repeat(spaces));
            column += spaces;
        } else {
            expanded.push(character);
            column += char_display_width(character, column);
        }
    }
    expanded
}

fn byte_offset_at_display_column(text: &str, target_column: usize) -> usize {
    let mut column = 0;
    for (offset, character) in text.char_indices() {
        if column >= target_column {
            return offset;
        }
        let next_column = column + char_display_width(character, column);
        if next_column > target_column {
            return offset;
        }
        column = next_column;
    }
    text.len()
}

fn source_line_bounds(source: &str, offset: usize) -> (usize, usize, usize) {
    let offset = char_boundary_at_or_before(source, offset);
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let mut line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    if line_end > line_start && source.as_bytes()[line_end - 1] == b'\r' {
        line_end -= 1;
    }
    (line_start, line_end, offset.min(line_end))
}

fn source_location(source: &str, offset: usize) -> (usize, usize) {
    let (line_start, _, offset) = source_line_bounds(source, offset);
    let line = source[..line_start].bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = display_width(&source[line_start..offset], 0) + 1;
    (line, column)
}

fn missing_comma_offset(source: &str, offset: usize) -> usize {
    let mut offset = char_boundary_at_or_before(source, offset);

    // The parser reports the position of the next field. Move past the
    // whitespace before it so the diagnostic points at the comma's insertion
    // point, immediately after the previous field value.
    while offset > 0 {
        let previous = source[..offset].chars().next_back().unwrap();
        if !previous.is_whitespace() {
            break;
        }
        offset -= previous.len_utf8();
    }

    offset
}

fn source_context(source: &str, offset: usize, line: usize) -> String {
    let (line_start, line_end, offset) = source_line_bounds(source, offset);
    let line_text = &source[line_start..line_end];
    let relative_offset = offset.saturating_sub(line_start);
    let marker_column = display_width(&line_text[..relative_offset], 0);
    let full_width = display_width(line_text, 0);

    let (snippet, marker_column) = if full_width <= CONTEXT_RADIUS * 2 {
        (expand_tabs(line_text, 0), marker_column)
    } else {
        let window_start = marker_column.saturating_sub(CONTEXT_RADIUS);
        let window_end = (marker_column + CONTEXT_RADIUS).min(full_width);
        let start_offset = byte_offset_at_display_column(line_text, window_start);
        let end_offset = byte_offset_at_display_column(line_text, window_end);
        let left_truncated = start_offset > 0;
        let right_truncated = end_offset < line_text.len();
        let absolute_start_column = display_width(&line_text[..start_offset], 0);

        let mut snippet = String::new();
        if left_truncated {
            snippet.push_str("...");
        }
        snippet.push_str(&expand_tabs(
            &line_text[start_offset..end_offset],
            absolute_start_column,
        ));
        if right_truncated {
            snippet.push_str("...");
        }

        let marker_column = (if left_truncated { 3 } else { 0 })
            + display_width(
                &line_text[start_offset..relative_offset],
                absolute_start_column,
            );
        (snippet, marker_column)
    };

    let line_number_width = line.to_string().len();
    let line_label = format!("{line:>width$}", width = line_number_width);
    let marker_offset = byte_offset_at_display_column(&snippet, marker_column);
    let mut marked_snippet = String::with_capacity(snippet.len() + 1);
    marked_snippet.push_str(&snippet[..marker_offset]);
    marked_snippet.push('^');
    marked_snippet.push_str(&snippet[marker_offset..]);

    format!("  {line_label} | {marked_snippet}")
}

fn format_biblatex_error(error: io::BibLaTeXError, source: &str) -> String {
    match error {
        io::BibLaTeXError::Parse(error) => {
            let is_missing_comma = error.span.is_empty()
                && matches!(&error.kind, &ParseErrorKind::Expected(Token::Comma));
            let offset = if is_missing_comma {
                missing_comma_offset(source, error.span.start)
            } else {
                error.span.start
            };
            let (line, column) = source_location(source, offset);
            let mut message = format!(
                "biblatex parse error: {} at line {line}, column {column}",
                error.kind
            );
            message.push('\n');
            message.push_str(&source_context(source, offset, line));
            message
        }
        io::BibLaTeXError::Type(error) => {
            let (line, column) = source_location(source, error.span.start);
            let mut message = format!(
                "biblatex type error: {} at line {line}, column {column}",
                error.kind
            );
            message.push('\n');
            message.push_str(&source_context(source, error.span.start, line));
            message
        }
    }
}

/// Holds the mutable state a caller builds up across `hayagriva_set_*` calls:
/// the loaded bibliography, the chosen style, and the chosen locale. All three
/// can be replaced independently at any time.
pub struct HayagrivaCtx {
    library: Option<Library>,
    style: Option<(String, IndependentStyle)>,
    locale: Option<LocaleCode>,
    locales: Vec<Locale>,
}

impl HayagrivaCtx {
    pub fn new() -> Self {
        Self { library: None, style: None, locale: None, locales: locales() }
    }

    pub fn set_bib(&mut self, bib_str: &str) -> Result<(), String> {
        let library = io::from_biblatex_str(bib_str).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| format_biblatex_error(error, bib_str))
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        self.library = Some(library);
        Ok(())
    }

    pub fn set_style(&mut self, style_name: &str) -> Result<(), String> {
        let archived = ArchivedStyle::by_name(style_name)
            .ok_or_else(|| format!("embedded style not found: `{style_name}`"))?;
        let Style::Independent(style) = archived.get() else {
            return Err(format!(
                "embedded style `{style_name}` is not an independent CSL style"
            ));
        };
        self.style = Some((style_name.to_string(), style));
        Ok(())
    }

    pub fn set_locale(&mut self, locale: Option<&str>) -> Result<(), String> {
        let Some(code) = locale else {
            self.locale = None;
            return Ok(());
        };

        let code = LocaleCode(code.to_string());
        if self.locales.iter().any(|item| item.lang.as_ref() == Some(&code)) {
            self.locale = Some(code);
            Ok(())
        } else {
            Err(format!("embedded locale not found: `{}`", code.0))
        }
    }

    pub fn list_entries(&self) -> Result<String, String> {
        let library = self.library.as_ref().ok_or("no bibliography loaded")?;
        Ok(json::list_entries(library))
    }

    pub fn render(&self, citation_groups_json: &str) -> Result<String, String> {
        let library = self.library.as_ref().ok_or("no bibliography loaded")?;
        let (_, style) = self.style.as_ref().ok_or("no style set")?;

        let citation_groups: Vec<Vec<String>> = miniserde::json::from_str(citation_groups_json)
            .map_err(|err| format!("invalid citation groups JSON: {err}"))?;

        let mut driver = BibliographyDriver::new();
        for group in &citation_groups {
            let items = group
                .iter()
                .map(|key| {
                    library.get(key).map(CitationItem::with_entry).ok_or_else(|| {
                        format!("citation key `{key}` was not found in the bibliography")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            driver.citation(CitationRequest::new(
                items,
                style,
                self.locale.clone(),
                &self.locales,
                None,
            ));
        }

        let result =
            driver.finish(BibliographyRequest::new(style, self.locale.clone(), &self.locales));
        Ok(json::render_result(result))
    }
}

/// Only styles a slide-based host can meaningfully use: `Note`-class styles
/// render a full formatted reference as the in-text citation itself, which
/// depends on paginated footnotes that slide software doesn't have.
pub fn list_styles() -> String {
    let mut styles: Vec<(String, String, Option<String>, Option<String>)> = ArchivedStyle::all()
        .iter()
        .copied()
        .filter_map(|archived| {
            let Style::Independent(style) = archived.get() else {
                return None;
            };
            if style.settings.class != StyleClass::InText {
                return None;
            }
            Some((
                recommended_style_key(archived).to_string(),
                archived.display_name().to_string(),
                style.default_locale.as_ref().map(|locale| locale.0.clone()),
                citation_format(&style).map(str::to_string),
            ))
        })
        .collect();

    styles.sort_by(|a, b| a.1.cmp(&b.1));
    json::list_styles(&styles)
}

fn citation_format(style: &IndependentStyle) -> Option<&'static str> {
    style.info.category.iter().find_map(|category| {
        let StyleCategory::CitationFormat { format } = category else {
            return None;
        };
        Some(match *format {
            CitationFormat::AuthorDate => "author-date",
            CitationFormat::Author => "author",
            CitationFormat::Numeric => "numeric",
            CitationFormat::Label => "label",
            CitationFormat::Note => "note",
        })
    })
}

pub fn list_locales() -> String {
    let mut locale_codes: Vec<String> = locales()
        .into_iter()
        .filter_map(|locale| locale.lang.map(|lang| lang.0))
        .collect();
    locale_codes.sort();
    locale_codes.dedup();
    json::list_locales(&locale_codes)
}

fn recommended_style_key(style: ArchivedStyle) -> &'static str {
    style
        .names()
        .iter()
        .copied()
        .min_by_key(|name| (name.len(), *name))
        .unwrap_or(style.names()[0])
}

#[cfg(test)]
mod tests {
    use super::{missing_comma_offset, source_context, source_location};

    #[test]
    fn source_location_is_one_based() {
        assert_eq!(source_location("first\n  second", 8), (2, 3));
    }

    #[test]
    fn source_location_counts_unicode_characters() {
        let source = "标题\n作者";
        let offset = source.find('作').unwrap();
        assert_eq!(source_location(source, offset), (2, 1));
    }

    #[test]
    fn missing_comma_location_points_to_insertion_point() {
        let source = "title = {value}\n    author = {Someone}";
        let offset = source.find("author").unwrap();
        let offset = missing_comma_offset(source, offset);
        assert_eq!(source_location(source, offset), (1, 16));
    }

    #[test]
    fn source_context_shows_single_line_and_caret() {
        let source = "title = {value} author = {Someone}";
        let offset = source.find("author").unwrap();
        let offset = missing_comma_offset(source, offset);
        let context = source_context(source, offset, 1);
        assert!(context.contains("  1 | title = {value}^ author = {Someone}"));
    }

    #[test]
    fn source_context_truncates_long_lines() {
        let source = format!("{}{}{}", "a".repeat(60), "X", "b".repeat(60));
        let context = source_context(source.as_str(), 60, 1);
        assert!(context.contains("..."));
        assert!(context.contains("X"));
    }

    #[test]
    fn list_styles_includes_gb_7714_2025() {
        let styles = super::list_styles();
        assert!(styles.contains("gb-7714-2025-author-date"));
        assert!(styles.contains("gb-7714-2025-numeric"));
        assert!(!styles.contains("gb-7714-2015"));
    }

    #[test]
    fn set_style_supports_gb_7714_2025() {
        let mut ctx = super::HayagrivaCtx::new();
        assert!(ctx.set_style("gb-7714-2025-numeric").is_ok());
        assert!(ctx.set_style("gb-7714-2025-author-date").is_ok());
        assert!(ctx.set_style("gb-7714-2025-note").is_err());
        assert!(ctx.set_style("gb-7714-2015-numeric").is_err());
    }

    #[test]
    fn gb_7714_2025_author_date_halfwidth_and_et_al() {
        let mut ctx = super::HayagrivaCtx::new();
        ctx.set_style("gb-7714-2025-author-date").unwrap();
        ctx.set_bib(
            r#"
            @article{banks,
              author = {Banks, A. and Gupta, R. and Other, C. and Last, D.},
              title = {MQTT Version 3.1.1},
              date = {2014},
            }
            @article{zhang,
              author = {张伟 and 李娜 and 王五 and 赵六},
              title = {基于图优化的视觉SLAM综述},
              date = {2023},
            }
            "#,
        )
        .unwrap();

        let json = ctx.render(r#"[["banks"], ["zhang"]]"#).unwrap();
        assert!(json.contains(r#""text":"(Banks et al., 2014)""#));
        assert!(json.contains(r#""text":"(张伟 等, 2023)""#));
        assert!(json.contains(r#""text":", et al.""#));
        assert!(json.contains(r#""text":", 等""#));

        // Verify no Chinese full-width punctuation exists in the output
        for c in ['（', '）', '，', '：', '；'] {
            assert!(
                !json.contains(c),
                "rendered output unexpectedly contains full-width character '{c}': {json}"
            );
        }
    }

    #[test]
    fn gb_7714_2025_numeric_halfwidth_and_et_al() {
        let mut ctx = super::HayagrivaCtx::new();
        ctx.set_style("gb-7714-2025-numeric").unwrap();
        ctx.set_bib(
            r#"
            @article{banks,
              author = {Banks, A. and Gupta, R. and Other, C. and Last, D.},
              title = {MQTT Version 3.1.1},
              journal = {Journal of Networks},
              volume = {45},
              number = {2},
              pages = {123-145},
              date = {2014},
            }
            @article{zhang,
              author = {张伟 and 李娜 and 王五 and 赵六},
              title = {基于图优化的视觉SLAM综述},
              journal = {机器人},
              volume = {45},
              number = {2},
              pages = {123-145},
              date = {2023},
            }
            "#,
        )
        .unwrap();

        let json = ctx.render(r#"[["banks"], ["zhang"]]"#).unwrap();
        assert!(json.contains(r#""text":"[1]""#));
        assert!(json.contains(r#""text":"[2]""#));
        assert!(json.contains(r#""text":", et al""#));
        assert!(json.contains(r#""text":", 等""#));
        assert!(json.contains(r#""text":"(2)""#));
        assert!(json.contains(r#""text":": ""#));

        // Verify no Chinese full-width punctuation exists in the output
        for c in ['（', '）', '，', '：', '；'] {
            assert!(
                !json.contains(c),
                "rendered output unexpectedly contains full-width character '{c}': {json}"
            );
        }
    }

    #[test]
    fn gb_7714_2025_book_volume_and_edition() {
        let mut ctx = super::HayagrivaCtx::new();
        ctx.set_style("gb-7714-2025-numeric").unwrap();
        ctx.set_bib(
            r#"
            @book{goodfellow,
              author = {Goodfellow, Ian and Bengio, Yoshua and Courville, Aaron and Someone, Else},
              title = {Deep learning},
              volume = {1},
              edition = {2},
              publisher = {MIT Press},
              date = {2016},
            }
            @book{chinese_book,
              author = {张三 and 李四 and 王五 and 赵六},
              title = {深度学习},
              volume = {1},
              edition = {2},
              publisher = {清华大学出版社},
              date = {2020},
            }
            "#,
        )
        .unwrap();

        let json = ctx.render(r#"[["goodfellow"], ["chinese_book"]]"#).unwrap();
        assert!(json.contains(r#""text":"Vol. ""#));
        assert!(json.contains(r#""text":"2nd""#));
        assert!(json.contains(r#""text":"ed""#));
        assert!(json.contains(r#""text":", et al""#));

        assert!(json.contains(r#""text":"卷""#));
        assert!(json.contains(r#""text":"版""#));
        assert!(json.contains(r#""text":", 等""#));
    }
}

