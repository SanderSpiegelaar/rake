use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Subscription,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};

mod form;
mod icon_picker;
mod input;

use form::{ConfigDraft, Field};
use input::TextInput;

actions!(builder, [NextField, PreviousField, Toggle, Quit]);

struct Builder {
    draft: ConfigDraft,
    inputs: Vec<Entity<TextInput>>,
    toggles: [FocusHandle; 4],
    icon_picker: icon_picker::IconPicker,
    _subscriptions: Vec<Subscription>,
}

impl Builder {
    fn new(cx: &mut Context<Self>) -> Self {
        let draft = ConfigDraft::default();
        let inputs: Vec<_> = Field::ALL
            .iter()
            .map(|field| {
                cx.new(|cx| {
                    TextInput::new(draft.text[*field as usize].clone(), field.is_list(), cx)
                })
            })
            .collect();
        let subscriptions = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                cx.subscribe(input, move |builder, input, _: &input::Changed, cx| {
                    builder.draft.text[index] = input.read(cx).content.to_string();
                    if index == Field::Url as usize {
                        builder.icon_picker.url_changed();
                    }
                    cx.notify();
                })
            })
            .collect();
        Self {
            draft,
            inputs,
            toggles: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            icon_picker: icon_picker::IconPicker::new(cx),
            _subscriptions: subscriptions,
        }
    }

    fn field(&self, field: Field, error: Option<&str>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .min_w_0()
            .child(field.label())
            .child(self.inputs[field as usize].clone())
            .when_some(
                error.filter(|error| error.starts_with(&format!("{}:", field.key()))),
                |field, error| field.child(div().text_color(rgb(0xb42318)).child(error.to_owned())),
            )
    }
}

impl Render for Builder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.draft.config().err();
        let mut form = div().flex().flex_col().gap_3();
        for pair in [
            [Field::Url, Field::Name],
            [Field::Identifier, Field::Icon],
            [Field::Width, Field::Height],
            [Field::MinWidth, Field::MinHeight],
            [Field::Title, Field::UserAgent],
            [Field::Inject, Field::SafeDomains],
        ] {
            form = form.child(div().flex().gap_4().children(pair.map(|field| match field {
                Field::Icon => self.icon_row(window, cx).into_any_element(),
                _ => self.field(field, error.as_deref()).into_any_element(),
            })));
        }
        for (index, (label, checked)) in [
            ("Hide title bar", self.draft.hide_title_bar),
            ("Dark mode", self.draft.dark_mode),
            ("Debug", self.draft.debug),
            ("Install", self.draft.install),
        ]
        .into_iter()
        .enumerate()
        {
            let toggle = cx.listener(move |builder: &mut Self, _: &Toggle, _, cx| {
                let value = match index {
                    0 => &mut builder.draft.hide_title_bar,
                    1 => &mut builder.draft.dark_mode,
                    2 => &mut builder.draft.debug,
                    _ => &mut builder.draft.install,
                };
                *value = !*value;
                cx.notify();
            });
            form = form.child(
                div()
                    .id(("toggle", index))
                    .key_context("Toggle")
                    .track_focus(&self.toggles[index])
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .on_action(toggle)
                    .on_click(cx.listener(move |builder, _, window, cx| {
                        window.focus(&builder.toggles[index], cx);
                        window.dispatch_action(Box::new(Toggle), cx);
                    }))
                    .child(
                        div()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(if self.toggles[index].is_focused(window) {
                                rgb(0x0066cc)
                            } else {
                                rgb(0x888888)
                            })
                            .bg(if checked {
                                rgb(0x0066cc)
                            } else {
                                rgb(0xffffff)
                            })
                            .text_color(rgb(0xffffff))
                            .child(if checked { "✓" } else { "" }),
                    )
                    .child(label),
            );
        }
        div()
            .id("builder")
            .size_full()
            .overflow_y_scroll()
            .on_action(|_: &NextField, window, cx| window.focus_next(cx))
            .on_action(|_: &PreviousField, window, cx| window.focus_prev(cx))
            .p_6()
            .bg(rgb(0xf5f5f5))
            .text_color(rgb(0x202020))
            .text_size(px(14.))
            .child(form)
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        input::bind_keys(cx);
        icon_picker::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("tab", NextField, None),
            KeyBinding::new("shift-tab", PreviousField, None),
            KeyBinding::new("space", Toggle, Some("Toggle")),
            KeyBinding::new("enter", Toggle, Some("Toggle")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(720.0), px(760.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Rake".into()),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(420.), px(400.))),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let builder = Builder::new(cx);
                    window.focus(&builder.inputs[0].focus_handle(cx), cx);
                    builder
                })
            },
        )
        .expect("Cannot open the Rake builder window");
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
