//! Voice call state machine and call control commands for [`super::Phone`].
use super::ops::{AudioWant, Call, Ended, Op};
use super::store::CallRec;
use super::{is_emergency, parse, wall_ms, Phone};
use crate::modem::at::{AtError, Final, Outcome, Reply};
use crate::modem::MobileError;
use serde_json::{json, Value};
use std::time::Instant;
use walpad_proto::CallState;

impl Phone {
    pub(super) fn dial_result(&mut self, o: Option<&Outcome>, now: Instant) {
        let reason = match o {
            Some(Ok(r)) => match &r.fin {
                Final::Ok => {
                    self.next_clcc = now;
                    return;
                }
                Final::Busy => "busy",
                Final::NoAnswer => "no_answer",
                _ => "failed",
            },
            // no answer in 5 s: the call list decides
            Some(Err(AtError::Timeout)) => {
                self.next_clcc = now;
                return;
            }
            _ => "failed",
        };
        if self.call.as_ref().is_some_and(|c| !c.seen) {
            self.end_call(Some(reason), now);
        }
    }

    pub(super) fn apply_clcc(&mut self, r: &Reply, now: Instant) {
        use parse::ClccStat as S;
        if parse::new_sms_urc(&r.lines) {
            self.next_sms = now;
        }
        if let (Some(c), Some(call)) = (parse::ecpi_cause(&r.lines), self.call.as_mut()) {
            call.cause = Some(c);
        }
        // a premature `NO CARRIER` URC can cut the reply short: not a call list
        if r.fin != Final::Ok {
            return;
        }
        let list = parse::clcc(&r.lines);
        let primary = list
            .iter()
            .find(|c| c.stat == S::Active)
            .or_else(|| list.iter().find(|c| c.stat != S::Waiting))
            .or_else(|| list.first());
        let Some(c) = primary else {
            if let Some(call) = &self.call {
                if call.seen || now.duration_since(call.dialed_at) >= self.t.dial_grace {
                    self.end_call(None, now);
                }
            }
            return;
        };
        let state = match c.stat {
            S::Active => CallState::Active,
            S::Held => CallState::Held,
            S::Dialing | S::Alerting => CallState::Dialing,
            S::Incoming | S::Waiting => CallState::Ringing,
        };
        if self.call.is_none() {
            // a new call the owner did not start here (incoming, or placed elsewhere)
            self.ended = None;
            self.muted = false;
            self.call = Some(Call {
                number: c.number.clone(),
                incoming: c.incoming,
                state,
                dialed_at: now,
                seen: true,
                answered_at: (state == CallState::Active).then_some(now),
                hangup: None,
                cause: None,
                started_ms: wall_ms(),
            });
            self.dirty = true;
            if c.incoming && state == CallState::Ringing {
                let name = self.name_of(c.number.as_deref());
                self.notice(json!({"kind": "incoming_call", "number": c.number, "name": name}));
                eprintln!("walpad-core: incoming call");
            }
            if state == CallState::Active {
                self.audio_start();
            }
            self.next_clcc = now + self.t.clcc_call;
            return;
        }
        let mut start_audio = false;
        if let Some(call) = self.call.as_mut() {
            call.seen = true;
            if call.number.is_none() {
                call.number = c.number.clone();
                self.dirty = true;
            }
            if call.state != state {
                call.state = state;
                self.dirty = true;
                if state == CallState::Active {
                    if call.answered_at.is_none() {
                        call.answered_at = Some(now);
                    }
                    start_audio = true;
                }
            }
        }
        if start_audio {
            self.audio_start();
        }
    }

    fn audio_start(&mut self) {
        self.audio_on = true;
        self.audio_want = Some(AudioWant::Start(self.speaker));
    }

    /// The call is over. `reason` None = work it out from what is known.
    pub(super) fn end_call(&mut self, reason: Option<&str>, now: Instant) {
        let Some(call) = self.call.take() else {
            return;
        };
        let answered = call.answered_at;
        let dur = answered.map_or(0, |t| now.duration_since(t).as_secs());
        let reason: String = match reason {
            Some(r) => r.into(),
            None => match (call.hangup, call.cause) {
                (Some(h), _) => h.into(),
                (None, Some(c)) => parse::cause_reason(c).into(),
                (None, None) if answered.is_some() => "remote".into(),
                (None, None) if call.incoming => "missed".into(),
                (None, None) => "remote".into(),
            },
        };
        let kind = match (call.incoming, answered.is_some()) {
            (true, true) => "in",
            (true, false) => "missed",
            (false, _) => "out",
        };
        self.calls.add(CallRec {
            number: call.number.clone().unwrap_or_default(),
            ts: call.started_ms,
            dur_s: dur,
            kind: kind.into(),
        });
        let name = self.name_of(call.number.as_deref());
        self.notice(
            json!({"kind": "call_ended", "reason": reason, "incoming": call.incoming,
            "number": call.number, "name": name, "duration_s": dur}),
        );
        eprintln!("walpad-core: call ended ({reason}, {dur} s)");
        self.ended = Some(Ended {
            until: now + self.t.ended_show,
            reason,
            number: call.number,
        });
        self.muted = false;
        self.dirty = true;
        if self.audio_on {
            self.audio_on = false;
            self.audio_want = Some(AudioWant::Stop);
        }
        // a message may have arrived during the call
        self.next_sms = now;
        self.next_clcc = now + self.t.clcc_idle;
    }

    pub(super) fn need_line(&self, emergency: bool) -> Result<(), MobileError> {
        let g = self.gate;
        if !g.ready {
            return Err(MobileError::Refused("the modem is not ready".into()));
        }
        if !g.radio_on {
            return Err(MobileError::Refused(
                "turn the mobile radio on first (airplane mode)".into(),
            ));
        }
        if !emergency {
            if !g.sim_ready {
                return Err(MobileError::Refused("no usable SIM card".into()));
            }
            if !g.registered {
                return Err(MobileError::Refused(
                    "not registered on a mobile network".into(),
                ));
            }
        }
        Ok(())
    }

    /// `phone_dial`: strict number check, then `ATD<number>;` (or `AT+CUSD` for `*...#` USSD).
    pub fn dial(&mut self, number: &str, now: Instant) -> Result<Value, MobileError> {
        if !walpad_proto::valid_dial_number(number) {
            return Err(MobileError::Invalid(
                "number must be 1-32 characters of 0-9 + * # ,".into(),
            ));
        }
        if (number.starts_with('*') || number.starts_with('#')) && number.ends_with('#') {
            return self.ussd(number, now);
        }
        self.need_line(is_emergency(number))?;
        if self.call.is_some() {
            return Err(MobileError::Busy("a call is already in progress".into()));
        }
        self.ended = None;
        self.muted = false;
        self.call = Some(Call {
            number: Some(number.to_string()),
            incoming: false,
            state: CallState::Dialing,
            dialed_at: now,
            seen: false,
            answered_at: None,
            hangup: None,
            cause: None,
            started_ms: wall_ms(),
        });
        self.queue.push_front(Op::Dial(number.to_string()));
        self.dirty = true;
        eprintln!("walpad-core: dialling");
        Ok(json!({"state": "dialing"}))
    }

    pub fn answer(&mut self, _now: Instant) -> Result<Value, MobileError> {
        match &self.call {
            Some(c) if c.state == CallState::Ringing => {
                self.queue.push_front(Op::Answer);
                Ok(json!({"state": "answering"}))
            }
            _ => Err(MobileError::Refused("no call is ringing".into())),
        }
    }

    /// End the call, or reject the ringing one.
    pub fn hangup(&mut self, now: Instant) -> Result<Value, MobileError> {
        let Some(c) = self.call.as_mut() else {
            return Err(MobileError::Refused("no call".into()));
        };
        c.hangup = Some(if c.incoming && c.state == CallState::Ringing {
            "rejected"
        } else {
            "hangup"
        });
        // a dial still queued never goes out: nothing to hang up
        let queued = self.queue.len();
        self.queue.retain(|o| !matches!(o, Op::Dial(_)));
        if self.queue.len() != queued {
            self.end_call(None, now);
            return Ok(json!({"state": "ended"}));
        }
        self.queue.push_front(Op::Hangup { ath: false });
        Ok(json!({"state": "ending"}))
    }

    pub fn dtmf(&mut self, digit: char, _now: Instant) -> Result<Value, MobileError> {
        if !matches!(self.call.as_ref().map(|c| c.state), Some(CallState::Active)) {
            return Err(MobileError::Refused("no active call".into()));
        }
        if !(digit.is_ascii_digit() || digit == '*' || digit == '#') {
            return Err(MobileError::Invalid("digit must be 0-9 * #".into()));
        }
        // tones are queued in order behind each other
        self.queue.push_back(Op::Dtmf(digit));
        Ok(json!({}))
    }

    pub fn mute(&mut self, on: bool, _now: Instant) -> Result<Value, MobileError> {
        if !matches!(self.call.as_ref().map(|c| c.state), Some(CallState::Active)) {
            return Err(MobileError::Refused("no active call".into()));
        }
        self.queue.retain(|o| !matches!(o, Op::Mute(_)));
        self.queue.push_front(Op::Mute(on));
        Ok(json!({"on": on}))
    }

    /// Speaker or earpiece. The choice is kept for the next call; during a call the audio script
    /// switches the route (experimental).
    pub fn speaker(&mut self, on: bool, _now: Instant) -> Result<Value, MobileError> {
        self.speaker = on;
        self.dirty = true;
        if self.audio_on {
            self.audio_want = Some(AudioWant::Start(on));
        }
        Ok(json!({"on": on}))
    }
}
