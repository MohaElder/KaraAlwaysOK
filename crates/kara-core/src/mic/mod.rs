//! Phone mics: turning each phone's sound into one mix for the Mac's speakers.

mod buffer;
mod howl;

pub use buffer::JitterBuffer;
pub use howl::Howl;
