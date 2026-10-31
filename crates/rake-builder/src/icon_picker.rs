use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use gpui::{
    Context, FocusHandle, Image, ImageFormat, KeyBinding, PathPromptOptions, Window, actions, div,
    img, prelude::*, px, rgb,
};
use rake_core::Icon;

use crate::{
    Builder,
    form::{ConfigDraft, Field},
};

actions!(icon_picker, [ChooseIcon, FetchIcon]);

#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Loading,
    Failed(String),
}

pub struct IconPicker {
    state: IconState,
    focus: [FocusHandle; 2],
}

#[derive(Default)]
struct IconState {
    preview: Option<Arc<Image>>,
    status: Status,
    request: u64,
}

struct PreparedIcon {
    path: PathBuf,
    preview: Vec<u8>,
}

impl IconPicker {
    pub fn new(cx: &mut Context<Builder>) -> Self {
        Self {
            state: IconState::default(),
            focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
        }
    }

    pub fn url_changed(&mut self) {
        self.state.cancel();
    }
}

impl IconState {
    fn cancel(&mut self) {
        self.request += 1;
        if matches!(self.status, Status::Loading) {
            self.status = Status::Idle;
        }
    }

    fn begin(&mut self) -> u64 {
        self.request += 1;
        self.status = Status::Loading;
        self.request
    }

    fn finish(
        &mut self,
        request: u64,
        result: Result<PreparedIcon, String>,
        draft: &mut ConfigDraft,
    ) {
        if request != self.request {
            return;
        }
        match result {
            Ok(icon) => {
                draft.text[Field::Icon as usize] = icon.path.to_string_lossy().into_owned();
                self.preview = Some(Arc::new(Image::from_bytes(ImageFormat::Png, icon.preview)));
                self.status = Status::Idle;
            }
            Err(error) => {
                draft.text[Field::Icon as usize].clear();
                self.preview = None;
                self.status = Status::Failed(error);
            }
        }
    }
}

fn prepare(page: &str, icon: Option<Icon>, directory: &Path) -> Result<PreparedIcon, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!(
        "{}-{}-{}.icns",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let preview =
        rake_core::prepare_icon(page, icon.as_ref(), &path).map_err(|error| error.to_string())?;
    Ok(PreparedIcon { path, preview })
}

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("space", ChooseIcon, Some("ChooseIcon")),
        KeyBinding::new("enter", ChooseIcon, Some("ChooseIcon")),
        KeyBinding::new("space", FetchIcon, Some("FetchIcon")),
        KeyBinding::new("enter", FetchIcon, Some("FetchIcon")),
    ]);
}

impl Builder {
    fn load_icon(&mut self, icon: Option<Icon>, cx: &mut Context<Self>) {
        let request = self.icon_picker.state.begin();
        let page = self.draft.text[Field::Url as usize].clone();
        let task = cx.background_executor().spawn(async move {
            let home = std::env::var_os("HOME").ok_or("Cannot find the user cache directory")?;
            prepare(
                &page,
                icon,
                &PathBuf::from(home).join("Library/Caches/rake/icons"),
            )
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |builder, cx| {
                builder
                    .icon_picker
                    .state
                    .finish(request, result, &mut builder.draft);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn choose_icon(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose icon".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |builder, cx| {
                    builder.load_icon(Some(Icon::Path(path)), cx)
                });
            }
        })
        .detach();
    }

    pub fn icon_row(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The template's legacy monochrome ICNS placeholder is an opaque white square.
        let preview = self.icon_picker.state.preview.clone().unwrap_or_else(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
                    <rect width="32" height="32" fill="#fff"/>
                </svg>"##
                    .to_vec(),
            ))
        });
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child("Icon")
            .child(img(preview).size(px(48.)));
        for (index, label, context) in [
            (0, "Choose", "ChooseIcon"),
            (
                1,
                if matches!(self.icon_picker.state.status, Status::Loading) {
                    "Loading…"
                } else {
                    "Fetch"
                },
                "FetchIcon",
            ),
        ] {
            row = row.child(
                div()
                    .id(("icon-button", index))
                    .key_context(context)
                    .track_focus(&self.icon_picker.focus[index])
                    .px_3()
                    .py_1()
                    .border_1()
                    .rounded_sm()
                    .border_color(if self.icon_picker.focus[index].is_focused(window) {
                        rgb(0x0066cc)
                    } else {
                        rgb(0x888888)
                    })
                    .bg(rgb(0xffffff))
                    .cursor_pointer()
                    .on_action(
                        cx.listener(|builder, _: &ChooseIcon, _, cx| builder.choose_icon(cx)),
                    )
                    .on_action(
                        cx.listener(|builder, _: &FetchIcon, _, cx| builder.load_icon(None, cx)),
                    )
                    .on_click(cx.listener(move |builder, _, window, cx| {
                        window.focus(&builder.icon_picker.focus[index], cx);
                        if index == 0 {
                            builder.choose_icon(cx);
                        } else {
                            builder.load_icon(None, cx);
                        }
                    }))
                    .child(label),
            );
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(row)
            .when_some(
                match &self.icon_picker.state.status {
                    Status::Failed(error) => Some(error.clone()),
                    _ => None,
                },
                |row, error| row.child(div().text_color(rgb(0xb42318)).child(error)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 132, 25, 96, 12, 0, 71, 202, 7, 249, 103, 89, 110, 183, 0, 0, 0, 0, 73, 69, 78, 68,
        174, 66, 96, 130,
    ];

    fn directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rake-icon-picker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn draft() -> ConfigDraft {
        let mut draft = ConfigDraft::default();
        draft.text[Field::Url as usize] = "https://example.com".into();
        draft
    }

    #[test]
    fn file_pick_updates_preview_and_build_config() {
        let directory = directory();
        let source = directory.join("chosen.png");
        fs::write(&source, PNG).unwrap();
        let mut state = IconState::default();
        let mut draft = draft();
        assert!(state.preview.is_none());
        let request = state.begin();
        assert!(matches!(state.status, Status::Loading));
        let prepared = prepare(
            &draft.text[Field::Url as usize],
            Some(Icon::Path(source)),
            &directory,
        )
        .unwrap();
        let expected = prepared.path.clone();
        let preview = prepared.preview.clone();
        state.finish(request, Ok(prepared), &mut draft);
        assert!(matches!(state.status, Status::Idle));
        assert_eq!(state.preview.as_ref().unwrap().bytes, preview);
        assert_eq!(
            draft.config().unwrap().icon,
            Some(expected.to_string_lossy().into_owned())
        );
        assert!(fs::read(expected).unwrap().starts_with(b"icns"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fetches_local_favicon_and_failure_restores_default() {
        let directory = directory();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let page = format!("http://{}/page", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for expected in ["/page", "/favicon.ico"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..count])
                        .starts_with(&format!("GET {expected} "))
                );
                let body = if expected == "/page" {
                    b"<html></html>".as_slice()
                } else {
                    PNG
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });
        let mut state = IconState::default();
        let mut draft = draft();
        let request = state.begin();
        state.finish(request, prepare(&page, None, &directory), &mut draft);
        server.join().unwrap();
        assert!(matches!(state.status, Status::Idle));
        assert!(
            state
                .preview
                .as_ref()
                .unwrap()
                .bytes
                .starts_with(b"\x89PNG")
        );
        assert!(Path::new(draft.config().unwrap().icon.as_ref().unwrap()).is_file());
        let request = state.begin();
        state.finish(
            request,
            prepare("http://127.0.0.1:1/missing", None, &directory),
            &mut draft,
        );
        assert!(state.preview.is_none());
        assert!(draft.config().unwrap().icon.is_none());
        assert!(
            matches!(&state.status, Status::Failed(message) if message.contains("No usable icon found"))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn an_older_fetch_cannot_replace_a_newer_selection() {
        let mut state = IconState::default();
        let mut draft = draft();
        let old = state.begin();
        let new = state.begin();
        state.finish(
            new,
            Ok(PreparedIcon {
                path: "chosen.icns".into(),
                preview: PNG.into(),
            }),
            &mut draft,
        );
        state.finish(old, Err("Old fetch failed".into()), &mut draft);
        assert_eq!(draft.config().unwrap().icon.as_deref(), Some("chosen.icns"));
        assert_eq!(state.preview.as_ref().unwrap().bytes, PNG);
        assert!(matches!(state.status, Status::Idle));
    }

    #[test]
    fn changing_the_url_ignores_an_in_flight_fetch() {
        let mut state = IconState::default();
        let mut draft = draft();
        let request = state.begin();
        state.cancel();
        state.finish(
            request,
            Ok(PreparedIcon {
                path: "old-site.icns".into(),
                preview: PNG.into(),
            }),
            &mut draft,
        );
        assert!(draft.config().unwrap().icon.is_none());
        assert!(state.preview.is_none());
        assert!(matches!(state.status, Status::Idle));
    }
}
