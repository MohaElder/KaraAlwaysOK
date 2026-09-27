//! Who is in the room: up to four phones, each keeping its row through a dropped connection.

use tokio::sync::mpsc::UnboundedSender;

pub const MAX_PHONES: usize = 4;
pub const ENDED: u16 = 4001;

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
}

impl Room {
    pub fn new(code: &str) -> Self {
        Self { code: digits(code), guests: Vec::new() }
    }

    /// Lets a phone in with the right code: back into its own row if it was here (false), or a new row while there is room (true).
    pub fn admit(&mut self, code: &str, id: &str, name: &str, conn: u64, tx: UnboundedSender<Out>) -> Result<bool, Refusal> {
        if digits(code) != self.code {
            return Err(Refusal::WrongCode);
        }
        if let Some(g) = self.guest(id) {
            g.name = name.to_string();
            g.conn = conn;
            g.tx = Some(tx);
            return Ok(false);
        }
        if self.guests.len() >= MAX_PHONES {
            return Err(Refusal::Full);
        }
        self.guests.push(Guest { id: id.to_string(), name: name.to_string(), conn, tx: Some(tx) });
        Ok(true)
    }

    /// Marks a phone's connection gone, unless it already came back on a newer one.
    pub fn dropped(&mut self, id: &str, conn: u64) {
        if let Some(g) = self.guests.iter_mut().find(|g| g.id == id && g.conn == conn) {
            g.tx = None;
        }
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
pub fn new_code() -> String {
    let mut b = [0u8; 2];
    let _ = getrandom::fill(&mut b);
    format!("{:04}", u16::from_le_bytes(b) % 10_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx() -> UnboundedSender<Out> {
        tokio::sync::mpsc::unbounded_channel().0
    }

    #[test]
    fn a_phone_needs_the_code_and_the_fifth_is_refused() {
        let mut room = Room::new("4827");
        assert_eq!(room.admit("1234", "a", "Aiko", 1, tx()), Err(Refusal::WrongCode));
        assert_eq!(room.admit("oki-4827", "a", "Aiko", 1, tx()), Ok(true));
        for id in ["b", "c", "d"] {
            assert_eq!(room.admit("4827", id, id, 2, tx()), Ok(true));
        }
        assert_eq!(room.admit("4827", "e", "Emi", 3, tx()), Err(Refusal::Full));
    }

    #[test]
    fn a_phone_that_drops_gets_its_own_row_back() {
        let mut room = Room::new("4827");
        for id in ["a", "b", "c", "d"] {
            room.admit("4827", id, id, 1, tx()).unwrap();
        }
        room.dropped("a", 1);
        assert!(room.guests[0].tx.is_none(), "greyed, not gone");
        assert_eq!(room.admit("4827", "e", "Emi", 2, tx()), Err(Refusal::Full), "its row is kept for it");
        assert_eq!(room.admit("4827", "a", "Aiko", 3, tx()), Ok(false), "back in its own row");
        room.dropped("a", 1);
        let a = &room.guests[0];
        assert_eq!((room.guests.len(), a.name.as_str(), a.tx.is_some()), (4, "Aiko", true), "a late close of the old connection changes nothing");
    }
}
