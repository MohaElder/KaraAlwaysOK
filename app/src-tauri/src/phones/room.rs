//! Who is in the room: up to four phones, each keeping its row through a dropped connection.

use kara_core::mic::Effect;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

pub const MAX_PHONES: usize = 4;
pub const ENDED: u16 = 4001;
const MAX_NAME: usize = 40;
const MAX_ID: usize = 64;
const TRIES: u32 = 5;
const LOCKOUT_MS: i64 = 10_000;
pub const GONE_AFTER: Duration = Duration::from_secs(120);

/// What goes out on a phone's connection.
pub enum Out {
    Text(String),
    Close(u16),
}

pub struct Guest {
    pub id: String,
    pub name: String,
    pub conn: u64,
    pub tx: Option<UnboundedSender<Out>>,
    pub volume: u8,
    pub voice: u8,
    pub effect: (Effect, u8),
    pub dropped_at: Option<Instant>,
}

impl Guest {
    pub fn send(&self, out: Out) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(out);
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Refusal {
    WrongCode,
    Full,
}

impl Refusal {
    pub fn close_code(&self) -> u16 {
        match self {
            Self::Full => 4002,
            Self::WrongCode => 4003,
        }
    }
}

pub struct Room {
    pub code: String,
    pub guests: Vec<Guest>,
    misses: u32,
    locked_until: i64,
}

impl Room {
    pub fn new(code: &str) -> Self {
        Self { code: digits(code), guests: Vec::new(), misses: 0, locked_until: 0 }
    }

    /// Lets a phone in with the right code at `now` (ms): back into its own row if it was here (false), or a new row while there
    /// is room (true). After five wrong codes only phones already here get in, for ten seconds.
    pub fn admit(&mut self, code: &str, id: &str, name: &str, conn: u64, tx: UnboundedSender<Out>, now: i64) -> Result<bool, Refusal> {
        let right = digits(code) == self.code;
        if id.len() > MAX_ID || (now < self.locked_until && !(right && self.guests.iter().any(|g| g.id == id))) {
            return Err(Refusal::WrongCode);
        }
        if !right {
            self.misses += 1;
            if self.misses >= TRIES {
                self.misses = 0;
                self.locked_until = now + LOCKOUT_MS;
            }
            return Err(Refusal::WrongCode);
        }
        self.misses = 0;
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        if let Some(g) = self.guest(id) {
            g.name = name;
            g.conn = conn;
            g.tx = Some(tx);
            g.dropped_at = None;
            return Ok(false);
        }
        if self.guests.len() >= MAX_PHONES {
            return Err(Refusal::Full);
        }
        self.guests.push(Guest { id: id.to_string(), name, conn, tx: Some(tx), volume: 80, voice: 80, effect: (Effect::None, 0), dropped_at: None });
        Ok(true)
    }

    /// Marks a phone's connection gone at `at`, unless it already came back on a newer one.
    pub fn dropped(&mut self, id: &str, conn: u64, at: Instant) {
        if let Some(g) = self.guests.iter_mut().find(|g| g.id == id && g.conn == conn) {
            g.tx = None;
            g.dropped_at = Some(at);
        }
    }

    /// Removes and returns the rows whose phone has been gone for GONE_AFTER or longer.
    pub fn expire(&mut self, now: Instant) -> Vec<Guest> {
        self.guests.extract_if(.., |g| g.dropped_at.is_some_and(|at| now.duration_since(at) >= GONE_AFTER)).collect()
    }

    pub fn guest(&mut self, id: &str) -> Option<&mut Guest> {
        self.guests.iter_mut().find(|g| g.id == id)
    }

    pub fn remove(&mut self, id: &str) -> Option<Guest> {
        let i = self.guests.iter().position(|g| g.id == id)?;
        Some(self.guests.remove(i))
    }
}

/// Only the digits of a typed code, so "OKI-4827", "oki 4827" and "4827" match.
pub fn digits(code: &str) -> String {
    code.chars().filter(char::is_ascii_digit).collect()
}

/// Four random digits.
pub fn new_code() -> Result<String, getrandom::Error> {
    let mut b = [0u8; 2];
    getrandom::fill(&mut b)?;
    Ok(format!("{:04}", u16::from_le_bytes(b) % 10_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn tx() -> UnboundedSender<Out> {
        tokio::sync::mpsc::unbounded_channel().0
    }

    #[test]
    fn a_phone_needs_the_code_and_the_fifth_is_refused() {
        let mut room = Room::new("4827");
        assert_eq!(room.admit("1234", "a", "Aiko", 1, tx(), 0), Err(Refusal::WrongCode));
        assert_eq!(room.admit("oki-4827", "a", "Aiko", 1, tx(), 0), Ok(true));
        for id in ["b", "c", "d"] {
            assert_eq!(room.admit("4827", id, id, 2, tx(), 0), Ok(true));
        }
        assert_eq!(room.admit("4827", "e", "Emi", 3, tx(), 0), Err(Refusal::Full));
    }

    #[test]
    fn five_wrong_codes_shut_the_door_for_ten_seconds() {
        let mut room = Room::new("4827");
        room.admit("4827", "b", "Ben", 1, tx(), 0).unwrap();
        for _ in 0..5 {
            assert_eq!(room.admit("0000", "x", "X", 1, tx(), 0), Err(Refusal::WrongCode));
        }
        assert_eq!(room.admit("4827", "a", "Aiko", 2, tx(), 9_999), Err(Refusal::WrongCode), "even the right code waits");
        assert_eq!(room.admit("4827", "b", "Ben", 2, tx(), 9_999), Ok(false), "a phone already here comes back");
        assert_eq!(room.admit("4827", "a", "  Aiko  ", 3, tx(), 10_000), Ok(true));
        assert_eq!(room.guests[1].name, "Aiko");
    }

    #[test]
    fn a_phone_that_drops_gets_its_own_row_back() {
        let mut room = Room::new("4827");
        for id in ["a", "b", "c", "d"] {
            room.admit("4827", id, id, 1, tx(), 0).unwrap();
        }
        room.dropped("a", 1, Instant::now());
        assert!(room.guests[0].tx.is_none(), "greyed, not gone");
        assert_eq!(room.admit("4827", "e", "Emi", 2, tx(), 0), Err(Refusal::Full), "its row is kept for it");
        assert_eq!(room.admit("4827", "a", "Aiko", 3, tx(), 0), Ok(false), "back in its own row");
        room.dropped("a", 1, Instant::now());
        let a = &room.guests[0];
        assert_eq!((room.guests.len(), a.name.as_str(), a.tx.is_some()), (4, "Aiko", true), "a late close of the old connection changes nothing");
    }

    #[test]
    fn a_phone_gone_for_two_minutes_loses_its_row() {
        let mut room = Room::new("4827");
        room.admit("4827", "a", "Aiko", 1, tx(), 0).unwrap();
        room.admit("4827", "b", "Ben", 1, tx(), 0).unwrap();
        let t = Instant::now();
        room.dropped("a", 1, t);
        assert!(room.expire(t + GONE_AFTER - Duration::from_secs(1)).is_empty(), "kept while it may still come back");
        let gone = room.expire(t + GONE_AFTER);
        assert_eq!((gone.len(), gone[0].id.as_str(), room.guests.len()), (1, "a", 1));
    }
}
