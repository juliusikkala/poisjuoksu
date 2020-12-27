#![no_std]
#![allow(incomplete_features)]
#![feature(const_generics, const_evaluatable_checked)]


// Position of fixed point, in general. Some situations need more precision or
// more range, so multiples or halves of FP_POS are sometimes used too.
// In functions which do lots of fixed point calculations, the point is
// annotated with comments like FP1, FP2 where the number determines the
// multiple of FP_POS.
pub const FP_POS: i32 = 8;

// http://www.azillionmonkeys.com/qed/ulerysqroot.pdf
fn isqrt(num: i32) -> i32 {
    let mut v = num;
    let mut n = 0;
    let mut b = 0x8000;
    let mut bshft = 15;

    loop {
        let tmp = ((n << 1) + b) << bshft;
        bshft -= 1;
        if v >= tmp {
            n += b;
            v -= tmp;
        }
        b >>= 1;
        if b == 0 {
            break;
        }
    }
    n
}

pub trait Painter {
    type ColorType;

    // This function should draw a single pixel of the given color.
    fn draw(&mut self, x: i32, y: i32, color: &Self::ColorType);
    fn sky_color(&self, y: i32) -> Self::ColorType;
    // tx world-space X in FP2, t is world-space distance from start.
    fn road_color(&self, tx: i32, t: i32) -> Self::ColorType;
    fn ground_color(&self, tx: i32, t: i32) -> Self::ColorType;
    fn road_width(&self) -> i32;
}

#[derive(Copy, Clone)]
pub enum SideInclination {
    Uphill,
    Flat,
    Downhill,
}

pub struct Segment {
    pub side_style: (SideInclination, SideInclination),
    pub length: i32,
    pub x_curve: i32,
    pub y_curve: i32,
}

impl Segment {
    pub fn new(side_style: (SideInclination, SideInclination), length: i32, x_curve: i32, y_curve: i32) -> Self {
        Segment {
            side_style,
            length,
            x_curve,
            y_curve,
        }
    }
}

// The const generics implementation in Rust is just wonderful.
const fn i32_to_usize(n: i32) -> usize { n as usize }

pub struct RoadRenderer<'a> {
    segments: &'a [Segment], // The road is built out of segments with constant curvature and style.
    cur_segment: usize,      // Index of the current segment
    near: i32,               // Near plane, practically just controls field of view
    cur_t: i32,              // Distance from the start of the road
    base_t: i32,             // Distance of the current segment from the start of the road
}

// Per-line visibility information, needed for road rendering.
#[derive(Copy, Clone)]
pub struct LineVisibility {
    // If true, the range between end and begin is masked. Otherwise, it is
    // available.
    road: bool,
    begin: i16,
    end: i16,
}

impl<'a> RoadRenderer<'a> {
    pub fn new(segments: &'a [Segment], near: i32) -> Self {
        Self {
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

    fn render_sky<P: Painter>(
        &mut self,
        painter: &mut P,
        (w, h): (i32, i32),
        horizon: &[LineVisibility]
    ) {
        for y in 0..h {
            let color = painter.sky_color(y);
            let line = &horizon[y as usize];
            if line.road {
                for x in 0..(line.begin as i32) {
                    painter.draw(x, y, &color);
                }
                for x in (line.end as i32)..w {
                    painter.draw(x, y, &color);
                }
            } else {
                for x in (line.begin as i32)..(line.end as i32) {
                    painter.draw(x, y, &color);
                }
            }
        }
    }

    fn update_state_at_segment_length(
        &self,
        index: usize,
        length: i32,
        x_offset: &mut i32, // FP1
        y_offset: &mut i32, // FP1
        z_offset: &mut i32, // FP1
        x_slope: &mut i32,  // FP1
        y_slope: &mut i32,  // FP1
    ) {
        let y_curve = self.segments[index].y_curve;
        let x_curve = self.segments[index].x_curve;
        let z;

        if y_curve == 0 {
            // Flat plane as far as Y axis is concerned
            let t_factor = isqrt((1 << (2 * FP_POS)) + *y_slope * *y_slope); // FP1

            z = (length << FP_POS) / t_factor; // FP1
            *y_offset += (*y_slope * z) >> FP_POS; // FP1
        } else {
            let abs_y_curve = if y_curve < 0 { -y_curve } else { y_curve };
            let tsqrtcurve = isqrt(abs_y_curve << FP_POS); // FP1
            let z2 = 4 * length / tsqrtcurve;
            z = isqrt(z2 << FP_POS) << (FP_POS / 2); // FP1

            *y_offset += y_curve * z2 + ((*y_slope * z) >> FP_POS); // FP1
            *y_slope += (y_curve * z * 2) >> FP_POS; // FP1
        }
        *z_offset += z;

        if x_curve == 0 {
            // X-axis is linear.
            *x_offset += (*x_slope * z) >> FP_POS; // FP1
        } else {
            *x_offset += ((x_curve * z >> FP_POS) * z >> FP_POS) + (*x_slope * z >> FP_POS); // FP1
            *x_slope += 2 * x_curve * z >> FP_POS; // FP1
        }
    }

    pub fn get_screen_pos(
        &self,
        (w, h): (i32, i32),
        camera_x_offset: i32,
        camera_y_offset: i32,
        point_t_offset: i32,
        point_x_offset: i32,
        point_y_offset: i32,
        x_px: &mut i32, // FP1 screen coordinate
        y_px: &mut i32, // FP1 screen coordinate
        inv_z: &mut i32  // 1/z, FP3, negative values are behind camera
    ) {
        let mut x_offset = camera_x_offset;
        let mut y_offset = camera_y_offset;
        let mut z_offset = 0;
        let mut x_slope = 0;
        let mut y_slope = 0;
        let mut t_left = point_t_offset;

        for render_segment in self.cur_segment..self.segments.len() {
            let seg = &self.segments[render_segment];
            let length_left = seg.length - (if render_segment == self.cur_segment {
                self.cur_t - self.base_t
            } else {
                0
            });
            let length = if t_left < length_left { t_left } else { length_left };
            self.update_state_at_segment_length(
                render_segment,
                length,
                &mut x_offset,
                &mut y_offset,
                &mut z_offset,
                &mut x_slope,
                &mut y_slope,
            );
            t_left -= length;
            if t_left == 0 {
                break;
            }
        }

        // Prevent division by zero.
        if z_offset == 0 {
            z_offset = 1;
        }

        *inv_z = (1<<(3*FP_POS))/z_offset;
        *x_px = w/2+((self.near*(point_x_offset - x_offset))/z_offset);
        *y_px = h/2+((self.near*(y_offset - point_y_offset))/z_offset);
    }

    fn render_road_line<P: Painter>(
        &mut self,
        painter: &mut P,
        (w, h): (i32, i32),
        style: (SideInclination, SideInclination),
        base_tx: i32,  // FP1
        x_offset: i32, // FP1
        x_slope: i32,  // FP1
        x_curve: i32,  // FP1
        y: i32,
        z: i32,        // FP1
        z_local: i32,  // FP1
        t_global: i32, // FP1
        horizon: &mut [LineVisibility],
    ) {
        let tx_step = base_tx * z; // FP2

        let z_tmp = z_local >> (FP_POS / 2); // FP0.5

        let mut tx =
            tx_step * -w / 2 + (x_offset << FP_POS) + x_curve * z_tmp * z_tmp + x_slope * z_local; // FP2

        let road_width = painter.road_width();
        let road_left = 1 - (1 + road_width + tx) / tx_step;
        let road_right = 1 + (road_width - tx) / tx_step;

        let mut line = horizon[y as usize];
        let road_begin = road_left.max(line.begin as i32).min(line.end as i32);
        let road_end = road_right.max(line.begin as i32).min(line.end as i32);

        line.road = true;

        let side_color = painter.ground_color(0, t_global);
        // Left side of road
        match style.0 {
            SideInclination::Uphill => {
                for x in (line.begin as i32)..road_left {
                    let mut x0 = x;
                    let mut y_start = y+1;
                    if x0 >= w {
                        y_start -= x0 - w + 1;
                        x0 = w-1;
                    }

                    for y0 in (0..(y_start)).rev() {
                        let l = &mut horizon[y0 as usize];
                        l.begin = l.begin.max(x0 as i16 + 1);

                        if l.end as i32 > x0 {
                            painter.draw(x0, y0, &side_color);
                        }
                        x0 -= 1;
                        // TODO: Do this by calculating the active range
                        // instead!
                        if x0 < 0 {
                            break;
                        }
                    }
                }

                line.begin = 0;
            },
            SideInclination::Flat => {
                for x in (line.begin as i32)..road_begin {
                    painter.draw(x, y, &side_color);
                }
                line.begin = 0;
            },
            SideInclination::Downhill => {
                // TODO
                if line.begin > 0 {
                    line.begin = 0;
                } else {
                    line.begin = road_begin as i16;
                }
            }
        }

        // Center part of road, could be fully hidden in which case
        // road_begin >= road_end.
        tx += tx_step * road_begin;
        for x in road_begin..road_end {
            let color = painter.road_color(tx, t_global);
            painter.draw(x, y, &color);
            tx += tx_step;
        }

        // Right side of road
        match style.1 {
            SideInclination::Uphill => {
                for x in road_right..(line.end as i32) {
                    let mut x0 = x;
                    let mut y_start = y+1;
                    if x0 < 0 {
                        y_start += x0;
                        x0 = 0;
                    }

                    for y0 in (0..(y_start)).rev() {
                        let l = &mut horizon[y0 as usize];
                        l.end = l.end.min(x0 as i16);

                        if l.begin as i32 <= x0 {
                            painter.draw(x0, y0, &side_color);
                        }

                        x0 += 1;
                        // TODO: Do this by calculating the active range
                        // instead!
                        if x0 >= w {
                            break;
                        }
                    }
                }
                line.end = w as i16;
            },
            SideInclination::Flat => {
                let color = painter.ground_color(0, t_global);
                for x in road_end..(line.end as i32) {
                    painter.draw(x, y, &color);
                }
                line.end = w as i16;
            },
            SideInclination::Downhill => {
                // TODO
                if line.end < w as i16 {
                    line.end = w as i16;
                } else {
                    line.end = road_end as i16;
                }
            }
        }

        horizon[y as usize] = line;
    }

    fn render_road<P: Painter>(
        &mut self,
        painter: &mut P,
        (w, h): (i32, i32),
        y: &mut i32,
        style: (SideInclination, SideInclination),
        x_offset: i32, // FP1
        y_offset: i32, // FP1
        z_offset: i32, // FP1
        x_slope: i32,  // FP1
        y_slope: i32,  // FP1
        x_curve: i32,  // FP1
        y_curve: i32,  // FP1
        length: i32,   // FP1
        t_start: i32,  // FP1
        horizon: &mut [LineVisibility],
    ) {
        let base_tx = (1 << FP_POS) / self.near; // FP1

        if y_curve == 0 {
            // Simple plane
            let t_factor = isqrt((1 << (2 * FP_POS)) + y_slope * y_slope); // FP1
            while *y >= 0 {
                let vy = *y - h / 2;
                let div = (self.near * y_slope >> FP_POS) - vy;
                if div == 0 {
                    break;
                }

                let z = z_offset + (z_offset * vy - y_offset * self.near) / div; // FP1
                if z < 0 {
                    break;
                }

                let t_local = ((z - z_offset) * t_factor) >> FP_POS; // FP1
                if t_local < 0 || t_local >= length {
                    break;
                }

                self.render_road_line(
                    painter,
                    (w, h),
                    style,
                    base_tx,
                    x_offset,
                    x_slope,
                    x_curve,
                    *y,
                    z,
                    z - z_offset,
                    t_start + t_local,
                    horizon
                );
                *y -= 1;
            }
        } else {
            // Curved plane
            let inv_near = (1 << FP_POS) / self.near; // FP1
            let abs_y_curve = if y_curve < 0 { -y_curve } else { y_curve };
            let tsqrtcurve = isqrt(abs_y_curve << FP_POS); // FP1
            while *y >= 0 {
                let vy = (*y - h / 2) * inv_near; // FP1
                let vym = vy - y_slope; // FP1
                let disc = vym * vym + 4 * (((z_offset * vy) >> FP_POS) - y_offset) * y_curve; // FP2
                if disc < 0 {
                    break;
                }
                let sqrt_disc = isqrt(disc << (FP_POS / 2)) << (FP_POS - FP_POS / 4); // FP2
                let z = ((vym << FP_POS) - sqrt_disc) / (2 * y_curve); // FP1
                if z < 0 {
                    break;
                }

                let z_tmp = z >> (FP_POS / 2); // FP0.5
                let t_local = tsqrtcurve * ((z_tmp * z_tmp / 4) >> FP_POS); // FP1
                if t_local < 0 || t_local >= length {
                    break;
                }

                self.render_road_line(
                    painter,
                    (w, h),
                    style,
                    base_tx,
                    x_offset,
                    x_slope,
                    x_curve,
                    *y,
                    z + z_offset,
                    z,
                    t_start + t_local,
                    horizon
                );
                *y -= 1;
            }
        }
    }

    pub fn render<P: Painter, const W: i32, const H: i32>(
        &mut self,
        painter: &mut P,
        initial_x_offset: i32, // FP1
        initial_y_offset: i32, // FP1
    ) where [LineVisibility; i32_to_usize(H)]: Sized
    {
        let mut x_offset = initial_x_offset;
        let mut y_offset = initial_y_offset;
        let mut x_slope = 0;
        let mut y_slope = 0;
        let mut z_offset = 0;
        let mut t_start = self.cur_t;
        let mut y_start = H - 1;
        // If only VLAs were supported in Rust... If they were supported,
        // W and H would not have to be const generics and could be dynamically
        // determined instead.
        let mut horizon = [
            LineVisibility{road: false, begin: 0, end: W as i16};
            i32_to_usize(H)
        ];

        for render_segment in self.cur_segment..self.segments.len() {
            let local_t = if render_segment == self.cur_segment {
                self.cur_t - self.base_t
            } else {
                0
            };
            let seg = &self.segments[render_segment];
            self.render_road(
                painter,
                (W, H),
                &mut y_start,
                seg.side_style,
                x_offset,
                y_offset,
                z_offset,
                x_slope,
                y_slope,
                seg.x_curve,
                seg.y_curve,
                seg.length - local_t,
                t_start,
                &mut horizon
            );
            self.update_state_at_segment_length(
                render_segment,
                seg.length - local_t,
                &mut x_offset,
                &mut y_offset,
                &mut z_offset,
                &mut x_slope,
                &mut y_slope,
            );
            t_start += seg.length - local_t;
        }

        self.render_sky(painter, (W, H), &horizon);
    }
}
