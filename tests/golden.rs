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

// Fixtures sao scores PROJETADOS para cobertura (nao musicas legadas):
//  demo     - peca curta com presets reais (cordas fisicas, bateria,
//             baixo) + master chain; tambem e o exemplo do README
//  features - a DSL de score inteira (sections/arrange, swing, humanize,
//             automate, tempo map, acordes, repeticao)
//  physics  - um synth por primitivo fisico (bow, string, flute, reed,
//             brass, voz, modal2, nwave+convolve)
//  showcase - todos os presets de SFX

#[test]
fn golden_demo() {
    golden_case("demo", "tests/fixtures/demo.synth", "tests/fixtures/demo.score");
}

#[test]
fn golden_score_features() {
    golden_case("features", "tests/fixtures/features.synth", "tests/fixtures/features.score");
}

#[test]
fn golden_physics() {
    golden_case("physics", "tests/fixtures/physics.synth", "tests/fixtures/physics.score");
}

#[test]
fn golden_sfx_showcase() {
    golden_case("sfx_showcase", "tests/fixtures/showcase.synth", "tests/fixtures/showcase.score");
}
