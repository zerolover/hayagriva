use citationberg::{FontStyle, FontVariant, FontWeight, TextDecoration, VerticalAlign};
use hayagriva::{
    BibliographyItem, Elem, ElemChild, ElemChildren, Formatted, Formatting, Library, Rendered,
};
use miniserde::Serialize;

#[derive(Serialize)]
pub struct Run {
    text: String,
    italic: bool,
    font_weight: &'static str,
    small_caps: bool,
    underline: bool,
    vertical_align: &'static str,
    url: Option<String>,
}

fn font_weight_str(weight: FontWeight) -> &'static str {
    match weight {
        FontWeight::Normal => "normal",
        FontWeight::Bold => "bold",
        FontWeight::Light => "light",
    }
}

fn vertical_align_str(align: VerticalAlign) -> &'static str {
    match align {
        VerticalAlign::None => "none",
        VerticalAlign::Baseline => "baseline",
        VerticalAlign::Sup => "sup",
        VerticalAlign::Sub => "sub",
    }
}

fn run_from_formatting(text: String, formatting: &Formatting, url: Option<String>) -> Run {
    Run {
        text,
        italic: formatting.font_style == FontStyle::Italic,
        font_weight: font_weight_str(formatting.font_weight),
        small_caps: formatting.font_variant == FontVariant::SmallCaps,
        underline: formatting.text_decoration == TextDecoration::Underline,
        vertical_align: vertical_align_str(formatting.vertical_align),
        url,
    }
}

fn plain_run(text: String) -> Run {
    run_from_formatting(text, &Formatting::default(), None)
}

fn trim_run_edges(runs: &mut Vec<Run>) {
    while let Some(first) = runs.first_mut() {
        let trimmed = first.text.trim_start().to_string();
        if trimmed.is_empty() {
            runs.remove(0);
            continue;
        }
        first.text = trimmed;
        break;
    }

    while let Some(last) = runs.last_mut() {
        let trimmed = last.text.trim_end().to_string();
        if trimmed.is_empty() {
            runs.pop();
            continue;
        }
        last.text = trimmed;
        break;
    }
}

fn ends_with_whitespace(runs: &[Run]) -> bool {
    runs.last()
        .and_then(|run| run.text.chars().last())
        .is_some_and(char::is_whitespace)
}

fn starts_with_whitespace(runs: &[Run]) -> bool {
    runs.first()
        .and_then(|run| run.text.chars().next())
        .is_some_and(char::is_whitespace)
}

fn push_children(children: &ElemChildren, out: &mut Vec<Run>) {
    for child in &children.0 {
        push_child(child, out);
    }
}

fn push_elem(elem: &Elem, out: &mut Vec<Run>) {
    push_children(&elem.children, out);
}

fn push_child(child: &ElemChild, out: &mut Vec<Run>) {
    match child {
        ElemChild::Text(Formatted { text, formatting }) => {
            out.push(run_from_formatting(text.clone(), formatting, None))
        }
        ElemChild::Elem(elem) => push_elem(elem, out),
        // Math-mode chunks meant for a Typst renderer; there is no math
        // renderer on this side, so pass the source through as plain text,
        // matching what hayagriva's own Plain/VT100/Html writers already do.
        ElemChild::Markup(text) => out.push(plain_run(text.clone())),
        ElemChild::Link { text, url } => {
            out.push(run_from_formatting(text.text.clone(), &text.formatting, Some(url.clone())))
        }
        // A placeholder the consumer is supposed to resolve into a specific
        // citation item; not reachable for the in-text-only styles this
        // crate restricts callers to. Nothing to render without that
        // resolution logic, so it is skipped rather than guessed at.
        ElemChild::Transparent { .. } => {}
    }
}

fn runs(children: &ElemChildren) -> Vec<Run> {
    let mut out = Vec::new();
    push_children(children, &mut out);
    out
}

/// Whether any run in `children` is set to superscript. Citations render as
/// either plain text or, for numeric-superscript styles, entirely
/// superscript — so this collapses to a single flag instead of the general
/// run list bibliography entries need.
fn any_sup(children: &ElemChildren) -> bool {
    fn elem_any_sup(elem: &Elem) -> bool {
        children_any_sup(&elem.children)
    }
    fn children_any_sup(children: &ElemChildren) -> bool {
        children.0.iter().any(|child| match child {
            ElemChild::Text(Formatted { formatting, .. }) => {
                formatting.vertical_align == VerticalAlign::Sup
            }
            ElemChild::Elem(elem) => elem_any_sup(elem),
            ElemChild::Markup(_) => false,
            ElemChild::Link { text, .. } => text.formatting.vertical_align == VerticalAlign::Sup,
            ElemChild::Transparent { format, .. } => format.vertical_align == VerticalAlign::Sup,
        })
    }
    children_any_sup(children)
}

fn plain_text(children: &ElemChildren) -> String {
    runs(children).into_iter().map(|run| run.text).collect()
}

#[derive(Serialize)]
struct CitationOut {
    text: String,
    sup: bool,
}

#[derive(Serialize)]
struct BibliographyItemOut {
    key: String,
    prefix_runs: Vec<Run>,
    runs: Vec<Run>,
}

#[derive(Serialize)]
struct BibliographyOut {
    items: Vec<BibliographyItemOut>,
}

#[derive(Serialize)]
struct RenderOut {
    citations: Vec<CitationOut>,
    bibliography: BibliographyOut,
}

fn bibliography_item_out(item: &BibliographyItem) -> BibliographyItemOut {
    let mut prefix_runs = Vec::new();
    if let Some(first) = &item.first_field {
        push_child(first, &mut prefix_runs);
    }
    trim_run_edges(&mut prefix_runs);

    let mut content_runs = runs(&item.content);
    trim_run_edges(&mut content_runs);

    if !prefix_runs.is_empty()
        && !content_runs.is_empty()
        && !ends_with_whitespace(&prefix_runs)
        && !starts_with_whitespace(&content_runs)
    {
        prefix_runs.push(plain_run(" ".to_string()));
    }

    BibliographyItemOut {
        key: item.key.clone(),
        prefix_runs,
        runs: content_runs,
    }
}

pub fn render_result(result: Rendered) -> String {
    let citations = result
        .citations
        .into_iter()
        .map(|row| CitationOut {
            text: plain_text(&row.citation),
            sup: any_sup(&row.citation),
        })
        .collect();

    let items = result
        .bibliography
        .map(|bibliography| bibliography.items.iter().map(bibliography_item_out).collect())
        .unwrap_or_default();

    let out = RenderOut { citations, bibliography: BibliographyOut { items } };
    miniserde::json::to_string(&out)
}

#[derive(Serialize)]
struct EntryOut {
    key: String,
    title: Option<String>,
    authors: Vec<String>,
    year: Option<i32>,
    container_title: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    page_range: Option<String>,
}

pub fn list_entries(library: &Library) -> String {
    let entries: Vec<EntryOut> = library
        .iter()
        .map(|entry| EntryOut {
            key: entry.key().to_string(),
            title: entry.title().map(|title| title.value.to_string()),
            authors: entry
                .authors()
                .map(|authors| authors.iter().map(|person| person.name_first(false, false)).collect())
                .unwrap_or_default(),
            year: entry.date().map(|date| date.year),
            container_title: entry
                .parents()
                .first()
                .and_then(|parent| parent.title())
                .map(|title| title.value.to_string()),
            volume: entry.volume().map(|volume| volume.to_string()),
            issue: entry.issue().map(|issue| issue.to_string()),
            page_range: entry.page_range().map(|pages| pages.to_string()),
        })
        .collect();

    miniserde::json::to_string(&entries)
}

#[derive(Serialize)]
struct StyleOut<'a> {
    key: &'a str,
    display_name: &'a str,
    default_locale: Option<&'a str>,
}

pub fn list_styles(styles: &[(String, String, Option<String>)]) -> String {
    let out: Vec<StyleOut> = styles
        .iter()
        .map(|(key, display_name, default_locale)| StyleOut {
            key,
            display_name,
            default_locale: default_locale.as_deref(),
        })
        .collect();
    miniserde::json::to_string(&out)
}

pub fn list_locales(locales: &[String]) -> String {
    miniserde::json::to_string(locales)
}
