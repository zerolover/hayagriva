use citationberg::{IndependentStyle, Locale, LocaleCode, Style, StyleClass};
use hayagriva::archive::{ArchivedStyle, locales};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, Library, io,
};

use crate::json;

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
            errors.into_iter().map(|err| err.to_string()).collect::<Vec<_>>().join("\n")
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
    let mut styles: Vec<(String, String, Option<String>)> = ArchivedStyle::all()
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
            ))
        })
        .collect();

    styles.sort_by(|a, b| a.1.cmp(&b.1));
    json::list_styles(&styles)
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
