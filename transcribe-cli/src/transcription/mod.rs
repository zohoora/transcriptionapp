pub mod segment;
pub mod whisper_provider;

pub use segment::{ProviderType, Segment, SessionRecord};
pub use whisper_provider::{Utterance, WhisperModel, WhisperProvider};
