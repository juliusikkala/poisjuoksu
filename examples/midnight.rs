#![feature(nll)]
use poisjuoksu::{Painter, RoadRenderer, Segment, SegmentStyle, FP_POS};
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

const ROAD_WIDTH: i32 = 50 << (FP_POS * 2);
const ROAD_EDGE_X0: i32 = 41 << (FP_POS * 2);
const ROAD_EDGE_X1: i32 = 45 << (FP_POS * 2);
const ROAD_LINE_WIDTH: i32 = 2 << (FP_POS * 2);
const ROAD_COLOR: u16 = 0x3187;
const ROAD_EDGE_COLOR: u16 = 0xBDB5;

impl<'a> Painter for SdlPainter<'a> {
    type ColorType = u16;

    fn draw(&mut self, x: i32, y: i32, color: &Self::ColorType) {
        let i = (x as usize) * std::mem::size_of::<Self::ColorType>() + (y as usize) * self.pitch;
        // Believe or not, doing this with unsafe is a significant optimization
        // due to the forced runtime bounds checking with [] -.-
        unsafe {
            *self.pixels.get_unchecked_mut(i) = (color & 0xFF) as u8;
            *self.pixels.get_unchecked_mut(i + 1) = (color >> 8) as u8;
        }
    }

    fn sky_color(&self, y: i32) -> Self::ColorType {
        let dither = if y & 0xF < 0x8 { y & 1 } else { 0 };
        let r = 16 - (y >> 4) + dither;
        let g = 19 - (y >> 4) + dither;
        let b = 14 - (y >> 5) + dither;
        ((r << 11) | (g << 5) | b) as u16
    }

    fn road_color(&self, tx: i32, t: i32) -> Self::ColorType {
        let atx = if tx < 0 { -tx } else { tx };
        if atx < ROAD_EDGE_X1 && atx >= ROAD_EDGE_X0 || atx < ROAD_LINE_WIDTH && (t & 0xFFF) < 0x800
        {
            ROAD_EDGE_COLOR
        } else {
            ROAD_COLOR
        }
    }

    fn road_width(&self) -> i32 {
        ROAD_WIDTH
    }
}

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video = sdl_context.video()?;
    let mut timer = sdl_context.timer()?;

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
        Segment::new(SegmentStyle::Field, 200 << FP_POS, 10, 0),
        Segment::new(SegmentStyle::Field, 100 << FP_POS, -10, -10),
        Segment::new(SegmentStyle::Field, 100 << FP_POS, 0, 10),
        Segment::new(SegmentStyle::Field, 65536 << FP_POS, 0, 0),
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
        road.advance(1 << FP_POS);
        screen_buffer.with_lock(
            Rect::new(0, 0, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
            |pixels, pitch| {
                let mut painter = SdlPainter { pixels, pitch };
                road.render(
                    &mut painter,
                    (10000.0 * f32::sin(timer.ticks() as f32 * 0.001)) as i32,
                    10000,
                );
            },
        )?;
        ren.copy(&screen_buffer, None, None)?;
        ren.present();
    }

    Ok(())
}
