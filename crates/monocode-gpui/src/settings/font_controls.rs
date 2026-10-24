//! The Appearance page's Fonts group: searchable family pickers, weight
//! selects, size fields and a preview of the content and code fonts.
use super::{Preferences, SettingsView, widgets::secondary_button};
use crate::{
    fonts::{self, Family},
    input::{self, Input},
    popover::{Align, Anchor, Side, popover},
    theme::*,
};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, KeyDownEvent, Role, ScrollStrategy,
    SharedString, Subscription, UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use std::{collections::HashMap, ops::RangeInclusive, sync::Arc};

const ROW: f32 = 28.;
const VISIBLE_ROWS: usize = 10;
const FAMILY_WIDTH: f32 = 184.;
const WEIGHT_WIDTH: f32 = 116.;

/// A size preference and the field that edits it.
#[derive(Clone)]
struct SizeField {
    input: Entity<Input>,
    range: RangeInclusive<u32>,
    get: fn(&Preferences) -> u32,
    set: fn(&mut Preferences, u32),
}

/// The open family picker's search, highlight and scroll position.
pub(super) struct State {
    pub(super) open: Option<&'static str>,
    active: usize,
    query: String,
    search: Entity<Input>,
    scroll: UniformListScrollHandle,
    sizes: HashMap<&'static str, SizeField>,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub(super) fn new(cx: &mut Context<SettingsView>) -> Self {
        let search = cx.new(|cx| {
            let mut input = Input::field("Search fonts", "Search fonts", content(0.35), cx);
            input.set_font_size(12.);
            input
        });
        let changed = cx.subscribe(&search, |this, input, _: &input::Changed, cx| {
            this.fonts.query = input.read(cx).content.to_string();
            this.fonts.active = 0;
            this.fonts.scroll.scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        });
        Self {
            open: None,
            active: 0,
            query: String::new(),
            search,
            scroll: UniformListScrollHandle::new(),
            sizes: HashMap::new(),
            _subscriptions: vec![changed],
        }
    }
}

/// One row of a family picker: `None` is the default entry.
#[derive(Clone)]
struct Choice {
    family: Option<SharedString>,
    label: SharedString,
    mono: bool,
}

fn choices(families: &[Family], query: &str, default_label: &str, mono_first: bool) -> Vec<Choice> {
    let default = (query.trim().is_empty()
        || default_label
            .to_lowercase()
            .contains(&query.trim().to_lowercase()))
    .then(|| Choice {
        family: None,
        label: default_label.to_owned().into(),
        mono: false,
    });
    default
        .into_iter()
        .chain(
            fonts::matching(families, query, mono_first)
                .into_iter()
                .map(|family| Choice {
                    label: family.name.clone(),
                    family: Some(family.name),
                    mono: family.mono,
                }),
        )
        .collect()
}

impl SettingsView {
    /// The Fonts group, between Color and Translucency.
    pub(super) fn fonts_group(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.prefs.clone();
        let catalog = fonts::catalog(cx);
        let ui_weight = p.ui_font_weight;
        let ui_family = self.font_select(
            "ui-font",
            "UI font",
            p.ui_font_family.as_deref(),
            "System",
            false,
            catalog.clone(),
            |p, family| p.ui_font_family = family,
            cx,
        );
        let ui_weight_select =
            self.weight_select("ui-font-weight", "UI font weight", ui_weight, cx, |p, w| {
                p.ui_font_weight = w
            });
        let ui_size = self.size_field(
            "ui-font-size",
            "UI font size",
            fonts::UI_SIZES,
            |p| p.ui_font_size,
            |p, v| p.ui_font_size = v,
            window,
            cx,
        );
        let content_family = self.font_select(
            "content-font",
            "Content font",
            p.content_font_family.as_deref(),
            "Same as UI font",
            false,
            catalog.clone(),
            |p, family| p.content_font_family = family,
            cx,
        );
        let content_weight = self.weight_select(
            "content-font-weight",
            "Content font weight",
            p.content_font_weight.unwrap_or(ui_weight),
            cx,
            |p, w| p.content_font_weight = Some(w),
        );
        let code_family = self.font_select(
            "code-font",
            "Code font",
            p.code_font_family.as_deref(),
            "System",
            true,
            catalog,
            |p, family| p.code_font_family = family,
            cx,
        );
        let code_weight = self.weight_select(
            "code-font-weight",
            "Code font weight",
            p.code_font_weight,
            cx,
            |p, w| p.code_font_weight = w,
        );
        let code_size = self.size_field(
            "code-font-size",
            "Code font size",
            fonts::CODE_SIZES,
            |p| p.code_font_size,
            |p, v| p.code_font_size = v,
            window,
            cx,
        );
        let rows = vec![
            self.row(
                Some("ui-font"),
                true,
                "UI font",
                Some("Sidebars, tabs, menus and settings.".into()),
                vec![ui_family, ui_weight_select],
            ),
            self.row(
                Some("ui-font-size"),
                false,
                "UI font size",
                Some("The base size for the interface and conversation text.".into()),
                vec![ui_size],
            ),
            self.row(
                Some("content-font"),
                false,
                "Content font",
                Some("Messages, Markdown and the composer.".into()),
                vec![content_family, content_weight],
            ),
            self.row(
                Some("code-font"),
                false,
                "Code font",
                Some("Code blocks, diffs, the editor and the terminal.".into()),
                vec![code_family, code_weight],
            ),
            self.row(
                Some("code-font-size"),
                false,
                "Code font size",
                Some("The base size for code across chats, diffs and the terminal.".into()),
                vec![code_size],
            ),
            preview(),
        ];
        let mut defaults = p.clone();
        defaults.restore_fonts();
        let reset = (defaults != p).then(|| {
            secondary_button("reset-fonts", "Reset", false, false)
                .child("Reset")
                .on_click(cx.listener(|this, _, _, cx| this.update(cx, |p| p.restore_fonts())))
                .into_any_element()
        });
        self.group(Some("fonts"), false, "Fonts", None, reset, rows)
    }

    fn weight_select(
        &self,
        id: &'static str,
        label: &'static str,
        weight: u32,
        cx: &mut Context<Self>,
        set: fn(&mut Preferences, u32),
    ) -> AnyElement {
        let options = fonts::WEIGHTS
            .iter()
            .map(|&(w, name)| (w.to_string(), SharedString::from(name), None))
            .collect();
        self.select(
            id,
            label,
            &weight.to_string(),
            options,
            Some(WEIGHT_WIDTH),
            cx,
            move |this, v, cx| {
                if let Ok(w) = v.parse() {
                    this.update(cx, |p| set(p, w))
                }
            },
        )
    }

    /// FontSelect: a trigger like Select's opening a searchable, virtualized
    /// list of installed families, each previewed in its own face.
    #[allow(clippy::too_many_arguments)]
    fn font_select(
        &mut self,
        id: &'static str,
        label: &'static str,
        value: Option<&str>,
        default_label: &'static str,
        mono_first: bool,
        catalog: Option<Arc<[Family]>>,
        set: fn(&mut Preferences, Option<String>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.fonts.open == Some(id);
        let slot = self
            .select_anchors
            .borrow_mut()
            .entry(id)
            .or_default()
            .clone();
        let missing = value.filter(|name| !fonts::installed(catalog.as_deref(), name));
        let trigger_label: SharedString = match (value, missing) {
            (_, Some(name)) => format!("{name} — not installed").into(),
            (Some(name), None) => name.to_owned().into(),
            (None, _) => default_label.into(),
        };
        let current = value.map(str::to_owned);
        let trigger = div()
            .id(SharedString::from(format!("font-select-{id}")))
            .debug_selector(move || format!("settings-font-select-{id}"))
            .role(Role::ComboBox)
            .aria_label(format!("{label}: {trigger_label}"))
            .aria_expanded(open)
            .relative()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(content(0.1))
            .bg(content(0.05))
            .px_2()
            .py_1()
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(if missing.is_some() {
                super::widgets::red(0.9)
            } else {
                content(1.)
            })
            .cursor_pointer()
            .hover(|s| s.border_color(content(0.2)))
            .child(slot.probe())
            .child(div().min_w_0().flex_1().truncate().child(trigger_label))
            .child(
                icon("chevron-down-hi", 14.)
                    .text_color(content(0.5))
                    .when(open, |s| {
                        s.with_transformation(gpui::Transformation::rotate(gpui::percentage(0.5)))
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.fonts.open == Some(id) {
                    this.close_font_select(window, cx);
                    return;
                }
                this.select_open = None;
                this.fonts.open = Some(id);
                this.fonts.query.clear();
                this.fonts
                    .search
                    .update(cx, |i, cx| i.set_text(String::new(), cx));
                let families = fonts::catalog(cx).unwrap_or_default();
                let list = choices(&families, "", default_label, mono_first);
                this.fonts.active = list
                    .iter()
                    .position(|c| c.family.as_deref() == current.as_deref())
                    .unwrap_or(0);
                this.fonts
                    .scroll
                    .scroll_to_item(this.fonts.active, ScrollStrategy::Center);
                window.focus(&this.fonts.search.focus_handle(cx), cx);
                cx.notify();
            }));
        let picker = open.then(|| {
            let list: Arc<[Choice]> = choices(
                catalog.as_deref().unwrap_or_default(),
                &self.fonts.query,
                default_label,
                mono_first,
            )
            .into();
            self.font_popover(id, label, slot, list, catalog.is_none(), value, set, cx)
        });
        div()
            .relative()
            .w(px(FAMILY_WIDTH))
            .child(trigger)
            .children(picker)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn font_popover(
        &self,
        id: &'static str,
        label: &'static str,
        slot: crate::popover::AnchorSlot,
        list: Arc<[Choice]>,
        loading: bool,
        value: Option<&str>,
        set: fn(&mut Preferences, Option<String>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.weak_entity();
        let count = list.len();
        let active = self.fonts.active.min(count.saturating_sub(1));
        let selected = value.map(SharedString::from);
        let rows = {
            let list = list.clone();
            let weak = weak.clone();
            uniform_list(
                SharedString::from(format!("font-list-{id}")),
                count,
                move |range, _, _| {
                    range
                        .map(|ix| {
                            let choice = list[ix].clone();
                            let is_selected = choice.family == selected;
                            let weak = weak.clone();
                            let hover = weak.clone();
                            div()
                                .id(("font-option", ix))
                                .debug_selector(move || format!("settings-font-option-{id}-{ix}"))
                                .role(Role::ListBoxOption)
                                .aria_label(choice.label.clone())
                                .aria_selected(is_selected)
                                .h(px(ROW))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .rounded_lg()
                                .px_2()
                                .text_size(px(12.))
                                .text_color(content(1.))
                                .cursor_pointer()
                                .when(ix == active || is_selected, |d| d.bg(content(0.1)))
                                .when(ix != active && !is_selected, |d| {
                                    d.hover(|s| s.bg(content(0.05)))
                                })
                                .child(div().min_w_0().flex_1().truncate().child(choice.label))
                                .when(choice.mono, |d| {
                                    d.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(10.))
                                            .text_color(content(0.4))
                                            .child("Mono"),
                                    )
                                })
                                .when_some(choice.family.clone(), |d, family| {
                                    // Each family previews in its own face.
                                    d.child(
                                        div()
                                            .flex_shrink_0()
                                            .font_family(family)
                                            .text_size(px(13.))
                                            .text_color(content(0.6))
                                            .child("Ag"),
                                    )
                                })
                                .when(is_selected, |d| {
                                    d.child(icon("check-hi", 14.).text_color(content(1.)))
                                })
                                .on_mouse_move(move |_, _, cx| {
                                    let _ = hover.update(cx, |this, cx| {
                                        if this.fonts.active != ix {
                                            this.fonts.active = ix;
                                            cx.notify();
                                        }
                                    });
                                })
                                .on_click(move |_, window, cx| {
                                    let family = choice.family.as_ref().map(ToString::to_string);
                                    let _ = weak.update(cx, |this, cx| {
                                        this.update(cx, |p| set(p, family));
                                        this.close_font_select(window, cx);
                                    });
                                })
                                .into_any_element()
                        })
                        .collect()
                },
            )
            .track_scroll(&self.fonts.scroll)
            .h(px(ROW * count.clamp(1, VISIBLE_ROWS) as f32))
        };
        let keys = list.clone();
        popover(
            SharedString::from(format!("font-popover-{id}")),
            Anchor::Slot(slot),
        )
        .side(Side::Bottom)
        .align(Align::End)
        .width(280.)
        .padding(4.)
        .on_dismiss(move |window, cx| {
            let _ = weak.update(cx, |this, cx| this.close_font_select(window, cx));
        })
        .child(
            div()
                .id(SharedString::from(format!("font-listbox-{id}")))
                .role(Role::ListBox)
                .aria_label(label)
                .flex()
                .flex_col()
                .gap_1()
                .capture_action(cx.listener(|this, _: &input::Up, _, cx| {
                    this.fonts.active = this.fonts.active.saturating_sub(1);
                    this.fonts
                        .scroll
                        .scroll_to_item(this.fonts.active, ScrollStrategy::Nearest);
                    cx.stop_propagation();
                    cx.notify();
                }))
                .capture_action(cx.listener(move |this, _: &input::Down, _, cx| {
                    this.fonts.active = (this.fonts.active + 1).min(count.saturating_sub(1));
                    this.fonts
                        .scroll
                        .scroll_to_item(this.fonts.active, ScrollStrategy::Nearest);
                    cx.stop_propagation();
                    cx.notify();
                }))
                .capture_action(cx.listener(move |this, _: &input::Submit, window, cx| {
                    if let Some(choice) = keys.get(this.fonts.active) {
                        let family = choice.family.as_ref().map(ToString::to_string);
                        this.update(cx, |p| set(p, family));
                    }
                    this.close_font_select(window, cx);
                    cx.stop_propagation();
                }))
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.close_font_select(window, cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .h(px(28.))
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .border_1()
                        .border_color(content(0.1))
                        .child(div().flex_1().min_w_0().child(self.fonts.search.clone())),
                )
                .map(|d| {
                    if loading || count == 0 {
                        d.child(
                            div()
                                .px_2()
                                .py(px(6.))
                                .text_size(px(12.))
                                .text_color(content(0.45))
                                .child(if loading {
                                    "Loading fonts…"
                                } else {
                                    "No matching fonts"
                                }),
                        )
                    } else {
                        d.child(rows)
                    }
                }),
        )
        .into_any_element()
    }

    fn close_font_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fonts.open = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// SizeField: a short number field and its unit. Up and Down step by one,
    /// or two with Shift; a size commits on Enter or blur, clamped to `range`.
    #[allow(clippy::too_many_arguments)]
    fn size_field(
        &mut self,
        id: &'static str,
        label: &'static str,
        range: RangeInclusive<u32>,
        get: fn(&Preferences) -> u32,
        set: fn(&mut Preferences, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = match self.fonts.sizes.get(id) {
            Some(field) => field.clone(),
            None => {
                let input = cx.new(|cx| {
                    let mut input = Input::field(label, "", content(0.35), cx);
                    input.set_font_size(13.);
                    input
                });
                let focus = input.focus_handle(cx);
                let submitted = cx.subscribe(&input, move |this, _, _: &input::Submitted, cx| {
                    this.commit_size(id, cx)
                });
                let blurred =
                    cx.on_blur(&focus, window, move |this, _, cx| this.commit_size(id, cx));
                self.fonts._subscriptions.extend([submitted, blurred]);
                let field = SizeField {
                    input,
                    range,
                    get,
                    set,
                };
                self.fonts.sizes.insert(id, field.clone());
                field
            }
        };
        let value = (field.get)(&self.prefs).clamp(*field.range.start(), *field.range.end());
        let focus: FocusHandle = field.input.focus_handle(cx);
        // Show the saved value unless someone is typing.
        if !focus.is_focused(window) && field.input.read(cx).content.as_ref() != value.to_string() {
            field
                .input
                .update(cx, |i, cx| i.set_text(value.to_string(), cx));
        }
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id(SharedString::from(format!("size-field-{id}")))
                    .debug_selector(move || format!("settings-size-{id}"))
                    .w(px(56.))
                    .h(px(28.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(if focus.is_focused(window) {
                        content(0.3)
                    } else {
                        content(0.1)
                    })
                    .bg(content(0.05))
                    .capture_action(cx.listener(move |this, _: &input::Up, _, cx| {
                        this.step_size(id, 1, cx);
                        cx.stop_propagation();
                    }))
                    .capture_action(cx.listener(move |this, _: &input::Down, _, cx| {
                        this.step_size(id, -1, cx);
                        cx.stop_propagation();
                    }))
                    .capture_action(cx.listener(move |this, _: &input::SelectUp, _, cx| {
                        this.step_size(id, 2, cx);
                        cx.stop_propagation();
                    }))
                    .capture_action(cx.listener(move |this, _: &input::SelectDown, _, cx| {
                        this.step_size(id, -2, cx);
                        cx.stop_propagation();
                    }))
                    .child(div().flex_1().min_w_0().child(field.input.clone())),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(content(0.45))
                    .child("px"),
            )
            .into_any_element()
    }

    fn commit_size(&mut self, id: &'static str, cx: &mut Context<Self>) {
        let Some(field) = self.fonts.sizes.get(id).cloned() else {
            return;
        };
        let text = field.input.read(cx).content.to_string();
        let current = (field.get)(&self.prefs);
        // Invalid input shows the stored size again; clamped input its bound.
        let value = fonts::parse_size(&text, &field.range)
            .unwrap_or_else(|| current.clamp(*field.range.start(), *field.range.end()));
        if value != current {
            self.update(cx, |p| (field.set)(p, value));
        }
        if text != value.to_string() {
            field
                .input
                .update(cx, |i, cx| i.set_text(value.to_string(), cx));
        }
    }

    fn step_size(&mut self, id: &'static str, delta: i64, cx: &mut Context<Self>) {
        let Some(field) = self.fonts.sizes.get(id).cloned() else {
            return;
        };
        let (low, high) = (*field.range.start() as i64, *field.range.end() as i64);
        let next = ((field.get)(&self.prefs) as i64 + delta).clamp(low, high) as u32;
        field.input.update(cx, |i, cx| {
            i.set_text(next.to_string(), cx);
            i.select_range(0..i.content.len(), cx);
        });
        if next != (field.get)(&self.prefs) {
            self.update(cx, |p| (field.set)(p, next));
        }
    }
}

/// A conversation reply and a code block in the content and code fonts. The
/// rest of Settings already shows the UI font.
fn preview() -> AnyElement {
    let code = [
        "fn greet(name: &str) {",
        "    println!(\"Hello, {name}!\");",
        "}",
    ];
    div()
        .debug_selector(|| "settings-font-preview".into())
        .px_4()
        .py(px(14.))
        .border_t_1()
        .border_color(content(0.05))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .font_family(fonts::CONTENT)
                .text_size(px(14.))
                .line_height(px(24.))
                .text_color(content(0.9))
                .child("The quick brown fox jumps over the lazy dog."),
        )
        .child(
            div()
                .rounded_lg()
                .bg(content(0.05))
                .px_3()
                .py_2()
                .font_family(fonts::CODE)
                .text_size(px(12.))
                .line_height(px(20.))
                .text_color(content(0.8))
                .children(code.map(|line| div().whitespace_nowrap().child(line))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{SettingsContext, model::Section};
    use gpui::{
        Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowHandle, WindowOptions,
        point, size,
    };

    fn window(cx: &mut TestAppContext) -> WindowHandle<SettingsView> {
        let window = cx.update(|cx| {
            fonts::set_catalog(vec![family("Avenir", false), family("Menlo", true)], cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1200.), px(2400.)),
                    ))),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|cx| {
                        SettingsView::new(
                            Section::Appearance,
                            Preferences::default(),
                            SettingsContext::default(),
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        cx.refresh().unwrap();
        cx.run_until_parked();
        window
    }

    #[gpui::test]
    fn size_fields_commit_clamped_values_and_revert_invalid_text(cx: &mut TestAppContext) {
        let window = window(cx);
        let visual = &mut VisualTestContext::from_window(window.into(), cx);
        assert!(visual.debug_bounds("settings-size-ui-font-size").is_some());
        assert!(visual.debug_bounds("settings-font-preview").is_some());
        let typed = |text: &'static str, visual: &mut VisualTestContext| {
            window
                .update(visual, |view, _, cx| {
                    let input = view.fonts.sizes["ui-font-size"].input.clone();
                    input.update(cx, |i, cx| i.set_text(text.into(), cx));
                    view.commit_size("ui-font-size", cx);
                    (view.prefs.ui_font_size, input.read(cx).content.to_string())
                })
                .unwrap()
        };
        assert_eq!(typed("16", visual), (16, "16".into()));
        assert_eq!(typed("99", visual), (*fonts::UI_SIZES.end(), "18".into()));
        assert_eq!(typed("big", visual), (18, "18".into()));
        let code = window
            .update(visual, |view, _, cx| {
                view.step_size("code-font-size", 1, cx);
                view.step_size("code-font-size", -3, cx);
                view.prefs.code_font_size
            })
            .unwrap();
        assert_eq!(code, 10);
    }

    #[gpui::test]
    fn the_family_picker_searches_and_picks_with_the_keyboard(cx: &mut TestAppContext) {
        cx.update(crate::input::bind);
        let window = window(cx);
        let visual = &mut VisualTestContext::from_window(window.into(), cx);
        let trigger = visual
            .debug_bounds("settings-font-select-code-font")
            .unwrap();
        visual.simulate_click(trigger.center(), Default::default());
        visual.run_until_parked();
        // Monospace families lead the code font list, after System.
        assert!(
            visual
                .debug_bounds("settings-font-option-code-font-1")
                .is_some()
        );
        visual.simulate_keystrokes("down enter");
        visual.run_until_parked();
        window
            .update(visual, |view, _, _| {
                assert_eq!(view.prefs.code_font_family.as_deref(), Some("Menlo"));
                assert_eq!(view.fonts.open, None);
            })
            .unwrap();

        visual.simulate_click(trigger.center(), Default::default());
        visual.simulate_keystrokes("a v e enter");
        visual.run_until_parked();
        window
            .update(visual, |view, _, _| {
                assert_eq!(view.prefs.code_font_family.as_deref(), Some("Avenir"))
            })
            .unwrap();
    }

    fn family(name: &str, mono: bool) -> Family {
        Family {
            name: name.to_owned().into(),
            mono,
        }
    }

    #[test]
    fn the_default_entry_leads_and_filters_with_the_query() {
        let families = [family("Avenir", false), family("Menlo", true)];
        let labels = |list: Vec<Choice>| list.into_iter().map(|c| c.label).collect::<Vec<_>>();
        assert_eq!(
            labels(choices(&families, "", "System", true)),
            ["System", "Menlo", "Avenir"]
        );
        assert_eq!(labels(choices(&families, "men", "System", true)), ["Menlo"]);
        assert_eq!(
            labels(choices(&families, "same", "Same as UI font", false)),
            ["Same as UI font"]
        );
        assert_eq!(choices(&families, "", "System", false)[0].family, None);
    }
}
