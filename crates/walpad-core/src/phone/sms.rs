//! SMS sending, SIM polling/reassembly, and local message store access for [`super::Phone`].
use super::ops::{ok_final, Op};
use super::store::Sms;
use super::{parse, pdu, wall_ms, Phone};
use crate::modem::at::{AtError, Final, Outcome, Reply};
use crate::modem::MobileError;
use serde_json::{json, Value};
use std::time::Instant;

impl Phone {
    pub fn sms_send(&mut self, number: &str, text: &str) -> Result<Value, MobileError> {
        if !walpad_proto::valid_sms_number(number) {
            return Err(MobileError::Invalid(
                "number must be 3-20 digits, optionally starting with +".into(),
            ));
        }
        self.need_line(false)?;
        let reference = self.next_ref;
        let parts = pdu::encode_submit(self.smsc.as_deref(), number, text, reference).map_err(
            |e| match e {
                pdu::EncodeError::TooLong => MobileError::Invalid(format!(
                    "the message is too long (at most {} parts)",
                    pdu::MAX_PARTS
                )),
                pdu::EncodeError::BadNumber => MobileError::Invalid("bad number".into()),
            },
        )?;
        self.next_ref = self.next_ref.wrapping_add(1).max(1);
        let n = parts.len();
        let id = self.sms.add(Sms {
            id: 0,
            number: number.to_string(),
            text: text.to_string(),
            ts: wall_ms(),
            incoming: false,
            read: true,
            state: "sending".into(),
        });
        self.queue.push_back(Op::SmsSend { id, parts });
        self.dirty = true;
        Ok(json!({"id": id, "parts": n}))
    }

    pub(super) fn sms_result(&mut self, id: u64, parts: usize, results: &[Outcome]) {
        let all_ok = results.len() == parts && results.iter().all(|r| ok_final(Some(r)));
        let error = if all_ok {
            None
        } else {
            Some(match results.iter().find(|r| !ok_final(Some(r))) {
                Some(Err(AtError::Timeout)) => "the modem did not answer in time".to_string(),
                Some(Ok(Reply {
                    fin: Final::Cms(c), ..
                })) => format!(
                    "the network refused the message (error {})",
                    crate::modem::parse::clean(c, 12)
                ),
                _ => "the message could not be sent".to_string(),
            })
        };
        self.sms
            .set_state(id, if all_ok { "sent" } else { "failed" });
        self.dirty = true;
        eprintln!(
            "walpad-core: SMS {}",
            if all_ok { "sent" } else { "failed" }
        );
        self.notice(json!({"kind": "sms_sent", "id": id, "ok": all_ok, "error": error}));
    }

    /// Copy the messages the SIM holds into the store, then delete them from the SIM.
    pub(super) fn import_sms(&mut self, r: &Reply, _now: Instant) {
        let listed = parse::cmgl(&r.lines);
        if listed.is_empty() {
            return;
        }
        struct Piece {
            index: u32,
            read: bool,
            d: pdu::Deliver,
        }
        let mut singles: Vec<Piece> = Vec::new();
        let mut multi: Vec<Piece> = Vec::new();
        let mut junk: Vec<u32> = Vec::new();
        for l in listed.iter().filter(|l| l.stat <= 1) {
            match pdu::decode_deliver(&l.pdu) {
                Some(d) => {
                    let p = Piece {
                        index: l.index,
                        read: l.stat == 1,
                        d,
                    };
                    if p.d.concat.is_some_and(|c| c.1 > 1) {
                        multi.push(p);
                    } else {
                        singles.push(p);
                    }
                }
                None => junk.push(l.index),
            }
        }
        // (sender, text, ts, read, SIM indices)
        let mut done: Vec<(String, String, u64, bool, Vec<u32>)> = Vec::new();
        for p in singles {
            done.push((
                p.d.sender,
                p.d.text,
                p.d.ts_ms.unwrap_or_else(wall_ms),
                p.read,
                vec![p.index],
            ));
        }
        // reassemble concatenated messages: complete sets only, the rest waits on the SIM
        let mut used = vec![false; multi.len()];
        for i in 0..multi.len() {
            if used[i] {
                continue;
            }
            let Some((refn, total, _)) = multi[i].d.concat else {
                continue;
            };
            let sender = multi[i].d.sender.clone();
            let group: Vec<usize> = (0..multi.len())
                .filter(|&j| {
                    !used[j]
                        && multi[j].d.sender == sender
                        && multi[j]
                            .d
                            .concat
                            .is_some_and(|c| c.0 == refn && c.1 == total)
                })
                .collect();
            let mut by_seq: Vec<Option<usize>> = vec![None; total as usize];
            for &j in &group {
                if let Some((_, _, seq)) = multi[j].d.concat {
                    if (1..=total).contains(&seq) && by_seq[(seq - 1) as usize].is_none() {
                        by_seq[(seq - 1) as usize] = Some(j);
                    }
                }
            }
            if by_seq.iter().all(Option::is_some) {
                let mut text = String::new();
                let mut idx = Vec::new();
                let mut read = true;
                let mut ts = u64::MAX;
                for j in by_seq.iter().flatten() {
                    text.push_str(&multi[*j].d.text);
                    idx.push(multi[*j].index);
                    read &= multi[*j].read;
                    ts = ts.min(multi[*j].d.ts_ms.unwrap_or_else(wall_ms));
                }
                for j in group {
                    used[j] = true;
                }
                done.push((sender, text, ts, read, idx));
            }
        }
        let mut delete: Vec<u32> = junk;
        let n = done.len();
        for (sender, text, ts, read, idx) in done {
            let id = self.sms.add(Sms {
                id: 0,
                number: sender.clone(),
                text,
                ts,
                incoming: true,
                read,
                state: String::new(),
            });
            delete.extend(idx);
            if !read {
                let name = self.name_of(Some(&sender));
                self.notice(
                    json!({"kind": "sms_received", "id": id, "number": sender, "name": name}),
                );
            }
            self.dirty = true;
        }
        if n > 0 {
            eprintln!("walpad-core: {n} text message(s) received");
        }
        if !delete.is_empty() {
            delete.sort_unstable();
            self.queue.push_back(Op::SmsDelete(delete));
        }
    }

    pub fn sms_list(&self) -> Value {
        let msgs: Vec<Value> = self
            .sms
            .all()
            .iter()
            .map(|m| {
                json!({"id": m.id, "number": m.number, "name": self.name_of(Some(&m.number)),
                    "text": m.text, "ts": m.ts, "incoming": m.incoming, "read": m.read,
                    "state": m.state})
            })
            .collect();
        json!({"messages": msgs, "unread": self.sms.unread()})
    }

    pub fn sms_delete(&mut self, id: u64) -> Result<Value, MobileError> {
        let gone = self.sms.delete(id);
        self.dirty |= gone;
        Ok(json!({"deleted": gone}))
    }

    pub fn sms_read(&mut self, number: &str) -> Result<Value, MobileError> {
        let changed = self.sms.mark_read(number);
        self.dirty |= changed;
        Ok(json!({"unread": self.sms.unread()}))
    }
}
