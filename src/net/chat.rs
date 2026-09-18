use super::*;
use std::collections::VecDeque;
use web_time::{Duration, Instant};

pub const HISTORY: usize = 100;

#[derive(Debug, Clone)]
pub struct Line {
    pub id: u64,
    pub seat: u8,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Chat {
    pub lines: VecDeque<Line>,
    outgoing: parking_lot::Mutex<Vec<String>>,
}

impl Chat {
    pub fn send(&self, text: &str) -> Result<(), String> {
        let text = agni_net::session::chat_text(text).map_err(str::to_string)?;
        let mut queue = self.outgoing.lock();
        if queue.len() >= 4 {
            return Err("Wait before sending more messages".into());
        }
        queue.push(text);
        Ok(())
    }

    pub fn receive(&mut self, id: u64, seat: u8, text: String) {
        if agni_net::session::chat_text(&text).is_err()
            || self.lines.back().is_some_and(|line| line.id >= id)
        {
            return;
        }
        self.lines.push_back(Line { id, seat, text });
        while self.lines.len() > HISTORY {
            self.lines.pop_front();
        }
    }
}

#[derive(Default)]
pub(super) struct Relay {
    next: u64,
    recent: BTreeMap<u8, VecDeque<Instant>>,
}

impl Relay {
    fn accept(&mut self, seat: u8, text: &str) -> Result<HostMsg, String> {
        let text = agni_net::session::chat_text(text).map_err(str::to_string)?;
        let recent = self.recent.entry(seat).or_default();
        recent.retain(|at| at.elapsed() < Duration::from_secs(5));
        if recent.len() >= 5 {
            return Err("Chat rate limit: wait a few seconds".into());
        }
        recent.push_back(Instant::now());
        self.next += 1;
        Ok(HostMsg::Chat {
            id: self.next,
            seat,
            text,
        })
    }
}

impl HostState {
    pub(super) fn publish_chat(
        &mut self,
        info: &mut SessionInfo,
        seat: u8,
        text: &str,
    ) -> Result<(), String> {
        if !self
            .session
            .as_ref()
            .is_some_and(|s| s.roster().iter().any(|p| p.seat == seat && p.connected))
        {
            return Err("Join the table before chatting".into());
        }
        let msg = self.chat.accept(seat, text)?;
        if let HostMsg::Chat { id, seat, ref text } = msg {
            info.chat.receive(id, seat, text.clone());
        }
        for &conn in self.conns.seats.keys() {
            send_to(conn, msg.clone());
        }
        Ok(())
    }
}

pub fn route(mut info: ResMut<SessionInfo>, mut host: ResMut<HostState>, mine: Res<MySeat>) {
    let queued = std::mem::take(&mut *info.chat.outgoing.lock());
    for text in queued {
        let result = match info.role {
            SessionRole::Host => host.publish_chat(&mut info, mine.0 .0, &text),
            SessionRole::Client => {
                bridge::send_to_host(ClientMsg::Chat { text });
                Ok(())
            }
            _ => Err("Join or create a table to chat".into()),
        };
        if let Err(error) = result {
            info.notices.push(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_bounds_history_deduplicates_and_limits_queue_and_rate() {
        let mut chat = Chat::default();
        for id in 1..=120 {
            chat.receive(id, 0, "hello".into());
        }
        chat.receive(120, 0, "duplicate".into());
        assert_eq!(chat.lines.len(), HISTORY);
        assert_eq!(chat.lines.front().unwrap().id, 21);
        for _ in 0..4 {
            assert!(chat.send("hi").is_ok());
        }
        assert!(chat.send("hi").is_err());
        let mut relay = Relay::default();
        for _ in 0..5 {
            assert!(relay.accept(0, "hi").is_ok());
        }
        assert!(relay.accept(0, "hi").is_err());
        assert!(relay.accept(1, "hi").is_ok());
    }
}
