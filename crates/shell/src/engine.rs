//! The Engine seam: what the Shell needs from the web engine that runs an App.

use crate::apps::AppDefinition;
use crate::shell::Key;

/// One rendered page: tightly packed BGRA, upper-left origin.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub trait Engine {
    fn open(&mut self, app: &AppDefinition);
    fn close(&mut self);
    /// Sends a remote key to the open App's page.
    fn key(&mut self, key: Key);
    fn can_go_back(&self) -> bool;
    fn go_back(&mut self);
    /// The newest frame since the last call, if the page has painted.
    fn take_frame(&mut self) -> Option<Frame>;
    /// Whether an editable field has the focus in the open App's page.
    fn editable_focused(&self) -> bool;
    /// Types `text` into the focused field in one go.
    fn type_text(&mut self, text: &str);
}

/// An Engine that shows nothing, for hosts that have no web engine to run.
pub struct NoEngine;

impl Engine for NoEngine {
    fn open(&mut self, _app: &AppDefinition) {}
    fn close(&mut self) {}
    fn key(&mut self, _key: Key) {}
    fn can_go_back(&self) -> bool {
        false
    }
    fn go_back(&mut self) {}
    fn take_frame(&mut self) -> Option<Frame> {
        None
    }
    fn editable_focused(&self) -> bool {
        false
    }
    fn type_text(&mut self, _text: &str) {}
}

/// What an Engine tracks for the App that is open. The browser is created asynchronously and its
/// callbacks arrive late, so each is tagged with the id of the session that asked for it; anything
/// from a session that is over is refused, and a browser that arrives late is handed back to close.
pub struct Session<B> {
    current: u64,
    open: bool,
    browser: Option<B>,
    frame: Option<Frame>,
    editable: bool,
}

impl<B> Default for Session<B> {
    fn default() -> Self {
        Self { current: 0, open: false, browser: None, frame: None, editable: false }
    }
}

impl<B> Session<B> {
    /// Starts a session. Returns its id, and a browser left over from the last one to close.
    pub fn begin(&mut self) -> (u64, Option<B>) {
        let stale = self.end();
        self.current += 1;
        self.open = true;
        (self.current, stale)
    }

    /// Ends the session. Returns its browser, if it has one yet, to close.
    pub fn end(&mut self) -> Option<B> {
        self.open = false;
        self.frame = None;
        self.editable = false;
        self.browser.take()
    }

    /// Whether `session` is the one that is open.
    pub fn is_current(&self, session: u64) -> bool {
        self.open && session == self.current
    }

    /// Keeps the browser of the open session; hands any other straight back to close.
    pub fn attach(&mut self, session: u64, browser: B) -> Option<B> {
        if self.open && session == self.current {
            self.browser = Some(browser);
            None
        } else {
            Some(browser)
        }
    }

    /// Keeps the newest frame of the open session, dropping the one before it.
    pub fn paint(&mut self, session: u64, frame: Frame) {
        if self.open && session == self.current {
            self.frame = Some(frame);
        }
    }

    /// Records whether the page has an editable field focused, for the open session only.
    pub fn set_editable(&mut self, session: u64, editable: bool) {
        if self.open && session == self.current {
            self.editable = editable;
        }
    }

    pub fn editable(&self) -> bool {
        self.editable
    }

    pub fn browser(&self) -> Option<&B> {
        self.browser.as_ref()
    }

    pub fn take_frame(&mut self) -> Option<Frame> {
        self.frame.take()
    }
}
