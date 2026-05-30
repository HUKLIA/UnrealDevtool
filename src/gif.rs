use eframe::egui;
use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use std::io::Cursor;

pub struct GifPlayer {
    frames:        Vec<egui::ColorImage>,
    delays:        Vec<f32>,
    current_frame: usize,
    elapsed:       f32,
    texture:       Option<egui::TextureHandle>,
}

impl GifPlayer {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
        let mut frames = Vec::new();
        let mut delays = Vec::new();
        for result in decoder.into_frames() {
            let frame = result.ok()?;
            let (numer, denom) = frame.delay().numer_denom_ms();
            let delay_secs = (numer as f32 / denom as f32).max(20.0) / 1000.0;
            let rgba = frame.into_buffer();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.pixels()
                .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            frames.push(egui::ColorImage { size, pixels });
            delays.push(delay_secs);
        }
        if frames.is_empty() { return None; }
        Some(Self { frames, delays, current_frame: 0, elapsed: 0.0, texture: None })
    }

    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.elapsed       = 0.0;
        self.texture       = None;
    }

    pub fn advance(&mut self, ctx: &egui::Context, dt: f32) {
        self.elapsed += dt;
        let need_upload = if self.elapsed >= self.delays[self.current_frame] {
            self.elapsed -= self.delays[self.current_frame];
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            true
        } else {
            self.texture.is_none()
        };
        if need_upload {
            self.texture = Some(ctx.load_texture(
                "miku_gif_frame",
                self.frames[self.current_frame].clone(),
                egui::TextureOptions::LINEAR,
            ));
        }
        ctx.request_repaint();
    }

    pub fn show(&self, ui: &mut egui::Ui, size: egui::Vec2) {
        if let Some(tex) = &self.texture {
            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
        }
    }
}
