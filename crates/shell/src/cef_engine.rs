//! The Engine on the Dev host: stock CEF rendered off-screen, frames copied to the CPU (ADR 0001).

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use cef::*;
use m4tv_shell::{
    apps::AppDefinition,
    engine::{Engine, Frame},
    shell::Key,
};

// shortcut: the page is laid out at a fixed size; resize the browser with the window when the
// Pi build needs 1080p.
const VIEW: (i32, i32) = (1280, 720);

#[derive(Default)]
struct Shared {
    frame: Option<Frame>,
    browser: Option<Browser>,
}

wrap_render_handler! {
    struct Render {
        shared: Arc<Mutex<Shared>>,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                *rect = Rect { x: 0, y: 0, width: VIEW.0, height: VIEW.1 };
            }
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: std::os::raw::c_int,
            height: std::os::raw::c_int,
        ) {
            // Popups (select boxes) arrive as a second element; they wait for their own ticket.
            if type_ != PaintElementType::VIEW || buffer.is_null() {
                return;
            }
            let bgra = unsafe { std::slice::from_raw_parts(buffer, width as usize * height as usize * 4) };
            // Replacing the frame drops the previous one, so nothing accumulates.
            self.shared.lock().unwrap().frame =
                Some(Frame { width: width as u32, height: height as u32, bgra: bgra.to_vec() });
        }
    }
}

wrap_life_span_handler! {
    struct LifeSpan {
        shared: Arc<Mutex<Shared>>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            self.shared.lock().unwrap().browser = browser.cloned();
        }
    }
}

wrap_client! {
    struct AppClient {
        render: RenderHandler,
        life_span: LifeSpanHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }
    }
}

wrap_app! {
    struct ShellApp;

    impl App {}
}

pub struct CefEngine {
    shared: Arc<Mutex<Shared>>,
    cache_root: PathBuf,
    /// One request context per App id: its own cookies and storage, kept on disk under the id.
    contexts: HashMap<String, RequestContext>,
}

/// Runs CEF's sub-processes (which are this same executable) and starts CEF in the main one.
/// A sub-process exits here, before any window exists.
pub fn start(cache_root: PathBuf) -> CefEngine {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::new();
    let mut app = ShellApp::new();
    let exit_code =
        execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if exit_code >= 0 {
        std::process::exit(exit_code);
    }
    let cache_root = std::path::absolute(&cache_root).expect("cache path is absolute-able");
    let settings = Settings {
        no_sandbox: 1,
        windowless_rendering_enabled: 1,
        root_cache_path: cache_root.to_string_lossy().as_ref().into(),
        persist_session_cookies: 1,
        log_file: cache_root.join("cef.log").to_string_lossy().as_ref().into(),
        ..Default::default()
    };
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut()
        ),
        1,
        "CEF failed to start; is libcef.dll next to the executable or on PATH?"
    );
    CefEngine { shared: Arc::default(), cache_root, contexts: HashMap::new() }
}

/// Lets CEF do its pending work; call on the thread that started it, many times a second.
pub fn pump() {
    do_message_loop_work();
}

pub fn shutdown_cef() {
    shutdown();
}

impl Engine for CefEngine {
    fn open(&mut self, app: &AppDefinition) {
        let cache_path = self.cache_root.join(&app.id);
        let context = self.contexts.entry(app.id.clone()).or_insert_with(|| {
            let settings = RequestContextSettings {
                cache_path: cache_path.to_string_lossy().as_ref().into(),
                persist_session_cookies: 1,
                ..Default::default()
            };
            request_context_create_context(Some(&settings), None).expect("request context")
        });
        let mut client =
            AppClient::new(Render::new(self.shared.clone()), LifeSpan::new(self.shared.clone()));
        let window_info = WindowInfo::default().set_as_windowless(Default::default());
        browser_host_create_browser(
            Some(&window_info),
            Some(&mut client),
            Some(&CefString::from(app.start_url.as_str())),
            Some(&BrowserSettings { windowless_frame_rate: 30, ..Default::default() }),
            None,
            Some(context),
        );
    }

    fn close(&mut self) {
        let mut shared = self.shared.lock().unwrap();
        if let Some(host) = shared.browser.take().and_then(|b| b.host()) {
            host.close_browser(1);
        }
        shared.frame = None;
    }

    fn key(&mut self, key: Key) {
        let Some(code) = key.windows_key_code() else { return };
        let shared = self.shared.lock().unwrap();
        let Some(host) = shared.browser.as_ref().and_then(|b| b.host()) else { return };
        let event = |type_| KeyEvent {
            size: std::mem::size_of::<KeyEvent>(),
            type_,
            windows_key_code: code,
            ..Default::default()
        };
        host.send_key_event(Some(&event(KeyEventType::RAWKEYDOWN)));
        if key == Key::Enter {
            // Enter also produces a character, which is what activates focused page controls.
            host.send_key_event(Some(&KeyEvent {
                character: 13,
                unmodified_character: 13,
                ..event(KeyEventType::CHAR)
            }));
        }
        host.send_key_event(Some(&event(KeyEventType::KEYUP)));
    }

    fn can_go_back(&self) -> bool {
        self.shared.lock().unwrap().browser.as_ref().is_some_and(|b| b.can_go_back() != 0)
    }

    fn go_back(&mut self) {
        if let Some(browser) = self.shared.lock().unwrap().browser.as_ref() {
            browser.go_back();
        }
    }

    fn take_frame(&mut self) -> Option<Frame> {
        self.shared.lock().unwrap().frame.take()
    }
}
