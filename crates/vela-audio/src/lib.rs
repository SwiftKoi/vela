//! Declarative audio state changes; mixing is the host's responsibility.
//!
//! # Owns
//!
//! AudioCommand, Bus, the audio graph description.
//!
//! # Does not own
//!
//! Decoding or mixing (vela-host); when to play (vela-vm effects).
