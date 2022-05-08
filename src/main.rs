use std::env;
use std::fs::File;
use std::str::FromStr;
use png;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn select_block(img: &[u8], width: usize, x: usize, y: usize, output: &mut Vec<u8>) {
    output.extend_from_slice(&img[((y + 0)*width)+x .. ((y + 0)*width)+(x+4)]);
    output.extend_from_slice(&img[((y + 1)*width)+x .. ((y + 1)*width)+(x+4)]);
    output.extend_from_slice(&img[((y + 2)*width)+x .. ((y + 2)*width)+(x+4)]);
    output.extend_from_slice(&img[((y + 3)*width)+x .. ((y + 3)*width)+(x+4)]);
}

fn get_options_table(val0: u8, val1: u8, flip: bool) -> [u8; 8] {
    let v0 = val0 as u32;
    let v1 = val1 as u32;
    return [val0, val1, ((2*v0 + v1)/3) as u8, ((v0 + 2*v1)/3) as u8, 
            if flip {val0} else {val1}, if flip {val1} else {val0}, ((v0 + v1) / 2) as u8, 0]
}

fn choose_codeword(decreasing_order: bool, residuals: &[i32]) -> u32 {
    let start_i = if decreasing_order {0} else {4};
    let mut best_i = 0;
    let mut best_dist = 99999;
    for i in 0..4 {
        if residuals[start_i + i] < best_dist {
            best_i = i;
            best_dist = residuals[start_i + i]
        }
    }
    return best_i as u32;
}

fn compress_block(block: &[u8]) -> [u8; 6] {
    let max_val = *block.iter().max().unwrap();
    let min_val = *block.iter().min().unwrap();
    let options = get_options_table(max_val, min_val, false);

    let mut residuals = [0i32; 16*8];
    let mut total_residuals = [0i32; 8];
    for (i, val) in block.iter().enumerate() {
        let res_block_offset = i*8;
        for (j, option) in options.iter().enumerate() {
            let residual = (*val as i32 - *option as i32).abs();
            residuals[res_block_offset+j] = residual;
            total_residuals[j] += residual;
        }
    }
    let decreasing_order = total_residuals[2] + total_residuals[3] < total_residuals[6] + total_residuals[7];
    let val0 = if decreasing_order { max_val } else { min_val };
    let val1 = if decreasing_order { min_val } else { max_val };
    let mut codes : u32 = 0;
    for i in 0 .. 16 {
        codes |= choose_codeword(decreasing_order, &residuals[i*8 .. (i+1)*8]) << (2*i);
    }

    return [val0, val1, ((codes >> 24) & 0xFF) as u8, ((codes >> 16) & 0xFF) as u8, ((codes >> 8) & 0xFF) as u8, ((codes >> 0) & 0xFF) as u8]
}

fn compress(img: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    result.reserve_exact(width*height);

    let mut block = vec![0; 16];
    
    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            block.clear();
            select_block(img, width, x, y, &mut block);
            result.extend(compress_block(&block[..]));
        }
    }

    return Ok(result)
}

fn decompress_pixel(img: &[u8], width: usize, height: usize, x: usize, y: usize) -> u8 {
    let block_idx = ((y/4) * (width/4)) + (x / 4);
    let block_offset = block_idx * 6;
    let val0 = img[block_offset + 0];
    let val1 = img[block_offset + 1];
    let code_bytes = &img[block_offset + 2 .. block_offset + 6];
    let codes = u32::from_be_bytes(code_bytes.try_into().unwrap());
    let pixel_idx = ((y % 4) * 4) + (x % 4);
    let code = (codes >> (pixel_idx*2)) & 3;
    let idx = if val0 > val1 {code + 0} else {code + 4};
    return get_options_table(val0, val1, true)[idx as usize];
}

fn decompress(img: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let mut result = vec![0; width*height];
    for y in 0 .. height {
        for x in 0 .. width {
            result[(y * width) + x] = decompress_pixel(img, width, height, x, y);
        }
    }

    Ok(result)
}

static USAGE : &str = "Usage: $0 <encode/decode> <input_path> <output_path> [<width> <height>]";

fn main() -> Result<()> {
    let mode : std::string::String = env::args().nth(1).ok_or(USAGE)?;
    let input_file = env::args().nth(2).ok_or(USAGE)?;
    let output_file = env::args().nth(3).ok_or(USAGE)?;

    if mode.eq_ignore_ascii_case("encode") {
        let decoder = png::Decoder::new(File::open(input_file)?);
        let mut reader = decoder.read_info()?;
        let (color_type, bit_depth) = reader.output_color_type();
        if color_type != png::ColorType::Grayscale { return Err("Only no-alpha grayscale input PNGs allowed".into()); }
        if bit_depth != png::BitDepth::Eight { return Err("Only 8-bit input PNGs allowed".into()); }

        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf)?;
        let bytes = &buf[..info.buffer_size()];

        let compressed = compress(bytes, info.width as usize, info.height as usize)?;
        std::fs::write(output_file, compressed)?
    } else if mode.eq_ignore_ascii_case("decode") {
        let width = u32::from_str(&env::args().nth(4).ok_or(USAGE)?).expect(USAGE);
        let height = u32::from_str(&env::args().nth(5).ok_or(USAGE)?).expect(USAGE);
        let input = std::fs::read(input_file)?;
        let decompressed = decompress(&input[..], width as usize, height as usize)?;

        let file = File::create(output_file)?;
        let ref mut w = std::io::BufWriter::new(file);

        let mut encoder = png::Encoder::new(w, width, height); // Width is 2 pixels and height is 1.
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&decompressed[..]).unwrap();
    } else { return Err(USAGE.into()); }

    Ok(())
}
