use std::io::{self, Read, Write};
use std::process::{Command, Stdio};

fn grayscale_frame(frame: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut gray_frame = vec![0; width * height];
    for y in 0..height {
        for x in 0..width {
            let offset = (y * width + x) * 3;
            let r = frame[offset] as f32;
            let g = frame[offset + 1] as f32;
            let b = frame[offset + 2] as f32;
            let gray = 0.2989 * r + 0.5870 * g + 0.1140 * b;
            gray_frame[y * width + x] = gray.round() as u8;
        }
    }
    gray_frame
}

fn main() -> io::Result<()> {
    let mut ffmpeg = Command::new("ffmpeg")
        .args(&[
            "-i", "input.mkv",
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
            "-s:v", "1920x1080",
            "-pix_fmt", "gray",
            "-r", "60",
            "-i", "pipe:",
            "-c:v", "libx264",
            "-preset", "slow",
            "-crf", "22",
            "-y", "output2.mp4",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let ffmpeg_stdin = ffmpeg2.stdin.as_mut().unwrap();
    let mut buf = [0; 1920 * 1080 * 3];
    loop {
        match ffmpeg.stdout.as_mut().unwrap().read_exact(&mut buf) {
            Ok(_) => {
                let gray_frame = grayscale_frame(&buf, 1920, 1080);
                ffmpeg_stdin.write_all(&gray_frame)?;
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