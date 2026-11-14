//! The Media screen's slow work on one background thread, so the touch UI never waits: the folder
//! scan, thumbnails and media facts from `/usr/local/sbin/walpad-thumb` (one at a time, each with
//! a hard time limit), and the recent list file. Results come back as events the shell loop polls.
use crate::media::{self, Info, Item};
use std::collections::VecDeque;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_hw::Root;

pub const HELPER: &str = "/usr/local/sbin/walpad-thumb";
/// Longest a single helper run may take (the script has its own, shorter limits).
const JOB_LIMIT: Duration = Duration::from_secs(45);
/// A thumbnail file larger than this is not loaded.
const MAX_PNG: u64 = 512 * 1024;
/// Queued thumbnail and facts jobs; older ones are dropped beyond this.
const MAX_QUEUE: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Scan,
    Thumbs(Vec<String>),
    Info(Vec<String>),
    Remember(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Scanned(Vec<Item>),
    Recent(Vec<String>),
    Thumb(String, Option<Vec<u8>>),
    Info(String, Info),
    /// A queued job was dropped (too many): ask again when the item is on screen.
    Dropped(String),
}

enum Work {
    Thumb(String),
    Info(String),
}

pub struct Worker {
    tx: Sender<Job>,
    rx: Receiver<Event>,
    /// Jobs sent and not yet finished (the loop wakes up often while this is not zero).
    pending: Arc<AtomicUsize>,
}

impl Worker {
    pub fn start(root: Root) -> Worker {
        let (tx, jobs) = channel::<Job>();
        let (out, rx) = channel::<Event>();
        let pending = Arc::new(AtomicUsize::new(0));
        let p2 = pending.clone();
        std::thread::spawn(move || run(root, jobs, out, p2));
        Worker { tx, rx, pending }
    }

    pub fn send(&self, job: Job) {
        let n = match &job {
            Job::Thumbs(v) | Job::Info(v) => v.len(),
            _ => 1,
        };
        self.pending.fetch_add(n, Ordering::SeqCst);
        if self.tx.send(job).is_err() {
            self.pending.fetch_sub(n, Ordering::SeqCst);
        }
    }

    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    pub fn busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }
}

fn run(root: Root, jobs: Receiver<Job>, out: Sender<Event>, pending: Arc<AtomicUsize>) {
    let mut queue: VecDeque<Work> = VecDeque::new();
    let done = |n: usize| {
        pending.fetch_sub(n.min(pending.load(Ordering::SeqCst)), Ordering::SeqCst);
    };
    loop {
        // take every job that is waiting; block only when there is nothing to do
        let first = if queue.is_empty() {
            match jobs.recv() {
                Ok(j) => Some(j),
                Err(_) => return,
            }
        } else {
            None
        };
        let mut incoming: Vec<Job> = first.into_iter().collect();
        loop {
            match jobs.try_recv() {
                Ok(j) => incoming.push(j),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        for j in incoming {
            match j {
                Job::Scan => {
                    let items = media::scan(&root);
                    let _ = out.send(Event::Recent(media::read_recent(&root)));
                    let _ = out.send(Event::Scanned(items));
                    done(1);
                }
                Job::Remember(p) => {
                    let _ = out.send(Event::Recent(media::remember(&root, &p)));
                    done(1);
                }
                Job::Thumbs(v) => {
                    // the newest request is what is on screen now: it goes first
                    for p in v.into_iter().rev() {
                        queue.push_front(Work::Thumb(p));
                    }
                }
                Job::Info(v) => {
                    for p in v {
                        queue.push_back(Work::Info(p));
                    }
                }
            }
        }
        while queue.len() > MAX_QUEUE {
            if let Some(Work::Thumb(p) | Work::Info(p)) = queue.pop_back() {
                let _ = out.send(Event::Dropped(p));
            }
            done(1);
        }
        if let Some(w) = queue.pop_front() {
            match w {
                Work::Thumb(p) => {
                    let png = helper(&root, "thumb", &p).and_then(|o| {
                        let f = o.lines().next()?.trim().to_string();
                        let host = root.p(&f);
                        (std::fs::metadata(&host).ok()?.len() <= MAX_PNG)
                            .then(|| std::fs::read(host).ok())
                            .flatten()
                    });
                    let _ = out.send(Event::Thumb(p, png));
                }
                Work::Info(p) => {
                    let info = helper(&root, "info", &p)
                        .map(|o| parse_info(&o))
                        .unwrap_or_default();
                    let _ = out.send(Event::Info(p, info));
                }
            }
            done(1);
        }
    }
}

/// Run the helper with a hard time limit; its stdout when it exited 0.
fn helper(root: &Root, what: &str, path: &str) -> Option<String> {
    let bin: PathBuf = root.p(HELPER);
    let mut child = Command::new(bin)
        .arg(what)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < JOB_LIMIT => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut s = String::new();
    child
        .stdout
        .take()?
        .take(64 * 1024)
        .read_to_string(&mut s)
        .ok()?;
    Some(s)
}

/// `duration=187`, `title=...`, `artist=...` lines.
pub fn parse_info(out: &str) -> Info {
    let mut i = Info::default();
    for l in out.lines() {
        if let Some(v) = l.strip_prefix("duration=") {
            i.duration = v.trim().parse().ok();
        } else if let Some(v) = l.strip_prefix("title=") {
            i.title = Some(v.trim().to_string()).filter(|t| !t.is_empty());
        } else if let Some(v) = l.strip_prefix("artist=") {
            i.artist = Some(v.trim().to_string()).filter(|t| !t.is_empty());
        }
    }
    i
}

#[cfg(test)]
mod tests;
