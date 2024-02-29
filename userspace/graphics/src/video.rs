use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Point, Size};
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_9X15};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb888, RgbColor};
use embedded_graphics::primitives::{
    Circle, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, StyledDrawable,
};
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::Drawable;
use emerald_runtime::keyboard::Keyboard;
use emerald_runtime::mouse::Mouse;
use graphics::{Graphics, MovingAverage};
use image::codecs::jpeg::JpegDecoder;
use image::ImageDecoder;
use std::fs::File;
use std::io::BufReader;
use std::thread::sleep;

const RECT_WIDTH: i32 = 3;
const PROGRESS_PADDING: u32 = 100;
const PRGRESS_HEIGHT: u32 = 30;
const MOUSE_RADIUS: u32 = 10;

fn main() {
    let mut args = std::env::args();
    let mut fps = 30.0;
    let mut file = None;

    let this_exe = args.next().unwrap();

    while let Some(arg) = args.next() {
        if arg == "-f" {
            let arg = args.next().unwrap_or_else(|| {
                println!("missing argument for -f");
                std::process::exit(1);
            });
            fps = arg.parse::<f64>().unwrap_or_else(|_| {
                println!("invalid argument for -f");
                std::process::exit(1);
            });
        } else {
            file = Some(arg);
        }
    }

    if file.is_none() {
        println!("Usage {this_exe} [-f <fps>] <video.zip>\n  Use './tools/video_to_zip.sh' to convert a video to a zip file with images.");
        std::process::exit(1);
    }

    let images_zip_file = File::open(file.unwrap()).unwrap();
    let reader = BufReader::new(images_zip_file);

    let mut zip = zip::ZipArchive::new(reader).unwrap();

    let num_files = zip.len();
    println!("num frames: {}", num_files);

    let frame_time = std::time::Duration::from_secs_f64(1.0 / fps);
    let duration = std::time::Duration::from_secs_f64(frame_time.as_secs_f64() * num_files as f64);

    let mut graphics = Graphics::new();
    let mut keyboard = Keyboard::new();
    let mut mouse = Mouse::new();
    let mut fps_average = MovingAverage::<100>::new();
    let fps_text_style = MonoTextStyle::new(&FONT_9X15, Rgb888::RED);
    let progress_text_style = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);
    let mut fps_text = "FPS: 0".to_string();

    graphics.clear(Rgb888::BLACK).ok();

    let image_w = graphics.size().width / 2;
    let image_h = graphics.size().height / 2;
    let padding_top = graphics.size().height / 7;

    let progress_width = graphics.size().width - PROGRESS_PADDING * 2;

    let rect_border_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb888::WHITE)
        .stroke_width(RECT_WIDTH as u32)
        .reset_fill_color()
        .build();
    let progress_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::new(159, 225, 245))
        .stroke_width(0)
        .build();

    let mut progress_bar = Rectangle::new(
        Point::new(
            PROGRESS_PADDING as i32,
            graphics.size().height as i32 - padding_top as i32,
        ),
        Size::new(0, PRGRESS_HEIGHT),
    );
    let progress_border = Rectangle::new(
        Point::new(
            PROGRESS_PADDING as i32 - RECT_WIDTH,
            graphics.size().height as i32 - padding_top as i32 - RECT_WIDTH,
        ),
        Size::new(
            graphics.size().width - PROGRESS_PADDING * 2 + RECT_WIDTH as u32 * 2,
            PRGRESS_HEIGHT + RECT_WIDTH as u32 * 2,
        ),
    );

    let mut i: usize = 1;
    let mut current_time;
    let mut paused = false;
    let mut clear_rect = None;
    let mut last_frame_image = None;
    let mut mouse_position = Point::zero();
    let mut mouse_pressed = false;
    let mut video_w = 0;
    let mut video_h = 0;
    // force the display to update, used when moving the cursor, so that we know where we are
    // if we are paused
    let mut force_read = false;
    loop {
        let time = std::time::SystemTime::now();

        // update
        {
            const INC_SIZE: usize = 10;
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
                    emerald_runtime::keyboard::KeyType::LeftArrow => {
                        if i > INC_SIZE + 1 {
                            i -= INC_SIZE;
                        } else {
                            i = 1;
                        }
                        force_read = true;
                    }
                    emerald_runtime::keyboard::KeyType::RightArrow => {
                        if i < zip.len() - INC_SIZE {
                            i += INC_SIZE;
                        } else {
                            i = zip.len();
                        }
                        force_read = true;
                    }
                    emerald_runtime::keyboard::KeyType::Space => {
                        paused = !paused;
                    }
                    _ => {}
                }
            }

            for event in mouse.iter_events() {
                mouse_position.x += event.x as i32;
                mouse_position.y -= event.y as i32;
                let old_mouse_pressed = mouse_pressed;
                mouse_pressed = event.buttons & emerald_runtime::mouse::buttons::LEFT != 0;

                if mouse_pressed {
                    // if we area inside the progress bar, jump to that position
                    if mouse_position.x > progress_border.top_left.x
                        && mouse_position.x < progress_border.bottom_right().unwrap().x
                        && mouse_position.y > progress_border.top_left.y
                        && mouse_position.y < progress_border.bottom_right().unwrap().y
                    {
                        let x = mouse_position.x - progress_border.top_left.x;
                        let percent = x as f32 / progress_border.size.width as f32;
                        i = (percent * num_files as f32) as usize;
                        force_read = true;
                    }
                }

                if old_mouse_pressed && !mouse_pressed {
                    let x = graphics.size().width / 2 - video_w as u32 / 2;
                    let y = padding_top;

                    // if we are inside the image, pause/unpause
                    if mouse_position.x > x as i32
                        && mouse_position.x < x as i32 + video_w as i32
                        && mouse_position.y > y as i32
                        && mouse_position.y < y as i32 + video_h as i32
                    {
                        paused = !paused;
                    }
                }
            }
            if mouse_position.x < 0 {
                mouse_position.x = 0;
            }
            if mouse_position.y < 0 {
                mouse_position.y = 0;
            }
            if mouse_position.x > graphics.size().width as i32 {
                mouse_position.x = graphics.size().width as i32;
            }
            if mouse_position.y > graphics.size().height as i32 {
                mouse_position.y = graphics.size().height as i32;
            }

            current_time = std::time::Duration::from_secs_f64(i as f64 * frame_time.as_secs_f64());
        }

        // render
        {
            // only draw the changed part
            if let Some((x, y, w, h)) = clear_rect {
                let rect = Rectangle {
                    top_left: Point::new(x as i32, y as i32),
                    size: Size::new(w as u32, h as u32),
                };
                graphics.fill_solid(&rect, Rgb888::BLACK).ok();
            }

            if (!paused || force_read) && i < zip.len() {
                let file = zip.by_name(&format!("i{:03}.jpg", i)).unwrap();
                i += 1;

                let mut jpg_decoder = JpegDecoder::new(file).unwrap();

                let format = jpg_decoder.color_type();
                assert_eq!(format, image::ColorType::Rgb8);

                let size = jpg_decoder.scale(image_w as u16, image_h as u16).unwrap();

                video_w = size.0 as u32;
                video_h = size.1 as u32;

                let mut img_bytes = vec![0; jpg_decoder.total_bytes() as usize];
                jpg_decoder.read_image(&mut img_bytes).unwrap();

                last_frame_image = Some(img_bytes);
                force_read = false;
            }

            // draw in the center
            let x = graphics.size().width / 2 - video_w as u32 / 2;
            let y = padding_top;
            if let Some(img_bytes) = &last_frame_image {
                graphics.draw_image(
                    &img_bytes,
                    (x as i32, y as i32),
                    (video_w as usize, video_h as usize),
                );
            }

            // draw white rect around the image
            let rect = Rectangle::new(
                Point::new(x as i32 - RECT_WIDTH, y as i32 - RECT_WIDTH),
                Size::new(
                    video_w as u32 + 2 * RECT_WIDTH as u32,
                    video_h as u32 + 2 * RECT_WIDTH as u32,
                ),
            );
            rect.draw_styled(&rect_border_style, &mut graphics).ok();
            progress_border
                .draw_styled(&rect_border_style, &mut graphics)
                .ok();

            let tmp = graphics.last_changed_rect();
            progress_bar.size.width = progress_width * i as u32 / num_files as u32;
            progress_bar
                .draw_styled(&progress_style, &mut graphics)
                .ok();
            progress_bar.size.width = progress_width * i as u32 / num_files as u32;
            progress_bar
                .draw_styled(&progress_style, &mut graphics)
                .ok();

            // draw progress text
            let time_now = current_time.as_secs_f64();
            let time_total = duration.as_secs_f64();
            let minutes_now = (time_now / 60.0).floor() as u64;
            let seconds_now = (time_now - minutes_now as f64 * 60.0).floor() as u64;
            let minutes_total = (time_total / 60.0).floor() as u64;
            let seconds_total = (time_total - minutes_total as f64 * 60.0).floor() as u64;

            let paused_str = if paused { "PAUSED" } else { "" };

            let progress_text = format!(
                "{:02}:{:02} / {:02}:{:02} {paused_str}",
                minutes_now, seconds_now, minutes_total, seconds_total
            );

            let text = Text::with_baseline(
                &progress_text,
                Point::new(
                    progress_border.top_left.x + 10,
                    progress_border.top_left.y - 30,
                ),
                progress_text_style,
                Baseline::Bottom,
            );
            text.draw(&mut graphics).ok();

            let text = Text::with_baseline(&fps_text, Point::zero(), fps_text_style, Baseline::Top);
            text.draw(&mut graphics).ok();

            Circle::new(Point::new(mouse_position.x, mouse_position.y), MOUSE_RADIUS)
                .draw_styled(&PrimitiveStyle::with_fill(Rgb888::BLUE), &mut graphics)
                .ok();

            clear_rect = graphics.last_changed_rect();
            graphics.merge_clear_rect(tmp);
        }

        graphics.present_changed();
        let remaining = frame_time.checked_sub(time.elapsed().unwrap());
        if let Some(remaining) = remaining {
            // if its 1ms or more, sleep
            if remaining.as_millis() > 0 {
                sleep(remaining);
            } else {
                // spin
                let time = std::time::SystemTime::now();
                while time.elapsed().unwrap() < remaining {
                    core::hint::spin_loop();
                }
            }
        }
        let fps = 1.0 / time.elapsed().unwrap().as_secs_f64();
        fps_average.add(fps);
        fps_text = format!("FPS: {:.2}", fps_average.average());
    }
}
