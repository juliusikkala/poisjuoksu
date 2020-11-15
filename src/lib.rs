#![no_std]
#![allow(incomplete_features)]
#![feature(const_generics, const_evaluatable_checked)]

// Position of fixed point, in general. Some situations need more precision or
// more range, so multiples or halves of FP_POS are sometimes used too.
const FP_POS: i32 = 8;

pub trait Painter {
    type ColorType;

    // This function should draw a single pixel of the given color.
    fn draw(&mut self, x: i32, y: i32, color: &Self::ColorType);
    fn sky_color(&self, y: i32) -> Self::ColorType;
}

pub enum SegmentStyle {
    Field,
    Bridge,
    Canyon,
    LeftCliff,
    RightCliff,
    Tunnel,
}

pub struct Segment {
    pub style: SegmentStyle,
    pub length: i32,
    pub x_curve: i32,
    pub y_curve: i32,
}

impl Segment {
    pub fn new(style: SegmentStyle, length: i32, x_curve: i32, y_curve: i32) -> Self {
        Segment {
            style,
            length,
            x_curve,
            y_curve,
        }
    }
}

const fn compute_visibility_size(w: i32, h: i32) -> usize {
    (h * (w / 32)) as usize
}

pub struct RoadRenderer<'a, const W: i32, const H: i32>
where
    [u32; compute_visibility_size(W, H)]: Sized,
{
    // Looks like visibility.len() causes an ICE??? This also breaks iterators
    // with it, but at least indexing works...
    visibility: [u32; compute_visibility_size(W, H)],
    segments: &'a [Segment], // The road is built out of segments with constant curvature and style.
    cur_segment: usize,      // Index of the current segment
    near: i32,               // Near plane, practically just controls field of view
    cur_t: i32,              // Distance from the start of the road
    base_t: i32,             // Distance of the current segment from the start of the road
}

impl<'a, const W: i32, const H: i32> RoadRenderer<'a, W, H>
where
    [u32; compute_visibility_size(W, H)]: Sized,
{
    pub fn new(segments: &'a [Segment], near: i32) -> Self {
        Self {
            visibility: [0u32; compute_visibility_size(W, H)],
            segments,
            cur_segment: 0,
            near,
            cur_t: 0,
            base_t: 0,
        }
    }

    pub fn advance(&mut self, step: i32) {
        self.cur_t += step;
        while self.cur_segment < self.segments.len()
            && self.cur_t >= self.base_t + self.segments[self.cur_segment].length
        {
            self.base_t += self.segments[self.cur_segment].length;
            self.cur_segment += 1;
        }
    }

    pub fn set(&mut self, t: i32) {
        self.cur_t = 0;
        self.base_t = 0;
        self.cur_segment = 0;
        self.advance(t);
    }

    fn render_sky<P: Painter>(&mut self, painter: &mut P) {
        let mut visibility_index = 0;
        for y in 0..H {
            let color = painter.sky_color(y);
            let mut x = 0;
            while x < W {
                let v = self.visibility[visibility_index];
                visibility_index += 1;
                if v == 0xFFFFFFFF {
                    x += 32;
                    continue;
                }
                for i in 0..32 {
                    if ((v >> i) & 1) == 1 {
                        continue;
                    }
                    painter.draw(x, y, &color);
                    x += 1;
                }
            }
        }
    }

    pub fn render<P: Painter>(&mut self, painter: &mut P) {
        for i in 0..compute_visibility_size(W, H) {
            self.visibility[i] = 0;
        }

        // TODO: Render road

        self.render_sky(painter);
    }
}
