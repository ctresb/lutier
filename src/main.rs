// lutier CLI: render .synth + .score to wav/mp3.
use lutier::render::render_song;
use lutier::wavio;
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
    };
    let positional: Vec<&String> = {
        let mut skip = false;
        args[1..]
            .iter()
            .filter(|a| {
                if skip {
                    skip = false;
                    return false;
                }
                if a.starts_with('-') {
                    skip = ["-o", "--seed"].contains(&a.as_str());
                    return false;
                }
                true
            })
            .collect()
    };
    let synth_path = positional.first().map(|s| s.as_str()).unwrap_or("examples/demo.synth");
    let score_path = positional.get(1).map(|s| s.as_str()).unwrap_or("examples/demo.score");
    let out_path = flag("-o").unwrap_or_else(|| "out/song.wav".to_string());
    let seed: u64 = flag("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let bench = args.iter().any(|a| a == "--bench");

    let sr = 44100.0;
    let synth_src = fs::read_to_string(synth_path).expect("cannot read .synth file");
    let score_src = fs::read_to_string(score_path).expect("cannot read .score file");

    let res = render_song(&synth_src, &score_src, sr, seed).unwrap_or_else(|e| {
        eprintln!("{}: {}", synth_path, e);
        std::process::exit(1);
    });
    let dur = res.buf.len() as f64 / sr;
    println!("rendered {:.1}s", dur);
    if bench {
        println!(
            "--bench: {:.2}s wall, {:.1}x realtime",
            res.render_seconds,
            dur / res.render_seconds.max(1e-9)
        );
    }

    if let Some(dir) = std::path::Path::new(&out_path).parent() {
        let _ = fs::create_dir_all(dir);
    }
    wavio::write_wav(&out_path, &res.buf, sr as u32).expect("wav write failed");
    println!("wrote {}", out_path);

    let mp3 = out_path.replace(".wav", ".mp3");
    match std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i", &out_path, "-b:a", "192k", &mp3])
        .status()
    {
        Ok(st) if st.success() => println!("wrote {}", mp3),
        _ => println!("(ffmpeg not available or failed; wav only)"),
    }
}
