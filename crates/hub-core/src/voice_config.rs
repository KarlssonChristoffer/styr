//! The dictation settings. They live apart from `voice` so `config` can hold them without
//! depending on the code that runs whisper.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// A whisper.cpp `whisper-cli` to use instead of downloading one.
    pub command: Option<PathBuf>,
    /// Model file name in the voice folder.
    pub model: String,
    pub model_url: String,
    /// Zips with whisper-cli for Windows x64, tried in order. A release is pinned because the
    /// newest one does not always have its binaries attached yet.
    pub binary_urls: Vec<String>,
    /// "sv" or "en". "auto" made whisper guess the language of every few seconds of speech,
    /// which it gets wrong on short phrases (Swedish came out as English or Norwegian), so it is
    /// read as Swedish. English words inside Swedish speech still come out as spoken.
    pub language: String,
    /// Beam search width for a phrase's final text. Wider is more accurate and slower; the live
    /// preview always decodes greedily.
    pub beam_size: u32,
    /// Use the graphics card when there is an Nvidia one: null finds out, false never.
    pub gpu: Option<bool>,
    /// whisper.cpp built for CUDA, with its runtime, for Windows x64. Tried in order.
    pub gpu_binary_urls: Vec<String>,
    /// The bigger model the graphics card can afford, in the same voice folder.
    pub gpu_model: String,
    pub gpu_model_url: String,
    /// The biggest model, for a phrase's final text on the graphics card; the live preview stays
    /// on `gpu_model`, which keeps up with redoing it every half second.
    pub final_model: String,
    pub final_model_url: String,
    /// Silero voice activity detection for whisper-server: it cuts silence away before whisper
    /// hears it, so whisper does not make up words over a pause.
    pub vad_model: String,
    pub vad_model_url: String,
    /// A key (F13 to F24) held down while dictating, set in a voice chat app as its "Push to Mute"
    /// keybind, so the people in the call do not hear what is said to Hub.
    pub mute_key: Option<String>,
    /// When dictation stops, a small model tidies what was said (punctuation, tech words, no
    /// "eh"), on the subscription. Off unless chosen in Settings > Dictation.
    pub polish: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            command: None,
            // KBLab's Swedish fine-tune: same size and speed as OpenAI's small, a third of its
            // word errors on Swedish (7% against 21% on FLEURS). kb-whisper-medium is a little
            // better again at three times the size.
            model: "kb-whisper-small-q5_0.bin".into(),
            model_url: "https://huggingface.co/KBLab/kb-whisper-small/resolve/main/ggml-model-q5_0.bin".into(),
            binary_urls: vec![
                "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-x64.zip".into(),
                "https://sourceforge.net/projects/whisper-cpp.mirror/files/v1.9.2/whisper-bin-x64.zip/download".into(),
                "https://github.com/ggml-org/whisper.cpp/releases/latest/download/whisper-bin-x64.zip".into(),
            ],
            language: "sv".into(),
            beam_size: 5,
            gpu: None,
            gpu_binary_urls: vec![
                "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-cublas-12.4.0-bin-x64.zip".into(),
                "https://sourceforge.net/projects/whisper-cpp.mirror/files/v1.9.2/whisper-cublas-12.4.0-bin-x64.zip/download".into(),
            ],
            // A fifth fewer word errors than small (6.6% against 7.3% on FLEURS) and, on a graphics
            // card, still quick enough to redo the live preview every second.
            gpu_model: "kb-whisper-medium-q5_0.bin".into(),
            gpu_model_url: "https://huggingface.co/KBLab/kb-whisper-medium/resolve/main/ggml-model-q5_0.bin".into(),
            // The fewest errors on Swedish of any model found (5.4% on FLEURS against 6.6% for
            // medium, 9.6% for Nvidia's Canary and 15% for Parakeet); about 1 GB.
            final_model: "kb-whisper-large-q5_0.bin".into(),
            final_model_url: "https://huggingface.co/KBLab/kb-whisper-large/resolve/main/ggml-model-q5_0.bin".into(),
            vad_model: "ggml-silero-v5.1.2.bin".into(),
            vad_model_url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin".into(),
            mute_key: None,
            polish: false,
        }
    }
}
