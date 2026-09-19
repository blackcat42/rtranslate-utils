use ocr_rs::OcrEngine;
//use ocr_rs::{OcrEngine, OcrEngineConfig, Backend};
use image::{imageops, DynamicImage, ImageBuffer, Rgba};
use std::io::{self, Read, Write, BufRead};

#[derive(PartialEq)]
enum RunMode {
    Help,
    File,
    Pipe
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn round_up(value: u32, multiple: u32) -> u32 {
    value.div_ceil(multiple) * multiple
}

fn run() -> Result<(), Box<dyn std::error::Error>> {

    let mut file: Option<String> = None;
    let mut det_model = "ocr_models/PP-OCRv6_small_det.mnn".to_string();
    let mut rec_model = "ocr_models/PP-OCRv6_small_rec.mnn".to_string();
    let mut charset = "ocr_models/ppocr_keys_v6_small.txt".to_string();

    let mut mode = RunMode::Help;
    let mut token_start: Option<String> = None;
    let mut token_end: Option<String> = None;

    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            mode = RunMode::Help
        }

        if arg.starts_with("--file=") && let Some((_flag, value)) = arg.split_once('=') {
            mode = RunMode::File;
            file = Some(value.to_string());
        } else if (arg == "--file" || arg == "-f") && let Some(value) = args.next() {
            mode = RunMode::File;
            file = Some(value.to_string());
        }

        if arg == "--pipe" || arg == "-p" {
            mode = RunMode::Pipe;
        }

        if arg.starts_with("--token_start=") && let Some((_flag, value)) = arg.split_once('=') {
            token_start = Some(value.to_string());
        } else if arg == "--token_start" && let Some(value) = args.next() {
            token_start = Some(value.to_string());
        }
        if arg.starts_with("--token_end=") && let Some((_flag, value)) = arg.split_once('=') {
            token_end = Some(value.to_string());
        } else if arg == "--token_end" && let Some(value) = args.next() {
            token_end = Some(value.to_string());
        }

        if arg.starts_with("--det_model=") && let Some((_flag, value)) = arg.split_once('=') {
            det_model = value.to_string();
        } else if arg == "--det_model" && let Some(value) = args.next() {
            det_model = value.to_string();
        }

        if arg.starts_with("--rec_model=") && let Some((_flag, value)) = arg.split_once('=') {
            rec_model = value.to_string();
        } else if arg == "--rec_model" && let Some(value) = args.next() {
            rec_model = value.to_string();
        }

        if arg.starts_with("--charset=") && let Some((_flag, value)) = arg.split_once('=') {
            charset = value.to_string();
        } else if arg == "--charset" && let Some(value) = args.next() {
            charset = value.to_string();
        }
    }

    /*let config = OcrEngineConfig::new()
    .with_backend(Backend::OpenGL);*/

    let engine = OcrEngine::new(
        &det_model,
        &rec_model,
        &charset,
        None,
    );

    let mut handle = io::stdout().lock();

    if mode == RunMode::File && let Some(f) = file {
        let engine = engine?;
        let img = image::open(f)?;
        let results = engine.recognize(&img)?;
        if let Some(t_s) = token_start && let Some(t_e) = token_end {
            write!(handle, "{}", t_s)?; //<OCR_RESULTS_BEGIN>
            for item in results {
                writeln!(handle, "{}", item.text)?;
            }
            writeln!(handle, "{}", t_e)?; //<OCR_RESULTS_END>
        } else {
            for item in results {
                writeln!(handle, "{}", item.text)?;
            }
        }
    } else if mode == RunMode::Pipe {
        let engine = engine?;
        let mut stdin = io::stdin().lock();
        let mut width_bytes = [0u8; 4];
        let mut height_bytes = [0u8; 4];
        stdin.read_exact(&mut width_bytes)?;
        stdin.read_exact(&mut height_bytes)?;
        let w = u32::from_le_bytes(width_bytes);
        let h = u32::from_le_bytes(height_bytes);

        let mut buffer = Vec::new();
        stdin.read_to_end(&mut buffer)?;

        let rgba_img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(w, h, buffer)
        .ok_or("ImageBuffer error")?;

        let padded_w = round_up(w + (10 * 2), 32);
        let padded_h = round_up(h + (10 * 2), 32);
        let mut padded_img = ImageBuffer::from_pixel(padded_w, padded_h, Rgba([255, 255, 255, 255]));
        imageops::overlay(&mut padded_img, &rgba_img, 10, 10);

        let img = DynamicImage::ImageRgba8(padded_img);

        let results = engine.recognize(&img)?;
        if let Some(t_s) = token_start && let Some(t_e) = token_end {
            write!(handle, "{}", t_s)?; //<OCR_RESULTS_BEGIN>
            for item in results {
                writeln!(handle, "{}", item.text)?;
            }
            writeln!(handle, "{}", t_e)?; //<OCR_RESULTS_END>
        } else {
            for item in results {
                writeln!(handle, "{}", item.text)?;
            }
        }
    } else {
        println!("Usage: rt_ocr [OPTIONS]\n");
        println!("Options:");
        println!("  -h, --help            Show this help message");
        println!("  -f, --file <PATH>     Path to the image file for text recognition");
        println!("  -p, --pipe            Pipeline mode: read binary data from stdin");
        println!("  --det_model <PATH>    Path to the detection model (e.g. PP-OCRv6_small_det.mnn)");
        println!("  --rec_model <PATH>    Path to the recognition model (e.g. PP-OCRv6_small_rec.mnn)");
        println!("  --charset <PATH>      Path to the charset file (e.g. ppocr_keys_v6_small.txt)");
        println!("  --token_start <str>");
        println!("  --token_end <str>");
        println!("\nPress Enter to exit...");
        let mut iterator = std::io::stdin().lock().lines();
        iterator.next();
        std::process::exit(0);
    }
    Ok(())
}
