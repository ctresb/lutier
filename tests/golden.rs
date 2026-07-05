// Golden audio regression tests (tier3 §2).
// Update intentionally with: UPDATE_GOLDEN=1 cargo test
use lutier::fp;
use lutier::render::render_song;
use std::fs;

fn golden_case(name: &str, synth: &str, score: &str) {
    let synth_src = fs::read_to_string(synth).expect("synth file");
    let score_src = fs::read_to_string(score).expect("score file");
    let res = render_song(&synth_src, &score_src, 44100.0, 1).expect("render");
    let got = fp::fingerprint(&res.buf, 44100);
    fs::create_dir_all("tests/golden").ok();
    let path = format!("tests/golden/{}.fp", name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        fs::write(&path, &got).expect("write golden");
        eprintln!("updated {}", path);
        return;
    }
    let want = match fs::read_to_string(&path) {
        Ok(w) => w,
        Err(_) => {
            fs::write(&path, &got).expect("write golden");
            eprintln!("created {} (first run)", path);
            return;
        }
    };
    if want != got {
        panic!(
            "golden mismatch for {}:\n{}\nrun UPDATE_GOLDEN=1 cargo test to accept",
            name,
            fp::diff(&want, &got)
        );
    }
}

#[test]
fn golden_song_vila() {
    golden_case("vila", "examples/songs/vila/vila.synth", "examples/songs/vila/vila.score");
}

#[test]
fn golden_song_funk() {
    golden_case("funky", "examples/songs/funky/funky.synth", "examples/songs/funky/funky.score");
}

#[test]
fn golden_sfx_showcase() {
    golden_case("sfx_showcase", "examples/sfx/showcase.synth", "examples/sfx/showcase.score");
}
