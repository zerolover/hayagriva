#[cfg(not(feature = "archive"))]
compile_error!("main2 requires the `archive` feature");
#[cfg(not(feature = "biblatex"))]
compile_error!("main2 requires the `biblatex` feature");

use std::env;
use std::fs;
use std::process::exit;

use citationberg::{Locale, LocaleCode, Style, VerticalAlign};
use hayagriva::archive::{ArchivedStyle, locales};
use hayagriva::io;
use hayagriva::{
    BibliographyDriver, BibliographyRequest, BufWriteFormat, CitationItem,
    CitationRequest, ElemChild, ElemChildren,
};

struct Config {
    bib_path: String,
    style_name: Option<String>,
    locale: Option<LocaleCode>,
    cite_path: String,
    output_format: BufWriteFormat,
}

fn main() {
    let config = parse_args().unwrap_or_else(|message| fail(2, &message));
    let bibliography =
        read_bibliography(&config.bib_path).unwrap_or_else(|message| fail(3, &message));
    let locales = locales();

    if let Some(locale) = config.locale.as_ref() {
        ensure_locale_exists(locale, &locales)
            .unwrap_or_else(|message| fail(5, &message));
    }

    let citation_groups =
        read_citation_groups(&config.cite_path).unwrap_or_else(|message| fail(6, &message));

    if citation_groups.is_empty() {
        fail(7, "citation text file does not contain any citation groups");
    }

    let citation_groups = resolve_citation_groups(&bibliography, &citation_groups, &config.bib_path)
        .unwrap_or_else(|message| fail(8, &message));

    if let Some(style_name) = config.style_name.as_deref() {
        let style = read_style(style_name).unwrap_or_else(|message| fail(4, &message));
        print_rendered_output(
            &style,
            config.locale,
            &locales,
            &citation_groups,
            config.output_format,
        );
    } else {
        let mut first_style = true;
        for archived in ArchivedStyle::all().iter().copied() {
            if !first_style {
                println!();
            }
            first_style = false;
            let Style::Independent(style) = archived.get() else {
                fail(4, "embedded style is not an independent CSL style");
            };
            println!(
                "== {} [--style {}] ==",
                archived.display_name(),
                recommended_style_key(archived)
            );
            print_rendered_output(
                &style,
                config.locale.clone(),
                &locales,
                &citation_groups,
                config.output_format,
            );
        }
    }
}

fn parse_args() -> Result<Config, String> {
    let mut bib_path = None;
    let mut style_name = None;
    let mut locale = None;
    let mut cite_path = None;
    let mut output_format = BufWriteFormat::VT100;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bib" => bib_path = Some(next_value(&mut args, "--bib")?),
            "--style" => style_name = Some(next_value(&mut args, "--style")?),
            "--locale" => locale = Some(LocaleCode(next_value(&mut args, "--locale")?)),
            "--cite" => cite_path = Some(next_value(&mut args, "--cite")?),
            "--plain" => output_format = BufWriteFormat::Plain,
            "--help" | "-h" => {
                print_usage();
                exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}\n\n{}", usage())),
        }
    }

    Ok(Config {
        bib_path: bib_path.ok_or_else(|| missing_argument("--bib"))?,
        style_name,
        locale,
        cite_path: cite_path.ok_or_else(|| missing_argument("--cite"))?,
        output_format,
    })
}

fn next_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}\n\n{}", usage()))
}

fn missing_argument(flag: &str) -> String {
    format!("missing required argument: {flag}\n\n{}", usage())
}

fn usage() -> &'static str {
    "usage: cargo run --bin hayagriva_lite -- --bib <file.bib> [--style <style-name>] [--locale <locale>] [--plain] --cite <file.txt>"
}

fn print_usage() {
    println!("{}", usage());
    println!("  --bib     read a BibTeX/BibLaTeX bibliography");
    println!("  --style   use an embedded CSL style, for example `apa` or `ieee`");
    println!("            if omitted, iterate all embedded styles");
    println!("  --locale  use an embedded locale such as `en-US` or `zh-CN`");
    println!("  --plain   emit plain text instead of ANSI/VT100 terminal formatting");
    println!("  --cite    read citation groups from a text file, one group per line");
}

fn read_bibliography(path: &str) -> Result<hayagriva::Library, String> {
    let input = fs::read_to_string(path)
        .map_err(|err| format!("failed to read bibliography `{path}`: {err}"))?;

    io::from_biblatex_str(&input).map_err(|errors| {
        let details = errors
            .into_iter()
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        format!("failed to parse bibliography `{path}`:\n{details}")
    })
}

fn read_style(name: &str) -> Result<citationberg::IndependentStyle, String> {
    let archived = ArchivedStyle::by_name(name)
        .ok_or_else(|| format!("embedded style not found: `{name}`"))?;
    let Style::Independent(style) = archived.get() else {
        return Err(format!("embedded style `{name}` is not an independent CSL style"));
    };
    Ok(style)
}

fn ensure_locale_exists(locale: &LocaleCode, locales: &[Locale]) -> Result<(), String> {
    if locales.iter().any(|item| item.lang.as_ref() == Some(locale)) {
        Ok(())
    } else {
        Err(format!("embedded locale not found: `{}`", locale.0))
    }
}

fn read_citation_groups(path: &str) -> Result<Vec<Vec<String>>, String> {
    let input = fs::read_to_string(path)
        .map_err(|err| format!("failed to read citation file `{path}`: {err}"))?;

    let mut groups = Vec::new();

    for (line_no, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let items: Vec<String> = if line.contains(',') {
            line.split(',').map(str::trim).map(str::to_string).collect()
        } else {
            line.split_whitespace().map(str::to_string).collect()
        };

        if items.iter().any(|item| item.is_empty()) {
            return Err(format!(
                "invalid citation group at {}:{}",
                path,
                line_no + 1
            ));
        }

        groups.push(items);
    }

    Ok(groups)
}

fn fail(code: i32, message: &str) -> ! {
    eprintln!("{message}");
    exit(code);
}

fn resolve_citation_groups<'a>(
    bibliography: &'a hayagriva::Library,
    citation_groups: &[Vec<String>],
    bib_path: &str,
) -> Result<Vec<Vec<CitationItem<'a, hayagriva::Entry>>>, String> {
    citation_groups
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|key| {
                    bibliography.get(key).map(CitationItem::with_entry).ok_or_else(|| {
                        format!("citation key `{key}` was not found in `{}`", bib_path)
                    })
                })
                .collect()
        })
        .collect()
}

fn print_rendered_output(
    style: &citationberg::IndependentStyle,
    locale: Option<LocaleCode>,
    locales: &[Locale],
    citation_groups: &[Vec<CitationItem<'_, hayagriva::Entry>>],
    output_format: BufWriteFormat,
) {
    let default_locale = style
        .default_locale
        .as_ref()
        .map(|locale| locale.0.as_str())
        .unwrap_or("en-US (fallback)");
    println!("default-locale: {default_locale}");

    let mut driver = BibliographyDriver::new();

    for group in citation_groups {
        driver.citation(CitationRequest::new(
            group.clone(),
            style,
            locale.clone(),
            locales,
            None,
        ));
    }

    let result = driver.finish(BibliographyRequest::new(style, locale, locales));

    let citation_has_sup = result
        .citations
        .iter()
        .any(|row| contains_sup_children(&row.citation));
    if citation_has_sup {
        println!("-- cite (sup) --");
    } else {
        println!("-- cite --");
    }
    for row in result.citations {
        if let Some(note_number) = row.note_number {
            println!("{note_number}.");
        }
        println!("{}", render_children(&row.citation, output_format).trim_start());
    }

    println!("-- bibliography --");
    for row in result
        .bibliography
        .map(|bibliography| bibliography.items)
        .unwrap_or_default()
    {
        println!("{}", bibliography_item_text(&row, output_format));
    }
}

fn recommended_style_key(style: ArchivedStyle) -> &'static str {
    style
        .names()
        .iter()
        .copied()
        .min_by_key(|name| (name.len(), *name))
        .unwrap_or(style.names()[0])
}

fn render_children(renderable: &ElemChildren, format: BufWriteFormat) -> String {
    let mut rendered = String::new();
    renderable
        .write_buf(&mut rendered, format)
        .expect("writing citation output should not fail");
    rendered
}

fn render_child(renderable: &ElemChild, format: BufWriteFormat) -> String {
    let mut rendered = String::new();
    renderable
        .write_buf(&mut rendered, format)
        .expect("writing bibliography prefix should not fail");
    rendered
}

fn bibliography_item_text(
    item: &hayagriva::BibliographyItem,
    format: BufWriteFormat,
) -> String {
    let mut out = String::new();

    if let Some(first) = &item.first_field {
        out.push_str(&render_child(first, format));
        if !out.ends_with(' ') {
            out.push(' ');
        }
    }

    out.push_str(&render_children(&item.content, format));
    out.trim().to_string()
}

fn contains_sup_children(children: &ElemChildren) -> bool {
    children.0.iter().any(contains_sup_child)
}

fn contains_sup_child(child: &ElemChild) -> bool {
    match child {
        ElemChild::Text(formatted) => formatted.formatting.vertical_align == VerticalAlign::Sup,
        ElemChild::Elem(elem) => contains_sup_children(&elem.children),
        ElemChild::Markup(_) => false,
        ElemChild::Link { text, .. } => text.formatting.vertical_align == VerticalAlign::Sup,
        ElemChild::Transparent { format, .. } => format.vertical_align == VerticalAlign::Sup,
    }
}
