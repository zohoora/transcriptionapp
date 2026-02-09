//! End-to-End Integration Tests
//!
//! These tests verify that the full transcription pipeline works from audio input
//! through to archived SOAP notes retrievable from history. They exercise the real
//! STT Router and LLM Router services, testing both session mode and continuous
//! charting mode.
//!
//! # Architecture
//!
//! Tests are layered so failures are easy to diagnose:
//!
//! ```text
//! Layer 1: STT Router          — WebSocket streaming works, returns transcript
//! Layer 2: LLM Router          — SOAP generation and encounter detection work
//! Layer 3: Local Archive       — Save and retrieve sessions with SOAP notes
//! Layer 4: Session Mode E2E    — Audio → Transcript → SOAP → Archive → History
//! Layer 5: Continuous Mode E2E — Audio → Transcript → Encounter Detection → SOAP → Archive
//! ```
//!
//! # Running
//!
//! These tests require live services on the local network:
//! - STT Router at http://10.241.15.154:8001
//! - LLM Router at http://10.241.15.154:8080
//!
//! ```bash
//! # Run all E2E tests
//! cargo test e2e_ -- --ignored --nocapture
//!
//! # Run a specific layer
//! cargo test e2e_layer1 -- --ignored --nocapture
//! cargo test e2e_layer4 -- --ignored --nocapture
//! ```
//!
//! # Test Audio
//!
//! Tests use a programmatically generated 2-second sine wave for STT connectivity
//! checks. Since non-speech audio produces empty transcripts, SOAP and archive tests
//! use a fixture transcript of a simulated clinical encounter. This ensures the LLM
//! and archive layers are always tested, even if the STT Router returns no text.

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::continuous_mode::{
        build_encounter_detection_prompt, parse_encounter_detection,
    };
    use crate::llm_client::LLMClient;
    use crate::local_archive;
    use crate::whisper_server::WhisperServerClient;
    use chrono::Utc;

    // ========================================================================
    // Test Constants
    // ========================================================================

    /// STT Router URL (must be running for E2E tests)
    const STT_ROUTER_URL: &str = "http://10.241.15.154:8001";

    /// LLM Router URL (must be running for E2E tests)
    const LLM_ROUTER_URL: &str = "http://10.241.15.154:8080";

    /// STT alias for streaming transcription
    const STT_ALIAS: &str = "medical-streaming";

    /// Model for SOAP note generation
    const SOAP_MODEL: &str = "soap-model-fast";

    /// Model for encounter detection
    const FAST_MODEL: &str = "fast-model";

    /// LLM client ID
    const LLM_CLIENT_ID: &str = "ai-scribe";

    // ========================================================================
    // Fixture Data
    // ========================================================================

    /// Simulated clinical encounter transcript.
    ///
    /// Used when STT returns empty text (non-speech test audio) or when testing
    /// LLM and archive layers independently. Contains a realistic doctor-patient
    /// exchange with enough clinical detail for SOAP generation.
    const FIXTURE_TRANSCRIPT: &str = "\
Speaker 1: Good morning, how are you feeling today?
Speaker 2: Hi doctor. I've been having these headaches for about two weeks now. \
They're mostly on the right side and they get worse in the afternoon.
Speaker 1: I see. On a scale of one to ten, how would you rate the pain?
Speaker 2: Usually about a six or seven. Sometimes it goes up to an eight.
Speaker 1: Are you experiencing any nausea, vision changes, or sensitivity to light?
Speaker 2: A little bit of light sensitivity, but no nausea.
Speaker 1: Have you tried any over-the-counter medications?
Speaker 2: I've been taking ibuprofen but it only helps for a couple of hours.
Speaker 1: Okay. Let's do a neurological exam and check your blood pressure. \
Based on what you're describing, this sounds like it could be tension headaches \
or possibly migraines. I'd like to start you on sumatriptan as needed and \
schedule a follow-up in two weeks. If the headaches get worse or you develop \
new symptoms, come back sooner.
Speaker 2: Thank you doctor. I'll see you in two weeks then.
Speaker 1: Take care. We'll see you soon.";

    /// Simulated continuous mode transcript with segment indices and speaker labels.
    /// Formatted as the encounter detector expects (numbered segments).
    const FIXTURE_CONTINUOUS_SEGMENTS: &str = "\
[0] (Speaker 1): Good morning, how are you feeling today?
[1] (Speaker 2): Hi doctor. I've been having these headaches for about two weeks now.
[2] (Speaker 2): They're mostly on the right side and they get worse in the afternoon.
[3] (Speaker 1): I see. On a scale of one to ten, how would you rate the pain?
[4] (Speaker 2): Usually about a six or seven. Sometimes it goes up to an eight.
[5] (Speaker 1): Are you experiencing any nausea, vision changes, or sensitivity to light?
[6] (Speaker 2): A little bit of light sensitivity, but no nausea.
[7] (Speaker 1): Have you tried any over-the-counter medications?
[8] (Speaker 2): I've been taking ibuprofen but it only helps for a couple of hours.
[9] (Speaker 1): Based on what you're describing, this sounds like tension headaches or possibly migraines. I'd like to start you on sumatriptan.
[10] (Speaker 1): Schedule a follow-up in two weeks. If the headaches get worse, come back sooner.
[11] (Speaker 2): Thank you doctor. I'll see you in two weeks then.
[12] (Speaker 1): Take care. We'll see you soon.";

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Generate synthetic test audio: a 2-second 440Hz sine wave at 16kHz.
    ///
    /// This is not speech — the STT Router will return an empty or near-empty
    /// transcript. The purpose is to exercise the WebSocket streaming protocol,
    /// not to test transcription accuracy.
    fn generate_test_audio() -> Vec<f32> {
        let sample_rate = 16000;
        let duration_secs = 2;
        let num_samples = sample_rate * duration_secs;
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.1
            })
            .collect()
    }

    /// Create a WhisperServerClient connected to the STT Router.
    fn create_stt_client() -> WhisperServerClient {
        WhisperServerClient::new(STT_ROUTER_URL, "large-v3-turbo")
            .expect("Failed to create STT client")
    }

    /// Create an LLMClient connected to the LLM Router.
    ///
    /// Reads the API key from the user's saved config (~/.transcriptionapp/config.json).
    /// Falls back to empty key if no config exists (will cause 401 errors).
    fn create_llm_client() -> LLMClient {
        let config = Config::load_or_default();
        let url = if config.llm_router_url.is_empty() { LLM_ROUTER_URL } else { &config.llm_router_url };
        let api_key = &config.llm_api_key;
        let client_id = if config.llm_client_id.is_empty() { LLM_CLIENT_ID } else { &config.llm_client_id };
        let fast_model = if config.fast_model.is_empty() { FAST_MODEL } else { &config.fast_model };

        LLMClient::new(url, api_key, client_id, fast_model)
            .expect("Failed to create LLM client")
    }

    /// Generate a unique test session ID to avoid collisions between test runs.
    fn test_session_id(prefix: &str) -> String {
        format!("e2e-test-{}-{}", prefix, uuid::Uuid::new_v4())
    }

    /// Clean up a test session from the archive (best-effort).
    fn cleanup_test_session(session_id: &str) {
        let date_str = Utc::now().format("%Y-%m-%d").to_string();
        if let Ok(details) = local_archive::get_session(session_id, &date_str) {
            if let Ok(archive_dir) = local_archive::get_archive_dir() {
                let now = Utc::now();
                let session_dir = archive_dir
                    .join(format!("{:04}", now.format("%Y")))
                    .join(format!("{:02}", now.format("%m")))
                    .join(format!("{:02}", now.format("%d")))
                    .join(&details.session_id);
                let _ = std::fs::remove_dir_all(session_dir);
            }
        }
    }

    // ========================================================================
    // Layer 1: STT Router Integration
    // ========================================================================

    /// Verify the STT Router is reachable and responds to health checks.
    #[test]
    #[ignore = "Requires live STT Router"]
    fn e2e_layer1_stt_health_check() {
        let client = create_stt_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let health = rt.block_on(client.check_health())
            .expect("STT health check failed");

        assert_eq!(health.status, "healthy", "STT Router is not healthy");
        assert!(health.router.unwrap_or(false), "STT Router flag not set");
        println!("[PASS] STT Router healthy (model: {:?})", health.model);
    }

    /// Verify the medical-streaming alias is available.
    #[test]
    #[ignore = "Requires live STT Router"]
    fn e2e_layer1_stt_alias_available() {
        let client = create_stt_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let aliases = rt.block_on(client.list_aliases())
            .expect("Failed to list aliases");

        let has_medical_streaming = aliases.iter().any(|a| a.alias == STT_ALIAS);
        assert!(has_medical_streaming, "Alias '{}' not found in {:?}",
            STT_ALIAS, aliases.iter().map(|a| &a.alias).collect::<Vec<_>>());
        println!("[PASS] Alias '{}' available", STT_ALIAS);
    }

    /// Verify WebSocket streaming protocol completes without errors.
    ///
    /// Sends synthetic audio through the full streaming flow:
    /// connect → config → audio → receive response → close.
    /// The transcript will be empty (non-speech audio), but the protocol
    /// must complete successfully.
    #[test]
    #[ignore = "Requires live STT Router"]
    fn e2e_layer1_stt_streaming_protocol() {
        let client = create_stt_client();
        let audio = generate_test_audio();

        let mut chunk_count = 0;
        let result = client.transcribe_streaming_blocking(
            &audio,
            STT_ALIAS,
            true,
            |_chunk| { chunk_count += 1; },
        );

        let transcript = result.expect("Streaming transcription protocol failed");
        println!(
            "[PASS] Streaming protocol complete: {} chars, {} chunks",
            transcript.len(),
            chunk_count
        );
    }

    // ========================================================================
    // Layer 2: LLM Router Integration
    // ========================================================================

    /// Verify the LLM Router generates a SOAP note from a clinical transcript.
    ///
    /// Uses the fixture transcript to test SOAP generation independently of STT.
    /// Asserts that the response contains at least one patient note with non-empty
    /// content.
    #[test]
    #[ignore = "Requires live LLM Router"]
    fn e2e_layer2_llm_soap_generation() {
        let client = create_llm_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = rt.block_on(client.generate_multi_patient_soap_note(
            SOAP_MODEL,
            FIXTURE_TRANSCRIPT,
            None,  // No audio events
            None,  // Default SOAP options
            None,  // No speaker context
        )).expect("SOAP generation failed");

        assert!(!result.notes.is_empty(), "SOAP result has no patient notes");
        let first_note = &result.notes[0];
        assert!(!first_note.content.is_empty(), "SOAP note content is empty");
        assert!(!result.model_used.is_empty(), "Model used field is empty");

        println!("[PASS] SOAP generated: {} note(s), {} chars in first note",
            result.notes.len(),
            first_note.content.len());
        println!("  Model: {}", result.model_used);
        println!("  Physician speaker: {:?}", result.physician_speaker);
    }

    /// Verify the LLM Router can detect a completed encounter in transcript segments.
    ///
    /// Sends the fixture continuous-mode segments through the encounter detection
    /// prompt and checks that the LLM identifies a complete encounter.
    #[test]
    #[ignore = "Requires live LLM Router"]
    fn e2e_layer2_llm_encounter_detection() {
        let client = create_llm_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let (system_prompt, user_prompt) =
            build_encounter_detection_prompt(FIXTURE_CONTINUOUS_SEGMENTS);

        let response = rt.block_on(client.generate(
            FAST_MODEL,
            &system_prompt,
            &user_prompt,
            "encounter_detection",
        )).expect("Encounter detection LLM call failed");

        let detection = parse_encounter_detection(&response)
            .expect("Failed to parse encounter detection response");

        assert!(detection.complete, "LLM did not detect a complete encounter");
        assert!(
            detection.end_segment_index.is_some(),
            "No end_segment_index in detection result"
        );

        println!(
            "[PASS] Encounter detected: complete={}, end_segment_index={:?}",
            detection.complete,
            detection.end_segment_index
        );
    }

    // ========================================================================
    // Layer 3: Local Archive Integration
    // ========================================================================

    /// Verify a session can be saved to the archive and retrieved.
    ///
    /// Creates a test session with transcript, adds a SOAP note, then verifies
    /// both are retrievable through the history API.
    #[test]
    #[ignore = "Writes to local archive filesystem"]
    fn e2e_layer3_archive_save_and_retrieve() {
        let session_id = test_session_id("archive");
        let now = Utc::now();
        let date_str = now.format("%Y-%m-%d").to_string();
        let soap_content = "S: Patient reports headaches for two weeks.\n\
            O: Alert, oriented. BP 120/80.\n\
            A: Tension headache vs migraine.\n\
            P: Start sumatriptan PRN, follow-up in 2 weeks.";

        // Step 1: Save session
        let session_dir = local_archive::save_session(
            &session_id,
            FIXTURE_TRANSCRIPT,
            300_000, // 5 minutes
            None,    // No audio file
            false,   // Not auto-ended
            None,    // No auto-end reason
        ).expect("Failed to save session to archive");

        assert!(session_dir.exists(), "Session directory not created");

        // Step 2: Add SOAP note
        local_archive::add_soap_note(
            &session_id,
            &now,
            soap_content,
            Some(5),                 // Detail level
            Some("problem_based"),   // Format
        ).expect("Failed to add SOAP note");

        // Step 3: Verify session appears in date listing
        let sessions = local_archive::list_sessions_by_date(&date_str)
            .expect("Failed to list sessions");

        let our_session = sessions.iter().find(|s| s.session_id == session_id);
        assert!(our_session.is_some(), "Session not found in date listing");

        let summary = our_session.unwrap();
        assert!(summary.has_soap_note, "SOAP note flag not set in summary");
        assert!(summary.word_count > 0, "Word count is zero");

        // Step 4: Verify full session details are retrievable
        let details = local_archive::get_session(&session_id, &date_str)
            .expect("Failed to get session details");

        assert_eq!(details.session_id, session_id);
        assert!(details.transcript.is_some(), "Transcript missing from details");
        assert!(details.soap_note.is_some(), "SOAP note missing from details");

        let retrieved_transcript = details.transcript.unwrap();
        assert!(
            retrieved_transcript.contains("headaches"),
            "Retrieved transcript doesn't match saved content"
        );

        let retrieved_soap = details.soap_note.unwrap();
        assert!(
            retrieved_soap.contains("sumatriptan"),
            "Retrieved SOAP doesn't match saved content"
        );

        assert_eq!(details.metadata.duration_ms, Some(300_000));
        assert!(!details.metadata.auto_ended);

        println!("[PASS] Archive save/retrieve: session_id={}", session_id);
        println!("  Transcript: {} chars", retrieved_transcript.len());
        println!("  SOAP: {} chars", retrieved_soap.len());
        println!("  Word count: {}", summary.word_count);

        // Cleanup
        cleanup_test_session(&session_id);
    }

    /// Verify archive works with continuous mode metadata.
    #[test]
    #[ignore = "Writes to local archive filesystem"]
    fn e2e_layer3_archive_continuous_mode_metadata() {
        let session_id = test_session_id("continuous");
        let now = Utc::now();
        let date_str = now.format("%Y-%m-%d").to_string();

        // Save session
        local_archive::save_session(
            &session_id,
            FIXTURE_TRANSCRIPT,
            600_000,
            None,
            false,
            None,
        ).expect("Failed to save session");

        // Update metadata with continuous mode fields
        // (This mirrors what continuous_mode.rs does after archiving)
        if let Ok(archive_dir) = local_archive::get_archive_dir() {
            let session_dir = archive_dir
                .join(now.format("%Y").to_string())
                .join(now.format("%m").to_string())
                .join(now.format("%d").to_string())
                .join(&session_id);
            let metadata_path = session_dir.join("metadata.json");

            if metadata_path.exists() {
                let content = std::fs::read_to_string(&metadata_path).unwrap();
                let mut metadata: local_archive::ArchiveMetadata =
                    serde_json::from_str(&content).unwrap();
                metadata.charting_mode = Some("continuous".to_string());
                metadata.encounter_number = Some(3);
                let json = serde_json::to_string_pretty(&metadata).unwrap();
                std::fs::write(&metadata_path, json).unwrap();
            }
        }

        // Verify continuous mode metadata is retrievable
        let sessions = local_archive::list_sessions_by_date(&date_str)
            .expect("Failed to list sessions");
        let our_session = sessions.iter().find(|s| s.session_id == session_id);
        assert!(our_session.is_some(), "Session not found");
        let summary = our_session.unwrap();
        assert_eq!(
            summary.charting_mode.as_deref(),
            Some("continuous"),
            "Charting mode not set"
        );
        assert_eq!(summary.encounter_number, Some(3), "Encounter number not set");

        println!("[PASS] Continuous mode metadata: charting_mode=continuous, encounter_number=3");

        // Cleanup
        cleanup_test_session(&session_id);
    }

    // ========================================================================
    // Layer 4: Session Mode End-to-End
    // ========================================================================

    /// Full session mode E2E: STT → SOAP → Archive → History.
    ///
    /// Simulates a complete session recording workflow:
    /// 1. Transcribe audio via STT Router (streaming)
    /// 2. If transcript empty (test audio), fall back to fixture transcript
    /// 3. Generate SOAP note via LLM Router
    /// 4. Archive session with transcript and SOAP
    /// 5. Verify session is retrievable from history with all data intact
    ///
    /// This test exercises the same code paths as a real recording session,
    /// minus the Tauri event emission and audio capture from microphone.
    #[test]
    #[ignore = "Requires live STT Router + LLM Router"]
    fn e2e_layer4_session_mode_full() {
        let session_id = test_session_id("session-e2e");
        let now = Utc::now();
        let date_str = now.format("%Y-%m-%d").to_string();

        // ── Step 1: Transcribe audio via STT Router ──────────────────────
        println!("Step 1: Transcribing audio via STT Router...");
        let client = create_stt_client();
        let audio = generate_test_audio();

        let mut chunks = Vec::new();
        let stt_result = client.transcribe_streaming_blocking(
            &audio,
            STT_ALIAS,
            true,
            |chunk| { chunks.push(chunk.to_string()); },
        ).expect("STT streaming failed");

        println!(
            "  STT result: {} chars, {} chunks",
            stt_result.len(),
            chunks.len()
        );

        // Use fixture if STT returned empty (expected for non-speech audio)
        let transcript = if stt_result.trim().is_empty() {
            println!("  Using fixture transcript (STT returned empty for test audio)");
            FIXTURE_TRANSCRIPT.to_string()
        } else {
            println!("  Using real STT transcript");
            stt_result
        };

        // ── Step 2: Generate SOAP note via LLM Router ────────────────────
        println!("Step 2: Generating SOAP note via LLM Router...");
        let llm_client = create_llm_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let soap_result = rt.block_on(llm_client.generate_multi_patient_soap_note(
            SOAP_MODEL,
            &transcript,
            None,
            None,
            None,
        )).expect("SOAP generation failed");

        assert!(!soap_result.notes.is_empty(), "No SOAP notes generated");
        let soap_content: String = soap_result.notes
            .iter()
            .map(|n| n.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        println!("  SOAP generated: {} notes, {} total chars",
            soap_result.notes.len(), soap_content.len());

        // ── Step 3: Archive session ──────────────────────────────────────
        println!("Step 3: Archiving session...");
        local_archive::save_session(
            &session_id,
            &transcript,
            300_000,
            None,
            false,
            None,
        ).expect("Failed to save session");

        local_archive::add_soap_note(
            &session_id,
            &now,
            &soap_content,
            Some(5),
            Some("problem_based"),
        ).expect("Failed to add SOAP note");
        println!("  Archived: session_id={}", session_id);

        // ── Step 4: Verify history retrieval ─────────────────────────────
        println!("Step 4: Verifying history retrieval...");

        // Check date listing
        let sessions = local_archive::list_sessions_by_date(&date_str)
            .expect("Failed to list sessions");
        let summary = sessions.iter().find(|s| s.session_id == session_id)
            .expect("Session not found in history listing");
        assert!(summary.has_soap_note, "SOAP flag not set in history");
        assert!(summary.word_count > 0, "Word count zero in history");

        // Check full details
        let details = local_archive::get_session(&session_id, &date_str)
            .expect("Failed to get session details");
        assert!(details.transcript.is_some(), "Transcript missing from history");
        assert!(details.soap_note.is_some(), "SOAP note missing from history");
        assert_eq!(details.metadata.duration_ms, Some(300_000));

        println!("  History OK: transcript={} chars, soap={} chars, words={}",
            details.transcript.as_ref().map(|t| t.len()).unwrap_or(0),
            details.soap_note.as_ref().map(|s| s.len()).unwrap_or(0),
            summary.word_count);

        println!("\n[PASS] Session mode E2E complete");
        println!("  Audio → STT Router (streaming) → OK");
        println!("  Transcript → LLM Router (SOAP) → OK");
        println!("  Session → Archive → OK");
        println!("  History → Retrieve → OK");

        // Cleanup
        cleanup_test_session(&session_id);
    }

    // ========================================================================
    // Layer 5: Continuous Mode End-to-End
    // ========================================================================

    /// Full continuous mode E2E: STT → Encounter Detection → SOAP → Archive → History.
    ///
    /// Simulates the continuous charting workflow:
    /// 1. Transcribe audio via STT Router (streaming)
    /// 2. Format transcript as continuous mode segments
    /// 3. Run encounter detection via LLM Router
    /// 4. Generate SOAP note for detected encounter
    /// 5. Archive with continuous mode metadata (charting_mode, encounter_number)
    /// 6. Verify history shows continuous mode session with correct metadata
    ///
    /// This test exercises the same code paths as `run_continuous_mode()`,
    /// minus the Tauri event emission, pipeline thread management, and
    /// periodic detection loop.
    #[test]
    #[ignore = "Requires live STT Router + LLM Router"]
    fn e2e_layer5_continuous_mode_full() {
        let session_id = test_session_id("continuous-e2e");
        let now = Utc::now();
        let date_str = now.format("%Y-%m-%d").to_string();

        // ── Step 1: Transcribe audio via STT Router ──────────────────────
        println!("Step 1: Transcribing audio via STT Router...");
        let client = create_stt_client();
        let audio = generate_test_audio();

        let stt_result = client.transcribe_streaming_blocking(
            &audio,
            STT_ALIAS,
            true,
            |_chunk| {},
        ).expect("STT streaming failed");

        println!("  STT result: {} chars", stt_result.len());

        // Use fixture transcript (expected for non-speech test audio)
        let transcript = if stt_result.trim().is_empty() {
            println!("  Using fixture transcript");
            FIXTURE_TRANSCRIPT.to_string()
        } else {
            stt_result
        };

        // ── Step 2: Encounter detection via LLM Router ───────────────────
        println!("Step 2: Running encounter detection...");
        let llm_client = create_llm_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let (system_prompt, user_prompt) =
            build_encounter_detection_prompt(FIXTURE_CONTINUOUS_SEGMENTS);

        let detection_response = rt.block_on(llm_client.generate(
            FAST_MODEL,
            &system_prompt,
            &user_prompt,
            "encounter_detection",
        )).expect("Encounter detection failed");

        let detection = parse_encounter_detection(&detection_response)
            .expect("Failed to parse encounter detection");

        assert!(detection.complete, "Encounter not detected as complete");
        let end_index = detection.end_segment_index
            .expect("No end_segment_index in detection");
        println!(
            "  Encounter detected: end_segment_index={}",
            end_index
        );

        // ── Step 3: Generate SOAP note for encounter ─────────────────────
        println!("Step 3: Generating SOAP note for encounter...");
        let soap_result = rt.block_on(llm_client.generate_multi_patient_soap_note(
            SOAP_MODEL,
            &transcript,
            None,
            None,
            None,
        )).expect("SOAP generation failed");

        assert!(!soap_result.notes.is_empty(), "No SOAP notes generated");
        let soap_content: String = soap_result.notes
            .iter()
            .map(|n| n.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        println!(
            "  SOAP generated: {} notes, {} chars",
            soap_result.notes.len(),
            soap_content.len()
        );

        // ── Step 4: Archive with continuous mode metadata ────────────────
        println!("Step 4: Archiving as continuous mode encounter...");
        local_archive::save_session(
            &session_id,
            &transcript,
            600_000, // 10 minutes
            None,
            false,
            None,
        ).expect("Failed to save session");

        local_archive::add_soap_note(
            &session_id,
            &now,
            &soap_content,
            Some(5),
            Some("problem_based"),
        ).expect("Failed to add SOAP note");

        // Update metadata with continuous mode fields
        // (mirrors continuous_mode.rs encounter archival logic)
        if let Ok(archive_dir) = local_archive::get_archive_dir() {
            let session_dir = archive_dir
                .join(now.format("%Y").to_string())
                .join(now.format("%m").to_string())
                .join(now.format("%d").to_string())
                .join(&session_id);
            let metadata_path = session_dir.join("metadata.json");

            let content = std::fs::read_to_string(&metadata_path)
                .expect("Failed to read metadata");
            let mut metadata: local_archive::ArchiveMetadata =
                serde_json::from_str(&content).expect("Failed to parse metadata");
            metadata.charting_mode = Some("continuous".to_string());
            metadata.encounter_number = Some(1);
            let json = serde_json::to_string_pretty(&metadata).unwrap();
            std::fs::write(&metadata_path, json)
                .expect("Failed to write continuous mode metadata");
        }

        println!("  Archived: session_id={}", session_id);

        // ── Step 5: Verify history retrieval with continuous metadata ─────
        println!("Step 5: Verifying history retrieval...");

        let sessions = local_archive::list_sessions_by_date(&date_str)
            .expect("Failed to list sessions");
        let summary = sessions.iter().find(|s| s.session_id == session_id)
            .expect("Session not found in history listing");

        assert!(summary.has_soap_note, "SOAP flag not set");
        assert_eq!(
            summary.charting_mode.as_deref(), Some("continuous"),
            "Charting mode not 'continuous' in history"
        );
        assert_eq!(
            summary.encounter_number, Some(1),
            "Encounter number not set in history"
        );

        let details = local_archive::get_session(&session_id, &date_str)
            .expect("Failed to get session details");
        assert!(details.transcript.is_some(), "Transcript missing");
        assert!(details.soap_note.is_some(), "SOAP note missing");
        assert_eq!(
            details.metadata.charting_mode.as_deref(), Some("continuous"),
            "Charting mode not in metadata"
        );
        assert_eq!(
            details.metadata.encounter_number, Some(1),
            "Encounter number not in metadata"
        );

        println!("  History OK: charting_mode=continuous, encounter_number=1");
        println!("  Transcript: {} chars", details.transcript.as_ref().map(|t| t.len()).unwrap_or(0));
        println!("  SOAP: {} chars", details.soap_note.as_ref().map(|s| s.len()).unwrap_or(0));

        println!("\n[PASS] Continuous mode E2E complete");
        println!("  Audio → STT Router (streaming) → OK");
        println!("  Segments → LLM Router (encounter detection) → OK");
        println!("  Transcript → LLM Router (SOAP) → OK");
        println!("  Session → Archive (continuous metadata) → OK");
        println!("  History → Retrieve (charting_mode + encounter_number) → OK");

        // Cleanup
        cleanup_test_session(&session_id);
    }
}
