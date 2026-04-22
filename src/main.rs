use b::{encode_rgba, decode, ColorCount, EccLevel, JabConfig};
use std::{env, fs, io::{self, Read, Write}, path::Path};

fn usage() {
    eprintln!(
        "Usage:\n\
         b encode [--colors N] [--ecc 0-3] [--module-px N] <infile|-> <outfile.[png|rgba]|->\n\
         b decode [--colors N] [--ecc 0-3] [--module-px N] <infile.[png|rgba]> <outfile|->\n\n\
         Use '-' for stdin/stdout\n\
         Supported output formats: .png, .rgba (raw RGBA with 8-byte header)"
    );
}

fn write_png(path: &str, rgba: &[u8], width: u32, height: u32) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let w = io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    writer.write_image_data(rgba).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}

fn read_png(path: &str) -> io::Result<(Vec<u8>, u32, u32)> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    buf.truncate(info.buffer_size());
    Ok((buf, info.width, info.height))
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { usage(); return Ok(()); }

    let cmd = &args[1];
    let mut colors   = 8u32;
    let mut ecc      = 1u32;
    let mut mod_px   = 4u32;
    let mut pos_args: Vec<String> = Vec::new();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--colors" | "-c"    => { i += 1; colors  = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(8); }
            "--ecc" | "-e"       => { i += 1; ecc     = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1); }
            "--module-px" | "-m" => { i += 1; mod_px  = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(4); }
            _                    => pos_args.push(args[i].clone()),
        }
        i += 1;
    }

    let ecc_level = match ecc {
        0 => EccLevel::Low,
        1 => EccLevel::Medium,
        2 => EccLevel::High,
        _ => EccLevel::Ultra,
    };
    let color_count = ColorCount::from_u32(colors).unwrap_or(ColorCount::C8);
    let cfg = JabConfig { colors: color_count, ecc: ecc_level, module_size: mod_px, ..Default::default() };

    match cmd.as_str() {
        "encode" | "e" => {
            if pos_args.len() < 2 { usage(); return Ok(()); }
            let data = if pos_args[0] == "-" {
                let mut buf = Vec::new();
                io::stdin().read_to_end(&mut buf)?;
                buf
            } else {
                fs::read(&pos_args[0])?
            };
            let (rgba, w, h) = encode_rgba(&data, &cfg).expect("encode failed");

            let outpath = &pos_args[1];

            if outpath == "-" {
                io::stdout().write_all(&w.to_le_bytes())?;
                io::stdout().write_all(&h.to_le_bytes())?;
                io::stdout().write_all(&rgba)?;
                eprintln!("encoded → {} × {} px ({} bytes RGBA)", w, h, rgba.len());
            } else {
                let ext = Path::new(outpath).extension().and_then(|s| s.to_str()).unwrap_or("");
                match ext.to_lowercase().as_str() {
                    "png" => {
                        write_png(outpath, &rgba, w, h)?;
                        eprintln!("encoded → {} × {} px (PNG)", w, h);
                    }
                    _ => {
                        let mut out = fs::File::create(outpath)?;
                        out.write_all(&w.to_le_bytes())?;
                        out.write_all(&h.to_le_bytes())?;
                        out.write_all(&rgba)?;
                        eprintln!("encoded → {} × {} px ({} bytes RGBA)", w, h, rgba.len());
                    }
                }
            }
        }
        "decode" | "d" => {
            if pos_args.is_empty() { usage(); return Ok(()); }

            let inpath = &pos_args[0];
            let ext = Path::new(inpath).extension().and_then(|s| s.to_str()).unwrap_or("");

            let (rgba, w, h) = match ext.to_lowercase().as_str() {
                "png" => read_png(inpath)?,
                _ => {
                    let raw = fs::read(inpath)?;
                    let w = u32::from_le_bytes(raw[0..4].try_into().unwrap());
                    let h = u32::from_le_bytes(raw[4..8].try_into().unwrap());
                    (raw[8..].to_vec(), w, h)
                }
            };

            let data = decode(&rgba, w, h, &cfg).expect("decode failed");
            if pos_args.len() < 2 || pos_args[1] == "-" {
                io::stdout().write_all(&data)?;
            } else {
                fs::write(&pos_args[1], &data)?;
            }
            eprintln!("decoded → {} bytes", data.len());
        }
        _ => { usage(); }
    }
    Ok(())
}
