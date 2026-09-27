//! ONNX Runtime-backed vocal model. One session per song, reused for every segment.

use super::mdx::VocalModel;
use anyhow::{anyhow, Result};
use ndarray::{Array4, Ix4};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Once;

static INIT: Once = Once::new();

pub struct OnnxModel {
    session: Session,
    input: String,
    output: String,
}

impl OnnxModel {
    /// `runtime_lib` is the downloaded ONNX Runtime dylib (see `assets::RUNTIME`).
    pub fn load(runtime_lib: &Path, model: &Path, use_coreml: bool) -> Result<Self> {
        let lib = runtime_lib.to_path_buf();
        INIT.call_once(|| std::env::set_var("ORT_DYLIB_PATH", lib));
        let mut builder = Session::builder()
            .map_err(|e| anyhow!("{e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("{e}"))?;
        #[cfg(target_os = "macos")]
        if use_coreml {
            use ort::execution_providers::coreml::ModelFormat;
            use ort::execution_providers::CoreMLExecutionProvider;
            // MLProgram cuts CoreML's peak memory by ~7x over the default NeuralNetwork
            // format for this model (see docs/superpowers/spikes/2026-09-26-model-choice.md).
            let ep = CoreMLExecutionProvider::default().with_model_format(ModelFormat::MLProgram);
            builder = builder.with_execution_providers([ep.build()]).map_err(|e| anyhow!("{e}"))?;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = use_coreml;
        let session = builder.commit_from_file(model).map_err(|e| anyhow!("load model: {e}"))?;
        // Community conversions name tensors differently; read the names from the model.
        let input = session.inputs()[0].name().to_string();
        let output = session.outputs()[0].name().to_string();
        Ok(Self { session, input, output })
    }
}

impl VocalModel for OnnxModel {
    fn infer(&mut self, input: Array4<f32>) -> Result<Array4<f32>> {
        let tensor = Tensor::from_array(input).map_err(|e| anyhow!("{e}"))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input.as_str() => tensor])
            .map_err(|e| anyhow!("inference: {e}"))?;
        let view = outputs[self.output.as_str()]
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("{e}"))?;
        Ok(view.to_owned().into_dimensionality::<Ix4>()?)
    }
}
