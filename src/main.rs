use std::io::{self, Read, Write};
use std::process::{Command, Stdio, exit};

// Define a function to compare two pixels
fn pixel_compare(a: &(u8, u8, u8), b: &(u8, u8, u8)) -> std::cmp::Ordering {
    // Compute the brightness of each pixel as the sum of the RGB components
    let brightness_a = a.0 as u32 + a.1 as u32 + a.2 as u32;
    let brightness_b = b.0 as u32 + b.1 as u32 + b.2 as u32;
    
    // Compare the brightness values to determine the order
    brightness_a.cmp(&brightness_b)
}

fn sort_pixels_by_luminance(frame: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    let mut frame2d = Vec::new();
    for x in 0..width {
        let mut col = Vec::new();
        for y in 0..height {
            let i = (y * width + x) * 3;
            let r = frame[i];
            let g = frame[i + 1];
            let b = frame[i + 2];
            col.push((r, g, b));
        }
        frame2d.push(col);
    }

    for i in 0..frame2d.len() {
        frame2d[i].sort_by(pixel_compare);
    }

    // let final_frame = frame2d
    // .concat()
    // .iter()
    // .cloned()
    // .flat_map(|(a, b, c)| vec![a, b, c])
    // .collect();

    let mut final_frame = Vec::new();

    for y in 0..height {
        for x in 0..width {
            if let Some(pixel) = frame2d.get(x)?.get(y) {
                // let g = rgb_to_grayscale(pixel.0, pixel.1, pixel.2);
                // final_frame.push(g);
                // final_frame.push(g);
                // final_frame.push(g);
                final_frame.push(pixel.0);
                final_frame.push(pixel.1);
                final_frame.push(pixel.2);
            }
        }
    }

    Some(final_frame)
}

fn get_resolution() -> (usize, usize) {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("default=noprint_wrappers=1")
        .arg("clip.mkv")
        .output()
        .expect("failed to execute ffprobe command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut width: Option<usize> = None;
    let mut height: Option<usize> = None;

    for line in stdout.lines() {
        if let Some(index) = line.find('=') {
            let (key, value) = line.split_at(index + 1);
            match key.trim() {
                "width=" => width = value.trim().parse::<usize>().ok(),
                "height=" => height = value.trim().parse::<usize>().ok(),
                _ => (),
            }
        }
    }

    if let (Some(width), Some(height)) = (width, height) {
        println!("The video resolution is {}x{}", width, height);
    } else {
        eprintln!("Failed to parse video resolution from ffprobe output");
        exit(0x0100);
    }

    (width.unwrap(), height.unwrap())
}

fn process_video(width: usize, height: usize) -> io::Result<()> {
    let mut ffmpeg = Command::new("ffmpeg")
    .args(&[
        "-i", "clip.mkv",
        "-vf", "format=rgb24",
        "-f", "rawvideo",
        "-",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()?;

let mut ffmpeg2 = Command::new("ffmpeg")
    .args(&[
        "-f", "rawvideo",
        "-s:v", &format!("{}x{}", width, height),
        "-pix_fmt", "rgb24",
        "-r", "24",
        "-i", "pipe:",
        "-c:v", "libx264",
        "-preset", "medium",
        "-crf", "22",
        "-y", "output2.mp4",
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()?;

let ffmpeg_stdin = ffmpeg2.stdin.as_mut().unwrap();
let mut buf = vec![0; width * height * 3];
loop {
    match ffmpeg.stdout.as_mut().unwrap().read_exact(&mut buf) {
        Ok(_) => {
            //let gray_frame = grayscale_frame(&buf, width, height);
            
            if let Some(sorted_frame) = sort_pixels_by_luminance(&buf, width, height){
                ffmpeg_stdin.write_all(&sorted_frame)?;
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
        Err(e) => return Err(e),
    }
}

let ffmpeg_status = ffmpeg.wait()?;
let ffmpeg2_status = ffmpeg2.wait()?;

if !ffmpeg_status.success() {
    return Err(io::Error::new(io::ErrorKind::Other, "FFmpeg exited with error"));
}
if !ffmpeg2_status.success() {
    return Err(io::Error::new(io::ErrorKind::Other, "FFmpeg 2 exited with error"));
}

Ok(())
}

fn main() -> io::Result<()> {
    let (width, height) = get_resolution();
    let status = process_video(width, height);

    if status.is_err() {
        return status
    }
    Ok(())
}