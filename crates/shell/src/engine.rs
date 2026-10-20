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
}
