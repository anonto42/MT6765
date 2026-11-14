//! Client for walpad-core's control socket: non-blocking, subscribes to status, wifi and input events,
//! reconnects every 2 s when the core is not there (it restarts independently of X).
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walpad_proto::{encode_line, LineDecoder, ShellInput, Status};

const RETRY_EVERY: Duration = Duration::from_secs(2);
const MAX_OUT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    Connected,
    Disconnected,
    Status(Box<Status>),
    /// A `wifi` event payload (scan results, connect result, disconnected).
    Wifi(Value),
    /// A `notice` event payload ({"kind":"app_crashed","app":..,"name":..}).
    Notice(Value),
    /// A hardware-key event for the shell (`input` topic): long power press, volume keys, power,
    /// system keyboard show/hide.
    Input(ShellInput),
    /// An `audio` event payload ({"volume", "muted", "available", "source": "key"|"ipc"}).
    Audio(Value),
    /// An `apps` event payload ({"kind":"started","app":"terminal","pid":..}, exited, ...).
    Apps(Value),
    /// The response to a request we sent (handshake replies are not reported).
    Reply {
        id: u64,
        ok: bool,
        data: Value,
        error: Option<ReplyError>,
    },
}

/// `code` is the protocol error code ("busy", "invalid_args", ...).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyError {
    pub code: String,
    pub msg: String,
}

pub struct CoreClient {
    path: PathBuf,
    stream: Option<UnixStream>,
    dec: LineDecoder,
    out: Vec<u8>,
    next_try: Instant,
    ready: bool,
    next_id: u64,
}

impl CoreClient {
    pub fn new(path: PathBuf) -> Self {
        CoreClient {
            path,
            stream: None,
            dec: LineDecoder::new(),
            out: Vec::new(),
            next_try: Instant::now(),
            ready: false,
            next_id: 10,
        }
    }

    /// Connected and past the hello handshake.
    pub fn connected(&self) -> bool {
        self.ready
    }
    pub fn fd(&self) -> Option<&UnixStream> {
        self.stream.as_ref()
    }

    fn queue(&mut self, id: u64, cmd: &str, args: Value) {
        let line = encode_line(&json!({"v": 1, "id": id, "cmd": cmd, "args": args}));
        if self.out.len() + line.len() <= MAX_OUT {
            self.out.extend(line);
        }
    }

    /// Send a request; returns its id (None when not connected). The response arrives as
    /// `CoreEvent::Reply`, state changes as status/wifi events.
    pub fn send(&mut self, cmd: &str, args: Value) -> Option<u64> {
        self.stream.as_ref()?;
        self.next_id += 1;
        let id = self.next_id;
        self.queue(id, cmd, args);
        self.flush();
        Some(id)
    }

    fn drop_connection(&mut self, out: &mut Vec<CoreEvent>) {
        if self.stream.take().is_some() {
            if self.ready {
                out.push(CoreEvent::Disconnected);
            }
            self.ready = false;
            self.out.clear();
            self.dec = LineDecoder::new();
        }
    }

    fn flush(&mut self) -> bool {
        let Some(s) = self.stream.as_mut() else {
            return false;
        };
        while !self.out.is_empty() {
            match s.write(&self.out) {
                Ok(0) => return false,
                Ok(n) => {
                    self.out.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                Err(_) => return false,
            }
        }
        true
    }

    /// Connect/reconnect, flush, read. Call whenever the socket is readable and at least every
    /// couple of seconds.
    pub fn tick(&mut self, now: Instant) -> Vec<CoreEvent> {
        let mut out = Vec::new();
        if self.stream.is_none() {
            if now < self.next_try {
                return out;
            }
            self.next_try = now + RETRY_EVERY;
            match UnixStream::connect(&self.path) {
                Ok(s) if s.set_nonblocking(true).is_ok() => {
                    self.stream = Some(s);
                    self.queue(1, "hello", json!({"client": "walpad-shell", "proto": 1}));
                    self.queue(
                        2,
                        "subscribe",
                        json!({"topics": ["status", "wifi", "notice", "input", "apps"]}),
                    );
                }
                _ => return out,
            }
        }
        if !self.flush() {
            self.drop_connection(&mut out);
            return out;
        }
        let mut buf = [0u8; 8192];
        while let Some(s) = self.stream.as_mut() {
            match s.read(&mut buf) {
                Ok(0) => {
                    self.drop_connection(&mut out);
                    break;
                }
                Ok(n) => match self.dec.feed(&buf[..n]) {
                    Ok(lines) => {
                        for l in lines {
                            self.handle_line(&l, &mut out);
                        }
                    }
                    Err(_) => {
                        self.drop_connection(&mut out);
                        break;
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.drop_connection(&mut out);
                    break;
                }
            }
        }
        out
    }

    fn handle_line(&mut self, line: &[u8], out: &mut Vec<CoreEvent>) {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if let Some(ev) = v["event"].as_str() {
            match ev {
                "status" => {
                    if let Ok(s) = serde_json::from_value::<Status>(v["data"].clone()) {
                        out.push(CoreEvent::Status(Box::new(s)));
                    }
                }
                "wifi" => out.push(CoreEvent::Wifi(v["data"].clone())),
                "notice" => out.push(CoreEvent::Notice(v["data"].clone())),
                "apps" => out.push(CoreEvent::Apps(v["data"].clone())),
                "audio" => out.push(CoreEvent::Audio(v["data"].clone())),
                "input" => {
                    if let Ok(k) = serde_json::from_value::<ShellInput>(v["data"].clone()) {
                        out.push(CoreEvent::Input(k));
                    }
                }
                _ => {}
            }
        } else if v["id"] == 1 && v["ok"] == true {
            self.ready = true;
            out.push(CoreEvent::Connected);
        } else if let (Some(id), Some(ok)) = (v["id"].as_u64(), v["ok"].as_bool()) {
            if id > 2 {
                let error = (!ok).then(|| ReplyError {
                    code: v["error"]["code"].as_str().unwrap_or("error").to_string(),
                    msg: v["error"]["msg"]
                        .as_str()
                        .unwrap_or("request failed")
                        .to_string(),
                });
                out.push(CoreEvent::Reply {
                    id,
                    ok,
                    data: v["data"].clone(),
                    error,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
