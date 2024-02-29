//! This is a demo of using the graphics API to draw a bouncing circle and text on the screen.

use std::thread::sleep;

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, OriginDimensions, Point, Size},
    mono_font::{ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::{Rgb888, RgbColor},
    primitives::{Circle, Primitive, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
    transform::Transform,
    Drawable,
};
use emerald_runtime::keyboard::Keyboard;
use graphics::{Graphics, MovingAverage};

fn main() {
    let mut graphics = Graphics::new();

    let mut circle =
        Circle::new(Point::new(64, 64), 64).into_styled(PrimitiveStyle::with_fill(Rgb888::RED));
    let mut v = Point::new(10, 10);

    // Create a new character style
    let style = MonoTextStyle::new(&FONT_9X15, Rgb888::WHITE);
    let mut fps_average = MovingAverage::<100>::new();
    let mut fps_text = "FPS: 0".to_string();

    graphics.clear(Rgb888::BLACK).ok();
    let mut changed_rect = graphics.last_changed_rect();

    let mut keyboard = Keyboard::new();

    loop {
        let time = std::time::SystemTime::now();

        // update
        {
            for key in keyboard.iter_keys() {
                if !key.pressed {
                    continue;
                }
                match key.key_type {
                    emerald_runtime::keyboard::KeyType::Escape => {
                        graphics.clear(Rgb888::BLACK).ok();
                        graphics.present_changed();
                        std::process::exit(0);
                    }
                    _ => {}
                }
            }

            // move the circle
            circle.translate_mut(v);

            // bounce the circle
            if circle.bounding_box().top_left.x < 0
                || circle.bounding_box().bottom_right().unwrap().x >= graphics.size().width as i32
            {
                v.x = -v.x;
            }
            if circle.bounding_box().top_left.y < 0
                || circle.bounding_box().bottom_right().unwrap().y >= graphics.size().height as i32
            {
                v.y = -v.y;
            }
        }
        // render
        let previous_changed_rect = changed_rect;
        {
            // only draw the changed part
            if let Some((x, y, w, h)) = changed_rect {
                let rect = Rectangle {
                    top_left: Point::new(x as i32, y as i32),
                    size: Size::new(w as u32, h as u32),
                };
                graphics.fill_solid(&rect, Rgb888::BLACK).ok();
            }
            graphics.clear_changed();
            circle.draw(&mut graphics).ok();
            let text = Text::with_baseline(&fps_text, Point::new(0, 0), style, Baseline::Top);
            text.draw(&mut graphics).ok();
        }
        // take the changes before presenting, as it will be cleared after presenting
        changed_rect = graphics.last_changed_rect();
        graphics.merge_clear_rect(previous_changed_rect);
        graphics.present_changed();
        let remaining =
            std::time::Duration::from_millis(1000 / 60).checked_sub(time.elapsed().unwrap());
        if let Some(remaining) = remaining {
            sleep(remaining);
        }
        let fps = 1.0 / time.elapsed().unwrap().as_secs_f64();
        fps_average.add(fps);
        fps_text = format!("FPS: {:.2}", fps_average.average());
    }
}
