//! IPC client connection state, line framing, and `devd.cmd` FIFO opener.

use serde_json::json;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Instant;
use walpad_hw::Root;
use walpad_proto::{
    encode_line, parse_request, Command, ErrorCode, Event, LineDecoder, Response, Topic,
    PROTO_VERSION,
};

use super::{Daemon, MAX_OUT};

pub(super) struct Client {
    pub(super) stream: UnixStream,
    pub(super) dec: LineDecoder,
    pub(super) hello: bool,
    pub(super) topics: Vec<Topic>,
    pub(super) out: Vec<u8>,
    pub(super) stalled_since: Option<Instant>,
    pub(super) dead: bool,
    pub(super) sync_pending: bool,
}

impl Client {
    pub(super) fn queue(&mut self, bytes: Vec<u8>) {
        if self.out.len() + bytes.len() > MAX_OUT {
            self.dead = true;
            return;
        }
        self.out.extend(bytes);
        self.flush();
    }

    pub(super) fn flush(&mut self) {
        while !self.out.is_empty() {
            match self.stream.write(&self.out) {
                Ok(0) => {
                    self.dead = true;
                    return;
                }
                Ok(n) => {
                    self.out.drain(..n);
                    self.stalled_since = None;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    self.stalled_since.get_or_insert_with(Instant::now);
                    return;
                }
                Err(_) => {
                    self.dead = true;
                    return;
                }
            }
        }
    }
}

impl Daemon {
    pub(super) fn handle_line(&mut self, ci: usize, line: &[u8]) {
        let resp = match parse_request(line) {
            Err(r) => r,
            Ok((id, cmd)) => {
                let hello = self.clients[ci].hello;
                match cmd {
                    Command::Hello { proto, .. } => {
                        if proto != PROTO_VERSION {
                            Response::err(id, ErrorCode::Version, "unsupported protocol version")
                        } else {
                            self.clients[ci].hello = true;
                            Response::ok(
                                id,
                                json!({"proto": PROTO_VERSION, "server": "walpad-core"}),
                            )
                        }
                    }
                    _ if !hello => {
                        Response::err(id, ErrorCode::NotHello, "first request must be hello")
                    }
                    Command::Subscribe { topics } => {
                        self.clients[ci].topics = topics;
                        self.clients[ci].sync_pending = true;
                        Response::ok(id, json!({}))
                    }
                    cmd => match self.execute(cmd) {
                        Ok(v) => Response::ok(id, v),
                        Err((c, m)) => Response::err(id, c, m),
                    },
                }
            }
        };
        self.clients[ci].queue(encode_line(&resp));
        // a new status subscriber starts in sync
        if resp.ok
            && self.clients[ci].topics.contains(&Topic::Status)
            && self.clients[ci].sync_pending
        {
            self.clients[ci].sync_pending = false;
            let ev = Event::new(
                "status",
                serde_json::to_value(self.status()).unwrap_or_default(),
            );
            self.clients[ci].queue(encode_line(&ev));
        }
    }
}

pub(super) fn open_fifo(root: &Root) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let p = root.p("/run/walpad/devd.cmd");
    if !p.exists() {
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &p,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o666),
            0,
        )?;
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(&p)?;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o666))?;
    Ok(f)
}
