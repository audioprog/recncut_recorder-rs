use betrayer::{Icon, Menu, MenuItem, TrayEvent, TrayIconBuilder};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleRate;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};
use std::env;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

fn decode_icon_to_rgba(icon_data: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let decoder = png::Decoder::new(icon_data);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    Ok((buf, info.width, info.height))
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Signal {
    Quit,
}


fn build_menu() -> Menu<Signal> {
    Menu::new([
        MenuItem::button("Quit", Signal::Quit)
    ])
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "-s" {
        let host = cpal::default_host();
        let devices = host.input_devices().expect("Error retrieving input devices");
        let mut devices: Vec<_> = devices.collect();
        devices.sort_by_key(|device| device.name().unwrap_or_default());
        println!("List of audio input devices:");
        for (index, device) in devices.iter().enumerate() {
            println!("{}={}", index + 1, device.name().unwrap_or("Unknown".to_string()));
        }
    } else if args.len() > 2 {
        let device_index: usize = args[1].parse().expect("Please provide a valid device number");
        let host = cpal::default_host();
        let mut devices: Vec<_> = host.input_devices().expect("Error retrieving input devices").collect();
        devices.sort_by_key(|device| device.name().unwrap_or_default());
        if device_index == 0 || device_index > devices.len() {
            eprintln!("Invalid device number.");
            return;
        }

        let device = &devices[device_index - 1];
        println!("Selected device: {}", device.name().unwrap_or("Unknown".to_string()));

        let config = device.default_input_config().expect("Error retrieving default configuration");

        // Print SupportedBufferSize to the console
        println!("SupportedBufferSize: {:?}", config.buffer_size());

        let supported_config = cpal::StreamConfig {
            channels: config.channels(),
            sample_rate: SampleRate(48000),
            buffer_size: match config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => {
                    println!("Buffer Size Range: min = {}, max = {}", min, max);
                    let size = (*max).min(1024 * 4 * 1024); // Reduce the maximum buffer size
                    if size < *min {
                        println!("Buffer size adjusted to minimum: {}", min);
                        cpal::BufferSize::Fixed(*min)
                    } else if size >= *max {
                        println!("Buffer Size: Unknown");
                        cpal::BufferSize::Default
                    } else {
                        println!("Buffer Size: {}", size);
                        cpal::BufferSize::Fixed(size)
                    }
                }
                cpal::SupportedBufferSize::Unknown => {
                    println!("Buffer Size: Unknown");
                    cpal::BufferSize::Default
                }
            },
        };

        let file_name = if args[2..].join(" ").ends_with(".raw") {
            args[2..].join(" ")
        } else {
            args[2..].join(" ") + ".raw"
        };
        let file = Arc::new(Mutex::new(File::create(&file_name).expect("Error creating file")));

        println!("Writing audio data to file: {}", file_name);

        let stream = device.build_input_stream(
            &supported_config.into(),
            {
                let file = Arc::clone(&file);
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    let mut buffer = Vec::new();
                    for &sample in data {
                        buffer.extend_from_slice(&sample.to_le_bytes()[1..]);
                    }
                    let mut file = file.lock().unwrap();
                    file.write_all(&buffer).expect("Error writing to file");
                }
            },
            move |err| {
                eprintln!("Stream error: {}", err);
            },
            Some(std::time::Duration::from_secs(1)),
        ).expect("Error creating stream");

        println!("Stream started. Exit the program using the tray icon.");
        stream.play().expect("Error starting stream");

        let event_loop = EventLoop::<Signal>::with_user_event()
        .build().expect("Error creating event loop");
        
        let tray = TrayIconBuilder::new()
            .with_icon({
                let icon_data = include_bytes!("icon.png");
                let (rgba, width, height) = decode_icon_to_rgba(icon_data).expect("Error decoding icon");
                Icon::from_rgba(rgba, width, height).expect("Error loading icon")
            })
            .with_tooltip(&format!("Recording: {}", file_name))
            .with_menu(build_menu())
            // with `winit` feature:
            .build({
                let proxy = event_loop.create_proxy();
                move |s| {
                    if let TrayEvent::Menu(signal) = s {
                        let _ = proxy.send_event(signal);
                    }
                }
            }).expect("Error creating tray icon");

        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop.run(|event, evtl| {
            match event {
                Event::UserEvent(event) => {
                    println!("tray event: {:?}", event);
                    match event {
                        Signal::Quit => evtl.exit(),
                        _ => {}
                    }
                }
                _ => {}
            }
        }).expect("Error running event loop");
    } else {
        println!("Usage:");
        println!("  -s: Display list of audio input devices");
        println!("  <device number> <file name>: Select device and save audio data");
    }
}
