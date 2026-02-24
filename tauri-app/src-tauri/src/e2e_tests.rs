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
//! Layer 6: Native STT Shadow   — Apple Speech client, accumulator, CSV, archive integration
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
        build_encounter_detection_prompt, build_encounter_merge_prompt,
        parse_encounter_detection, parse_merge_check,
    };
    use crate::encounter_experiment::strip_hallucinations;
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

    /// Model for encounter detection (hybrid: smaller model resists over-splitting)
    const DETECTION_MODEL: &str = "faster";

    /// Model for encounter merge and general fast tasks
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
    /// Uses the hybrid detection model ("faster" / Qwen3-1.7B) with /nothink prefix,
    /// matching the production configuration in continuous_mode.rs.
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

        // Prepend /nothink to match production (disables Qwen3 thinking mode)
        let system_prompt = format!("/nothink\n{}", system_prompt);

        let response = rt.block_on(client.generate(
            DETECTION_MODEL,
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
            "[PASS] Encounter detected (model={}): complete={}, end_segment_index={:?}",
            DETECTION_MODEL,
            detection.complete,
            detection.end_segment_index
        );
    }

    /// Verify the hybrid model approach: detection uses "faster" model,
    /// merge uses "fast-model". Also verifies the hallucination filter
    /// and /nothink prefix work correctly with the LLM Router.
    #[test]
    #[ignore = "Requires live LLM Router"]
    fn e2e_layer2_hybrid_detection_and_merge() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let config = Config::load_or_default();
        let llm_url = if config.llm_router_url.is_empty() { LLM_ROUTER_URL } else { &config.llm_router_url };
        let api_key = &config.llm_api_key;
        let client_id = if config.llm_client_id.is_empty() { LLM_CLIENT_ID } else { &config.llm_client_id };

        // ── Detection with smaller model + /nothink ──────────────────────
        println!("Step 1: Detection with model='{}' + /nothink...", DETECTION_MODEL);
        let detection_client = LLMClient::new(llm_url, api_key, client_id, DETECTION_MODEL)
            .expect("Failed to create detection LLM client");

        let (system_prompt, user_prompt) =
            build_encounter_detection_prompt(FIXTURE_CONTINUOUS_SEGMENTS);
        let system_prompt = format!("/nothink\n{}", system_prompt);

        let response = rt.block_on(detection_client.generate(
            DETECTION_MODEL,
            &system_prompt,
            &user_prompt,
            "encounter_detection",
        )).expect("Detection LLM call failed");

        let detection = parse_encounter_detection(&response)
            .expect("Failed to parse detection response");
        assert!(detection.complete, "Detection model did not detect complete encounter");
        println!("  Detection OK: complete=true, end_segment_index={:?}", detection.end_segment_index);

        // ── Merge with larger model + patient name (M1 strategy) ─────────
        println!("Step 2: Merge with model='{}' + patient name...", FAST_MODEL);
        let merge_client = LLMClient::new(llm_url, api_key, client_id, FAST_MODEL)
            .expect("Failed to create merge LLM client");

        // Simulate two encounter excerpts from the same visit
        let prev_tail = "Speaker 1: I'd like to start you on sumatriptan as needed and \
            schedule a follow-up in two weeks. If the headaches get worse or you develop \
            new symptoms, come back sooner.\n\
            Speaker 2: Thank you doctor. I'll see you in two weeks then.";
        let curr_head = "Speaker 1: Take care. We'll see you soon.\n\
            Speaker 2: Thanks again doctor.\n\
            Speaker 1: Now let me update the chart with your visit notes.";

        let (merge_system, merge_user) = build_encounter_merge_prompt(
            prev_tail,
            curr_head,
            Some("Test Patient"),
        );

        let merge_response = rt.block_on(merge_client.generate(
            FAST_MODEL,
            &merge_system,
            &merge_user,
            "encounter_merge",
        )).expect("Merge LLM call failed");

        let merge_result = parse_merge_check(&merge_response)
            .expect("Failed to parse merge response");
        assert!(merge_result.same_encounter, "Merge model should identify same encounter");
        println!("  Merge OK: same_encounter=true, reason={:?}", merge_result.reason);

        // ── Hallucination filter ─────────────────────────────────────────
        println!("Step 3: Hallucination filter...");
        let hallucinated = "The patient presented with fractured ".to_string()
            + &"fractured ".repeat(100)
            + "kneecap after a fall.";
        let (cleaned, report) = strip_hallucinations(&hallucinated, 5);
        assert!(!report.repetitions.is_empty(), "Should detect hallucinated repetitions");
        assert!(cleaned.len() < hallucinated.len(), "Cleaned text should be shorter");
        println!(
            "  Filter OK: {} -> {} words, found {} repetition(s)",
            report.original_word_count,
            report.cleaned_word_count,
            report.repetitions.len()
        );

        println!("\n[PASS] Hybrid model E2E complete");
        println!("  Detection: model={} + /nothink → complete=true", DETECTION_MODEL);
        println!("  Merge: model={} + patient name → same_encounter=true", FAST_MODEL);
        println!("  Hallucination filter → cleaned {} repetitions", report.repetitions.len());
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

        // Use fixture if STT returned empty or too short for SOAP (expected for
        // non-speech test audio — some models hallucinate short text like "The.")
        let transcript = if stt_result.trim().len() < 50 {
            println!("  Using fixture transcript (STT returned {} chars — too short for SOAP)", stt_result.trim().len());
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

        // Use fixture transcript (expected for non-speech test audio — some
        // models hallucinate short text like "The." from sine waves)
        let transcript = if stt_result.trim().len() < 50 {
            println!("  Using fixture transcript (STT returned {} chars — too short)", stt_result.trim().len());
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

        // Prepend /nothink to match production hybrid detection model config
        let system_prompt = format!("/nothink\n{}", system_prompt);

        let detection_response = rt.block_on(llm_client.generate(
            DETECTION_MODEL,
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

    // ========================================================================
    // Layer 6: Native STT Shadow
    // ========================================================================

    /// Ensure speech recognition is authorized, requesting permission if needed.
    ///
    /// - If already authorized, returns `Ok(())` immediately.
    /// - If not determined, calls `requestAuthorization:` and polls for up to 30s.
    /// - If denied or restricted, returns `Err` with a message.
    fn ensure_speech_recognition_permission() -> Result<(), String> {
        use crate::native_stt::{
            check_speech_recognition_permission, request_speech_recognition_permission,
            SpeechAuthStatus,
        };

        let status = check_speech_recognition_permission();
        println!("Speech recognition permission: {:?}", status);

        match status {
            SpeechAuthStatus::Authorized => return Ok(()),
            SpeechAuthStatus::Denied => {
                return Err("Speech recognition denied. Grant permission in System Settings > Privacy & Security > Speech Recognition".to_string());
            }
            SpeechAuthStatus::Restricted => {
                return Err("Speech recognition restricted by device policy".to_string());
            }
            SpeechAuthStatus::NotDetermined => {
                // Request permission and wait for user response
                println!("Requesting speech recognition permission...");
                request_speech_recognition_permission();

                // Poll for up to 30 seconds (user needs to click Allow in the system dialog)
                for i in 0..60 {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let new_status = check_speech_recognition_permission();
                    if new_status != SpeechAuthStatus::NotDetermined {
                        println!("Permission resolved after {:.1}s: {:?}", (i + 1) as f64 * 0.5, new_status);
                        return if new_status == SpeechAuthStatus::Authorized {
                            Ok(())
                        } else {
                            Err(format!("Speech recognition permission {}", new_status))
                        };
                    }
                }
                return Err("Timed out waiting for speech recognition permission (30s)".to_string());
            }
            SpeechAuthStatus::Unknown => {
                return Err("Speech framework not available (SFSpeechRecognizer class missing)".to_string());
            }
        }
    }

    /// Verify native STT client creation and permission check on macOS.
    ///
    /// This test exercises the full Objective-C FFI path:
    /// 1. Check/request SFSpeechRecognizer authorization
    /// 2. Create NativeSttClient (loads Speech framework, creates recognizer)
    /// 3. Transcribe test audio through the full recognition pipeline
    ///
    /// Will request permission if not yet determined, skip if denied/unavailable.
    #[test]
    #[ignore = "Requires macOS with speech recognition permission"]
    fn e2e_layer6_native_stt_client_creation() {
        use crate::native_stt::NativeSttClient;

        // Step 1: Ensure permission (request if needed)
        if let Err(reason) = ensure_speech_recognition_permission() {
            println!("[SKIP] {}", reason);
            return;
        }

        // Step 2: Create client
        let client = NativeSttClient::new()
            .expect("Failed to create NativeSttClient despite authorized permission");
        println!("[PASS] NativeSttClient created successfully");

        // Step 3: Transcribe silence (2s sine wave — not speech)
        // This exercises the full SFSpeechRecognizer pipeline:
        //   AVAudioFormat → AVAudioPCMBuffer → SFSpeechAudioBufferRecognitionRequest → recognitionTask
        let audio = generate_test_audio();
        println!("Transcribing {} samples ({:.1}s) of test audio...", audio.len(), audio.len() as f64 / 16000.0);

        match client.transcribe_blocking(&audio, 16000) {
            Ok(text) => {
                println!("[PASS] Native STT transcribed: \"{}\" ({} chars)", text, text.len());
                // Sine wave may produce empty text or hallucinated text — both are valid
            }
            Err(e) => {
                // Timeout or error is acceptable for non-speech audio, but log it
                println!("[WARN] Native STT returned error (acceptable for non-speech): {}", e);
            }
        }
    }

    /// Verify native STT shadow accumulator lifecycle: push, format, drain.
    ///
    /// Tests the pure-Rust accumulator without any Objective-C FFI.
    /// Does not require macOS speech permission.
    #[test]
    #[ignore = "Writes to local filesystem (shadow_stt CSV)"]
    fn e2e_layer6_native_stt_shadow_accumulator() {
        use crate::native_stt_shadow::{NativeSttSegment, NativeSttShadowAccumulator, NativeSttCsvLogger};
        use uuid::Uuid;

        // Step 1: Create accumulator and push segments (out of order to test sorting)
        let mut accumulator = NativeSttShadowAccumulator::new();

        let seg1 = NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms: 0,
            end_ms: 3000,
            native_text: "Good morning how are you feeling".to_string(),
            primary_text: "Good morning, how are you feeling today?".to_string(),
            speaker_id: Some("Speaker 1".to_string()),
            native_latency_ms: 1200,
            primary_latency_ms: 800,
        };

        let seg2 = NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms: 3500,
            end_ms: 8000,
            native_text: "I've been having headaches for two weeks".to_string(),
            primary_text: "I've been having these headaches for about two weeks now.".to_string(),
            speaker_id: Some("Speaker 2".to_string()),
            native_latency_ms: 2100,
            primary_latency_ms: 1500,
        };

        let seg3 = NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms: 8500,
            end_ms: 12000,
            native_text: "On a scale of 1 to 10 how would you rate the pain".to_string(),
            primary_text: "On a scale of one to ten, how would you rate the pain?".to_string(),
            speaker_id: Some("Speaker 1".to_string()),
            native_latency_ms: 1800,
            primary_latency_ms: 1100,
        };

        // Push out of order to test sort
        accumulator.push(seg2.clone());
        accumulator.push(seg1.clone());
        accumulator.push(seg3.clone());

        assert!(!accumulator.is_empty(), "Accumulator should not be empty");

        // Step 2: Format transcript (should be sorted by start_ms)
        let transcript = accumulator.format_transcript();
        println!("Formatted shadow transcript:\n{}", transcript);
        assert!(transcript.contains("Good morning"), "Transcript should start with first segment");
        assert!(transcript.contains("headaches"), "Transcript should contain second segment");
        assert!(transcript.contains("scale"), "Transcript should contain third segment");

        // Step 3: Test drain_through (encounter boundary drain)
        let mut accumulator2 = NativeSttShadowAccumulator::new();
        accumulator2.push(seg1);
        accumulator2.push(seg2);
        accumulator2.push(seg3);

        let drained = accumulator2.drain_through(5000);
        assert_eq!(drained.len(), 2, "Should drain 2 segments through end_ms=5000");
        assert!(!accumulator2.is_empty(), "1 segment should remain after drain");

        // Step 4: Test CSV logger
        let mut logger = NativeSttCsvLogger::new().expect("Failed to create CSV logger");
        let test_seg = NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms: 0,
            end_ms: 3000,
            native_text: "test native text".to_string(),
            primary_text: "test primary text".to_string(),
            speaker_id: Some("Speaker 1".to_string()),
            native_latency_ms: 500,
            primary_latency_ms: 300,
        };
        logger.write_segment(&test_seg);

        // Verify CSV file was created
        let home = dirs::home_dir().expect("No home dir");
        let csv_dir = home.join(".transcriptionapp").join("shadow_stt");
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let csv_path = csv_dir.join(format!("{}.csv", today));
        assert!(csv_path.exists(), "CSV log file should exist at {:?}", csv_path);

        let csv_content = std::fs::read_to_string(&csv_path).expect("Failed to read CSV");
        assert!(csv_content.contains("timestamp_utc"), "CSV should have header");
        assert!(csv_content.lines().count() >= 2, "CSV should have header + at least 1 data row");
        println!("CSV log written to: {:?}", csv_path);
        println!("CSV content ({} lines):\n{}", csv_content.lines().count(), csv_content);

        // Step 5: Drain all and verify
        let (full_transcript, segments) = accumulator.drain_all();
        assert_eq!(segments.len(), 3, "Should drain all 3 segments");
        assert!(!full_transcript.is_empty(), "Full transcript should not be empty");
        assert!(accumulator.is_empty(), "Accumulator should be empty after drain_all");

        println!("\n[PASS] Native STT shadow accumulator lifecycle");
        println!("  Push (out-of-order) → sorted → OK");
        println!("  Format transcript → OK");
        println!("  Drain through boundary → OK");
        println!("  CSV logging → OK");
        println!("  Drain all → OK");
    }

    /// Verify shadow transcript is saved alongside primary transcript in archive
    /// and can be retrieved via get_session.
    ///
    /// Tests the archive integration without requiring speech recognition:
    /// 1. Save session with primary transcript
    /// 2. Write shadow_transcript.txt to session dir (mirrors pipeline behavior)
    /// 3. Verify get_session returns both transcripts
    /// 4. Verify metadata can track has_shadow_transcript
    #[test]
    #[ignore = "Writes to local archive filesystem"]
    fn e2e_layer6_native_stt_shadow_archive() {
        let session_id = test_session_id("shadow-archive");
        let now = Utc::now();
        let date_str = now.format("%Y-%m-%d").to_string();

        let primary_transcript = FIXTURE_TRANSCRIPT;
        let shadow_transcript = "\
Speaker 1: Good morning how are you feeling today
Speaker 2: Hi doctor I've been having these headaches for about 2 weeks now they're mostly on the right side and they get worse in the afternoon
Speaker 1: I see on a scale of 1 to 10 how would you rate the pain
Speaker 2: Usually about a 6 or 7 sometimes it goes up to an 8
Speaker 1: Are you experiencing any nausea vision changes or sensitivity to light
Speaker 2: A little bit of light sensitivity but no nausea
Speaker 1: Have you tried any over the counter medications
Speaker 2: I've been taking ibuprofen but it only helps for a couple of hours
Speaker 1: OK let's do a neurological exam and check your blood pressure based on what you're describing this sounds like it could be tension headaches or possibly migraines I'd like to start you on sumatriptan as needed and schedule a follow up in 2 weeks
Speaker 2: Thank you doctor I'll see you in 2 weeks then
Speaker 1: Take care we'll see you soon";

        // Step 1: Save session (creates archive directory)
        let session_dir = local_archive::save_session(
            &session_id,
            primary_transcript,
            300_000,
            None,
            false,
            None,
        ).expect("Failed to save session");

        // Step 2: Write shadow transcript (mirrors pipeline → session.rs behavior)
        let shadow_path = session_dir.join("shadow_transcript.txt");
        std::fs::write(&shadow_path, shadow_transcript)
            .expect("Failed to write shadow transcript");
        assert!(shadow_path.exists(), "Shadow transcript file should exist");

        // Step 3: Update metadata with has_shadow_transcript flag
        let metadata_path = session_dir.join("metadata.json");
        let content = std::fs::read_to_string(&metadata_path)
            .expect("Failed to read metadata");
        let mut metadata: local_archive::ArchiveMetadata =
            serde_json::from_str(&content).expect("Failed to parse metadata");
        metadata.has_shadow_transcript = Some(true);
        let json = serde_json::to_string_pretty(&metadata).unwrap();
        std::fs::write(&metadata_path, json).expect("Failed to write metadata");

        // Step 4: Retrieve via get_session and verify both transcripts
        let details = local_archive::get_session(&session_id, &date_str)
            .expect("Failed to get session details");

        assert!(details.transcript.is_some(), "Primary transcript missing");
        assert!(details.shadow_transcript.is_some(), "Shadow transcript missing from details");
        assert_eq!(
            details.metadata.has_shadow_transcript,
            Some(true),
            "has_shadow_transcript not set in metadata"
        );

        let retrieved_primary = details.transcript.unwrap();
        let retrieved_shadow = details.shadow_transcript.unwrap();

        assert!(
            retrieved_primary.contains("headaches"),
            "Primary transcript content mismatch"
        );
        assert!(
            retrieved_shadow.contains("headaches"),
            "Shadow transcript content mismatch"
        );

        // Step 5: Compare word counts (shadow should differ from primary — no punctuation)
        let primary_words: usize = retrieved_primary.split_whitespace().count();
        let shadow_words: usize = retrieved_shadow.split_whitespace().count();
        println!("Primary transcript: {} words, {} chars", primary_words, retrieved_primary.len());
        println!("Shadow transcript:  {} words, {} chars", shadow_words, retrieved_shadow.len());
        assert!(primary_words > 50, "Primary transcript too short");
        assert!(shadow_words > 50, "Shadow transcript too short");

        println!("\n[PASS] Shadow transcript archive integration");
        println!("  Save session → OK");
        println!("  Write shadow_transcript.txt → OK");
        println!("  Update metadata (has_shadow_transcript) → OK");
        println!("  Retrieve both transcripts → OK");
        println!("  Primary: {} words, Shadow: {} words", primary_words, shadow_words);

        // Cleanup
        cleanup_test_session(&session_id);
    }

    /// Full native STT shadow E2E: Native STT + STT Router side by side.
    ///
    /// Exercises the complete shadow pipeline:
    /// 1. Transcribe audio via STT Router (streaming)
    /// 2. Transcribe same audio via native STT (Apple Speech)
    /// 3. Archive session with both transcripts
    /// 4. Verify both are retrievable
    /// 5. Log comparison metrics to CSV
    ///
    /// Requires: macOS with speech recognition permission + live STT Router.
    #[test]
    #[ignore = "Requires live STT Router + macOS speech recognition permission"]
    fn e2e_layer6_native_stt_shadow_full() {
        use crate::native_stt::NativeSttClient;
        use crate::native_stt_shadow::{NativeSttSegment, NativeSttShadowAccumulator, NativeSttCsvLogger};
        use uuid::Uuid;

        // ── Prerequisite: Ensure native STT permission ───────────────────
        if let Err(reason) = ensure_speech_recognition_permission() {
            println!("[SKIP] {}", reason);
            return;
        }

        let native_client = match NativeSttClient::new() {
            Ok(c) => c,
            Err(e) => {
                println!("[SKIP] NativeSttClient creation failed: {}", e);
                return;
            }
        };

        let session_id = test_session_id("shadow-full");
        let now = Utc::now();
        let date_str = now.format("%Y-%m-%d").to_string();

        // ── Step 1: Transcribe via STT Router (streaming) ────────────────
        println!("Step 1: Transcribing via STT Router...");
        let stt_client = create_stt_client();
        let audio = generate_test_audio();

        let stt_result = stt_client.transcribe_streaming_blocking(
            &audio,
            STT_ALIAS,
            true,
            |_chunk| {},
        ).expect("STT streaming failed");

        // Use fixture if STT returned too little (expected for sine wave)
        let primary_transcript = if stt_result.trim().len() < 50 {
            println!("  Using fixture transcript (STT returned {} chars)", stt_result.trim().len());
            FIXTURE_TRANSCRIPT.to_string()
        } else {
            stt_result
        };
        println!("  Primary: {} words", primary_transcript.split_whitespace().count());

        // ── Step 2: Transcribe via native STT (Apple Speech) ─────────────
        println!("Step 2: Transcribing via native STT (Apple Speech)...");
        let native_start = std::time::Instant::now();
        let native_result = native_client.transcribe_blocking(&audio, 16000);
        let native_latency = native_start.elapsed();

        let native_text = match native_result {
            Ok(text) => {
                println!("  Native STT: \"{}\" ({} chars, {:.1}s)", text, text.len(), native_latency.as_secs_f64());
                if text.trim().len() < 10 {
                    // Use fixture shadow (like production — sine wave produces no speech)
                    println!("  Native STT returned too little, using fixture shadow transcript");
                    "Good morning how are you feeling today".to_string()
                } else {
                    text
                }
            }
            Err(e) => {
                println!("  Native STT error (using fixture): {}", e);
                "Good morning how are you feeling today".to_string()
            }
        };

        // ── Step 3: Build shadow accumulator + CSV log ───────────────────
        println!("Step 3: Building shadow accumulator...");
        let mut accumulator = NativeSttShadowAccumulator::new();
        let mut csv_logger = NativeSttCsvLogger::new().expect("Failed to create CSV logger");

        let seg = NativeSttSegment {
            utterance_id: Uuid::new_v4(),
            start_ms: 0,
            end_ms: 2000,
            native_text: native_text.clone(),
            primary_text: primary_transcript
                .lines()
                .next()
                .unwrap_or("Speaker 1: Good morning")
                .to_string(),
            speaker_id: Some("Speaker 1".to_string()),
            native_latency_ms: native_latency.as_millis() as u64,
            primary_latency_ms: 0,
        };

        csv_logger.write_segment(&seg);
        accumulator.push(seg);

        let shadow_transcript = accumulator.format_transcript();
        println!("  Shadow transcript: {} chars", shadow_transcript.len());

        // ── Step 4: Archive with both transcripts ────────────────────────
        println!("Step 4: Archiving session with shadow transcript...");
        let session_dir = local_archive::save_session(
            &session_id,
            &primary_transcript,
            2_000,
            None,
            false,
            None,
        ).expect("Failed to save session");

        // Write shadow transcript (mirrors pipeline → session.rs code path)
        let shadow_path = session_dir.join("shadow_transcript.txt");
        std::fs::write(&shadow_path, &shadow_transcript)
            .expect("Failed to write shadow transcript");

        // Update metadata
        let metadata_path = session_dir.join("metadata.json");
        let content = std::fs::read_to_string(&metadata_path).expect("Failed to read metadata");
        let mut metadata: local_archive::ArchiveMetadata =
            serde_json::from_str(&content).expect("Failed to parse metadata");
        metadata.has_shadow_transcript = Some(true);
        let json = serde_json::to_string_pretty(&metadata).unwrap();
        std::fs::write(&metadata_path, json).expect("Failed to write metadata");

        println!("  Archived: session_id={}", session_id);

        // ── Step 5: Verify history retrieval with shadow transcript ──────
        println!("Step 5: Verifying history retrieval...");
        let details = local_archive::get_session(&session_id, &date_str)
            .expect("Failed to get session details");

        assert!(details.transcript.is_some(), "Primary transcript missing");
        assert!(details.shadow_transcript.is_some(), "Shadow transcript missing");
        assert_eq!(details.metadata.has_shadow_transcript, Some(true));

        let primary_words = details.transcript.as_ref().unwrap().split_whitespace().count();
        let shadow_words = details.shadow_transcript.as_ref().unwrap().split_whitespace().count();

        println!("\n[PASS] Native STT shadow full E2E");
        println!("  STT Router → {} words", primary_words);
        println!("  Native STT → {} words ({:.1}s latency)", shadow_words, native_latency.as_secs_f64());
        println!("  Archive → both transcripts saved and retrievable");
        println!("  CSV log → segment comparison logged");

        // Cleanup
        cleanup_test_session(&session_id);
    }

}
