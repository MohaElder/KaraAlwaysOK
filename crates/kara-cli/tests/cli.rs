use std::process::Command;

fn kara(data: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kara")).arg("--data").arg(data).args(args).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}

#[test]
fn add_then_search_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("kitchen-light-waltz.wav");
    let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&wav, spec).unwrap();
    for i in 0..44_100 {
        let s = ((i as f32 * 0.05).sin() * 8000.0) as i16;
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    let data = dir.path().join("data");

    let (ok, out, _) = kara(&data, &["add", wav.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("kitchen light waltz"), "{out}");

    let (ok, out, _) = kara(&data, &["search", "kitch"]);
    assert!(ok);
    assert!(out.contains("kitchen light waltz"), "{out}");
}

#[test]
fn streaming_links_are_refused_with_a_plain_message() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, _, err) = kara(dir.path(), &["add", "https://open.spotify.com/track/abc"]);
    assert!(!ok);
    assert!(err.contains("can't be downloaded"), "{err}");
}
