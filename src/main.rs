use std::{fs, io::Cursor};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result};
use image::{io::Reader as ImageReader, Rgba, RgbaImage};
use itertools::Itertools;
use nokhwa::{
    pixel_format::{RgbAFormat, RgbFormat},
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    CallbackCamera,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// the image should will always be a PNG.
    #[arg(short, long, default_value = "seed")]
    seed: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Calculate seed from captured image and save it to a file.
    Save {
        #[command(subcommand)]
        operation: Operation,
    },
    /// Load captured image and calculate a seed.
    Load {
        #[command(subcommand)]
        operation: Operation,
    },
}

#[derive(Subcommand)]
enum Operation {
    RNG,
    Hash {
        /// message to be hashed
        msg: String,
    },
}

fn capture_image(seed_file: &str) -> Result<RgbaImage> {
    let mut threaded = CallbackCamera::new(
        CameraIndex::Index(1),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate),
        |_| {},
    )
    .wrap_err("Failed to create a camera")?;
    threaded
        .open_stream()
        .wrap_err("Failed to open the camera")?;

    let frame = threaded
        .poll_frame()
        .wrap_err("Failed to capture a frame")?;
    let image = frame
        .decode_image::<RgbAFormat>()
        .wrap_err("Failed to decode the frame")?;

    let mut bytes: Vec<u8> = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageOutputFormat::Png)
        .wrap_err("Failed to encode captured image to PNG")?;

    fs::write(seed_file, &bytes).wrap_err("Failed to save captured image")?;

    let image = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png)
        .decode()
        .expect("decode previously encoded image; this should never fail");

    Ok(image.into_rgba8())
}

fn load_image(seed_file: &str) -> Result<RgbaImage> {
    let data = fs::read(seed_file).wrap_err(format!("Unable to read image: '{}'", &seed_file))?;

    let image = ImageReader::with_format(Cursor::new(data), image::ImageFormat::Png)
        .decode()
        .wrap_err("Failed to decode image")?;

    Ok(image.into_rgba8())
}

fn calculate_seed(image: RgbaImage) -> ([u8; 32], usize) {
    let pixels = image.pixels();
    let chunk = pixels.len() / 32;
    let seed: [u8; 32] = pixels
        .map(|p: &Rgba<u8>| p.0.into_iter().fold(0u8, |acc, p| acc.wrapping_add(p)))
        .chunks(chunk)
        .into_iter()
        .map(|chunk| chunk.fold(0u8, |acc, p| acc.wrapping_add(p)))
        .collect::<Vec<_>>()
        .try_into()
        .expect("turn Vec<u8> into [u8; 32], this should never fail");

    let seed_num: usize = seed.iter().map(|p| *p as usize).sum();

    (seed, seed_num)
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();

    cli.seed.push_str(".png");

    let image = match cli.command {
        Commands::Save { .. } => capture_image(&cli.seed),
        Commands::Load { .. } => load_image(&cli.seed),
    }?;

    let operation = match cli.command {
        Commands::Save { operation, .. } => operation,
        Commands::Load { operation, .. } => operation,
    };

    let (seed, seed_num) = calculate_seed(image);
    let mut rng = StdRng::from_seed(seed);

    match operation {
        Operation::RNG => {
            println!("seed: {}", seed_num);
            println!(
                "random numbers: {:?}",
                (0..10).map(|_| rng.gen_range(0..10)).collect::<Vec<_>>()
            );
            println!(
                "random bools: {:?}",
                (0..10).map(|_| rng.gen_bool(0.5)).collect::<Vec<_>>()
            );
        }
        Operation::Hash { msg } => {
            let hash = Sha256::new()
                .chain_update(seed)
                .chain_update(msg.into_bytes())
                .finalize();

            let hex_hash = base16ct::lower::encode_string(&hash);
            println!("hash: {}", hex_hash);
        }
    }

    Ok(())
}
