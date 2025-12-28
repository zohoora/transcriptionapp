//! YAMNet audio event detection module
//!
//! Uses YAMNet ONNX model to detect audio events in continuous audio streams.
//! YAMNet is trained on AudioSet with 521 audio event classes.
//!
//! ## Implementation
//! - Sliding window: 3 seconds (48000 samples) with 1s hop (yamnet_3s model)
//! - Outputs logits (not probabilities) - threshold ~1.5 works well
//!
//! ## Model
//! yamnet_3s.onnx (~16MB) - 3-second input variant

mod sliding_window;

use anyhow::Result;
use std::path::Path;
use tracing::info;

use super::CoughEvent;
pub use sliding_window::SlidingWindow;

#[cfg(feature = "biomarkers")]
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};

/// Complete YAMNet class names for all 521 AudioSet classes
const CLASS_NAMES: &[&str] = &[
    "Speech", "Child speech", "Conversation", "Narration", "Babbling",
    "Speech synthesizer", "Shout", "Bellow", "Whoop", "Yell",
    "Children shouting", "Screaming", "Whispering", "Laughter", "Baby laughter",
    "Giggle", "Snicker", "Belly laugh", "Chuckle", "Crying/sobbing",
    "Baby cry", "Whimper", "Wail/moan", "Sigh", "Singing",
    "Choir", "Yodeling", "Chant", "Mantra", "Child singing",
    "Synthetic singing", "Rapping", "Humming", "Groan", "Grunt",
    "Whistling", "Breathing", "Wheeze", "Snoring", "Gasp",
    "Pant", "Snort", "Cough", "Throat clearing", "Sneeze",
    "Sniff", "Run", "Shuffle", "Walk/footsteps", "Chewing",
    "Biting", "Gargling", "Stomach rumble", "Burping", "Hiccup",
    "Fart", "Hands", "Finger snapping", "Clapping", "Heart sounds",
    "Heart murmur", "Cheering", "Applause", "Chatter", "Crowd",
    "Hubbub", "Children playing", "Animal", "Domestic animals", "Dog",
    "Bark", "Yip", "Howl", "Bow-wow", "Growling",
    "Whimper (dog)", "Cat", "Purr", "Meow", "Hiss",
    "Caterwaul", "Livestock", "Horse", "Clip-clop", "Neigh",
    "Cattle", "Moo", "Cowbell", "Pig", "Oink",
    "Goat", "Bleat", "Sheep", "Fowl", "Chicken/rooster",
    "Cluck", "Cock-a-doodle-doo", "Turkey", "Gobble", "Duck",
    "Quack", "Goose", "Honk", "Wild animals", "Roaring cats",
    "Roar", "Bird", "Bird call", "Chirp/tweet", "Squawk",
    "Pigeon/dove", "Coo", "Crow", "Caw", "Owl",
    "Hoot", "Bird flight", "Canidae", "Rodents", "Mouse",
    "Patter", "Insect", "Cricket", "Mosquito", "Fly",
    "Buzz", "Bee/wasp", "Frog", "Croak", "Snake",
    "Rattle", "Whale vocalization", "Music", "Musical instrument", "Plucked string",
    "Guitar", "Electric guitar", "Bass guitar", "Acoustic guitar", "Steel guitar",
    "Tapping (guitar)", "Strum", "Banjo", "Sitar", "Mandolin",
    "Zither", "Ukulele", "Keyboard", "Piano", "Electric piano",
    "Organ", "Electronic organ", "Hammond organ", "Synthesizer", "Sampler",
    "Harpsichord", "Percussion", "Drum kit", "Drum machine", "Drum",
    "Snare drum", "Rimshot", "Drum roll", "Bass drum", "Timpani",
    "Tabla", "Cymbal", "Hi-hat", "Wood block", "Tambourine",
    "Rattle (instrument)", "Maraca", "Gong", "Tubular bells", "Mallet percussion",
    "Marimba/xylophone", "Glockenspiel", "Vibraphone", "Steelpan", "Orchestra",
    "Brass instrument", "French horn", "Trumpet", "Trombone", "Bowed string",
    "String section", "Violin/fiddle", "Pizzicato", "Cello", "Double bass",
    "Wind instrument", "Flute", "Saxophone", "Clarinet", "Harp",
    "Bell", "Church bell", "Jingle bell", "Bicycle bell", "Tuning fork",
    "Chime", "Wind chime", "Change ringing", "Harmonica", "Accordion",
    "Bagpipes", "Didgeridoo", "Shofar", "Theremin", "Singing bowl",
    "Scratching", "Pop music", "Hip hop", "Beatboxing", "Rock music",
    "Heavy metal", "Punk rock", "Grunge", "Progressive rock", "Rock and roll",
    "Psychedelic rock", "R&B", "Soul music", "Reggae", "Country",
    "Swing music", "Bluegrass", "Funk", "Folk music", "Middle Eastern music",
    "Jazz", "Disco", "Classical music", "Opera", "Electronic music",
    "House music", "Techno", "Dubstep", "Drum and bass", "Electronica",
    "EDM", "Ambient music", "Trance music", "Latin music", "Salsa",
    "Flamenco", "Blues", "Children's music", "New-age music", "Vocal music",
    "A capella", "African music", "Afrobeat", "Christian music", "Gospel",
    "Asian music", "Carnatic music", "Bollywood", "Ska", "Traditional music",
    "Indie music", "Song", "Background music", "Theme music", "Jingle",
    "Soundtrack", "Lullaby", "Video game music", "Christmas music", "Dance music",
    "Wedding music", "Happy music", "Sad music", "Tender music", "Exciting music",
    "Angry music", "Scary music", "Wind", "Rustling leaves", "Wind noise",
    "Thunderstorm", "Thunder", "Water", "Rain", "Raindrop",
    "Rain on surface", "Stream", "Waterfall", "Ocean", "Waves/surf",
    "Steam", "Gurgling", "Fire", "Crackle", "Vehicle",
    "Boat", "Sailboat", "Rowboat/canoe", "Motorboat", "Ship",
    "Motor vehicle", "Car", "Car horn", "Toot", "Car alarm",
    "Power windows", "Skidding", "Tire squeal", "Car passing by", "Race car",
    "Truck", "Air brake", "Truck horn", "Reversing beeps", "Ice cream truck",
    "Bus", "Emergency vehicle", "Police siren", "Ambulance siren", "Fire truck siren",
    "Motorcycle", "Traffic noise", "Rail transport", "Train", "Train whistle",
    "Train horn", "Train wagon", "Train wheels", "Subway/metro", "Aircraft",
    "Aircraft engine", "Jet engine", "Propeller", "Helicopter", "Airplane",
    "Bicycle", "Skateboard", "Engine", "Light engine", "Dental drill",
    "Lawn mower", "Chainsaw", "Medium engine", "Heavy engine", "Engine knocking",
    "Engine starting", "Idling", "Accelerating/vroom", "Door", "Doorbell",
    "Ding-dong", "Sliding door", "Slam", "Knock", "Tap",
    "Squeak", "Cupboard", "Drawer", "Dishes/pots/pans", "Cutlery",
    "Chopping (food)", "Frying (food)", "Microwave oven", "Blender", "Water tap",
    "Sink", "Bathtub", "Hair dryer", "Toilet flush", "Toothbrush",
    "Electric toothbrush", "Vacuum cleaner", "Zipper", "Keys jangling", "Coin dropping",
    "Scissors", "Electric shaver", "Shuffling cards", "Typing", "Typewriter",
    "Computer keyboard", "Writing", "Alarm", "Telephone", "Telephone ring",
    "Ringtone", "Telephone dialing", "Dial tone", "Busy signal", "Alarm clock",
    "Siren", "Civil defense siren", "Buzzer", "Smoke detector", "Fire alarm",
    "Foghorn", "Whistle", "Steam whistle", "Mechanisms", "Ratchet",
    "Clock", "Tick", "Tick-tock", "Gears", "Pulleys",
    "Sewing machine", "Mechanical fan", "Air conditioning", "Cash register", "Printer",
    "Camera", "SLR camera", "Tools", "Hammer", "Jackhammer",
    "Sawing", "Filing", "Sanding", "Power tool", "Drill",
    "Explosion", "Gunshot", "Machine gun", "Fusillade", "Artillery fire",
    "Cap gun", "Fireworks", "Firecracker", "Burst/pop", "Eruption",
    "Boom", "Wood", "Chop", "Splinter", "Crack",
    "Glass", "Chink/clink", "Shatter", "Liquid", "Splash",
    "Slosh", "Squish", "Drip", "Pour", "Trickle",
    "Gush", "Fill (liquid)", "Spray", "Pump (liquid)", "Stir",
    "Boiling", "Sonar", "Arrow", "Whoosh/swoosh", "Thump/thud",
    "Thunk", "Electronic tuner", "Effects unit", "Chorus effect", "Basketball bounce",
    "Bang", "Slap/smack", "Whack", "Smash/crash", "Breaking",
    "Bouncing", "Whip", "Flap", "Scratch", "Scrape",
    "Rub", "Roll", "Crushing", "Crumpling", "Tearing",
    "Beep/bleep", "Ping", "Ding", "Clang", "Squeal",
    "Creak", "Rustle", "Whir", "Clatter", "Sizzle",
    "Clicking", "Clickety-clack", "Rumble", "Plop", "Jingle/tinkle",
    "Hum", "Zing", "Boing", "Crunch", "Silence",
    "Sine wave", "Harmonic", "Chirp tone", "Sound effect", "Pulse",
    "Inside small room", "Inside large room", "Inside public space", "Outside urban", "Outside rural",
    "Reverberation", "Echo", "Noise", "Environmental noise", "Static",
    "Mains hum", "Distortion", "Sidetone", "Cacophony", "White noise",
    "Pink noise", "Throbbing", "Vibration", "Television", "Radio",
    "Field recording",
];

/// Get the class name for a given class ID
fn get_class_name(class_id: usize) -> &'static str {
    CLASS_NAMES.get(class_id).copied().unwrap_or("Unknown")
}

/// YAMNet audio event classifier
#[cfg(feature = "biomarkers")]
pub struct YamnetProvider {
    session: Session,
    sliding_window: SlidingWindow,
}

#[cfg(feature = "biomarkers")]
impl YamnetProvider {
    /// Create a new YAMNet provider
    pub fn new(model_path: &Path, n_threads: usize) -> Result<Self> {
        info!("Loading YAMNet model from {:?}", model_path);

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Failed to set optimization level: {}", e))?
            .with_intra_threads(n_threads)
            .map_err(|e| anyhow::anyhow!("Failed to set threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;

        info!("YAMNet model loaded successfully");

        Ok(Self {
            session,
            sliding_window: SlidingWindow::new(),
        })
    }

    /// Process an audio chunk and return any detected cough events
    pub fn process_chunk(
        &mut self,
        samples: &[f32],
        timestamp_ms: u64,
        threshold: f32,
    ) -> Result<Vec<CoughEvent>> {
        let mut events = Vec::new();

        // Add samples to sliding window
        self.sliding_window.add_samples(samples);

        // Process any complete windows
        while let Some((window, window_start_offset)) = self.sliding_window.next_window() {
            // Calculate timestamp for this window
            let window_timestamp_ms =
                timestamp_ms.saturating_sub((samples.len() as u64 * 1000) / 16000)
                    + (window_start_offset as u64 * 1000) / 16000;

            // Run inference
            let predictions = self.infer(&window)?;

            // Check ALL classes above threshold
            // Note: yamnet_3s outputs logits, not probabilities - threshold ~1.5 works well
            for (class_id, score) in predictions.iter().enumerate() {
                if *score > threshold {
                    let label = get_class_name(class_id);
                    // Skip generic "Speech" class (too noisy during conversation)
                    if class_id == 0 {
                        continue;
                    }
                    events.push(CoughEvent {
                        timestamp_ms: window_timestamp_ms,
                        duration_ms: 3000, // 3 second window (yamnet_3s model)
                        confidence: *score,
                        label: label.to_string(),
                    });
                }
            }
        }

        Ok(events)
    }

    /// Run YAMNet inference on a 1-second window
    fn infer(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        use tracing::debug;

        // YAMNet expects [batch, samples] = [1, 16000]
        debug!("YAMNet inference: {} samples", samples.len());

        let input_tensor = Value::from_array(([1_usize, samples.len()], samples.to_vec()))
            .map_err(|e| anyhow::anyhow!("Failed to create input tensor: {}", e))?;

        let outputs = self.session
            .run(ort::inputs![input_tensor])
            .map_err(|e| anyhow::anyhow!("Inference failed: {}", e))?;

        // Output is [batch, num_classes] = [1, 521]
        let output = outputs
            .iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No output from YAMNet"))?;

        let tensor = output.1
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract tensor: {}", e))?;

        // tensor is (Shape, &[f32]) - extract the data slice
        let predictions: Vec<f32> = tensor.1.to_vec();

        // Log top 3 predictions above 1.0 for debugging
        let mut scored: Vec<(usize, f32)> = predictions
            .iter()
            .enumerate()
            .filter(|(_, &s)| s > 1.0)
            .map(|(i, &s)| (i, s))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if !scored.is_empty() {
            let top: Vec<String> = scored
                .iter()
                .take(3)
                .map(|(id, score)| format!("{}={:.1}", get_class_name(*id), score))
                .collect();
            info!("YAMNet top: {}", top.join(", "));
        }

        Ok(predictions)
    }
}

/// Stub for when biomarkers feature is disabled
#[cfg(not(feature = "biomarkers"))]
pub struct YamnetProvider;

#[cfg(not(feature = "biomarkers"))]
impl YamnetProvider {
    pub fn new(_model_path: &Path, _n_threads: usize) -> Result<Self> {
        anyhow::bail!("YAMNet requires the 'biomarkers' feature")
    }

    pub fn process_chunk(
        &mut self,
        _samples: &[f32],
        _timestamp_ms: u64,
        _threshold: f32,
    ) -> Result<Vec<CoughEvent>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sliding_window_creation() {
        let mut window = SlidingWindow::new();
        assert!(window.next_window().is_none());
    }
}
