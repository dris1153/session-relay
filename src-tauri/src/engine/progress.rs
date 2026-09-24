use std::mem::{discriminant, Discriminant};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Each event crosses IPC and re-renders the window: at most one per this interval, plus the last.
const MIN_GAP: Duration = Duration::from_millis(100);

/// What a long operation is doing, for the GUI's progress bar. Transfers report percent;
/// file steps report the file being worked on (`current` of `total`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Progress {
    /// A save (or maintenance) holds the store.
    Waiting,
    /// Asking GitHub for the latest snapshot.
    Checking,
    Download { percent: u8 },
    Upload { percent: u8 },
    /// Cloning a project repo (not the store).
    Clone { percent: u8 },
    Restore { current: usize, total: usize },
    Save { current: usize, total: usize },
    Evaluate { current: usize, total: usize },
}

impl Progress {
    fn is_last(&self) -> bool {
        match *self {
            Progress::Download { percent } | Progress::Upload { percent } | Progress::Clone { percent } => percent == 100,
            Progress::Restore { current, total } | Progress::Save { current, total } | Progress::Evaluate { current, total } => current == total,
            Progress::Waiting | Progress::Checking => true,
        }
    }
}

/// Set once by the GUI; hook workers and tests leave it empty.
#[derive(Default)]
pub struct ProgressSink {
    sink: OnceLock<Box<dyn Fn(Progress) + Send + Sync>>,
    last_sent: Mutex<Option<(Instant, Discriminant<Progress>)>>,
}

impl ProgressSink {
    pub fn set(&self, sink: impl Fn(Progress) + Send + Sync + 'static) {
        let _ = self.sink.set(Box::new(sink));
    }

    pub fn report(&self, progress: Progress) {
        let Some(sink) = self.sink.get() else { return };
        let mut last_sent = self.last_sent.lock().unwrap_or_else(|e| e.into_inner());
        let kind = discriminant(&progress);
        // A new step or the last event always gets through, so the bar never sticks mid-way.
        if !progress.is_last() && last_sent.is_some_and(|(at, k)| k == kind && at.elapsed() < MIN_GAP) {
            return;
        }
        *last_sent = Some((Instant::now(), kind));
        sink(progress);
    }

    /// Consumer of git's `--progress` stderr that reports each new percent as `step`.
    pub fn git_reporter(&self, step: fn(u8) -> Progress) -> impl FnMut(&str) + '_ {
        let mut last = None;
        move |text| {
            if let Some(percent) = transfer_percent(text).filter(|p| last != Some(*p)) {
                last = Some(percent);
                self.report(step(percent));
            }
        }
    }
}

/// Percent of the last transfer line: "Receiving objects", "Unpacking objects" (small fetches
/// below `fetch.unpackLimit`) or "Writing objects". Other phases ("Counting objects: 100%")
/// would make the bar jump; a line split across chunks is simply skipped.
pub(crate) fn transfer_percent(text: &str) -> Option<u8> {
    const TRANSFER: [&str; 3] = ["Receiving objects:", "Unpacking objects:", "Writing objects:"];
    let line = text.split(['\r', '\n']).rev().find(|l| TRANSFER.iter().any(|t| l.contains(t)))?;
    let (_, after) = line.split_once("objects:")?;
    let (digits, _) = after.trim_start().split_once('%')?;
    digits.parse::<u8>().ok().filter(|p| *p <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_transfer_percent_only_and_throttles() {
        assert_eq!(transfer_percent("Receiving objects:  45% (9/20)\rReceiving objects:  50% (10/20)"), Some(50));
        assert_eq!(transfer_percent("remote: Counting objects: 100% (5/5), done."), None);
        assert_eq!(transfer_percent("Receiving obj"), None);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = ProgressSink::default();
        let log = seen.clone();
        sink.set(move |p| log.lock().unwrap().push(p));
        let mut feed = sink.git_reporter(|percent| Progress::Download { percent });
        ["Receiving objects:  10%", "Receiving objects:  20%", "Receiving objects: 100%"].into_iter().for_each(&mut feed);
        // 20% arrives within the throttle gap; the final 100% always gets through.
        assert_eq!(*seen.lock().unwrap(), vec![Progress::Download { percent: 10 }, Progress::Download { percent: 100 }]);
    }
}
