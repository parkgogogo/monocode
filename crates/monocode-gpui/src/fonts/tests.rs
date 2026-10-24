use super::*;
use gpui::{
    AnyElement, Bounds, Context, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId,
    IntoElement, LineLayout, ListAlignment, ListOffset, ListState, NoopTextSystem, ParentElement,
    Pixels, PlatformTextSystem, Render, RenderGlyphParams, Size, Styled, TestAppContext,
    TextRenderingMode, TextSystem, Window, canvas, div, list, point, prelude::*, size,
};
use std::{borrow::Cow, cell::Cell, rc::Rc, sync::Mutex};

/// Records requested fonts and fails to load the family "Missing".
#[derive(Default)]
struct Recording(Mutex<Vec<Font>>);

impl PlatformTextSystem for Recording {
    fn add_fonts(&self, _: Vec<Cow<'static, [u8]>>) -> gpui::Result<()> {
        Ok(())
    }
    fn all_font_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn font_id(&self, font: &Font) -> gpui::Result<FontId> {
        let mut requested = self.0.lock().unwrap();
        requested.push(font.clone());
        if font.family == "Missing" {
            return Err(std::io::Error::other("missing").into());
        }
        Ok(FontId(requested.len()))
    }
    fn font_metrics(&self, id: FontId) -> FontMetrics {
        NoopTextSystem.font_metrics(id)
    }
    fn typographic_bounds(&self, id: FontId, glyph: GlyphId) -> gpui::Result<Bounds<f32>> {
        NoopTextSystem.typographic_bounds(id, glyph)
    }
    fn advance(&self, id: FontId, glyph: GlyphId) -> gpui::Result<Size<f32>> {
        NoopTextSystem.advance(id, glyph)
    }
    fn glyph_for_char(&self, id: FontId, ch: char) -> Option<GlyphId> {
        NoopTextSystem.glyph_for_char(id, ch)
    }
    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> gpui::Result<Bounds<DevicePixels>> {
        NoopTextSystem.glyph_raster_bounds(params)
    }
    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        bounds: Bounds<DevicePixels>,
    ) -> gpui::Result<(Size<DevicePixels>, Vec<u8>)> {
        NoopTextSystem.rasterize_glyph(params, bounds)
    }
    fn layout_line(&self, text: &str, size: Pixels, runs: &[FontRun]) -> LineLayout {
        NoopTextSystem.layout_line(text, size, runs)
    }
    fn recommended_rendering_mode(&self, id: FontId, size: Pixels) -> TextRenderingMode {
        NoopTextSystem.recommended_rendering_mode(id, size)
    }
    fn glyph_dilation_for_color(&self, _: gpui::Hsla) -> u8 {
        4
    }
}

fn recording() -> (Arc<Recording>, TextSystem) {
    let platform = Arc::new(Recording::default());
    (platform.clone(), TextSystem::new(platform))
}

fn requested(platform: &Recording) -> Vec<(SharedString, f32)> {
    platform
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|font| (font.family.clone(), font.weight.0))
        .collect()
}

fn ui(alias: FontAlias) -> [(SharedString, FontAlias); 1] {
    [(UI.into(), alias)]
}

#[test]
fn alias_resolves_to_its_family_with_shifted_clamped_weights() {
    let (platform, text) = recording();
    text.set_font_aliases(ui(FontAlias {
        weight_offset: 200.,
        ..FontAlias::new("Inter")
    }));
    text.resolve_font(&gpui::font(UI));
    text.resolve_font(&Font {
        weight: gpui::FontWeight::BOLD,
        ..gpui::font(UI)
    });
    assert_eq!(
        requested(&platform),
        vec![("Inter".into(), 600.), ("Inter".into(), 900.)]
    );
}

#[test]
fn an_uninstalled_family_falls_back_to_the_system_font() {
    let (platform, text) = recording();
    text.set_font_aliases(ui(FontAlias {
        fallback: SYSTEM_CODE.into(),
        ..FontAlias::new("Missing")
    }));
    text.resolve_font(&gpui::font(UI));
    assert_eq!(
        requested(&platform),
        vec![("Missing".into(), 400.), (SYSTEM_CODE.into(), 400.)]
    );
}

#[test]
fn changing_aliases_invalidates_resolution_and_bumps_the_generation() {
    let (platform, text) = recording();
    assert!(text.set_font_aliases(ui(FontAlias::new("Inter"))));
    assert!(!text.set_font_aliases(ui(FontAlias::new("Inter"))));
    let generation = text.font_generation();
    text.resolve_font(&gpui::font(UI));
    text.resolve_font(&gpui::font(UI));
    assert_eq!(platform.0.lock().unwrap().len(), 1, "resolution is cached");

    assert!(text.set_font_aliases(ui(FontAlias {
        size_scale: 1.25,
        ..FontAlias::new("Geist")
    })));
    assert!(text.font_generation() > generation);
    assert_eq!(text.font_size_scale(UI), 1.25);
    assert_eq!(text.font_size_scale("Inter"), 1.);
    text.resolve_font(&gpui::font(UI));
    assert_eq!(requested(&platform).last().unwrap().0, "Geist");
}

#[test]
fn turning_font_smoothing_off_drops_glyph_dilation() {
    let (_, text) = recording();
    let white = gpui::white();
    assert!(
        text.font_smoothing(),
        "the framework keeps platform smoothing"
    );
    assert_eq!(text.glyph_dilation_for_color(white), 4);

    assert!(text.set_font_smoothing(false));
    assert!(!text.set_font_smoothing(false));
    assert_eq!(text.glyph_dilation_for_color(white), 0);

    assert!(text.set_font_smoothing(true));
    assert_eq!(text.glyph_dilation_for_color(white), 4);
}

#[gpui::test]
fn preferences_turn_font_smoothing_off_by_default(cx: &mut TestAppContext) {
    cx.update(|cx| {
        apply(&Preferences::default(), cx);
        assert!(!cx.text_system().font_smoothing());
        apply(
            &Preferences {
                font_smoothing: true,
                ..Preferences::default()
            },
            cx,
        );
        assert!(cx.text_system().font_smoothing());
    });
}

#[test]
fn default_preferences_keep_todays_fonts_and_sizes() {
    let [(_, ui), (_, content), (_, code)] = aliases(&Preferences::default());
    for alias in [&ui, &content] {
        assert_eq!(alias.family, SYSTEM_UI);
        assert_eq!(alias.size_scale, 1.);
        assert_eq!(alias.weight_offset, 0.);
    }
    assert_eq!(code.family, SYSTEM_CODE);
    assert_eq!(code.size_scale, 1.);
}

#[test]
fn content_follows_the_ui_font_until_chosen_and_sizes_clamp() {
    let mut p = Preferences {
        ui_font_family: Some("Inter".into()),
        ui_font_weight: 500,
        ui_font_size: 40,
        code_font_family: Some("JetBrains Mono".into()),
        code_font_size: 18,
        ..Preferences::default()
    };
    let [(_, ui), (_, content), (_, code)] = aliases(&p);
    assert_eq!(content, ui);
    assert_eq!(ui.size_scale, *UI_SIZES.end() as f32 / 14.);
    assert_eq!(ui.weight_offset, 100.);
    assert_eq!(
        (code.family.as_ref(), code.fallback.as_ref()),
        ("JetBrains Mono", SYSTEM_CODE)
    );
    assert_eq!(code.size_scale, 1.5);

    p.content_font_family = Some("Charter".into());
    p.content_font_weight = Some(300);
    let [_, (_, content), _] = aliases(&p);
    assert_eq!(content.family, "Charter");
    assert_eq!(content.weight_offset, -100.);
}

#[test]
fn saved_workspaces_without_font_fields_load_defaults() {
    let p: Preferences = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
    assert_eq!(p.ui_font_size, DEFAULT_UI_SIZE);
    assert_eq!(p.code_font_size, DEFAULT_CODE_SIZE);
    assert_eq!(p.ui_font_weight, REGULAR);
    assert_eq!(p.content_font_family, None);
    assert!(!p.font_smoothing);

    let mut p = Preferences {
        ui_font_family: Some("Inter".into()),
        code_font_size: 20,
        font_smoothing: true,
        ..p
    };
    p.restore_appearance();
    assert!(!p.font_smoothing);
    assert_eq!(aliases(&p), aliases(&Preferences::default()));
}

#[test]
fn typed_sizes_parse_round_and_clamp() {
    assert_eq!(parse_size("15", &UI_SIZES), Some(15));
    assert_eq!(parse_size(" 15.6 px", &UI_SIZES), Some(16));
    assert_eq!(parse_size("4", &UI_SIZES), Some(11));
    assert_eq!(parse_size("99", &CODE_SIZES), Some(24));
    assert_eq!(parse_size("", &UI_SIZES), None);
    assert_eq!(parse_size("big", &UI_SIZES), None);
    assert_eq!(parse_size("NaN", &UI_SIZES), None);
}

#[test]
fn the_catalog_hides_private_families_and_orders_monospace_first() {
    assert_eq!(
        visible(vec![
            "Menlo".into(),
            ".AppleSystemUIFont".into(),
            "avenir".into(),
            "Menlo".into(),
            String::new(),
        ]),
        vec![SharedString::from("avenir"), "Menlo".into()]
    );
    let families = [
        Family {
            name: "Avenir".into(),
            mono: false,
        },
        Family {
            name: "Menlo".into(),
            mono: true,
        },
    ];
    let names = |found: Vec<Family>| found.into_iter().map(|f| f.name).collect::<Vec<_>>();
    assert_eq!(
        names(matching(&families, "", true)),
        vec![SharedString::from("Menlo"), "Avenir".into()]
    );
    assert_eq!(
        names(matching(&families, "AVE", true)),
        vec![SharedString::from("Avenir")]
    );
    assert!(installed(Some(&families), "Menlo"));
    assert!(!installed(Some(&families), "Gone"));
    assert!(installed(None, "Gone"), "unknown until listed");
}

struct Probe(Rc<Cell<(Pixels, Pixels)>>);
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.0.clone();
        div()
            .font_family(CODE)
            .text_size(px(12.))
            .line_height(px(20.))
            .child(
                canvas(
                    move |_, window, _| {
                        let style = window.text_style();
                        seen.set((
                            style.font_size.to_pixels(window.rem_size()),
                            style.line_height_in_pixels(window.rem_size()),
                        ));
                    },
                    |_, _, _, _| {},
                )
                .size_full(),
            )
    }
}

#[gpui::test]
fn text_styles_scale_with_their_alias(cx: &mut TestAppContext) {
    let seen = Rc::new(Cell::new((px(0.), px(0.))));
    cx.update(|cx| {
        apply(&Preferences::default(), cx);
        cx.open_window(Default::default(), |_, cx| cx.new(|_| Probe(seen.clone())))
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(seen.get(), (px(12.), px(20.)));

    cx.update(|cx| {
        apply(
            &Preferences {
                code_font_size: 18,
                ..Preferences::default()
            },
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(seen.get(), (px(18.), px(30.)));
}

struct Rows {
    state: ListState,
    height: Rc<Cell<f32>>,
}
impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let height = self.height.get();
        list(self.state.clone(), move |_, _, _| -> AnyElement {
            div().h(px(height)).w_full().into_any_element()
        })
        .size_full()
    }
}

#[gpui::test]
fn lists_remeasure_proportionally_when_fonts_change(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let height = Rc::new(Cell::new(100.));
    let state = ListState::new(20, ListAlignment::Top, px(10.));
    let view = cx.update(|_, cx| {
        cx.new(|_| Rows {
            state: state.clone(),
            height: height.clone(),
        })
    });
    state.scroll_to(ListOffset {
        item_ix: 5,
        offset_in_item: px(40.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    // Rows grow with the font, but nothing calls remeasure.
    height.set(200.);
    cx.update(|_, cx| {
        apply(
            &Preferences {
                ui_font_size: 16,
                ..Preferences::default()
            },
            cx,
        )
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    let top = state.logical_scroll_top();
    assert_eq!((top.item_ix, top.offset_in_item), (5, px(80.)));
}
