#![feature(nll)]
use poisjuoksu::{Painter, RoadRenderer, Segment, SegmentStyle};
use sdl2;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::TextureAccess;

const SCREEN_WIDTH: i32 = 320;
const SCREEN_HEIGHT: i32 = 240;

struct SdlPainter<'a> {
    pixels: &'a mut [u8],
    pitch: usize,
}

impl<'a> Painter for SdlPainter<'a> {
    type ColorType = u16;
    fn draw(&mut self, x: i32, y: i32, color: &Self::ColorType) {
        // TODO: Transmute self.pixels into [Self::ColorType] if performance is
        // not adequate with this approach.
        let i = (x as usize) * std::mem::size_of::<Self::ColorType>() + (y as usize) * self.pitch;
        self.pixels[i] = (color & 0xFF) as u8;
        self.pixels[i + 1] = (color >> 8) as u8;
    }

    fn sky_color(&self, y: i32) -> Self::ColorType {
        let dither = if y & 0xF < 0x8 { y & 1 } else { 0 };
        let r = 16 - (y >> 4) + dither;
        let g = 19 - (y >> 4) + dither;
        let b = 14 - (y >> 5) + dither;
        ((r << 11) | (g << 5) | b) as u16
    }
}

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video = sdl_context.video()?;

    let window = video
        .window("Night Cruising", SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)
        .build()
        .map_err(|e| e.to_string())?;

    let mut ren = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;
    let texture_creator = ren.texture_creator();

    let mut event_pump = sdl_context.event_pump()?;

    let segments = [
        Segment::new(SegmentStyle::Field, 200 << 8, 10, 0),
        Segment::new(SegmentStyle::Field, 100 << 8, -10, -10),
        Segment::new(SegmentStyle::Field, 100 << 8, 0, 10),
        Segment::new(SegmentStyle::Field, 65536 << 8, 0, 0),
    ];
    let mut road = RoadRenderer::<SCREEN_WIDTH, SCREEN_HEIGHT>::new(&segments, 32);

    let mut screen_buffer = texture_creator
        .create_texture(
            PixelFormatEnum::RGB565,
            TextureAccess::Streaming,
            SCREEN_WIDTH as u32,
            SCREEN_HEIGHT as u32,
        )
        .map_err(|e| e.to_string())?;

    'mainloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'mainloop,
                _ => {}
            }
        }
        road.advance(256);
        screen_buffer.with_lock(
            Rect::new(0, 0, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
            |pixels, pitch| {
                let mut painter = SdlPainter { pixels, pitch };
                road.render(&mut painter);
            },
        )?;
        ren.copy(&screen_buffer, None, None)?;
        ren.present();
    }

    Ok(())
}
