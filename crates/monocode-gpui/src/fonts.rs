//! The font families elements name. Each is a GPUI font alias that the font
//! preferences retarget, so a change never touches individual call sites:
//! `Window::text_style` scales an alias's absolute sizes, and the text system
//! resolves it to the chosen family with its weights shifted.
use crate::settings::Preferences;
use gpui::{App, FontAlias, Global, SharedString, Window, px};
use std::{ops::RangeInclusive, sync::Arc};

/// Interface chrome: sidebars, tabs, menus, settings.
pub(crate) const UI: &str = ".MonoUI";
/// Conversation text: messages, Markdown and the composer.
pub(crate) const CONTENT: &str = ".MonoContent";
/// Code blocks, inline code, diffs, the file editor and the terminal.
pub(crate) const CODE: &str = ".MonoCode";

/// GPUI resolves this family to the platform system UI font.
const SYSTEM_UI: &str = ".SystemUIFont";
/// The monospace font used when no code font is chosen.
pub(crate) const SYSTEM_CODE: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "monospace"
};

pub(crate) const REGULAR: u32 = 400;
pub(crate) const WEIGHTS: [(u32, &str); 5] = [
    (300, "Light"),
    (REGULAR, "Regular"),
    (500, "Medium"),
    (600, "Semibold"),
    (700, "Bold"),
];

/// Sizes the designs use at scale 1: the Markdown body and code-block sizes.
pub(crate) const DEFAULT_UI_SIZE: u32 = 14;
pub(crate) const DEFAULT_CODE_SIZE: u32 = 12;
/// Chrome rows have fixed heights, so UI text stays within a narrower range.
pub(crate) const UI_SIZES: RangeInclusive<u32> = 11..=18;
pub(crate) const CODE_SIZES: RangeInclusive<u32> = 9..=24;

fn clamp(value: u32, range: &RangeInclusive<u32>) -> u32 {
    value.clamp(*range.start(), *range.end())
}

fn alias(family: Option<&str>, system: &str, weight: u32, size: u32, base: u32) -> FontAlias {
    FontAlias {
        family: family.unwrap_or(system).to_owned().into(),
        fallback: system.into(),
        weight_offset: weight as f32 - REGULAR as f32,
        size_scale: size as f32 / base as f32,
    }
}

/// The alias table the preferences describe.
pub(crate) fn aliases(p: &Preferences) -> [(SharedString, FontAlias); 3] {
    let ui_size = clamp(p.ui_font_size, &UI_SIZES);
    let code_size = clamp(p.code_font_size, &CODE_SIZES);
    let ui = alias(
        p.ui_font_family.as_deref(),
        SYSTEM_UI,
        p.ui_font_weight,
        ui_size,
        DEFAULT_UI_SIZE,
    );
    let content = alias(
        p.content_font_family
            .as_deref()
            .or(p.ui_font_family.as_deref()),
        SYSTEM_UI,
        p.content_font_weight.unwrap_or(p.ui_font_weight),
        ui_size,
        DEFAULT_UI_SIZE,
    );
    let code = alias(
        p.code_font_family.as_deref(),
        SYSTEM_CODE,
        p.code_font_weight,
        code_size,
        DEFAULT_CODE_SIZE,
    );
    [
        (UI.into(), ui),
        (CONTENT.into(), content),
        (CODE.into(), code),
    ]
}

thread_local! {
    /// The chosen content and code families, for HTML that GPUI does not render.
    static WEB: std::cell::RefCell<(Option<String>, Option<String>)> = Default::default();
}

/// Point the aliases at the preferred fonts and repaint every window when
/// anything changed.
pub(crate) fn apply(p: &Preferences, cx: &mut App) {
    let content = p.content_font_family.clone().or(p.ui_font_family.clone());
    WEB.with(|web| *web.borrow_mut() = (content, p.code_font_family.clone()));
    if cx.text_system().set_font_aliases(aliases(p)) {
        cx.refresh_windows();
    }
}

/// CSS `font-family` lists that put the chosen families before `sans` and `mono`.
pub(crate) fn css_families(sans: &str, mono: &str) -> (String, String) {
    let list = |family: &Option<String>, generic: &str| match family {
        // Font names are quoted into HTML, so keep only plain name characters.
        Some(name) => {
            let name: String = name
                .chars()
                .filter(|c| c.is_alphanumeric() || " -_.".contains(*c))
                .collect();
            format!("'{name}',{generic}")
        }
        None => generic.to_owned(),
    };
    WEB.with(|web| {
        let (content, code) = &*web.borrow();
        (list(content, sans), list(code, mono))
    })
}

/// The code font's size multiplier, for elements that size code text
/// themselves instead of reading `Window::text_style`.
pub(crate) fn code_scale(cx: &App) -> f32 {
    cx.text_system().font_size_scale(CODE)
}
/// The size scale of the alias the current text style inherits, for text
/// shaped by hand at a literal size.
pub(crate) fn inherited_scale(window: &Window) -> f32 {
    window
        .text_system()
        .font_size_scale(&window.text_style().font_family)
}

/// Advances whenever fonts change; part of every measured-text cache key.
pub(crate) fn generation(cx: &App) -> usize {
    cx.text_system().font_generation()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Family {
    pub name: SharedString,
    pub mono: bool,
}

/// Installed font families, listed on a worker the first time Settings asks.
#[derive(Default)]
pub(crate) struct Catalog {
    families: Option<Arc<[Family]>>,
    loading: bool,
}
impl Global for Catalog {}

/// The installed families, or `None` while they are still being listed.
pub(crate) fn catalog(cx: &mut App) -> Option<Arc<[Family]>> {
    let state = cx.default_global::<Catalog>();
    if state.families.is_none() && !state.loading {
        state.loading = true;
        let text = cx.text_system().clone();
        let list = cx.background_executor().spawn(async move {
            visible(text.all_font_names())
                .into_iter()
                .map(|name| {
                    // Monospace fonts give a narrow and a wide glyph the same advance.
                    let id = text.resolve_font(&gpui::font(name.clone()));
                    let width = |ch| text.advance(id, px(12.), ch).map(|size| size.width).ok();
                    let mono = matches!(
                        (width('i'), width('M')),
                        (Some(i), Some(m)) if (i - m).abs() < px(0.01)
                    );
                    Family { name, mono }
                })
                .collect::<Arc<[_]>>()
        });
        cx.spawn(async move |cx| {
            let families = list.await;
            cx.update(|cx| {
                cx.global_mut::<Catalog>().families = Some(families);
                cx.refresh_windows();
            })
        })
        .detach();
    }
    cx.global::<Catalog>().families.clone()
}

#[cfg(test)]
pub(crate) fn set_catalog(families: Vec<Family>, cx: &mut App) {
    cx.set_global(Catalog {
        families: Some(families.into()),
        loading: false,
    });
}

/// Families a person can pick: platform-private names start with a dot.
fn visible(names: Vec<String>) -> Vec<SharedString> {
    let mut names: Vec<SharedString> = names
        .into_iter()
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .map(Into::into)
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

/// Families matching a picker query, monospace first when `mono_first`.
pub(crate) fn matching(families: &[Family], query: &str, mono_first: bool) -> Vec<Family> {
    let query = query.trim().to_lowercase();
    let mut found: Vec<Family> = families
        .iter()
        .filter(|family| query.is_empty() || family.name.to_lowercase().contains(&query))
        .cloned()
        .collect();
    if mono_first {
        found.sort_by_key(|family| !family.mono);
    }
    found
}

/// Whether a saved family is still installed; unknown until the catalog loads.
pub(crate) fn installed(families: Option<&[Family]>, name: &str) -> bool {
    families.is_none_or(|families| families.iter().any(|family| family.name == name))
}

/// Parse a typed size, clamped to `range`; `None` keeps the previous value.
pub(crate) fn parse_size(text: &str, range: &RangeInclusive<u32>) -> Option<u32> {
    let value: f32 = text.trim().trim_end_matches("px").trim().parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(clamp(value.round().max(0.) as u32, range))
}

#[cfg(test)]
mod tests;
