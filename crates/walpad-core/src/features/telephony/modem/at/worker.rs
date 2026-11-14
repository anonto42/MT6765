use super::{AtCmd, AtError, Port, Reply};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::Duration;

pub type Outcome = Result<Reply, AtError>;

pub struct Job<K> {
    pub kind: K,
    pub cmds: Vec<AtCmd>,
}

/// The results in command order; a job stops at the first transport error (open, timeout, I/O),
/// so `results` can be shorter than the job.
pub struct Done<K> {
    pub kind: K,
    pub results: Vec<Outcome>,
}

pub enum Recv<K> {
    Done(Done<K>),
    Empty,
    /// The worker thread is gone.
    Gone,
}

/// The single AT worker thread. Dropping it ends the thread after its current job.
pub struct Worker<K> {
    tx: Sender<Job<K>>,
    rx: Receiver<Done<K>>,
}

impl<K: Send + 'static> Worker<K> {
    pub fn spawn(path: PathBuf, per_cmd: Duration) -> std::io::Result<Worker<K>> {
        let (tx, jobs) = channel::<Job<K>>();
        let (done, rx) = channel::<Done<K>>();
        std::thread::Builder::new()
            .name("walpad-at".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let results = run_job(&path, &job.cmds, per_cmd);
                    if done
                        .send(Done {
                            kind: job.kind,
                            results,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Worker { tx, rx })
    }

    pub fn send(&self, job: Job<K>) -> bool {
        self.tx.send(job).is_ok()
    }

    /// A finished job, if any (never blocks).
    pub fn try_recv(&self) -> Recv<K> {
        match self.rx.try_recv() {
            Ok(d) => Recv::Done(d),
            Err(TryRecvError::Empty) => Recv::Empty,
            Err(TryRecvError::Disconnected) => Recv::Gone,
        }
    }
}

pub(super) fn run_job(path: &Path, cmds: &[AtCmd], per_cmd: Duration) -> Vec<Outcome> {
    let mut port = match Port::open(path) {
        Ok(p) => p,
        Err(e) => return vec![Err(e)],
    };
    let mut out = Vec::new();
    for c in cmds {
        let r = port.command(c, per_cmd);
        let stop = r.is_err();
        out.push(r);
        if stop {
            break;
        }
    }
    out
}
