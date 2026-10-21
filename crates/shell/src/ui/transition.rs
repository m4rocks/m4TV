//! The launch and leave transition (ADR 0005). Opening an App grows its Tile to fill the screen and
//! holds the logo until the Engine's first frame, then crossfades to the App; leaving reverses it.
//! Nothing here waits on a timer for the Engine: the hold lasts as long as the Engine takes.

use std::time::{Duration, Instant};

use m4tv_shell::tokens;

/// How the transition looks at one moment. All values run 0 to 1.
#[derive(Debug, PartialEq)]
pub struct Look {
    /// How far the Tile has grown towards the whole screen.
    pub grow: f32,
    /// How visible the logo's cover is.
    pub cover: f32,
    /// How visible the App's picture is, on top of the cover.
    pub app: f32,
    /// The transition is over, and the screen it led to needs no help.
    pub done: bool,
}

pub struct Transition {
    started: Instant,
    painted: Option<Instant>,
    leaving: Option<Instant>,
    /// Reduce motion: fades instead of growth.
    reduce: bool,
}

fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

/// How far `ms` milliseconds starting at `since` have got at `now`.
fn progress(now: Instant, since: Instant, ms: u64) -> f32 {
    (now.saturating_duration_since(since).as_secs_f32() / (ms as f32 / 1000.)).clamp(0., 1.)
}

impl Transition {
    pub fn new(now: Instant, reduce: bool) -> Self {
        Self { started: now, painted: None, leaving: None, reduce }
    }

    /// The Engine delivered a frame. Only the first one counts.
    pub fn painted(&mut self, now: Instant) {
        self.painted.get_or_insert(now);
    }

    pub fn is_leaving(&self) -> bool {
        self.leaving.is_some()
    }

    pub fn leave(&mut self, now: Instant) {
        self.leaving.get_or_insert(now);
    }

    /// How long the Tile takes to grow, or the cover to fade in, in milliseconds.
    fn entry_ms(&self) -> u64 {
        if self.reduce { tokens::MOTION_FADE_MS } else { tokens::MOTION_LAUNCH_MS }
    }

    pub fn look(&self, now: Instant) -> Look {
        let entry_ms = self.entry_ms();
        let entry = ease_out(progress(now, self.started, entry_ms));
        // The crossfade starts once the Tile is in and the Engine has a frame, whichever is later.
        let shown = |painted: Instant| {
            ease_out(progress(now, painted.max(self.started + Duration::from_millis(entry_ms)), tokens::MOTION_CROSSFADE_MS))
        };
        let app_in = self.painted.map_or(0., shown);
        let (grow, cover) = if self.reduce { (1., entry) } else { (entry, 1.) };

        let Some(left) = self.leaving else {
            let done = app_in >= 1.;
            return Look { grow, cover, app: app_in, done };
        };
        // Leaving: the App fades out to the logo, then the Tile shrinks back. Without a picture
        // there is nothing to fade out.
        // It reverses from where the entry had got to, not from fully in.
        let entry_left = ease_out(progress(left, self.started, entry_ms));
        let fade_ms = if self.painted.is_some() { tokens::MOTION_CROSSFADE_MS } else { 0 };
        let app = app_in.min(1. - ease_out(progress(now, left, fade_ms.max(1))));
        let back_from = left + Duration::from_millis(fade_ms);
        let back = ease_out(progress(now, back_from, entry_ms));
        let (grow, cover) =
            if self.reduce { (1., entry_left * (1. - back)) } else { (entry_left * (1. - back), 1.) };
        Look { grow, cover, app, done: now >= back_from + Duration::from_millis(entry_ms) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn the_tile_grows_then_holds_for_as_long_as_the_engine_takes() {
        let t0 = Instant::now();
        let transition = Transition::new(t0, false);
        assert_eq!(transition.look(t0), Look { grow: 0., cover: 1., app: 0., done: false });
        let grown = transition.look(t0 + MS(tokens::MOTION_LAUNCH_MS));
        assert_eq!(grown, Look { grow: 1., cover: 1., app: 0., done: false });
        // A slow Engine: ten seconds on, the logo is still held, never an empty screen.
        let slow = transition.look(t0 + Duration::from_secs(10));
        assert_eq!(slow, Look { grow: 1., cover: 1., app: 0., done: false });
    }

    #[test]
    fn the_first_frame_starts_a_crossfade_and_ends_the_transition() {
        let t0 = Instant::now();
        let mut transition = Transition::new(t0, false);
        let frame = t0 + MS(500);
        transition.painted(frame);
        transition.painted(frame + MS(100)); // only the first counts
        let halfway = transition.look(frame + MS(tokens::MOTION_CROSSFADE_MS / 2));
        assert!(halfway.app > 0. && halfway.app < 1. && !halfway.done);
        let over = transition.look(frame + MS(tokens::MOTION_CROSSFADE_MS));
        assert_eq!((over.app, over.done), (1., true));
    }

    #[test]
    fn a_frame_that_beats_the_growth_waits_for_it() {
        let t0 = Instant::now();
        let mut transition = Transition::new(t0, false);
        transition.painted(t0 + MS(10));
        assert_eq!(transition.look(t0 + MS(tokens::MOTION_LAUNCH_MS)).app, 0.);
        assert_eq!(transition.look(t0 + MS(tokens::MOTION_LAUNCH_MS + tokens::MOTION_CROSSFADE_MS)).app, 1.);
    }

    #[test]
    fn leaving_fades_the_app_out_then_shrinks_the_tile_back() {
        let t0 = Instant::now();
        let mut transition = Transition::new(t0, false);
        transition.painted(t0 + MS(300));
        let open = t0 + MS(2000);
        assert_eq!(transition.look(open).app, 1.);
        transition.leave(open);
        let fading = transition.look(open + MS(tokens::MOTION_CROSSFADE_MS / 2));
        assert!(fading.app < 1. && fading.grow == 1. && !fading.done);
        let shrinking = transition.look(open + MS(tokens::MOTION_CROSSFADE_MS + tokens::MOTION_LAUNCH_MS / 2));
        assert!(shrinking.app == 0. && shrinking.grow < 1. && shrinking.grow > 0.);
        let end = open + MS(tokens::MOTION_CROSSFADE_MS + tokens::MOTION_LAUNCH_MS);
        assert_eq!(transition.look(end), Look { grow: 0., cover: 1., app: 0., done: true });
    }

    #[test]
    fn leaving_before_any_frame_goes_straight_back() {
        let t0 = Instant::now();
        let mut transition = Transition::new(t0, false);
        let left = t0 + MS(100);
        transition.leave(left);
        assert!(transition.look(left + MS(tokens::MOTION_LAUNCH_MS)).done);
    }

    #[test]
    fn leaving_mid_growth_shrinks_from_where_the_tile_was() {
        let t0 = Instant::now();
        let mut transition = Transition::new(t0, false);
        let left = t0 + MS(tokens::MOTION_LAUNCH_MS / 2);
        let grown = transition.look(left).grow;
        assert!(grown > 0. && grown < 1.);
        transition.leave(left);
        assert_eq!(transition.look(left).grow, grown, "no jump");
        assert!(transition.look(left + MS(tokens::MOTION_LAUNCH_MS)).done);
    }

    #[test]
    fn reduce_motion_fades_the_cover_and_never_grows_the_tile() {
        let t0 = Instant::now();
        let mut transition = Transition::new(t0, true);
        let half = transition.look(t0 + MS(tokens::MOTION_FADE_MS / 2));
        assert!(half.grow == 1. && half.cover > 0. && half.cover < 1.);
        transition.painted(t0 + MS(tokens::MOTION_FADE_MS));
        let shown = t0 + MS(tokens::MOTION_FADE_MS + tokens::MOTION_CROSSFADE_MS);
        assert_eq!(transition.look(shown).app, 1.);
        transition.leave(shown);
        let back = shown + MS(tokens::MOTION_CROSSFADE_MS + tokens::MOTION_FADE_MS);
        assert_eq!(transition.look(back), Look { grow: 1., cover: 0., app: 0., done: true });
    }
}
