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
//  everything - o SUPER GOLDEN: todo no da engine (osciladores, ruidos,
//               filtros, nao-lineares+oversampling, delays, fisica de
//               cordas/modal/sopros/voz, mod matrix, params, mono glide,
//               espacializacao, sidechain) e toda a DSL de score
//               (sections/arrange, tempo map, swing, humanize, acordes,
//               repeticao, set, automate). Qualquer DSP que mudar em
//               qualquer lugar, este fingerprint acusa.
//  demo       - peca curta com presets reais (cordas fisicas, bateria,
//               baixo); tambem e o exemplo do README
//  showcase   - todos os presets de SFX

#[test]
fn golden_everything() {
    golden_case(
        "everything",
        "tests/fixtures/everything.synth",
        "tests/fixtures/everything.score",
    );
}

//  mixer      - MIXER completo: canais, inserts, fx de usuario (locais e
//               importados, expansao recursiva), sends pre/pos, sidechain
//               entre canais, EQ parametrico, automacao de canal no score

#[test]
fn golden_mixer() {
    golden_case("mixer", "tests/fixtures/mixer.synth", "tests/fixtures/mixer.score");
}

#[test]
fn golden_demo() {
    golden_case("demo", "tests/fixtures/demo.synth", "tests/fixtures/demo.score");
}

#[test]
fn golden_sfx_showcase() {
    golden_case("sfx_showcase", "tests/fixtures/showcase.synth", "tests/fixtures/showcase.score");
}
