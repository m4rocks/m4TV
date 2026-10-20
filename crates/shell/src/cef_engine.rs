//! The Engine on the Dev host: stock CEF rendered off-screen, frames copied to the CPU (ADR 0001).

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use cef::*;
use m4tv_shell::{
    apps::{AppDefinition, page_script},
    engine::{Engine, Frame, Session},
    shell::Key,
};

// shortcut: the page is laid out at a fixed size; resize the browser with the window when the
// Pi build needs 1080p.
const VIEW: (i32, i32) = (1280, 720);

type Shared = Arc<Mutex<Session<Browser>>>;

fn close_browser(browser: Option<Browser>) {
    if let Some(host) = browser.and_then(|b| b.host()) {
        host.close_browser(1);
    }
}

wrap_render_handler! {
    struct Render {
        shared: Shared,
        session: u64,
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
            // Painting replaces the previous frame, so nothing accumulates.
            let frame = Frame { width: width as u32, height: height as u32, bgra: bgra.to_vec() };
            self.shared.lock().unwrap().paint(self.session, frame);
        }
    }
}

wrap_life_span_handler! {
    struct LifeSpan {
        shared: Shared,
        session: u64,
        app: AppDefinition,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser.cloned() else { return };
            // The App may have been closed while this browser was still being created.
            let late = self.shared.lock().unwrap().attach(self.session, browser.clone());
            let stale = late.is_some();
            close_browser(late);
            if stale {
                return;
            }
            // The browser starts on a blank page so the user agent is in place before the App loads.
            if let (Some(ua), Some(host)) = (&self.app.user_agent, browser.host()) {
                if let Some(mut params) = dictionary_value_create() {
                    params.set_string(Some(&"userAgent".into()), Some(&ua.as_str().into()));
                    host.execute_dev_tools_method(
                        0,
                        Some(&"Emulation.setUserAgentOverride".into()),
                        Some(&mut params),
                    );
                }
            }
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&self.app.start_url.as_str().into()));
            }
        }
    }
}

wrap_load_handler! {
    struct Tuning {
        script: String,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut cef::Frame>,
            _http_status_code: std::os::raw::c_int,
        ) {
            // Every page of the App gets the TV tuning; sub-frames (ads, embeds) do not.
            if let Some(frame) = frame.filter(|f| f.is_main() != 0) {
                frame.execute_java_script(Some(&self.script.as_str().into()), None, 0);
            }
        }
    }
}

wrap_client! {
    struct AppClient {
        render: RenderHandler,
        life_span: LifeSpanHandler,
        tuning: LoadHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.tuning.clone())
        }
    }
}

wrap_app! {
    struct ShellApp;

    impl App {}
}

pub struct CefEngine {
    shared: Shared,
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
    CefEngine { shared: Shared::default(), cache_root, contexts: HashMap::new() }
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
        let (session, stale) = self.shared.lock().unwrap().begin();
        close_browser(stale);
        let mut client = AppClient::new(
            Render::new(self.shared.clone(), session),
            LifeSpan::new(self.shared.clone(), session, app.clone()),
            Tuning::new(page_script(app)),
        );
        let window_info = WindowInfo::default().set_as_windowless(Default::default());
        browser_host_create_browser(
            Some(&window_info),
            Some(&mut client),
            Some(&CefString::from("about:blank")),
            Some(&BrowserSettings { windowless_frame_rate: 30, ..Default::default() }),
            None,
            Some(context),
        );
    }

    fn close(&mut self) {
        let browser = self.shared.lock().unwrap().end();
        close_browser(browser);
    }

    fn key(&mut self, key: Key) {
        let Some(code) = key.windows_key_code() else { return };
        let shared = self.shared.lock().unwrap();
        let Some(host) = shared.browser().and_then(|b| b.host()) else { return };
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
        self.shared.lock().unwrap().browser().is_some_and(|b| b.can_go_back() != 0)
    }

    fn go_back(&mut self) {
        if let Some(browser) = self.shared.lock().unwrap().browser() {
            browser.go_back();
        }
    }

    fn take_frame(&mut self) -> Option<Frame> {
        self.shared.lock().unwrap().take_frame()
    }
}
