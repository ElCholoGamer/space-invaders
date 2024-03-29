#![windows_subsystem = "windows"]

use std::time::{Duration, Instant};
use colored::Colorize;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;

use core::{Emulator, ExecutionStatus, EmulatorEvent, Sound};
use frontend::input;
use frontend::{WIDTH, HEIGHT};
use frontend::audio::AudioManager;

const SCALE_X: f32 = 2.0;
const SCALE_Y: f32 = 2.5;
const FPS: f64 = 60.0;
const CYCLES_PER_FRAME: u32 = (2_000_000.0 / FPS) as u32;

fn main() {
    let program = include_bytes!("../assets/invaders");

    run(program).unwrap_or_else(|e| {
        eprintln!("{} {}", "Error:".red().bold(), e.to_string().red())
    });
}

fn run(program: &[u8]) -> Result<(), String> {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let window = video_subsystem
        .window("Space Invaders", (WIDTH as f32 * SCALE_X) as u32, (HEIGHT as f32 * SCALE_Y) as u32)
        .position_centered()
        .build().expect("could not build window");

    let audio_subsystem = sdl_context.audio()?;
    let mut audio = AudioManager::new(audio_subsystem)?;

    let mut event_pump = sdl_context.event_pump()?;
    let mut canvas = window.into_canvas().present_vsync().build().expect("could not build renderer");

    canvas.set_scale(SCALE_X, SCALE_Y)?;
    canvas.present();

    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_target(PixelFormatEnum::RGB24, HEIGHT, WIDTH)
        .expect("could not create texture");

    let mut pixel_data = [0; (WIDTH * HEIGHT * 3) as usize];

    let mut emulator = Emulator::new(program);
    let mut save_state: Option<Emulator> = None;
    let mut paused = false;

    let now = Instant::now();
    let mut frame: u64 = 0;

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown { keycode: Some(keycode), keymod, .. } if frontend::has_ctrl(keymod) => {
                    match keycode {
                        Keycode::Q => break 'main,
                        Keycode::S => save_state = Some(emulator.clone()),
                        Keycode::D => {
                            if let Some(state) = &save_state {
                                emulator = state.clone();
                            }
                        }
                        Keycode::R => {
                            emulator.cpu_mut().reset();
                            audio.stop_all();
                        }
                        _ => {}
                    };
                }
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => paused = !paused,
                Event::KeyDown { keycode: Some(k), .. } => input::handle_keydown(k, &mut emulator),
                Event::KeyUp { keycode: Some(k), .. } => input::handle_keyup(k, &mut emulator),
                _ => {}
            }
        }

        if !paused {
            let mut cycles = 0;
            let mut isr_done = false;

            while cycles < CYCLES_PER_FRAME {
                let status = emulator.step().map_err(|e| e.to_string())?;
                match status {
                    ExecutionStatus::Continue(c) => cycles += c * 4,
                    ExecutionStatus::Halt => break,
                }

                // Handle sounds
                if let Some(event) = emulator.event() {
                    match event {
                        EmulatorEvent::PlaySound(sound) => audio.play(sound),
                        EmulatorEvent::StopSound(Sound::UFO) => audio.stop(Sound::UFO),
                        _ => {}
                    }
                }

                // Mid-line interrupt
                if !isr_done && cycles >= CYCLES_PER_FRAME / 2 {
                    emulator.cpu_mut().interrupt(1);
                    isr_done = true;
                }
            }

            emulator.cpu_mut().interrupt(2); // VBlank interrupt
        }

        if frontend::update_pixel_data(&mut pixel_data, emulator.video_ram()) {
            texture.update(None, &pixel_data, HEIGHT as usize * 3).unwrap();
            canvas.copy_ex(&texture, None, Rect::from_center(canvas.viewport().center(), HEIGHT, WIDTH), -90.0, None, false, false)?;
            canvas.present();
        }

        frame += 1;
        let next_frame = ((1_000.0 / FPS) * frame as f64) as u64;
        let sleep_ms = next_frame.saturating_sub(now.elapsed().as_millis() as u64);
        spin_sleep::sleep(Duration::from_millis(sleep_ms));
    }

    Ok(())
}
