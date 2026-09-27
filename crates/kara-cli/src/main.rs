use anyhow::Result;
use clap::{Parser, Subcommand};
use kara_core::assets;
use kara_core::audio::{self, Stereo, SAMPLE_RATE};
use kara_core::separate::mdx::{self, MdxParams};
use kara_core::separate::onnx::OnnxModel;
use kara_core::separate::CHUNK_LEN;
use kara_core::store::Store;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "kara", about = "kara-always-oki engine")]
struct Cli {
    /// Data folder (defaults to ~/Library/Application Support/kara-always-oki)
    #[arg(long, global = true)]
    data: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Measure separation speed and memory for one model on one song.
    Bench {
        input: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        compensate: f32,
        /// Run on CPU instead of CoreML.
        #[arg(long)]
        cpu: bool,
        /// Use this ONNX Runtime dylib instead of downloading one.
        #[arg(long)]
        runtime: Option<PathBuf>,
        /// Write vocals.wav and instrumental.wav here for a listening test.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(cli.data.clone().unwrap_or_else(Store::default_root));
    match cli.cmd {
        Cmd::Bench { input, model, compensate, cpu, runtime, out } => {
            bench(&store, &input, &model, compensate, !cpu, runtime, out.as_deref())
        }
    }
}

pub(crate) fn runtime_lib(store: &Store, over: Option<PathBuf>) -> Result<PathBuf> {
    match over {
        Some(p) => Ok(p),
        None => assets::ensure(&assets::RUNTIME, &store.runtime_dir(), &mut |done, total| {
            if let Some(t) = total {
                eprint!("\rDownloading ONNX Runtime… {}%", done * 100 / t.max(1));
            }
        }),
    }
}

/// Peak physical memory footprint, matching `/usr/bin/time -l`'s "peak memory footprint".
fn peak_footprint_mb() -> Option<f64> {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let buffer = &mut info as *mut libc::rusage_info_v4 as *mut libc::rusage_info_t;
    let ret = unsafe { libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V4, buffer) };
    (ret == 0).then(|| info.ri_lifetime_max_phys_footprint as f64 / (1024.0 * 1024.0))
}

fn write_wav(path: &Path, a: &Stereo) -> Result<()> {
    let spec = hound::WavSpec { channels: 2, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for (l, r) in a.left.iter().zip(&a.right) {
        w.write_sample((l.clamp(-1.0, 1.0) * 32767.0) as i16)?;
        w.write_sample((r.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn bench(store: &Store, input: &Path, model_path: &Path, compensate: f32, coreml: bool, runtime: Option<PathBuf>, out: Option<&Path>) -> Result<()> {
    let lib = runtime_lib(store, runtime)?;
    let t0 = Instant::now();
    let mix = audio::decode_file(input)?.audio;
    let decode_s = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let mut model = OnnxModel::load(&lib, model_path, coreml)?;
    let load_s = t1.elapsed().as_secs_f64();
    let params = MdxParams { n_fft: 7680, hop: 1024, dim_f: 3072, dim_t: 256, compensate };
    let (mut vocals, mut inst) = (Stereo::default(), Stereo::default());
    let mut first_chunk_s = None;
    let t2 = Instant::now();
    mdx::separate(&mut model, &params, &mix, CHUNK_LEN, 0, &AtomicBool::new(false), |c| {
        first_chunk_s.get_or_insert(t2.elapsed().as_secs_f64());
        vocals.append(&c.vocals);
        inst.append(&c.inst);
        Ok(())
    })?;
    let sep_s = t2.elapsed().as_secs_f64();
    let audio_s = mix.len() as f64 / SAMPLE_RATE as f64;
    println!("song             {:.1} s", audio_s);
    println!("decode           {:.2} s", decode_s);
    println!("model load       {:.2} s  ({})", load_s, if coreml { "CoreML" } else { "CPU" });
    println!("first 10 s chunk {:.2} s", first_chunk_s.unwrap_or(0.0));
    println!("separation       {:.1} s  = {:.2}x real time", sep_s, audio_s / sep_s);
    match peak_footprint_mb() {
        Some(mb) => println!("peak memory      {mb:.0} MB"),
        None => println!("peak memory      unavailable"),
    }
    if let Some(dir) = out {
        std::fs::create_dir_all(dir)?;
        write_wav(&dir.join("vocals.wav"), &vocals)?;
        write_wav(&dir.join("instrumental.wav"), &inst)?;
        println!("wrote {}", dir.display());
    }
    Ok(())
}
