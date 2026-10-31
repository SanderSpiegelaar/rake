use gpui::{Context, KeyBinding, PathPromptOptions, Window, actions, div, prelude::*, rgb};

use crate::{Builder, form::ConfigDraft};

actions!(config_file, [ActivateConfigFile]);

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("enter", ActivateConfigFile, Some("ConfigFile")),
        KeyBinding::new("space", ActivateConfigFile, Some("ConfigFile")),
    ]);
}

impl Builder {
    pub fn config_file_controls(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(["Import", "Export"].into_iter().enumerate().map(
                        |(index, label)| {
                            div()
                                .id(("config-file", index))
                                .key_context("ConfigFile")
                                .track_focus(&self.config_file_focus[index])
                                .px_3()
                                .py_1()
                                .border_1()
                                .border_color(if self.config_file_focus[index].is_focused(window) {
                                    rgb(0x0066cc)
                                } else {
                                    rgb(0x888888)
                                })
                                .rounded_md()
                                .cursor_pointer()
                                .on_action(cx.listener(
                                    move |builder, _: &ActivateConfigFile, _, cx| {
                                        builder.prompt_config_file(index == 0, cx);
                                    },
                                ))
                                .on_click(cx.listener(move |builder, _, _, cx| {
                                    builder.prompt_config_file(index == 0, cx);
                                }))
                                .child(label)
                        },
                    )),
            )
            .when_some(self.config_file_error.clone(), |controls, error| {
                controls.child(div().text_color(rgb(0xb42318)).child(error))
            })
    }

    fn prompt_config_file(&mut self, import: bool, cx: &mut Context<Self>) {
        self.config_file_error = None;
        if import {
            let paths = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Import".into()),
            });
            cx.spawn(async move |this, cx| {
                let result = match paths.await {
                    Ok(Ok(Some(paths))) => paths.first().map(|path| ConfigDraft::import(path)),
                    Ok(Ok(None)) => None,
                    Ok(Err(error)) => Some(Err(error.to_string())),
                    Err(error) => Some(Err(error.to_string())),
                };
                if let Some(result) = result {
                    this.update(cx, |builder, cx| {
                        match result {
                            Ok(mut draft) => {
                                draft.install = builder.draft.install;
                                for (input, value) in builder.inputs.iter().zip(&draft.text) {
                                    input.update(cx, |input, cx| {
                                        input.set_content(value.clone(), cx)
                                    });
                                }
                                builder.draft = draft;
                            }
                            Err(error) => builder.config_file_error = Some(error),
                        }
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        } else {
            let draft = self.draft.clone();
            if let Err(error) = draft.config() {
                self.config_file_error = Some(error);
                cx.notify();
                return;
            }
            let directory = match std::env::current_dir() {
                Ok(directory) => directory,
                Err(error) => {
                    self.config_file_error = Some(error.to_string());
                    cx.notify();
                    return;
                }
            };
            let path = cx.prompt_for_new_path(&directory, Some("rake.json"));
            cx.spawn(async move |this, cx| {
                let result = match path.await {
                    Ok(Ok(Some(path))) => Some(draft.export(&path)),
                    Ok(Ok(None)) => None,
                    Ok(Err(error)) => Some(Err(error.to_string())),
                    Err(error) => Some(Err(error.to_string())),
                };
                if let Some(result) = result {
                    this.update(cx, |builder, cx| {
                        builder.config_file_error = result.err();
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        }
        cx.notify();
    }
}
