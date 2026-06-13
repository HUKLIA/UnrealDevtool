use rodio::{Decoder, DeviceSinkBuilder, Float, MixerDeviceSink, Player};
use std::io::Cursor;

/// Loops a (possibly user-supplied) audio track in the background, with adjustable mute/volume.
pub struct AudioPlayer {
    _stream: MixerDeviceSink,
    player:  Player,
    bytes:   Vec<u8>,
    playing: bool,
    pub muted:  bool,
    pub volume: u32, // 0..=100
}

impl AudioPlayer {
    pub fn new(bytes: Vec<u8>, muted: bool, volume: u32) -> Option<Self> {
        let stream = DeviceSinkBuilder::open_default_sink().ok()?;
        let player = Player::connect_new(stream.mixer());
        let volume = volume.min(100);
        player.set_volume(Self::effective_volume(muted, volume));
        Some(Self { _stream: stream, player, bytes, playing: false, muted, volume })
    }

    fn effective_volume(muted: bool, volume: u32) -> Float {
        if muted { 0.0 } else { volume as Float / 100.0 }
    }

    pub fn play_looping(&mut self) {
        self.player.stop();
        if let Ok(source) = Decoder::new_looped(Cursor::new(self.bytes.clone())) {
            self.player.append(source);
            self.playing = true;
        }
    }

    pub fn stop(&mut self) {
        if self.playing {
            self.player.stop();
            self.playing = false;
        }
    }

    /// Swap in a different track (e.g. a user-picked MP3/WAV), stopping any
    /// playback in progress. Takes effect the next time `play_looping` runs.
    pub fn set_source(&mut self, bytes: Vec<u8>) {
        self.stop();
        self.bytes = bytes;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.player.set_volume(Self::effective_volume(self.muted, self.volume));
    }

    pub fn set_volume(&mut self, volume: u32) {
        self.volume = volume.min(100);
        self.player.set_volume(Self::effective_volume(self.muted, self.volume));
    }
}
