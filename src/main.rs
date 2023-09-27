use std::{process::{Stdio, exit }, sync::{atomic::{AtomicBool, Ordering, AtomicU16}, Arc}, io::Write};
use ctrlc;
use tokio::{io::{self, AsyncWriteExt, AsyncReadExt}, process::Command, task::{JoinHandle}};
use crossbeam::channel::{bounded, Receiver, Sender};

fn pixel_compare(a: &(u8, u8, u8), b: &(u8, u8, u8)) -> std::cmp::Ordering {
    // Compute the brightness of each pixel as the sum of the RGB components
    let brightness_a = a.0 as u32 + a.1 as u32 + a.2 as u32;
    let brightness_b = b.0 as u32 + b.1 as u32 + b.2 as u32;
    brightness_a.cmp(&brightness_b)
}

fn sort_pixels_by_luminance(frame: Vec<u8>, width: usize, height: usize) -> Vec<u8> {
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

    let mut final_frame = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let pixel = frame2d[x][y];
            final_frame.push(pixel.0);
            final_frame.push(pixel.1);
            final_frame.push(pixel.2);
        }
    }

    final_frame
}

async fn get_resolution(file_path: &str) -> (usize, usize) {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("default=noprint_wrappers=1")
        .arg(file_path)
        .output()
        .await.expect("failed to execute ffprobe command");

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

async fn get_video_packet_count(file_path: &str) -> Result<u16, Box<dyn std::error::Error>> {
    // Create the FFprobe command
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-count_packets")
        .arg("-show_entries")
        .arg("stream=nb_read_packets")
        .arg("-of")
        .arg("csv=p=0")
        .arg(file_path)
        .stdout(Stdio::piped())
        .spawn()?;

    // Get the output stream of the command
    let stdout = output.stdout.ok_or("Failed to capture standard output")?;

    // Read the output as a string
    let mut output_str = String::new();
    io::BufReader::new(stdout).read_to_string(&mut output_str).await?;

    // Parse the output string as a u16
    let nb_read_packets= output_str.trim().parse::<u16>().unwrap();
    Ok(nb_read_packets)
}

fn create_ffmpeg_input(file_path: &str) -> Result<tokio::process::Child, io::Error> {
    let command = Command::new("ffmpeg")
    .args(&[
        "-i", file_path,
        "-vf", "format=rgb24",
        "-f", "rawvideo",
        "-",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn();
    command
}

fn create_ffmpeg_output(width: usize, height: usize, file_path: &str) -> Result<tokio::process::Child, io::Error> {
    let command = Command::new("ffmpeg")
    .args(&[
        "-f", "rawvideo",
        "-s:v", &format!("{}x{}", width, height),
        "-pix_fmt", "rgb24",
        "-r", "60",
        "-i", "pipe:",
        "-c:v", "libx264",
        "-preset", "medium",
        "-crf", "22",
        "-y", file_path,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn();
    command
}

async fn frame_extracting_worker(
    producer_tx: Sender<(i32, Vec<u8>)>,
    file_path: &str,
    width: usize,
    height: usize,
    process: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut frame_n = 0;
    let mut ffmpeg = create_ffmpeg_input(file_path)?;
    let buf = vec![0; width * height * 3];
    while process.load(Ordering::SeqCst) {
        let b = &mut buf.clone();
        match ffmpeg.stdout.as_mut().unwrap().read_exact(b).await {
            Ok(_) => {
                if let Ok(_res) = producer_tx.send((frame_n, b.to_vec())) {
                    frame_n += 1;
                    //println!("sent frame #{}", frame_n);
                }
            },
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

async fn frame_sorting_worker(
    _id: usize,
    consumer_rx: Receiver<(i32, Vec<u8>)>,
    sorter_tx: Sender<(i32, Vec<u8>)>,
    width: usize,
    height: usize,
    process: Arc<AtomicBool>,
    sorted_frames: Arc<AtomicU16>,
) {
    while process.load(Ordering::SeqCst) {
        if let Ok(frame) = consumer_rx.recv() {
            let sorted_frame = sort_pixels_by_luminance(frame.1, width, height);
            while sorted_frames.load(Ordering::SeqCst) != frame.0 as u16 && process.load(Ordering::SeqCst) {}
            if let Ok(_res) = sorter_tx.send((frame.0, sorted_frame)) {
                // println!("[{}]: {}", id, frame.0);
            }
        }
    }
}

async fn frame_encoding_worker(
    sorter_rx: Receiver<(i32, Vec<u8>)>,
    output_path: &str,
    width: usize,
    height: usize,
    process: Arc<AtomicBool>,
    sorted_frames: Arc<AtomicU16>,
    frame_count: u16,
    tasks: Vec<JoinHandle<()>>,
) -> io::Result<()>  {
    let mut ffmpeg = create_ffmpeg_output(width, height, output_path)?;
    let ffmpeg_stdin = ffmpeg.stdin.as_mut().unwrap();
    while sorted_frames.load(Ordering::SeqCst) < frame_count && process.load(Ordering::SeqCst) {
        if !sorter_rx.is_empty() {
            if let Ok(frame) = sorter_rx.recv() {
                print!("\r{}/{}", frame.0 + 1, frame_count);
                std::io::stdout().flush()?;
                sorted_frames.fetch_add(1, Ordering::SeqCst);
                ffmpeg_stdin.write_all(&frame.1).await?;
            }
        }
    }
    println!();

    process.store(false, Ordering::SeqCst);
    for t in tasks {
        t.abort();
    }

    Ok(())
}

async fn process_video(
    file_path: &str, 
    output_path: &str, 
    width: usize, 
    height: usize, 
    frame_count: u16
) -> io::Result<()> {
    let process = Arc::new(AtomicBool::new(true));
    let p = process.clone();
    let num_workers = 14;
    println!("Starting {} workers...", num_workers);

    let (producer_tx, consumer_rx) = bounded(1); // Adjust buffer size as needed
    let (sorter_tx, sorter_rx) = bounded(num_workers);

    let ep = process.clone();
    let fp: String = String::from(file_path);
    let extractor = tokio::spawn(async move {
        let res = frame_extracting_worker(producer_tx, &fp, width, height, ep).await;
        if res.is_err() {
            println!("finished extracting {:?}", res);
        }
    });

    let sorted_frames = Arc::new(AtomicU16::new(0));
    let mut tasks = Vec::new();
    for i in 0..num_workers {
        let consumer_rx = consumer_rx.clone();
        let sorter_tx = sorter_tx.clone();
        let sorted_frames = sorted_frames.clone();

        let sp = process.clone();
        let st = tokio::spawn(async move {
            frame_sorting_worker(i, consumer_rx, sorter_tx, width, height, sp, sorted_frames).await;
        });

        tasks.push(st);
    }

    let cp = process.clone();
    let op = String::from(output_path);
    let consumer_task = tokio::spawn(async move {
        let res = frame_encoding_worker(sorter_rx, &op, width, height, cp, sorted_frames, frame_count, tasks).await;
        if res.is_ok() {
            println!("finished encoding");
        }
    });

    ctrlc::set_handler(move || {
        p.store(false, Ordering::SeqCst);
        extractor.abort();
    })
    .expect("Error setting Ctrl-C handler");

    while !consumer_task.is_finished() {}

    Ok(())
}

#[tokio::main]
async fn main() {
    let filename = "replay.mkv";
    let output = "sorted.mp4";
    let (width, height) = get_resolution(filename).await;
    if let Ok(frame_count) = get_video_packet_count(filename).await {
        println!("frames: {}", frame_count);
        let status = process_video(filename, output, width, height, frame_count).await;
        if status.is_err() {
            println!("oh no");
        }
    }
}