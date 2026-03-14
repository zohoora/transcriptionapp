# Evidence Appendix

This appendix contains direct code excerpts from the app to support or challenge claims in `codex_edge_case_detection.md`. The excerpts are intentionally selective rather than exhaustive.

## 1. Continuous Mode Runtime Skeleton

Source: `tauri-app/src-tauri/src/continuous_mode.rs`

### Pipeline startup, transcript buffering, and silence-triggered checks

```rust
   294	    // Build pipeline config — same as session but with auto_end disabled
   295	    let pipeline_config = PipelineConfig::from_config(
   296	        &config,
   297	        config.input_device_id.clone(),
   298	        audio_output_path,
   299	        None,            // No initial audio buffer in continuous mode
   300	        false,           // Never auto-end in continuous mode
   301	        0,
   302	    );
   303	
   304	    // Create message channel
   305	    let (tx, mut rx) = mpsc::channel::<PipelineMessage>(32);
   306	
   307	    // Start the pipeline
   308	    let pipeline_handle = match start_pipeline(pipeline_config, tx) {
   309	        Ok(h) => h,
   310	        Err(e) => {
   311	            error!("Failed to start continuous mode pipeline: {}", e);
   312	            if let Ok(mut state) = handle.state.lock() {
   313	                *state = ContinuousState::Error(e.to_string());
   314	            } else {
   315	                warn!("State lock poisoned while setting error state");
   316	            }
   317	            let _ = app.emit("continuous_mode_event", serde_json::json!({
   318	                "type": "error",
   319	                "error": e.to_string()
   320	            }));
   321	            return Err(e.to_string());
   322	        }
   323	    };
   324	
   325	    info!("Continuous mode pipeline started");
   326	
   327	    // Clone the biomarker reset flag so the detector task can trigger resets on encounter boundaries
   328	    let reset_bio_for_detector = pipeline_handle.reset_biomarkers_flag();
   329	
   330	    // Get native STT shadow accumulator for draining at encounter boundaries
   331	    let native_stt_accumulator_for_detector = pipeline_handle.native_stt_accumulator();
   332	
   333	    // Pipeline started successfully — now set state and emit event
   334	    if let Ok(mut state) = handle.state.lock() {
   335	        *state = ContinuousState::Recording;
   336	    } else {
   337	        warn!("State lock poisoned while setting recording state");
   338	    }
   339	    let _ = app.emit("continuous_mode_event", serde_json::json!({
   340	        "type": "started"
   341	    }));
   342	
   343	    // Tag the buffer with this pipeline's generation so stale segments are rejected
   344	    let pipeline_generation: u64 = 1; // Single pipeline per continuous mode run
   345	    if let Ok(mut buffer) = handle.transcript_buffer.lock() {
   346	        buffer.set_generation(pipeline_generation);
   347	    } else {
   348	        warn!("Buffer lock poisoned while setting generation");
   349	    }
   350	
   351	    // Clone handles for the segment consumer task
   352	    let buffer_for_consumer = handle.transcript_buffer.clone();
   353	    let stop_for_consumer = handle.stop_flag.clone();
   354	    let app_for_consumer = app.clone();
   355	
   356	    // Track silence duration for trigger
   357	    let silence_start = Arc::new(Mutex::new(Option::<std::time::Instant>::None));
   358	    let silence_trigger_tx = Arc::new(tokio::sync::Notify::new());
   359	    let silence_trigger_rx = silence_trigger_tx.clone();
   360	    let silence_threshold_secs = config.encounter_silence_trigger_secs;
   361	    let silence_start_for_consumer = silence_start.clone();
   362	
   363	    // Spawn segment consumer task
   364	    let consumer_task = tokio::spawn(async move {
   365	        while let Some(msg) = rx.recv().await {
   366	            if stop_for_consumer.load(Ordering::Relaxed) {
   367	                break;
   368	            }
   369	
   370	            match msg {
   371	                PipelineMessage::Segment(segment) => {
   372	                    // Reset silence tracking on speech
   373	                    if let Ok(mut s) = silence_start_for_consumer.lock() {
   374	                        *s = None;
   375	                    } else {
   376	                        warn!("Silence tracking lock poisoned, silence state may be stale");
   377	                    }
   378	
   379	                    if let Ok(mut buffer) = buffer_for_consumer.lock() {
   380	                        buffer.push(
   381	                            segment.text.clone(),
   382	                            segment.end_ms,
   383	                            segment.speaker_id.clone(),
   384	                            segment.speaker_confidence,
   385	                            pipeline_generation,
   386	                        );
   387	                    } else {
   388	                        warn!("Buffer lock poisoned, segment dropped: {}", segment.text);
   389	                    }
   390	
   391	                    // Emit transcript preview for live monitoring view (with speaker labels)
   392	                    if let Ok(buffer) = buffer_for_consumer.lock() {
   393	                        let text = buffer.full_text_with_speakers();
   394	                        // Only send last ~500 chars for preview (char-boundary safe)
   395	                        let preview = if text.len() > 500 {
   396	                            let target = text.len() - 500;
   397	                            // Find the nearest char boundary at or after the target offset
   398	                            let start = text.ceil_char_boundary(target);
   399	                            format!("...{}", &text[start..])
   400	                        } else {
   401	                            text
   402	                        };
   403	                        let _ = app_for_consumer.emit("continuous_transcript_preview", serde_json::json!({
   404	                            "finalized_text": preview,
   405	                            "draft_text": null,
   406	                            "segment_count": 0
   407	                        }));
   408	                    } else {
   409	                        warn!("Buffer lock poisoned, transcript preview skipped");
   410	                    }
   411	                }
   412	                PipelineMessage::Status { is_speech_active, .. } => {
   413	                    if !is_speech_active {
   414	                        // Track silence start
   415	                        let mut s = silence_start_for_consumer.lock().unwrap_or_else(|e| e.into_inner());
   416	                        if s.is_none() {
   417	                            *s = Some(std::time::Instant::now());
   418	                        } else if let Some(start) = *s {
   419	                            if start.elapsed().as_secs() >= silence_threshold_secs as u64 {
   420	                                // Silence gap detected — trigger encounter check
   421	                                // Use notify_waiters so both active detector AND shadow observer receive the event
   422	                                silence_trigger_tx.notify_waiters();
   423	                                *s = None; // Reset so we don't keep triggering
   424	                            }
   425	                        }
   426	                    } else {
   427	                        // Speech active — reset silence
   428	                        let mut s = silence_start_for_consumer.lock().unwrap_or_else(|e| e.into_inner());
   429	                        *s = None;
   430	                    }
   431	                }
   432	                PipelineMessage::Biomarker(update) => {
   433	                    let _ = app_for_consumer.emit("biomarker_update", update);
   434	                }
   435	                PipelineMessage::AudioQuality(snapshot) => {
   436	                    let _ = app_for_consumer.emit("audio_quality", snapshot);
   437	                }
   438	                PipelineMessage::Stopped => {
   439	                    info!("Continuous mode pipeline stopped");
   440	                    break;
   441	                }
   442	                PipelineMessage::Error(e) => {
   443	                    error!("Continuous mode pipeline error: {}", e);
   444	                    break;
   445	                }
   446	                PipelineMessage::TranscriptChunk { text } => {
   447	                    // Emit streaming chunk as draft_text for live preview
   448	                    let _ = app_for_consumer.emit("continuous_transcript_preview", serde_json::json!({
   449	                        "finalized_text": null,
   450	                        "draft_text": text,
   451	                        "segment_count": 0
   452	                    }));
   453	                }
   454	                // Ignore auto-end messages in continuous mode
   455	                PipelineMessage::AutoEndSilence { .. } | PipelineMessage::SilenceWarning { .. } => {}
   456	                // Shadow native STT transcript — ignore in continuous mode consumer
   457	                // (shadow accumulator is managed by pipeline thread, saved during encounter archival)
   458	                PipelineMessage::NativeSttShadowTranscript { .. } => {}
   459	            }
```

### Sensor mode, hybrid mode, and shadow mode setup

```rust
   463	    // Start presence sensor if in sensor, shadow, or hybrid detection mode
   464	    let is_shadow_mode = config.encounter_detection_mode == EncounterDetectionMode::Shadow;
   465	    let is_hybrid_mode = config.encounter_detection_mode == EncounterDetectionMode::Hybrid;
   466	    let shadow_active_method = config.shadow_active_method;
   467	    let needs_sensor = matches!(
   468	        config.encounter_detection_mode,
   469	        EncounterDetectionMode::Sensor | EncounterDetectionMode::Shadow | EncounterDetectionMode::Hybrid
   470	    );
   471	    let use_sensor_mode = needs_sensor && !config.presence_sensor_port.is_empty();
   472	    let mut sensor_handle: Option<crate::presence_sensor::PresenceSensor> = None;
   473	    let sensor_absence_trigger: Arc<tokio::sync::Notify>;
   474	    // Shadow sensor observer uses watch channel for state transitions (not Notify)
   475	    let mut shadow_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;
   476	    // Hybrid mode: dedicated watch receiver for sensor state in the detection loop
   477	    let mut hybrid_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;
   478	
   479	    if use_sensor_mode {
   480	        // Auto-detect sensor port if configured port is missing or changed
   481	        let sensor_port = crate::presence_sensor::auto_detect_port(&config.presence_sensor_port)
   482	            .unwrap_or_default();
   483	
   484	        let sensor_config = crate::presence_sensor::SensorConfig {
   485	            port: sensor_port,
   486	            debounce_secs: config.presence_debounce_secs,
   487	            absence_threshold_secs: config.presence_absence_threshold_secs,
   488	            csv_log_enabled: config.presence_csv_log_enabled,
   489	        };
   490	
   491	        match crate::presence_sensor::PresenceSensor::start(&sensor_config) {
   492	            Ok(sensor) => {
   493	                info!("Presence sensor started for encounter detection");
   494	                sensor_absence_trigger = sensor.absence_notifier();
   495	
   496	                // Store sensor state receivers in the handle for stats
   497	                if let Ok(mut rx) = handle.sensor_state_rx.lock() {
   498	                    *rx = Some(sensor.subscribe_state());
   499	                }
   500	                if let Ok(mut rx) = handle.sensor_status_rx.lock() {
   501	                    *rx = Some(sensor.subscribe_status());
   502	                }
   503	
   504	                // Emit sensor status event
   505	                let _ = app.emit("continuous_mode_event", serde_json::json!({
   506	                    "type": "sensor_status",
   507	                    "connected": true,
   508	                    "state": "unknown"
   509	                }));
   510	
   511	                // Get a dedicated state receiver for shadow sensor observer
   512	                shadow_sensor_state_rx = Some(sensor.subscribe_state());
   513	                // Get a dedicated state receiver for hybrid detection loop
   514	                if is_hybrid_mode {
   515	                    hybrid_sensor_state_rx = Some(sensor.subscribe_state());
   516	                }
   517	                sensor_handle = Some(sensor);
   518	            }
   519	            Err(e) => {
   520	                warn!("Failed to start presence sensor: {}. Falling back to LLM mode.", e);
   521	                let _ = app.emit("continuous_mode_event", serde_json::json!({
   522	                    "type": "error",
   523	                    "error": format!("Sensor failed to start: {}. Using LLM detection.", e)
   524	                }));
   525	                // Fall back: create a dummy Notify that never fires
   526	                sensor_absence_trigger = Arc::new(tokio::sync::Notify::new());
   527	            }
   528	        }
   529	    } else {
   530	        // LLM mode — no sensor absence trigger
   531	        sensor_absence_trigger = Arc::new(tokio::sync::Notify::new());
   532	    }
   533	
   534	    // Determine effective detection mode (may have fallen back from sensor to LLM)
   535	    // In shadow mode, the active method controls which detection branch runs
   536	    // In hybrid mode, sensor is handled separately (not via effective_sensor_mode)
   537	    let effective_sensor_mode = if is_shadow_mode {
   538	        shadow_active_method == ShadowActiveMethod::Sensor && sensor_handle.is_some()
   539	    } else if is_hybrid_mode {
   540	        false // Hybrid uses its own sensor integration in the detection loop
   541	    } else {
   542	        sensor_handle.is_some()
   543	    };
   544	
   545	    // Spawn sensor status monitoring task (emits events on state/status changes)
   546	    // Also spawn for hybrid mode when sensor is available (even though effective_sensor_mode is false)
   547	    let has_sensor = sensor_handle.is_some();
   548	    let sensor_monitor_task: Option<tokio::task::JoinHandle<()>> = if effective_sensor_mode || (is_hybrid_mode && has_sensor) {
   549	        let sensor = sensor_handle.as_ref().expect("sensor monitor requires sensor_handle.is_some()");
   550	        let mut state_rx = sensor.subscribe_state();
   551	        let mut status_rx = sensor.subscribe_status();
   552	        let stop_for_monitor = handle.stop_flag.clone();
   553	        let app_for_monitor = app.clone();
   554	
   555	        Some(tokio::spawn(async move {
   556	            loop {
   557	                if stop_for_monitor.load(Ordering::Relaxed) {
   558	                    break;
   559	                }
   560	
   561	                tokio::select! {
   562	                    Ok(()) = state_rx.changed() => {
   563	                        let state = *state_rx.borrow_and_update();
   564	                        let state_str = match state {
   565	                            crate::presence_sensor::PresenceState::Present => "present",
   566	                            crate::presence_sensor::PresenceState::Absent => "absent",
   567	                            crate::presence_sensor::PresenceState::Unknown => "unknown",
   568	                        };
   569	                        info!("Sensor state changed: {}", state_str);
   570	                        let _ = app_for_monitor.emit("continuous_mode_event", serde_json::json!({
   571	                            "type": "sensor_status",
   572	                            "connected": true,
   573	                            "state": state_str
   574	                        }));
   575	                    }
   576	                    Ok(()) = status_rx.changed() => {
   577	                        let status = status_rx.borrow_and_update().clone();
   578	                        let connected = matches!(status, crate::presence_sensor::SensorStatus::Connected);
   579	                        let _ = app_for_monitor.emit("continuous_mode_event", serde_json::json!({
   580	                            "type": "sensor_status",
   581	                            "connected": connected,
   582	                            "state": "unknown"
   583	                        }));
   584	                        if !connected {
   585	                            warn!("Sensor disconnected: {:?}", status);
   586	                        }
   587	                    }
   588	                    else => break,
   589	                }
   590	            }
   591	        }))
   592	    } else {
   593	        None
   594	    };
   595	
   596	    // Spawn shadow observer task (if shadow mode is active)
   597	    let shadow_task: Option<tokio::task::JoinHandle<()>> = if is_shadow_mode {
   598	        let shadow_method = if shadow_active_method == ShadowActiveMethod::Sensor { "llm" } else { "sensor" };
   599	        let active_method = shadow_active_method;
   600	        info!("Shadow mode: active={}, shadow={}", active_method, shadow_method);
   601	
   602	        // Initialize shadow CSV logger
   603	        let shadow_csv_logger: Option<Arc<Mutex<crate::shadow_log::ShadowCsvLogger>>> = if config.shadow_csv_log_enabled {
   604	            match crate::shadow_log::ShadowCsvLogger::new() {
   605	                Ok(logger) => Some(Arc::new(Mutex::new(logger))),
   606	                Err(e) => {
   607	                    warn!("Failed to create shadow CSV logger: {}", e);
   608	                    None
   609	                }
   610	            }
   611	        } else {
   612	            None
   613	        };
   614	
   615	        let shadow_decisions_for_task = handle.shadow_decisions.clone();
   616	        let last_shadow_for_task = handle.last_shadow_decision.clone();
   617	        let stop_for_shadow = handle.stop_flag.clone();
   618	        let app_for_shadow = app.clone();
   619	        let buffer_for_shadow = handle.transcript_buffer.clone();
   620	
   621	        if shadow_method == "sensor" {
   622	            // Active=LLM, Shadow=sensor — observe sensor state transitions
   623	            // Use watch channel (not Notify) so we only fire on Present→Absent transitions
   624	            if let Some(mut state_rx) = shadow_sensor_state_rx.take() {
   625	                Some(tokio::spawn(async move {
   626	                    info!("Shadow sensor observer started (watch-based)");
   627	                    let mut prev_state = crate::presence_sensor::PresenceState::Unknown;
   628	                    loop {
   629	                        if stop_for_shadow.load(Ordering::Relaxed) {
   630	                            break;
   631	                        }
   632	
   633	                        // Wait for next state change
   634	                        if state_rx.changed().await.is_err() {
   635	                            info!("Shadow sensor: watch channel closed");
   636	                            break;
   637	                        }
   638	
   639	                        if stop_for_shadow.load(Ordering::Relaxed) {
   640	                            break;
   641	                        }
   642	
   643	                        let new_state = *state_rx.borrow_and_update();
   644	
   645	                        // Determine shadow outcome based on state transition
   646	                        let outcome = match (prev_state, new_state) {
   647	                            (crate::presence_sensor::PresenceState::Present, crate::presence_sensor::PresenceState::Absent) => {
   648	                                // Present→Absent: this is an encounter boundary
   649	                                crate::shadow_log::ShadowOutcome::WouldSplit
   650	                            }
   651	                            (_, crate::presence_sensor::PresenceState::Present) => {
   652	                                // Any→Present: no split (patient arrived or still here)
   653	                                crate::shadow_log::ShadowOutcome::WouldNotSplit
   654	                            }
   655	                            _ => {
   656	                                // Unknown→Absent, Absent→Absent, etc: skip
   657	                                prev_state = new_state;
   658	                                continue;
   659	                            }
   660	                        };
   661	
   662	                        prev_state = new_state;
   663	
   664	                        // Read buffer state (non-destructive)
   665	                        let (word_count, last_segment) = buffer_for_shadow
   666	                            .lock()
   667	                            .map(|b| (b.word_count(), b.last_index()))
   668	                            .unwrap_or((0, None));
   669	
   670	                        let decision = crate::shadow_log::ShadowDecision {
   671	                            timestamp: Utc::now(),
   672	                            shadow_method: "sensor".to_string(),
   673	                            active_method: active_method.to_string(),
   674	                            outcome: outcome.clone(),
   675	                            confidence: Some(1.0),
   676	                            buffer_word_count: word_count,
   677	                            buffer_last_segment: last_segment,
   678	                        };
   679	
   680	                        let outcome_str = match outcome {
   681	                            crate::shadow_log::ShadowOutcome::WouldSplit => "would_split",
   682	                            crate::shadow_log::ShadowOutcome::WouldNotSplit => "would_not_split",
   683	                        };
   684	
   685	                        // Log to CSV
   686	                        if let Some(ref logger) = shadow_csv_logger {
   687	                            if let Ok(mut l) = logger.lock() {
   688	                                l.write_decision(&decision);
   689	                            }
   690	                        }
   691	
   692	                        // Store for encounter comparison
   693	                        let summary = crate::shadow_log::ShadowDecisionSummary::from(&decision);
   694	                        if let Ok(mut decisions) = shadow_decisions_for_task.lock() {
   695	                            decisions.push(summary);
   696	                        }
   697	                        if let Ok(mut last) = last_shadow_for_task.lock() {
   698	                            *last = Some(decision);
   699	                        }
   700	
   701	                        // Emit event for frontend
   702	                        let _ = app_for_shadow.emit("continuous_mode_event", serde_json::json!({
   703	                            "type": "shadow_decision",
   704	                            "shadow_method": "sensor",
   705	                            "outcome": outcome_str,
   706	                            "buffer_words": word_count,
   707	                            "sensor_state": new_state.as_str()
   708	                        }));
   709	
   710	                        info!("Shadow sensor: {} (state: {}, buffer {} words)", outcome_str, new_state.as_str(), word_count);
   711	                    }
   712	                    info!("Shadow sensor observer stopped");
   713	                }))
   714	            } else {
   715	                warn!("Shadow sensor observer: no sensor state receiver available (sensor failed to start)");
   716	                None
   717	            }
   718	        } else {
   719	            // Active=sensor, Shadow=LLM — run shadow LLM detection loop
   720	            let silence_trigger_for_shadow = silence_trigger_rx.clone();
   721	            let check_interval_shadow = config.encounter_check_interval_secs;
   722	            let shadow_detection_model = config.encounter_detection_model.clone();
   723	            let shadow_detection_nothink = config.encounter_detection_nothink;
   724	            let shadow_llm_client = if !config.llm_router_url.is_empty() {
   725	                LLMClient::new(
   726	                    &config.llm_router_url,
   727	                    &config.llm_api_key,
   728	                    &config.llm_client_id,
   729	                    &shadow_detection_model,
   730	                )
   731	                .ok()
   732	            } else {
   733	                None
   734	            };
   735	
   736	            Some(tokio::spawn(async move {
   737	                info!("Shadow LLM observer started");
   738	                loop {
   739	                    if stop_for_shadow.load(Ordering::Relaxed) {
   740	                        break;
   741	                    }
   742	
   743	                    // Wait for timer or silence trigger (same as active LLM detector)
   744	                    tokio::select! {
   745	                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval_shadow as u64)) => {}
   746	                        _ = silence_trigger_for_shadow.notified() => {
   747	                            debug!("Shadow LLM: silence trigger received");
   748	                        }
   749	                    }
   750	
   751	                    if stop_for_shadow.load(Ordering::Relaxed) {
   752	                        break;
   753	                    }
   754	
   755	                    // Read buffer state (non-destructive, full transcript for detection)
   756	                    let (formatted, word_count, last_segment) = buffer_for_shadow
   757	                        .lock()
   758	                        .map(|b| (b.format_for_detection(), b.word_count(), b.last_index()))
   759	                        .unwrap_or_else(|_| (String::new(), 0, None));
   760	
```

### Hybrid trigger handling and detector loop entry

```rust
   942	    let detector_task = tokio::spawn(async move {
   943	        let mut encounter_number: u32 = 0;
   944	        let mut consecutive_no_split: u32 = 0;
   945	        // Tracks how many times a split was merged back into the previous encounter.
   946	        // Each merge-back escalates the confidence threshold by +0.05, making
   947	        // repeated false-positive splits on long sessions increasingly unlikely.
   948	        let mut merge_back_count: u32 = 0;
   949	
   950	        // Hybrid mode: sensor absence tracking
   951	        let mut sensor_absent_since: Option<DateTime<Utc>> = None;
   952	        let mut prev_sensor_state = crate::presence_sensor::PresenceState::Unknown;
   953	        let mut sensor_available = hybrid_sensor_rx.is_some();
   954	        // Tracks whether the current split was triggered by sensor timeout (for metadata)
   955	        // Initialized inside the loop on each iteration — declared here so it's available across the loop body
   956	        let mut hybrid_sensor_timeout_triggered;
   957	
   958	        // Track previous encounter for retrospective merge checks
   959	        let mut prev_encounter_session_id: Option<String> = None;
   960	        let mut prev_encounter_text: Option<String> = None;
   961	        let mut prev_encounter_date: Option<DateTime<Utc>> = None;
   962	        let mut prev_encounter_is_clinical: bool = true;
   963	
   964	        loop {
   965	            // Reset per-iteration hybrid tracking
   966	            hybrid_sensor_timeout_triggered = false;
   967	
   968	            // Wait for trigger based on detection mode
   969	            // Returns (manual_triggered, sensor_triggered)
   970	            let (manual_triggered, sensor_triggered) = if is_hybrid_mode && sensor_available {
   971	                // Hybrid mode with sensor: timer + silence + manual + sensor
   972	                let sensor_rx = hybrid_sensor_rx.as_mut().unwrap();
   973	                tokio::select! {
   974	                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
   975	                        // Regular timer — handles back-to-back encounters without physical departure
   976	                        (false, false)
   977	                    }
   978	                    _ = silence_trigger_rx.notified() => {
   979	                        info!("Hybrid: silence gap detected — triggering encounter check");
   980	                        (false, false)
   981	                    }
   982	                    _ = manual_trigger_rx.notified() => {
   983	                        info!("Manual new patient trigger received");
   984	                        (true, false)
   985	                    }
   986	                    result = sensor_rx.changed() => {
   987	                        match result {
   988	                            Ok(()) => {
   989	                                let new_state = *sensor_rx.borrow_and_update();
   990	                                let old_state = prev_sensor_state;
   991	                                prev_sensor_state = new_state;
   992	                                match (old_state, new_state) {
   993	                                    (crate::presence_sensor::PresenceState::Present,
   994	                                     crate::presence_sensor::PresenceState::Absent) => {
   995	                                        sensor_absent_since = Some(Utc::now());
   996	                                        info!("Hybrid: sensor detected departure (Present→Absent), accelerating LLM check");
   997	                                        (false, true) // sensor_triggered → accelerate LLM check (NOT force-split)
   998	                                    }
   999	                                    (_, crate::presence_sensor::PresenceState::Present) => {
  1000	                                        if sensor_absent_since.is_some() {
  1001	                                            info!("Hybrid: person returned — cancelling sensor absence tracking");
  1002	                                            sensor_absent_since = None;
  1003	                                        }
  1004	                                        continue; // No check needed
  1005	                                    }
  1006	                                    _ => continue, // Other transitions (Absent→Absent, Unknown→Absent, etc.)
  1007	                                }
  1008	                            }
  1009	                            Err(_) => {
  1010	                                warn!("Hybrid: sensor watch channel closed — sensor disconnected. Falling back to LLM-only.");
  1011	                                sensor_available = false;
  1012	                                sensor_absent_since = None;
  1013	                                let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1014	                                    "type": "sensor_status",
  1015	                                    "connected": false,
  1016	                                    "state": "unknown"
  1017	                                }));
  1018	                                continue; // Re-enter loop; next iteration uses LLM-only path
  1019	                            }
  1020	                        }
  1021	                    }
  1022	                }
  1023	            } else if is_hybrid_mode {
  1024	                // Hybrid mode without sensor (sensor failed/disconnected): pure LLM fallback
  1025	                tokio::select! {
  1026	                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
  1027	                        (false, false)
  1028	                    }
  1029	                    _ = silence_trigger_rx.notified() => {
  1030	                        info!("Hybrid (LLM fallback): silence gap detected — triggering encounter check");
  1031	                        (false, false)
  1032	                    }
  1033	                    _ = manual_trigger_rx.notified() => {
  1034	                        info!("Manual new patient trigger received");
  1035	                        (true, false)
  1036	                    }
  1037	                }
  1038	            } else if effective_sensor_mode {
  1039	                // Pure sensor mode: wait for sensor absence threshold OR manual trigger
  1040	                tokio::select! {
  1041	                    _ = sensor_trigger_for_detector.notified() => {
  1042	                        info!("Sensor: absence threshold reached — triggering encounter split");
  1043	                        (false, true)
  1044	                    }
  1045	                    _ = manual_trigger_rx.notified() => {
  1046	                        info!("Manual new patient trigger received");
  1047	                        (true, false)
  1048	                    }
  1049	                }
  1050	            } else {
  1051	                // LLM / Shadow mode: wait for timer, silence, or manual trigger
  1052	                tokio::select! {
  1053	                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
  1054	                        (false, false)
  1055	                    }
  1056	                    _ = silence_trigger_rx.notified() => {
  1057	                        info!("Silence gap detected — triggering encounter check");
  1058	                        (false, false)
  1059	                    }
  1060	                    _ = manual_trigger_rx.notified() => {
  1061	                        info!("Manual new patient trigger received");
  1062	                        (true, false)
  1063	                    }
  1064	                }
  1065	            };
  1066	
  1067	            if stop_for_detector.load(Ordering::Relaxed) {
  1068	                break;
  1069	            }
  1070	
  1071	            // Check if buffer has enough content to analyze
  1072	            let (formatted, word_count, is_empty, first_ts) = {
  1073	                let buffer = match buffer_for_detector.lock() {
  1074	                    Ok(b) => b,
  1075	                    Err(_) => continue,
  1076	                };
  1077	                (buffer.format_for_detection(), buffer.word_count(), buffer.is_empty(), buffer.first_timestamp())
  1078	            };
  1079	
  1080	            // Pre-compute hallucination-cleaned word count for large buffers.
  1081	            // This prevents STT phrase loops from inflating word counts and triggering
  1082	            // premature force-splits. Only runs when buffer is large enough to matter.
  1083	            let (filtered_formatted, hallucination_report) = if word_count > FORCE_CHECK_WORD_THRESHOLD {
  1084	                let (filtered, report) = strip_hallucinations(&formatted, 5);
  1085	                if !report.repetitions.is_empty() || !report.phrase_repetitions.is_empty() {
  1086	                    info!(
  1087	                        "Hallucination filter: {} → {} words ({} single-word, {} phrase repetitions stripped)",
  1088	                        report.original_word_count, report.cleaned_word_count,
  1089	                        report.repetitions.len(), report.phrase_repetitions.len()
  1090	                    );
  1091	                    if let Ok(mut logger) = logger_for_detector.lock() {
  1092	                        logger.log_hallucination_filter(serde_json::json!({
  1093	                            "call_site": "detection",
  1094	                            "original_words": report.original_word_count,
  1095	                            "cleaned_words": report.cleaned_word_count,
  1096	                            "single_word_reps": report.repetitions.iter()
  1097	                                .map(|r| &r.word).collect::<Vec<_>>(),
  1098	                            "phrase_reps": report.phrase_repetitions.iter()
  1099	                                .map(|r| &r.phrase).collect::<Vec<_>>(),
  1100	                        }));
  1101	                    }
  1102	                }
  1103	                (Some(filtered), Some(report))
  1104	            } else {
  1105	                (None, None)
  1106	            };
```

### Detection, force-split, and confidence-gate logic

```rust
  1120	                    if sensor_triggered { "Sensor trigger" } else { "Manual trigger" }, word_count);
  1121	            } else {
  1122	                if is_empty || word_count < 100 {
  1123	                    debug!("Skipping detection: word_count={} (minimum 100)", word_count);
  1124	                    continue;
  1125	                }
  1126	
  1127	                // Also trigger if buffer is very large (safety valve).
  1128	                // Use hallucination-cleaned word count so STT phrase loops
  1129	                // don't inflate counts past the threshold prematurely.
  1130	                let force_check = cleaned_word_count > FORCE_CHECK_WORD_THRESHOLD;
  1131	
  1132	                // Minimum encounter duration: 2 minutes (unless force_check)
  1133	                if !force_check {
  1134	                    if let Some(first_time) = first_ts {
  1135	                        let buffer_age_secs = (Utc::now() - first_time).num_seconds();
  1136	                        if buffer_age_secs < 120 {
  1137	                            debug!("Skipping detection: buffer_age={}s (minimum 120s), word_count={}", buffer_age_secs, word_count);
  1138	                            continue;
  1139	                        }
  1140	                    }
  1141	                }
  1142	                if force_check {
  1143	                    info!("Buffer exceeds {} cleaned words (raw={}, cleaned={}) — forcing encounter check",
  1144	                        FORCE_CHECK_WORD_THRESHOLD, word_count, cleaned_word_count);
  1145	                }
  1146	            }
  1147	
  1148	            // Set state to checking
  1149	            if let Ok(mut state) = state_for_detector.lock() {
  1150	                *state = ContinuousState::Checking;
  1151	            } else {
  1152	                warn!("State lock poisoned while setting checking state");
  1153	            }
  1154	            let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1155	                "type": "checking"
  1156	            }));
  1157	
  1158	            // Run encounter detection via LLM (with 60s timeout to prevent blocking)
  1159	            // Manual trigger or pure-sensor trigger: skip LLM — directly split
  1160	            // Hybrid sensor trigger: accelerate LLM check (do NOT force-split)
  1161	            let detection_result = if manual_triggered || (sensor_triggered && !is_hybrid_mode) {
  1162	                let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
  1163	                let source = if sensor_triggered { "Sensor" } else { "Manual" };
  1164	                info!("{} trigger: forcing encounter split (last_index={:?})", source, last_idx);
  1165	                if let Ok(mut logger) = logger_for_detector.lock() {
  1166	                    logger.log_split_trigger(serde_json::json!({
  1167	                        "trigger": source.to_lowercase(),
  1168	                        "word_count": word_count,
  1169	                        "cleaned_word_count": cleaned_word_count,
  1170	                    }));
  1171	                }
  1172	                Some(EncounterDetectionResult {
  1173	                    complete: true,
  1174	                    end_segment_index: last_idx,
  1175	                    confidence: Some(1.0),
  1176	                })
  1177	            } else if let Some(ref client) = llm_client {
  1178	                // Reuse pre-computed hallucination-filtered text if available, otherwise filter now
  1179	                let filtered_for_llm = filtered_formatted.clone().unwrap_or_else(|| {
  1180	                    let (filtered, _) = strip_hallucinations(&formatted, 5);
  1181	                    filtered
  1182	                });
  1183	                // Build detection context from available signals (vision name change, sensor state)
  1184	                let detection_context = {
  1185	                    let mut ctx = EncounterDetectionContext::default();
  1186	                    if sensor_triggered {
  1187	                        ctx.sensor_departed = true;
  1188	                    } else if sensor_absent_since.is_some() {
  1189	                        ctx.sensor_departed = true;
  1190	                    }
  1191	                    // Tell LLM when sensor confirms someone is still present (suppresses false splits)
  1192	                    if sensor_available && !ctx.sensor_departed {
  1193	                        ctx.sensor_present = true;
  1194	                    }
  1195	                    ctx
  1196	                };
  1197	                let (system_prompt, user_prompt) = build_encounter_detection_prompt(
  1198	                    &filtered_for_llm,
  1199	                    Some(&detection_context),
  1200	                );
  1201	                // Prepend /nothink for Qwen3 models to disable thinking mode (improves detection accuracy)
  1202	                let system_prompt = if detection_nothink {
  1203	                    format!("/nothink\n{}", system_prompt)
  1204	                } else {
  1205	                    system_prompt
  1206	                };
  1207	                let detect_start = Instant::now();
  1208	                let llm_future = client.generate(&detection_model, &system_prompt, &user_prompt, "encounter_detection");
  1209	                let detect_ctx = serde_json::json!({
  1210	                    "word_count": word_count,
  1211	                    "cleaned_word_count": cleaned_word_count,
  1212	                    "sensor_present": detection_context.sensor_present,
  1213	                    "sensor_departed": detection_context.sensor_departed,
  1214	                    "nothink": detection_nothink,
  1215	                    "consecutive_no_split": consecutive_no_split,
  1216	                });
  1217	                match tokio::time::timeout(tokio::time::Duration::from_secs(90), llm_future).await {
  1218	                    Ok(Ok(response)) => {
  1219	                        let latency = detect_start.elapsed().as_millis() as u64;
  1220	                        match parse_encounter_detection(&response) {
  1221	                            Ok(result) => {
  1222	                                info!(
  1223	                                    "Detection result: complete={}, confidence={:?}, end_segment_index={:?}, word_count={}",
  1224	                                    result.complete, result.confidence, result.end_segment_index, word_count
  1225	                                );
  1226	                                // Clear any previous error on successful detection
  1227	                                if let Ok(mut err) = last_error_for_detector.lock() {
  1228	                                    *err = None;
  1229	                                }
  1230	                                if let Ok(mut logger) = logger_for_detector.lock() {
  1231	                                    let mut ctx = detect_ctx.clone();
  1232	                                    ctx["parsed_complete"] = serde_json::json!(result.complete);
  1233	                                    ctx["parsed_confidence"] = serde_json::json!(result.confidence);
  1234	                                    ctx["parsed_end_segment_index"] = serde_json::json!(result.end_segment_index);
  1235	                                    logger.log_detection(
  1236	                                        &detection_model, &system_prompt, &user_prompt,
  1237	                                        Some(&response), latency, true, None, ctx,
  1238	                                    );
  1239	                                }
  1240	                                Some(result)
  1241	                            }
  1242	                            Err(e) => {
  1243	                                warn!("Failed to parse encounter detection: {}", e);
  1244	                                if let Ok(mut logger) = logger_for_detector.lock() {
  1245	                                    let mut ctx = detect_ctx.clone();
  1246	                                    ctx["parse_error"] = serde_json::json!(true);
  1247	                                    logger.log_detection(
  1248	                                        &detection_model, &system_prompt, &user_prompt,
  1249	                                        Some(&response), latency, false, Some(&e), ctx,
  1250	                                    );
  1251	                                }
  1252	                                if let Ok(mut err) = last_error_for_detector.lock() {
  1253	                                    *err = Some(e);
  1254	                                } else {
  1255	                                    warn!("Last error lock poisoned, error state not updated");
  1256	                                }
  1257	                                None
  1258	                            }
  1259	                        }
  1260	                    }
  1261	                    Ok(Err(e)) => {
  1262	                        let latency = detect_start.elapsed().as_millis() as u64;
  1263	                        warn!("Encounter detection LLM call failed: {}", e);
  1264	                        if let Ok(mut logger) = logger_for_detector.lock() {
  1265	                            let mut ctx = detect_ctx.clone();
  1266	                            ctx["llm_error"] = serde_json::json!(true);
  1267	                            logger.log_detection(
  1268	                                &detection_model, &system_prompt, &user_prompt,
  1269	                                None, latency, false, Some(&e.to_string()), ctx,
  1270	                            );
  1271	                        }
  1272	                        if let Ok(mut err) = last_error_for_detector.lock() {
  1273	                            *err = Some(e);
  1274	                        } else {
  1275	                            warn!("Last error lock poisoned, error state not updated");
  1276	                        }
  1277	                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1278	                            "type": "error",
  1279	                            "error": "Encounter detection failed"
  1280	                        }));
  1281	                        None
  1282	                    }
  1283	                    Err(_elapsed) => {
  1284	                        let latency = detect_start.elapsed().as_millis() as u64;
  1285	                        warn!("Encounter detection LLM call timed out after 90s");
  1286	                        if let Ok(mut logger) = logger_for_detector.lock() {
  1287	                            let mut ctx = detect_ctx.clone();
  1288	                            ctx["timeout"] = serde_json::json!(true);
  1289	                            logger.log_detection(
  1290	                                &detection_model, &system_prompt, &user_prompt,
  1291	                                None, latency, false, Some("timeout_90s"), ctx,
  1292	                            );
  1293	                        }
  1294	                        if let Ok(mut err) = last_error_for_detector.lock() {
  1295	                            *err = Some("Encounter detection timed out".to_string());
  1296	                        } else {
  1297	                            warn!("Last error lock poisoned, error state not updated");
  1298	                        }
  1299	                        None
  1300	                    }
  1301	                }
  1302	            } else {
  1303	                warn!("No LLM client configured for encounter detection");
  1304	                None
  1305	            };
  1306	
  1307	            // Force-split safety valve: tracks consecutive non-split outcomes (both LLM
  1308	            // failures AND negative results). Prevents unbounded buffer growth when the
  1309	            // LLM consistently says "no encounter detected."
  1310	            let mut force_split = false;
  1311	            let mut detection_result = detection_result;
  1312	
  1313	            // Effective word count for force-split: max(cleaned, raw/2).
  1314	            // When the hallucination filter strips heavily (e.g. 4652→1537), raw/2 ensures
  1315	            // the force-split thresholds still engage for genuinely long encounters.
  1316	            let effective_word_count = cleaned_word_count.max(word_count / 2);
  1317	
  1318	            // Absolute word cap: unconditional force-split at ABSOLUTE_WORD_CAP effective words
  1319	            if effective_word_count > ABSOLUTE_WORD_CAP && !manual_triggered && !sensor_triggered {
  1320	                warn!("ABSOLUTE WORD CAP: force-splitting at {} effective words (cleaned: {}, raw: {})", effective_word_count, cleaned_word_count, word_count);
  1321	                if let Ok(mut logger) = logger_for_detector.lock() {
  1322	                    logger.log_split_trigger(serde_json::json!({
  1323	                        "trigger": "absolute_word_cap",
  1324	                        "effective_word_count": effective_word_count,
  1325	                        "cleaned_word_count": cleaned_word_count,
  1326	                        "raw_word_count": word_count,
  1327	                        "cap": ABSOLUTE_WORD_CAP,
  1328	                    }));
  1329	                }
  1330	                let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
  1331	                consecutive_no_split = 0;
  1332	                force_split = true;
  1333	                detection_result = Some(EncounterDetectionResult {
  1334	                    complete: true,
  1335	                    end_segment_index: last_idx,
  1336	                    confidence: Some(1.0),
  1337	                });
  1338	            }
  1339	
  1340	            // Track consecutive no-split outcomes
  1341	            if !force_split && !manual_triggered && !sensor_triggered {
  1342	                let is_negative = match &detection_result {
  1343	                    None => true,                    // LLM failure/timeout
  1344	                    Some(r) if !r.complete => true,  // LLM said no — THE BUG FIX
  1345	                    _ => false,                      // complete=true — resolved by confidence gate below
  1346	                };
  1347	                if is_negative {
  1348	                    consecutive_no_split += 1;
  1349	                    info!(
  1350	                        "Detection non-split: result={}, consecutive_no_split={}, cleaned_word_count={}, raw_word_count={}",
  1351	                        if detection_result.is_none() { "error/timeout" } else { "complete=false" },
  1352	                        consecutive_no_split, cleaned_word_count, word_count
  1353	                    );
  1354	                    // Graduated force-split (uses effective word count: max(cleaned, raw/2))
  1355	                    if effective_word_count > FORCE_SPLIT_WORD_THRESHOLD
  1356	                        && consecutive_no_split >= FORCE_SPLIT_CONSECUTIVE_LIMIT
  1357	                    {
  1358	                        warn!(
  1359	                            "Force-splitting: {} consecutive non-splits with {} effective words (cleaned: {}, raw: {})",
  1360	                            consecutive_no_split, effective_word_count, cleaned_word_count, word_count
  1361	                        );
  1362	                        if let Ok(mut logger) = logger_for_detector.lock() {
  1363	                            logger.log_split_trigger(serde_json::json!({
  1364	                                "trigger": "graduated_force_split",
  1365	                                "consecutive_no_split": consecutive_no_split,
  1366	                                "effective_word_count": effective_word_count,
  1367	                                "cleaned_word_count": cleaned_word_count,
  1368	                                "raw_word_count": word_count,
  1369	                            }));
  1370	                        }
  1371	                        let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
  1372	                        consecutive_no_split = 0;
  1373	                        force_split = true;
  1374	                        detection_result = Some(EncounterDetectionResult {
  1375	                            complete: true,
  1376	                            end_segment_index: last_idx,
  1377	                            confidence: Some(1.0),
  1378	                        });
  1379	                    }
  1380	                }
  1381	                // NOTE: Don't reset counter on complete=true here — confidence gate may reject
  1382	            }
  1383	
  1384	            // Hybrid sensor timeout force-split: sensor has been absent for > confirm_window
  1385	            // and buffer has enough words. This catches cases where LLM keeps saying "no split"
  1386	            // but the sensor correctly detected a departure.
  1387	            if is_hybrid_mode && !force_split && !manual_triggered {
  1388	                if let Some(absent_since) = sensor_absent_since {
  1389	                    let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
  1390	                    if elapsed >= hybrid_confirm_window_secs
  1391	                        && word_count >= hybrid_min_words_for_sensor_split
  1392	                    {
  1393	                        warn!(
  1394	                            "Hybrid: sensor absence timeout ({}s >= {}s) with {} words >= {} — force-splitting",
  1395	                            elapsed, hybrid_confirm_window_secs,
  1396	                            word_count, hybrid_min_words_for_sensor_split
  1397	                        );
  1398	                        if let Ok(mut logger) = logger_for_detector.lock() {
  1399	                            logger.log_split_trigger(serde_json::json!({
  1400	                                "trigger": "hybrid_sensor_timeout",
  1401	                                "absence_secs": elapsed,
  1402	                                "confirm_window_secs": hybrid_confirm_window_secs,
  1403	                                "word_count": word_count,
  1404	                                "min_words": hybrid_min_words_for_sensor_split,
  1405	                            }));
  1406	                        }
  1407	                        let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
  1408	                        force_split = true;
  1409	                        hybrid_sensor_timeout_triggered = true;
  1410	                        sensor_absent_since = None;
  1411	                        consecutive_no_split = 0;
  1412	                        detection_result = Some(EncounterDetectionResult {
  1413	                            complete: true,
  1414	                            end_segment_index: last_idx,
  1415	                            confidence: Some(1.0),
  1416	                        });
  1417	                    }
  1418	                }
  1419	            }
  1420	
  1421	            // Process detection result
  1422	            if let Some(result) = detection_result {
  1423	                if result.complete {
  1424	                    // Confidence gate: dynamic threshold based on buffer age
  1425	                    // Short encounters (<20 min) get a higher bar (0.85) to reduce false splits
  1426	                    // on natural pauses. Longer encounters use 0.7 (established threshold).
  1427	                    let confidence = result.confidence.unwrap_or(0.0);
  1428	                    let buffer_age_mins = first_ts
  1429	                        .map(|t| (Utc::now() - t).num_minutes())
  1430	                        .unwrap_or(0);
  1431	                    // Post-merge-back escalation: each merge-back raises the bar by +0.05,
  1432	                    // making repeated false-positive splits on long sessions increasingly unlikely.
  1433	                    let base_threshold = if buffer_age_mins < 20 { 0.85 } else { 0.7 };
  1434	                    let confidence_threshold = (base_threshold + merge_back_count as f64 * 0.05).min(0.99);
  1435	                    if confidence < confidence_threshold && !force_split {
  1436	                        consecutive_no_split += 1;
  1437	                        info!(
  1438	                            "Confidence gate rejected: confidence={:.2}, threshold={:.2} (base={:.2}, merge_backs={}), buffer_age_mins={}, word_count={}, consecutive_no_split={}",
  1439	                            confidence, confidence_threshold, base_threshold, merge_back_count, buffer_age_mins, word_count, consecutive_no_split
  1440	                        );
  1441	                        if let Ok(mut logger) = logger_for_detector.lock() {
  1442	                            logger.log_confidence_gate(serde_json::json!({
  1443	                                "confidence": confidence,
  1444	                                "threshold": confidence_threshold,
  1445	                                "base_threshold": base_threshold,
  1446	                                "merge_back_count": merge_back_count,
  1447	                                "buffer_age_mins": buffer_age_mins,
  1448	                                "word_count": word_count,
  1449	                                "consecutive_no_split": consecutive_no_split,
  1450	                                "rejected": true,
  1451	                            }));
  1452	                        }
  1453	                        // Return to recording state and continue
  1454	                        if let Ok(mut state) = state_for_detector.lock() {
  1455	                            if *state == ContinuousState::Checking {
  1456	                                *state = ContinuousState::Recording;
  1457	                            }
  1458	                        } else {
  1459	                            warn!("State lock poisoned while returning to recording state");
  1460	                        }
  1461	                        continue;
  1462	                    }
  1463	
  1464	                    if let Some(end_index) = result.end_segment_index {
  1465	                        consecutive_no_split = 0;
```

### Encounter archival, metadata, shadow transcript, and patient-name handling

```rust
  1507	                        // Archive the encounter transcript (pass actual start time for accurate duration)
  1508	                        if let Err(e) = local_archive::save_session(
  1509	                            &session_id,
  1510	                            &encounter_text,
  1511	                            0, // duration_ms unused when encounter_started_at is provided
  1512	                            None, // No per-encounter audio in continuous mode
  1513	                            false,
  1514	                            None,
  1515	                            encounter_start, // actual encounter start time for duration calc
  1516	                        ) {
  1517	                            warn!("Failed to archive encounter: {}", e);
  1518	                        }
  1519	
  1520	                        // Set pipeline logger to write to this session's archive folder
  1521	                        if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
  1522	                            if let Ok(mut logger) = logger_for_detector.lock() {
  1523	                                logger.set_session(&session_dir);
  1524	                            }
  1525	                        }
  1526	
  1527	                        // Drain native STT shadow accumulator for this encounter
  1528	                        let has_shadow_transcript = if let Some(ref accumulator) = stt_shadow_accumulator {
  1529	                            if let Ok(mut acc) = accumulator.lock() {
  1530	                                let drained = acc.drain_through(encounter_last_timestamp_ms);
  1531	                                if !drained.is_empty() {
  1532	                                    // Format as plain text transcript
  1533	                                    let shadow_text: String = drained
  1534	                                        .iter()
  1535	                                        .map(|s| {
  1536	                                            if let Some(ref spk) = s.speaker_id {
  1537	                                                format!("{}: {}", spk, s.native_text)
  1538	                                            } else {
  1539	                                                s.native_text.clone()
  1540	                                            }
  1541	                                        })
  1542	                                        .collect::<Vec<_>>()
  1543	                                        .join("\n");
  1544	
  1545	                                    // Save to archive directory
  1546	                                    if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
  1547	                                        let shadow_path = session_dir.join("shadow_transcript.txt");
  1548	                                        if let Err(e) = std::fs::write(&shadow_path, &shadow_text) {
  1549	                                            warn!("Failed to save shadow transcript: {}", e);
  1550	                                        } else {
  1551	                                            info!("Shadow transcript saved ({} segments, {} chars)", drained.len(), shadow_text.len());
  1552	                                        }
  1553	                                    }
  1554	                                    true
  1555	                                } else {
  1556	                                    false
  1557	                                }
  1558	                            } else {
  1559	                                warn!("Native STT shadow accumulator lock poisoned");
  1560	                                false
  1561	                            }
  1562	                        } else {
  1563	                            false
  1564	                        };
  1565	
  1566	                        // Update archive metadata with continuous mode info
  1567	                        if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
  1568	                            let date_path = session_dir.join("metadata.json");
  1569	                            if date_path.exists() {
  1570	                                if let Ok(content) = std::fs::read_to_string(&date_path) {
  1571	                                    if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
  1572	                                        metadata.charting_mode = Some("continuous".to_string());
  1573	                                        metadata.encounter_number = Some(encounter_number);
  1574	                                        // Record how this encounter was detected
  1575	                                        metadata.detection_method = Some(
  1576	                                            if manual_triggered {
  1577	                                                "manual".to_string()
  1578	                                            } else if is_hybrid_mode {
  1579	                                                if hybrid_sensor_timeout_triggered {
  1580	                                                    "hybrid_sensor_timeout".to_string()
  1581	                                                } else if sensor_triggered {
  1582	                                                    "hybrid_sensor_confirmed".to_string()
  1583	                                                } else if force_split {
  1584	                                                    "hybrid_force".to_string()
  1585	                                                } else {
  1586	                                                    "hybrid_llm".to_string()
  1587	                                                }
  1588	                                            } else if sensor_triggered {
  1589	                                                "sensor".to_string()
  1590	                                            } else {
  1591	                                                "llm".to_string()
  1592	                                            }
  1593	                                        );
  1594	                                        // Add patient name from vision extraction (majority vote)
  1595	                                        if let Ok(tracker) = name_tracker_for_detector.lock() {
  1596	                                            metadata.patient_name = tracker.majority_name();
  1597	                                        } else {
  1598	                                            warn!("Name tracker lock poisoned, patient name not written to metadata");
  1599	                                        }
  1600	                                        // Record whether shadow transcript was saved
  1601	                                        if has_shadow_transcript {
  1602	                                            metadata.has_shadow_transcript = Some(true);
  1603	                                        }
  1604	                                        // Add shadow comparison data if in shadow mode
  1605	                                        if is_shadow_mode {
  1606	                                            let shadow_method = if shadow_active_method == ShadowActiveMethod::Sensor { "llm" } else { "sensor" };
  1607	                                            let decisions: Vec<crate::shadow_log::ShadowDecisionSummary> = handle_shadow_decisions
  1608	                                                .lock()
  1609	                                                .unwrap_or_else(|e| {
  1610	                                                    warn!("Shadow decisions lock poisoned, recovering data");
  1611	                                                    e.into_inner()
  1612	                                                })
  1613	                                                .clone();
  1614	
  1615	                                            let active_split_at = Utc::now().to_rfc3339();
  1616	
  1617	                                            // Check if shadow agreed: any "would_split" decision in last 5 minutes
  1618	                                            let now = Utc::now();
  1619	                                            let shadow_agreed = if decisions.is_empty() {
  1620	                                                None
  1621	                                            } else {
  1622	                                                let agreed = decisions.iter().any(|d| {
  1623	                                                    d.outcome == "would_split" && {
  1624	                                                        chrono::DateTime::parse_from_rfc3339(&d.timestamp)
  1625	                                                            .map(|ts| (now - ts.with_timezone(&Utc)).num_seconds().abs() < 300)
  1626	                                                            .unwrap_or(false)
  1627	                                                    }
  1628	                                                });
  1629	                                                Some(agreed)
  1630	                                            };
  1631	
  1632	                                            metadata.shadow_comparison = Some(crate::shadow_log::ShadowEncounterComparison {
  1633	                                                shadow_method: shadow_method.to_string(),
  1634	                                                decisions,
  1635	                                                active_split_at,
  1636	                                                shadow_agreed,
  1637	                                            });
  1638	                                        }
  1639	
  1640	                                        if let Ok(json) = serde_json::to_string_pretty(&metadata) {
  1641	                                            let _ = std::fs::write(&date_path, json);
  1642	                                        }
  1643	                                    }
  1644	                                }
  1645	                            }
  1646	                        }
  1647	
  1648	                        // Clear shadow decisions for next encounter (if in shadow mode)
  1649	                        if is_shadow_mode {
  1650	                            if let Ok(mut decisions) = handle_shadow_decisions.lock() {
  1651	                                decisions.clear();
  1652	                            }
  1653	                        }
  1654	
  1655	                        // Extract patient name before resetting tracker
  1656	                        let encounter_patient_name = name_tracker_for_detector
  1657	                            .lock()
  1658	                            .ok()
  1659	                            .and_then(|t| t.majority_name());
  1660	
  1661	                        // Reset name tracker for next encounter
  1662	                        if let Ok(mut tracker) = name_tracker_for_detector.lock() {
  1663	                            tracker.reset();
  1664	                        } else {
  1665	                            warn!("Name tracker lock poisoned, tracker not reset for next encounter");
  1666	                        }
  1667	
  1668	                        // Record split timestamp (for stale screenshot detection)
  1669	                        if let Ok(mut t) = last_split_time_for_detector.lock() {
  1670	                            *t = Utc::now();
  1671	                        }
  1672	
  1673	                        // Read encounter notes AND clear atomically (SOAP generation needs them)
  1674	                        let notes_text = match encounter_notes_for_detector.lock() {
  1675	                            Ok(mut notes) => {
  1676	                                let text = notes.clone();
  1677	                                notes.clear();
  1678	                                text
  1679	                            }
  1680	                            Err(e) => {
  1681	                                warn!("Encounter notes lock poisoned, using recovered value: {}", e);
  1682	                                let mut notes = e.into_inner();
  1683	                                let text = notes.clone();
  1684	                                notes.clear();
  1685	                                text
  1686	                            }
  1687	                        };
  1688	
  1689	                        // Reset biomarker accumulators for the new encounter
  1690	                        reset_bio_flag.store(true, std::sync::atomic::Ordering::SeqCst);
  1691	
  1692	                        // Update stats
  1693	                        encounters_for_detector.fetch_add(1, Ordering::Relaxed);
  1694	                        if let Ok(mut at) = last_at_for_detector.lock() {
  1695	                            *at = Some(Utc::now());
  1696	                        } else {
  1697	                            warn!("Last encounter time lock poisoned, stats not updated");
  1698	                        }
  1699	                        if let Ok(mut words) = last_words_for_detector.lock() {
  1700	                            *words = Some(encounter_word_count as u32);
  1701	                        } else {
  1702	                            warn!("Last encounter words lock poisoned, stats not updated");
  1703	                        }
  1704	                        if let Ok(mut name) = last_patient_name_for_detector.lock() {
  1705	                            *name = encounter_patient_name.clone();
  1706	                        } else {
  1707	                            warn!("Last patient name lock poisoned, stats not updated");
  1708	                        }
  1709	
  1710	                        // Emit encounter detected event
  1711	                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1712	                            "type": "encounter_detected",
  1713	                            "session_id": session_id,
  1714	                            "word_count": encounter_word_count,
  1715	                            "patient_name": encounter_patient_name
  1716	                        }));
  1717	
```

### Non-clinical filtering and SOAP generation

```rust
  1718	                        // Two-pass clinical content check: flag non-clinical encounters
  1719	                        let mut is_clinical = true;
  1720	                        if encounter_word_count < MIN_WORDS_FOR_CLINICAL_CHECK {
  1721	                            is_clinical = false;
  1722	                            info!(
  1723	                                "Encounter #{} too small for clinical analysis ({} words < {} threshold) — treating as non-clinical",
  1724	                                encounter_number, encounter_word_count, MIN_WORDS_FOR_CLINICAL_CHECK
  1725	                            );
  1726	                        } else if let Some(ref client) = llm_client {
  1727	                            let (cc_system, cc_user) = build_clinical_content_check_prompt(&encounter_text);
  1728	                            let cc_start = Instant::now();
  1729	                            let cc_future = client.generate(&fast_model, &cc_system, &cc_user, "clinical_content_check");
  1730	                            match tokio::time::timeout(tokio::time::Duration::from_secs(30), cc_future).await {
  1731	                                Ok(Ok(cc_response)) => {
  1732	                                    let cc_latency = cc_start.elapsed().as_millis() as u64;
  1733	                                    match parse_clinical_content_check(&cc_response) {
  1734	                                        Ok(cc_result) => {
  1735	                                            if let Ok(mut logger) = logger_for_detector.lock() {
  1736	                                                logger.log_clinical_check(
  1737	                                                    &fast_model, &cc_system, &cc_user,
  1738	                                                    Some(&cc_response), cc_latency, true, None,
  1739	                                                    serde_json::json!({
  1740	                                                        "encounter_number": encounter_number,
  1741	                                                        "word_count": encounter_word_count,
  1742	                                                        "is_clinical": cc_result.clinical,
  1743	                                                        "reason": cc_result.reason,
  1744	                                                    }),
  1745	                                                );
  1746	                                            }
  1747	                                            if !cc_result.clinical {
  1748	                                                is_clinical = false;
  1749	                                                info!(
  1750	                                                    "Encounter #{} flagged as non-clinical: {:?}",
  1751	                                                    encounter_number, cc_result.reason
  1752	                                                );
  1753	                                            } else {
  1754	                                                info!(
  1755	                                                    "Encounter #{} confirmed clinical: {:?}",
  1756	                                                    encounter_number, cc_result.reason
  1757	                                                );
  1758	                                            }
  1759	                                        }
  1760	                                        Err(e) => {
  1761	                                            if let Ok(mut logger) = logger_for_detector.lock() {
  1762	                                                logger.log_clinical_check(
  1763	                                                    &fast_model, &cc_system, &cc_user,
  1764	                                                    Some(&cc_response), cc_latency, false, Some(&e),
  1765	                                                    serde_json::json!({"encounter_number": encounter_number, "parse_error": true}),
  1766	                                                );
  1767	                                            }
  1768	                                            warn!("Failed to parse clinical content check: {}", e);
  1769	                                        }
  1770	                                    }
  1771	                                }
  1772	                                Ok(Err(e)) => {
  1773	                                    let cc_latency = cc_start.elapsed().as_millis() as u64;
  1774	                                    if let Ok(mut logger) = logger_for_detector.lock() {
  1775	                                        logger.log_clinical_check(
  1776	                                            &fast_model, &cc_system, &cc_user,
  1777	                                            None, cc_latency, false, Some(&e.to_string()),
  1778	                                            serde_json::json!({"encounter_number": encounter_number, "llm_error": true}),
  1779	                                        );
  1780	                                    }
  1781	                                    warn!("Clinical content check LLM call failed: {}", e);
  1782	                                }
  1783	                                Err(_) => {
  1784	                                    let cc_latency = cc_start.elapsed().as_millis() as u64;
  1785	                                    if let Ok(mut logger) = logger_for_detector.lock() {
  1786	                                        logger.log_clinical_check(
  1787	                                            &fast_model, &cc_system, &cc_user,
  1788	                                            None, cc_latency, false, Some("timeout_30s"),
  1789	                                            serde_json::json!({"encounter_number": encounter_number, "timeout": true}),
  1790	                                        );
  1791	                                    }
  1792	                                    warn!("Clinical content check timed out (30s)");
  1793	                                }
  1794	                            }
  1795	                        }
  1796	
  1797	                        // Update metadata with non-clinical flag (single path for both word-count and LLM checks)
  1798	                        if !is_clinical {
  1799	                            if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
  1800	                                let nc_meta_path = session_dir.join("metadata.json");
  1801	                                if nc_meta_path.exists() {
  1802	                                    if let Ok(content) = std::fs::read_to_string(&nc_meta_path) {
  1803	                                        if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
  1804	                                            metadata.likely_non_clinical = Some(true);
  1805	                                            if let Ok(json) = serde_json::to_string_pretty(&metadata) {
  1806	                                                let _ = std::fs::write(&nc_meta_path, json);
  1807	                                            }
  1808	                                        }
  1809	                                    }
  1810	                                }
  1811	                            }
  1812	                        }
  1813	
  1814	                        // Generate SOAP note (with 120s timeout — SOAP is heavier than detection)
  1815	                        // Skip SOAP for non-clinical encounters to prevent hallucinated clinical content
  1816	                        if !is_clinical {
  1817	                            info!("Skipping SOAP for non-clinical encounter #{}", encounter_number);
  1818	                        } else if let Some(ref client) = llm_client {
  1819	                            // Strip hallucinated repetitions before SOAP generation
  1820	                            let (filtered_encounter_text, soap_filter_report) = strip_hallucinations(&encounter_text, 5);
  1821	                            if !soap_filter_report.repetitions.is_empty() || !soap_filter_report.phrase_repetitions.is_empty() {
  1822	                                if let Ok(mut logger) = logger_for_detector.lock() {
  1823	                                    logger.log_hallucination_filter(serde_json::json!({
  1824	                                        "call_site": "soap_prep",
  1825	                                        "original_words": soap_filter_report.original_word_count,
  1826	                                        "cleaned_words": soap_filter_report.cleaned_word_count,
  1827	                                        "single_word_reps": soap_filter_report.repetitions.iter()
  1828	                                            .map(|r| &r.word).collect::<Vec<_>>(),
  1829	                                        "phrase_reps": soap_filter_report.phrase_repetitions.iter()
  1830	                                            .map(|r| &r.phrase).collect::<Vec<_>>(),
  1831	                                    }));
  1832	                                }
  1833	                            }
  1834	                            // Build SOAP options with encounter notes from clinician (uses pre-cloned notes_text)
  1835	                            let soap_opts = crate::llm_client::SoapOptions {
  1836	                                detail_level: soap_detail_level,
  1837	                                format: crate::llm_client::SoapFormat::from_config_str(&soap_format),
  1838	                                session_notes: notes_text.clone(),
  1839	                                ..Default::default()
  1840	                            };
  1841	                            info!("Generating SOAP for encounter #{}", encounter_number);
  1842	                            let soap_system_prompt = crate::llm_client::build_simple_soap_prompt(&soap_opts);
  1843	                            let soap_start = Instant::now();
  1844	                            let soap_future = client.generate_multi_patient_soap_note(
  1845	                                &soap_model,
  1846	                                &filtered_encounter_text,
  1847	                                None, // No audio events in continuous mode
  1848	                                Some(&soap_opts),
  1849	                                None, // No speaker context
  1850	                            );
  1851	                            match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
  1852	                                Ok(Ok(soap_result)) => {
  1853	                                    let soap_latency = soap_start.elapsed().as_millis() as u64;
  1854	                                    // Save SOAP to archive
  1855	                                    let soap_content = &soap_result.notes
  1856	                                        .iter()
  1857	                                        .map(|n| n.content.clone())
  1858	                                        .collect::<Vec<_>>()
  1859	                                        .join("\n\n---\n\n");
  1860	
  1861	                                    let now = Utc::now();
  1862	                                    if let Err(e) = local_archive::add_soap_note(
  1863	                                        &session_id,
  1864	                                        &now,
  1865	                                        soap_content,
  1866	                                        Some(soap_detail_level),
  1867	                                        Some(&soap_format),
  1868	                                    ) {
  1869	                                        warn!("Failed to save SOAP for encounter: {}", e);
  1870	                                    }
  1871	
  1872	                                    if let Ok(mut logger) = logger_for_detector.lock() {
  1873	                                        logger.log_soap(
  1874	                                            &soap_model, &soap_system_prompt, "",
  1875	                                            Some(soap_content), soap_latency, true, None,
  1876	                                            serde_json::json!({
  1877	                                                "encounter_number": encounter_number,
  1878	                                                "word_count": encounter_word_count,
  1879	                                                "detail_level": soap_detail_level,
  1880	                                                "format": soap_format,
  1881	                                                "has_notes": !notes_text.is_empty(),
  1882	                                                "response_chars": soap_content.len(),
  1883	                                            }),
  1884	                                        );
  1885	                                    }
  1886	
  1887	                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1888	                                        "type": "soap_generated",
  1889	                                        "session_id": session_id
  1890	                                    }));
  1891	                                    info!("SOAP generated for encounter #{}", encounter_number);
  1892	
  1893	                                }
  1894	                                Ok(Err(e)) => {
  1895	                                    let soap_latency = soap_start.elapsed().as_millis() as u64;
  1896	                                    warn!("Failed to generate SOAP for encounter: {}", e);
  1897	                                    if let Ok(mut logger) = logger_for_detector.lock() {
  1898	                                        logger.log_soap(
  1899	                                            &soap_model, &soap_system_prompt, "", None, soap_latency, false, Some(&e.to_string()),
  1900	                                            serde_json::json!({"encounter_number": encounter_number, "llm_error": true}),
  1901	                                        );
  1902	                                    }
  1903	                                    if let Ok(mut err) = last_error_for_detector.lock() {
  1904	                                        *err = Some(format!("SOAP generation failed: {}", e));
  1905	                                    } else {
  1906	                                        warn!("Last error lock poisoned, error state not updated");
  1907	                                    }
  1908	                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1909	                                        "type": "soap_failed",
  1910	                                        "session_id": session_id,
  1911	                                        "error": e
  1912	                                    }));
  1913	                                }
  1914	                                Err(_elapsed) => {
  1915	                                    let soap_latency = soap_start.elapsed().as_millis() as u64;
  1916	                                    warn!("SOAP generation timed out after 120s for encounter #{}", encounter_number);
  1917	                                    if let Ok(mut logger) = logger_for_detector.lock() {
  1918	                                        logger.log_soap(
  1919	                                            &soap_model, &soap_system_prompt, "", None, soap_latency, false, Some("timeout_120s"),
  1920	                                            serde_json::json!({"encounter_number": encounter_number, "timeout": true}),
  1921	                                        );
  1922	                                    }
  1923	                                    if let Ok(mut err) = last_error_for_detector.lock() {
  1924	                                        *err = Some("SOAP generation timed out".to_string());
  1925	                                    } else {
  1926	                                        warn!("Last error lock poisoned, error state not updated");
  1927	                                    }
  1928	                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
  1929	                                        "type": "soap_failed",
  1930	                                        "session_id": session_id,
  1931	                                        "error": "SOAP generation timed out"
  1932	                                    }));
  1933	                                }
  1934	                            }
```

### Merge-back logic excerpt

```rust
  1937	                        // ---- Retrospective merge check ----
  1938	                        // After archiving + SOAP for encounter N, check if it should merge with N-1.
  1939	                        // Gap fix: when prev_encounter_session_id is None (first encounter in this
  1940	                        // continuous session), load the most recent same-day session from the archive
  1941	                        // so the very first split still gets merge-checked.
  1942	                        if merge_enabled && prev_encounter_session_id.is_none() {
  1943	                            let today_str = Utc::now().format("%Y-%m-%d").to_string();
  1944	                            if let Ok(sessions) = local_archive::list_sessions_by_date(&today_str) {
  1945	                                // Find the most recent session that isn't the one we just archived
  1946	                                if let Some(prev_summary) = sessions.iter().find(|s| s.session_id != session_id) {
  1947	                                    if let Ok(details) = local_archive::get_session(&prev_summary.session_id, &today_str) {
  1948	                                        if let Some(transcript) = details.transcript {
  1949	                                            info!(
  1950	                                                "Loaded previous same-day session {} from archive for merge check (first encounter fallback)",
  1951	                                                prev_summary.session_id
  1952	                                            );
  1953	                                            prev_encounter_session_id = Some(prev_summary.session_id.clone());
  1954	                                            prev_encounter_text = Some(transcript);
  1955	                                            prev_encounter_date = Some(Utc::now());
  1956	                                            prev_encounter_is_clinical = prev_summary.likely_non_clinical != Some(true);
  1957	                                        }
  1958	                                    }
  1959	                                }
  1960	                            }
  1961	                        }
  1962	
  1963	                        if merge_enabled {
  1964	                            if let (Some(ref prev_id), Some(ref prev_text), Some(ref prev_date)) =
  1965	                                (&prev_encounter_session_id, &prev_encounter_text, &prev_encounter_date)
  1966	                            {
  1967	                                let prev_tail = tail_words(prev_text, MERGE_EXCERPT_WORDS);
  1968	                                let curr_head = head_words(&encounter_text, MERGE_EXCERPT_WORDS);
  1969	
  1970	                                if let Some(ref client) = llm_client {
  1971	                                    // Strip hallucinated repetitions from merge excerpts
  1972	                                    let (filtered_prev_tail, _) = strip_hallucinations(&prev_tail, 5);
  1973	                                    let (filtered_curr_head, _) = strip_hallucinations(&curr_head, 5);
  1974	                                    // Get patient name from vision tracker for merge context (M1 strategy)
  1975	                                    let merge_patient_name = name_tracker_for_detector
  1976	                                        .lock()
  1977	                                        .ok()
  1978	                                        .and_then(|t| t.majority_name());
  1979	                                    let (merge_system, merge_user) = build_encounter_merge_prompt(
  1980	                                        &filtered_prev_tail,
  1981	                                        &filtered_curr_head,
  1982	                                        merge_patient_name.as_deref(),
  1983	                                    );
  1984	                                    let merge_ctx = serde_json::json!({
  1985	                                        "prev_session_id": prev_id,
  1986	                                        "curr_session_id": session_id,
  1987	                                        "patient_name": merge_patient_name,
  1988	                                        "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
  1989	                                        "curr_head_words": filtered_curr_head.split_whitespace().count(),
  1990	                                    });
  1991	                                    let merge_start = Instant::now();
  1992	                                    let merge_future = client.generate(&fast_model, &merge_system, &merge_user, "encounter_merge");
  1993	                                    match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
  1994	                                        Ok(Ok(merge_response)) => {
  1995	                                            let merge_latency = merge_start.elapsed().as_millis() as u64;
  1996	                                            match parse_merge_check(&merge_response) {
  1997	                                                Ok(merge_result) => {
  1998	                                                    if let Ok(mut logger) = logger_for_detector.lock() {
  1999	                                                        logger.log_merge_check(
  2000	                                                            &fast_model, &merge_system, &merge_user,
  2001	                                                            Some(&merge_response), merge_latency, true, None,
  2002	                                                            serde_json::json!({
  2003	                                                                "prev_session_id": prev_id,
  2004	                                                                "curr_session_id": session_id,
  2005	                                                                "patient_name": merge_patient_name,
  2006	                                                                "same_encounter": merge_result.same_encounter,
  2007	                                                                "reason": format!("{:?}", merge_result.reason),
  2008	                                                            }),
  2009	                                                        );
  2010	                                                    }
  2011	                                                    if merge_result.same_encounter {
  2012	                                                        info!(
  2013	                                                            "Merge check: encounters are the same visit (reason: {:?}). Merging {} into {}",
  2014	                                                            merge_result.reason, session_id, prev_id
  2015	                                                        );
  2016	
  2017	                                                        // Build merged transcript
  2018	                                                        let merged_text = format!("{}\n{}", prev_text, encounter_text);
  2019	                                                        let merged_wc = merged_text.split_whitespace().count();
  2020	                                                        let merged_duration = encounter_start
  2021	                                                            .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
  2022	                                                            .unwrap_or(0);
  2023	
  2024	                                                        // Get patient name from current vision tracker for merged encounter
  2025	                                                        let merge_vision_name = name_tracker_for_detector
  2026	                                                            .lock()
  2027	                                                            .ok()
  2028	                                                            .and_then(|t| t.majority_name());
  2029	                                                        if let Err(e) = local_archive::merge_encounters(
  2030	                                                            prev_id,
  2031	                                                            &session_id,
  2032	                                                            prev_date,
  2033	                                                            &merged_text,
  2034	                                                            merged_wc,
  2035	                                                            merged_duration,
  2036	                                                            merge_vision_name.as_deref(),
  2037	                                                        ) {
  2038	                                                            warn!("Failed to merge encounters: {}", e);
  2039	                                                        } else {
  2040	                                                            // Regenerate SOAP for the merged encounter (only if at least one is clinical)
  2041	                                                            if !(is_clinical || prev_encounter_is_clinical) {
  2042	                                                                info!("Skipping SOAP regeneration for merged non-clinical encounters");
  2043	                                                            } else if let Some(ref client) = llm_client {
  2044	                                                                let (filtered_merged, _) = strip_hallucinations(&merged_text, 5);
  2045	                                                                if let Ok(mut logger) = logger_for_detector.lock() {
  2046	                                                                    logger.log_hallucination_filter(serde_json::json!({
  2047	                                                                        "stage": "merge_soap_prep",
  2048	                                                                        "original_words": merged_text.split_whitespace().count(),
  2049	                                                                        "filtered_words": filtered_merged.split_whitespace().count(),
  2050	                                                                    }));
  2051	                                                                }
  2052	                                                                let merge_notes = encounter_notes_for_detector
  2053	                                                                    .lock()
  2054	                                                                    .map(|n| n.clone())
  2055	                                                                    .unwrap_or_default();
```

### Screenshot-based patient name extraction

```rust
  2422	    // Spawn screenshot-based patient name extraction task (if screen capture enabled)
  2423	    let screenshot_task = if config.screen_capture_enabled {
  2424	        let stop_for_screenshot = handle.stop_flag.clone();
  2425	        let name_tracker_for_screenshot = handle.name_tracker.clone();
  2426	        let last_split_time_for_screenshot = handle.last_split_time.clone();
  2427	        let vision_trigger_for_screenshot = handle.vision_name_change_trigger.clone();
  2428	        let vision_new_name_for_screenshot = handle.vision_new_name.clone();
  2429	        let vision_old_name_for_screenshot = handle.vision_old_name.clone();
  2430	        let debug_storage_for_screenshot = config.debug_storage_enabled;
  2431	        let screenshot_interval = config.screen_capture_interval_secs.max(30) as u64; // Clamp minimum 30s
  2432	        let llm_client_for_screenshot = if !config.llm_router_url.is_empty() {
  2433	            LLMClient::new(
  2434	                &config.llm_router_url,
  2435	                &config.llm_api_key,
  2436	                &config.llm_client_id,
  2437	                &config.fast_model,
  2438	            )
  2439	            .ok()
  2440	        } else {
  2441	            None
  2442	        };
  2443	
  2444	        Some(tokio::spawn(async move {
  2445	            info!(
  2446	                "Screenshot name extraction task started (interval: {}s)",
  2447	                screenshot_interval
  2448	            );
  2449	
  2450	            loop {
  2451	                tokio::time::sleep(tokio::time::Duration::from_secs(screenshot_interval)).await;
  2452	
  2453	                if stop_for_screenshot.load(Ordering::Relaxed) {
  2454	                    break;
  2455	                }
  2456	
  2457	                // Capture screen to base64 (runs on blocking thread since it uses CoreGraphics)
  2458	                let capture_result = tokio::task::spawn_blocking(|| {
  2459	                    crate::screenshot::capture_to_base64(1150)
  2460	                })
  2461	                .await;
  2462	
  2463	                let capture = match capture_result {
  2464	                    Ok(Ok(c)) => c,
  2465	                    Ok(Err(e)) => {
  2466	                        debug!("Screenshot capture failed (may not have permission): {}", e);
  2467	                        continue;
  2468	                    }
  2469	                    Err(e) => {
  2470	                        debug!("Screenshot capture task panicked: {}", e);
  2471	                        continue;
  2472	                    }
  2473	                };
  2474	
  2475	                // Skip vision call if the capture is blank (no screen recording permission)
  2476	                if capture.likely_blank {
  2477	                    warn!("Screenshot appears blank — screen recording permission likely not granted. Skipping vision analysis. Grant permission in System Settings → Privacy & Security → Screen Recording.");
  2478	                    continue;
  2479	                }
  2480	
  2481	                let image_base64 = capture.base64;
  2482	
  2483	                // Save screenshot to disk for debugging (only when debug storage is enabled)
  2484	                if debug_storage_for_screenshot {
  2485	                    use base64::Engine;
  2486	                    if let Ok(config_dir) = Config::config_dir() {
  2487	                        let debug_dir = config_dir
  2488	                            .join("debug")
  2489	                            .join("continuous-screenshots");
  2490	                        let _ = std::fs::create_dir_all(&debug_dir);
  2491	                        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
  2492	                        let filename = debug_dir.join(format!("{}.jpg", timestamp));
  2493	                        match base64::engine::general_purpose::STANDARD.decode(&image_base64) {
  2494	                            Ok(bytes) => {
  2495	                                if let Err(e) = std::fs::write(&filename, &bytes) {
  2496	                                    warn!("Failed to save debug screenshot: {}", e);
  2497	                                } else {
  2498	                                    debug!("Debug screenshot saved: {:?}", filename);
  2499	                                }
  2500	                            }
  2501	                            Err(e) => {
  2502	                                warn!("Failed to decode screenshot base64 for debug save: {}", e);
  2503	                            }
  2504	                        }
  2505	                    }
  2506	                }
  2507	
  2508	                // Send to vision model for name extraction
  2509	                let client = match &llm_client_for_screenshot {
  2510	                    Some(c) => c,
  2511	                    None => {
  2512	                        debug!("No LLM client for screenshot name extraction");
  2513	                        continue;
  2514	                    }
  2515	                };
  2516	
  2517	                let (system_prompt, user_text) = build_patient_name_prompt();
  2518	                let system_prompt_log = system_prompt.clone();
  2519	                let user_text_log = user_text.clone();
  2520	                let content_parts = vec![
  2521	                    crate::llm_client::ContentPart::Text { text: user_text },
  2522	                    crate::llm_client::ContentPart::ImageUrl {
  2523	                        image_url: crate::llm_client::ImageUrlContent {
  2524	                            url: format!("data:image/jpeg;base64,{}", image_base64),
  2525	                        },
  2526	                    },
  2527	                ];
  2528	
  2529	                let vision_start = Instant::now();
  2530	                let vision_future = client.generate_vision(
  2531	                    "vision-model",
  2532	                    &system_prompt,
  2533	                    content_parts,
  2534	                    "patient_name_extraction",
  2535	                    Some(0.1), // Low temperature for factual extraction
  2536	                    Some(50),  // Short max tokens — just a name
  2537	                    None,
  2538	                    None,
  2539	                );
  2540	
  2541	                match tokio::time::timeout(
  2542	                    tokio::time::Duration::from_secs(30),
  2543	                    vision_future,
  2544	                )
  2545	                .await
  2546	                {
  2547	                    Ok(Ok(response)) => {
  2548	                        let vision_latency = vision_start.elapsed().as_millis() as u64;
  2549	                        let parsed_name = parse_patient_name(&response);
  2550	                        if let Some(ref name) = parsed_name {
  2551	                            info!("Vision extracted patient name: {}", name);
  2552	
  2553	                            // Stale screenshot detection: suppress votes that match previous
  2554	                            // encounter's patient name within grace period after split
  2555	                            let is_stale = if let Ok(split_time) = last_split_time_for_screenshot.lock() {
  2556	                                let secs_since_split = (Utc::now() - *split_time).num_seconds();
  2557	                                if secs_since_split < SCREENSHOT_STALE_GRACE_SECS {
  2558	                                    if let Ok(tracker) = name_tracker_for_screenshot.lock() {
  2559	                                        tracker.previous_name() == Some(name.as_str())
  2560	                                    } else {
  2561	                                        false
  2562	                                    }
  2563	                                } else {
  2564	                                    false
  2565	                                }
  2566	                            } else {
  2567	                                false
  2568	                            };
  2569	
  2570	                            if let Ok(mut logger) = logger_for_screenshot.lock() {
  2571	                                logger.log_vision(
  2572	                                    "vision-model", &system_prompt_log, &user_text_log,
  2573	                                    Some(&response), vision_latency, true, None,
  2574	                                    serde_json::json!({
  2575	                                        "parsed_name": name,
  2576	                                        "screenshot_blank": false,
  2577	                                        "is_stale": is_stale,
  2578	                                    }),
  2579	                                );
  2580	                            }
  2581	
  2582	                            if is_stale {
  2583	                                info!(
  2584	                                    "Skipping stale screenshot vote '{}' — matches previous encounter name and within {}s grace period",
  2585	                                    name, SCREENSHOT_STALE_GRACE_SECS
  2586	                                );
  2587	                                continue;
  2588	                            }
  2589	
  2590	                            if let Ok(mut tracker) = name_tracker_for_screenshot.lock() {
  2591	                                let (changed, old_name, new_name) = tracker.record_and_check_change(name);
  2592	                                if changed {
  2593	                                    info!(
  2594	                                        "Vision detected patient name change: {:?} → {:?} — accelerating detection",
  2595	                                        old_name, new_name
  2596	                                    );
  2597	                                    // Store names for the detection loop to read
  2598	                                    if let Ok(mut n) = vision_new_name_for_screenshot.lock() {
  2599	                                        *n = new_name;
  2600	                                    }
  2601	                                    if let Ok(mut o) = vision_old_name_for_screenshot.lock() {
  2602	                                        *o = old_name;
  2603	                                    }
  2604	                                    // Wake the detection loop
  2605	                                    vision_trigger_for_screenshot.notify_one();
  2606	                                }
  2607	                            } else {
  2608	                                warn!("Name tracker lock poisoned, patient name vote dropped: {}", name);
  2609	                            }
  2610	                        } else {
  2611	                            if let Ok(mut logger) = logger_for_screenshot.lock() {
  2612	                                logger.log_vision(
  2613	                                    "vision-model", &system_prompt_log, &user_text_log,
  2614	                                    Some(&response), vision_latency, true, None,
  2615	                                    serde_json::json!({
  2616	                                        "parsed_name": serde_json::Value::Null,
  2617	                                        "screenshot_blank": false,
  2618	                                        "not_found": true,
  2619	                                    }),
  2620	                                );
  2621	                            }
  2622	                            debug!("Vision did not find a patient name on screen");
  2623	                        }
  2624	                    }
  2625	                    Ok(Err(e)) => {
  2626	                        let vision_latency = vision_start.elapsed().as_millis() as u64;
  2627	                        if let Ok(mut logger) = logger_for_screenshot.lock() {
  2628	                            logger.log_vision(
  2629	                                "vision-model", &system_prompt_log, &user_text_log,
  2630	                                None, vision_latency, false, Some(&e.to_string()),
  2631	                                serde_json::json!({"llm_error": true}),
  2632	                            );
  2633	                        }
  2634	                        debug!("Vision name extraction failed: {}", e);
  2635	                    }
  2636	                    Err(_) => {
  2637	                        let vision_latency = vision_start.elapsed().as_millis() as u64;
  2638	                        if let Ok(mut logger) = logger_for_screenshot.lock() {
  2639	                            logger.log_vision(
  2640	                                "vision-model", &system_prompt_log, &user_text_log,
  2641	                                None, vision_latency, false, Some("timeout_30s"),
  2642	                                serde_json::json!({"timeout": true}),
  2643	                            );
  2644	                        }
  2645	                        debug!("Vision name extraction timed out after 30s");
```

### Stop path, detector abort, orphaned SOAP recovery, and flush-on-stop

```rust
  2668	    }
  2669	
  2670	    // Cleanup: stop pipeline
  2671	    info!("Stopping continuous mode pipeline");
  2672	    pipeline_handle.stop();
  2673	
  2674	    // Join pipeline handle in a blocking task to avoid Drop blocking the Tokio thread
  2675	    tokio::task::spawn_blocking(move || {
  2676	        pipeline_handle.join();
  2677	    }).await.ok();
  2678	
  2679	    // Wait for tasks to finish
  2680	    let _ = consumer_task.await;
  2681	    detector_task.abort(); // Force stop the detector loop
  2682	    let _ = detector_task.await;
  2683	    if let Some(task) = screenshot_task {
  2684	        task.abort();
  2685	        let _ = task.await;
  2686	    }
  2687	    if let Some(task) = shadow_task {
  2688	        task.abort();
  2689	        let _ = task.await;
  2690	    }
  2691	    if let Some(task) = sensor_monitor_task {
  2692	        task.abort();
  2693	        let _ = task.await;
  2694	    }
  2695	
  2696	    // ---- Orphaned SOAP recovery ----
  2697	    // When detector_task.abort() fires, any in-flight SOAP generation for an already-archived
  2698	    // encounter is killed. Scan today's sessions for has_soap_note == false and regenerate.
  2699	    if let Some(ref client) = flush_llm_client {
  2700	        let today_str = Utc::now().format("%Y-%m-%d").to_string();
  2701	        if let Ok(sessions) = local_archive::list_sessions_by_date(&today_str) {
  2702	            let orphaned: Vec<_> = sessions.iter()
  2703	                .filter(|s| !s.has_soap_note && s.word_count > 100)
  2704	                .filter(|s| s.likely_non_clinical != Some(true))
  2705	                .collect();
  2706	            if !orphaned.is_empty() {
  2707	                info!("Found {} orphaned sessions without SOAP notes, recovering", orphaned.len());
  2708	            }
  2709	            for summary in orphaned {
  2710	                if let Ok(details) = local_archive::get_session(&summary.session_id, &today_str) {
  2711	                    if let Some(ref transcript) = details.transcript {
  2712	                        let (filtered_text, _) = strip_hallucinations(transcript, 5);
  2713	                        let word_count = filtered_text.split_whitespace().count();
  2714	                        if word_count < 100 {
  2715	                            info!("Orphaned session {} has only {} words after filtering, skipping SOAP", summary.session_id, word_count);
  2716	                            continue;
  2717	                        }
  2718	                        let orphan_soap_opts = crate::llm_client::SoapOptions {
  2719	                            detail_level: flush_soap_detail_level,
  2720	                            format: crate::llm_client::SoapFormat::from_config_str(&flush_soap_format),
  2721	                            ..Default::default()
  2722	                        };
  2723	                        info!("Generating SOAP for orphaned session {} ({} words)", summary.session_id, word_count);
  2724	                        let soap_start = std::time::Instant::now();
  2725	                        let soap_future = client.generate_multi_patient_soap_note(
  2726	                            &flush_soap_model,
  2727	                            &filtered_text,
  2728	                            None,
  2729	                            Some(&orphan_soap_opts),
  2730	                            None,
  2731	                        );
  2732	                        match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
  2733	                            Ok(Ok(soap_result)) => {
  2734	                                let soap_latency = soap_start.elapsed().as_millis() as u64;
  2735	                                let soap_content = &soap_result.notes
  2736	                                    .iter()
  2737	                                    .map(|n| n.content.clone())
  2738	                                    .collect::<Vec<_>>()
  2739	                                    .join("\n\n---\n\n");
  2740	                                if let Ok(mut logger) = logger_for_flush.lock() {
  2741	                                    logger.log_soap(
  2742	                                        &flush_soap_model, "", "",
  2743	                                        Some(soap_content), soap_latency, true, None,
  2744	                                        serde_json::json!({
  2745	                                            "stage": "orphaned_soap_recovery",
  2746	                                            "session_id": summary.session_id,
  2747	                                            "word_count": word_count,
  2748	                                            "response_chars": soap_content.len(),
  2749	                                        }),
  2750	                                    );
  2751	                                }
  2752	                                // Use session's original date, not Utc::now() — if SOAP generation
  2753	                                // crosses midnight, Utc::now() would save to the wrong date directory
  2754	                                let soap_date = chrono::DateTime::parse_from_rfc3339(&summary.date)
  2755	                                    .map(|dt| dt.with_timezone(&Utc))
  2756	                                    .unwrap_or_else(|_| Utc::now());
  2757	                                if let Err(e) = local_archive::add_soap_note(
  2758	                                    &summary.session_id,
  2759	                                    &soap_date,
  2760	                                    soap_content,
  2761	                                    Some(flush_soap_detail_level),
  2762	                                    Some(&flush_soap_format),
  2763	                                ) {
  2764	                                    warn!("Failed to save recovered SOAP for {}: {}", summary.session_id, e);
  2765	                                } else {
  2766	                                    info!("Recovered SOAP for orphaned session {}", summary.session_id);
  2767	                                    let _ = app.emit("continuous_mode_event", serde_json::json!({
  2768	                                        "type": "soap_generated",
  2769	                                        "session_id": summary.session_id,
  2770	                                        "recovered": true,
  2771	                                    }));
  2772	                                }
  2773	                            }
  2774	                            Ok(Err(e)) => {
  2775	                                let soap_latency = soap_start.elapsed().as_millis() as u64;
  2776	                                if let Ok(mut logger) = logger_for_flush.lock() {
  2777	                                    logger.log_soap(
  2778	                                        &flush_soap_model, "", "", None, soap_latency, false,
  2779	                                        Some(&e.to_string()),
  2780	                                        serde_json::json!({"stage": "orphaned_soap_recovery", "session_id": summary.session_id}),
  2781	                                    );
  2782	                                }
  2783	                                warn!("Failed to generate recovered SOAP for {}: {}", summary.session_id, e);
  2784	                            }
  2785	                            Err(_) => {
  2786	                                warn!("SOAP generation timed out for orphaned session {}", summary.session_id);
  2787	                            }
  2788	                        }
  2789	                    }
  2790	                }
  2791	            }
  2792	        }
  2793	    }
  2794	
  2795	    // Flush remaining buffer as final encounter check
  2796	    let remaining_text = {
  2797	        let buffer = handle.transcript_buffer.lock().unwrap_or_else(|e| e.into_inner());
  2798	        if !buffer.is_empty() {
  2799	            Some(buffer.full_text_with_speakers())
  2800	        } else {
  2801	            None
  2802	        }
  2803	    };
  2804	
  2805	    if let Some(text) = remaining_text {
  2806	        // Strip hallucinations before word count check and SOAP generation
  2807	        let (filtered_text, _) = strip_hallucinations(&text, 5);
  2808	        let word_count = filtered_text.split_whitespace().count();
  2809	        if let Ok(mut logger) = logger_for_flush.lock() {
  2810	            logger.log_hallucination_filter(serde_json::json!({
  2811	                "stage": "flush_on_stop",
  2812	                "original_words": text.split_whitespace().count(),
  2813	                "filtered_words": word_count,
  2814	            }));
  2815	        }
  2816	        if word_count > 100 {
  2817	            info!("Flushing remaining buffer ({} words after filtering) as final session", word_count);
  2818	            let session_id = uuid::Uuid::new_v4().to_string();
  2819	            if let Err(e) = local_archive::save_session(
  2820	                &session_id,
  2821	                &text, // Archive the raw text (preserve original for audit)
  2822	                0, // Unknown duration for flush
  2823	                None,
  2824	                false,
  2825	                Some("continuous_mode_stopped"),
  2826	                None, // No encounter start time for flush
  2827	            ) {
  2828	                warn!("Failed to archive final buffer: {}", e);
  2829	            } else {
  2830	                // Point logger to flush session's archive folder
  2831	                if let Ok(flush_session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
  2832	                    if let Ok(mut logger) = logger_for_flush.lock() {
  2833	                        logger.set_session(&flush_session_dir);
  2834	                    }
  2835	                }
  2836	
  2837	                // Generate SOAP note for the flushed buffer (the orphaned encounter fix)
  2838	                // NOTE: No clinical content check here — this is a rare shutdown-time flush
  2839	                // where no LLM call is available for classification. Generating SOAP unconditionally
  2840	                // is acceptable since the buffer likely contains real clinical audio.
  2841	                if let Some(ref client) = flush_llm_client {
  2842	                    let flush_notes = handle.encounter_notes
  2843	                        .lock()
  2844	                        .map(|n| n.clone())
  2845	                        .unwrap_or_default();
  2846	                    let flush_soap_opts = crate::llm_client::SoapOptions {
  2847	                        detail_level: flush_soap_detail_level,
  2848	                        format: crate::llm_client::SoapFormat::from_config_str(&flush_soap_format),
  2849	                        session_notes: flush_notes,
  2850	                        ..Default::default()
  2851	                    };
  2852	                    info!("Generating SOAP for flushed buffer ({} words)", word_count);
  2853	                    let flush_soap_system_prompt = crate::llm_client::build_simple_soap_prompt(&flush_soap_opts);
  2854	                    let flush_soap_start = Instant::now();
  2855	                    let soap_future = client.generate_multi_patient_soap_note(
  2856	                        &flush_soap_model,
  2857	                        &filtered_text,
  2858	                        None,
  2859	                        Some(&flush_soap_opts),
  2860	                        None,
  2861	                    );
  2862	                    match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
  2863	                        Ok(Ok(soap_result)) => {
  2864	                            let flush_soap_latency = flush_soap_start.elapsed().as_millis() as u64;
  2865	                            let soap_content = &soap_result.notes
```

## 2. Encounter Detection Inputs And Thresholds

Source: `tauri-app/src-tauri/src/encounter_detection.rs`

```rust
     1	//! Encounter detection logic for continuous mode.
     2	//!
     3	//! Provides the LLM prompt construction and response parsing for detecting
     4	//! transition points between patient encounters in a continuous transcript.
     5	
     6	use serde::{de::DeserializeOwned, Deserialize, Serialize};
     7	
     8	/// Word count forcing encounter check regardless of buffer age.
     9	pub const FORCE_CHECK_WORD_THRESHOLD: usize = 3000;
    10	/// Force-split when buffer exceeds this AND consecutive_no_split >= limit.
    11	pub const FORCE_SPLIT_WORD_THRESHOLD: usize = 5000;
    12	/// Consecutive non-split detection cycles before force-split (at FORCE_SPLIT_WORD_THRESHOLD).
    13	pub const FORCE_SPLIT_CONSECUTIVE_LIMIT: u32 = 3;
    14	/// Unconditional force-split -- hard safety valve, no counter needed.
    15	pub const ABSOLUTE_WORD_CAP: usize = 10_000;
    16	/// Minimum word count for clinical content check + SOAP generation.
    17	/// Encounters below this threshold are treated as non-clinical (still archived with transcript).
    18	pub const MIN_WORDS_FOR_CLINICAL_CHECK: usize = 100;
    19	/// Grace period (seconds) after encounter split during which screenshot votes matching the
    20	/// previous encounter's patient name are suppressed (stale screenshot detection).
    21	pub const SCREENSHOT_STALE_GRACE_SECS: i64 = 90;
    22	/// Minimum merged word count to trigger retrospective multi-patient check after merge-back.
    23	pub const MULTI_PATIENT_CHECK_WORD_THRESHOLD: usize = 2500;
    24	/// Minimum words per half for a retrospective split to be accepted (size gate).
    25	pub const MULTI_PATIENT_SPLIT_MIN_WORDS: usize = 500;
    26	
    27	/// Optional context signals for encounter detection.
    28	/// Provides real-time signals from sensor (departure/presence) to augment
    29	/// the LLM prompt. Vision-extracted patient names are used only for metadata
    30	/// labeling, NOT for split decisions (EMR chart name is unreliable — doctor
    31	/// may open family members, not open chart, or vision may parse same name
    32	/// differently).
    33	#[derive(Debug, Clone, Default)]
    34	pub struct EncounterDetectionContext {
    35	    /// Whether the presence sensor detected someone left the room
    36	    pub sensor_departed: bool,
    37	    /// Whether the presence sensor confirms someone is still in the room
    38	    pub sensor_present: bool,
    39	}
    40	
    41	/// Result of encounter detection
    42	#[derive(Debug, Clone, Serialize, Deserialize)]
    43	pub struct EncounterDetectionResult {
    44	    pub complete: bool,
    45	    #[serde(default)]
    46	    pub end_segment_index: Option<u64>,
    47	    /// Confidence score from the LLM (0.0-1.0). Used to gate low-confidence detections.
    48	    #[serde(default)]
    49	    pub confidence: Option<f64>,
    50	}
    51	
    52	/// Build the encounter detection prompt.
    53	/// Accepts optional context signals from vision and sensor to improve accuracy.
    54	pub fn build_encounter_detection_prompt(
    55	    formatted_segments: &str,
    56	    context: Option<&EncounterDetectionContext>,
    57	) -> (String, String) {
    58	    let system = r#"You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.
    59	
    60	You are analyzing a continuous transcript from a medical office where the microphone records all day.
    61	
    62	Your task: determine if there is a TRANSITION POINT where one patient encounter ends and another begins, or where a patient encounter has clearly concluded.
    63	
    64	Signs of a transition or completed encounter:
    65	- Farewell, wrap-up, or discharge instructions ("we'll see you in X weeks", "take care")
    66	- A greeting or introduction of a DIFFERENT patient after clinical discussion
    67	- A clear shift from one patient's clinical topics to another's
    68	- Extended non-clinical gap (scheduling, staff chat) after substantive clinical content
    69	- IN-ROOM PIVOT: the doctor transitions from one family member or companion to another without anyone leaving (e.g., "Okay, now let's talk about your husband's knee" or addressing a different person by name)
    70	- CHART SWITCH: the clinical discussion shifts to a different patient — different medications, conditions, or medical history than earlier in the transcript
    71	- The doctor begins taking a new history, asking "what brings you in today?" or similar intake questions after already having a substantive clinical discussion with someone else
    72	
    73	Examples of in-room transitions:
    74	- After discussing Mrs. Smith's diabetes, the doctor says "Now, Mr. Smith, how has your blood pressure been?" — this is a transition between two encounters
    75	- The doctor finishes discussing a child's ear infection with the mother, then asks the mother about her own back pain — this is a transition
    76	- The doctor says "Let me pull up your chart" after already having a full discussion about a different patient's condition — likely a transition
    77	
    78	This is NOT a transition:
    79	- Brief pauses, phone calls, or sidebar conversations DURING an ongoing patient visit
    80	- The very beginning of the first encounter (no prior encounter to split from)
    81	- Short exchanges or greetings with no substantive clinical content yet
    82	- Discussion of multiple body parts or conditions for the SAME patient (one visit can cover many topics)
    83	
    84	If you find a transition point or completed encounter, return:
    85	{"complete": true, "end_segment_index": <last segment index of the CONCLUDED encounter>, "confidence": <0.0-1.0>}
    86	
    87	If the current discussion is still one ongoing encounter with no transition, return:
    88	{"complete": false, "confidence": <0.0-1.0>}
    89	
    90	Respond with ONLY the JSON object."#;
    91	
    92	    // Build context section if signals are available
    93	    let context_section = if let Some(ctx) = context {
    94	        let mut parts = Vec::new();
    95	        // Sensor departure — soft signal, not a split trigger on its own
    96	        if ctx.sensor_departed {
    97	            parts.push(
    98	                "CONTEXT: The presence sensor detected possible movement away from the room. \
    99	                Note: brief departures during medical visits are common (hand washing, supplies, \
   100	                injection preparation, bathroom). Evaluate the TRANSCRIPT CONTENT to determine \
   101	                if the encounter has actually concluded — a sensor departure alone is not sufficient.".to_string()
   102	            );
   103	        }
   104	        // Sensor still present — use original production prompt (proven reliable)
   105	        if ctx.sensor_present && !ctx.sensor_departed {
   106	            parts.push(
   107	                "CONTEXT: The presence sensor confirms someone is still in the room. \
   108	                Topic changes or pauses within the same visit are NOT transitions. \
   109	                Only split if there is strong evidence of a different patient \
   110	                (new name, new history intake, greeting a new person).".to_string()
   111	            );
   112	        }
   113	        if parts.is_empty() {
   114	            String::new()
   115	        } else {
   116	            format!("\n\nReal-time context signals:\n{}", parts.join("\n"))
   117	        }
   118	    } else {
   119	        String::new()
   120	    };
```

## 3. Continuous Mode Defaults And Validation

Source: `tauri-app/src-tauri/src/config.rs`

### Continuous-mode settings fields and defaults

```rust
   150	    pub charting_mode: ChartingMode,
   151	    #[serde(default)]
   152	    pub continuous_auto_copy_soap: bool,
   153	    #[serde(default = "default_encounter_check_interval_secs")]
   154	    pub encounter_check_interval_secs: u32,
   155	    #[serde(default = "default_encounter_silence_trigger_secs")]
   156	    pub encounter_silence_trigger_secs: u32,
   157	    #[serde(default = "default_encounter_merge_enabled")]
   158	    pub encounter_merge_enabled: bool,
   159	    // Hybrid model: use a smaller/faster model for encounter detection
   160	    #[serde(default = "default_encounter_detection_model")]
   161	    pub encounter_detection_model: String,
   162	    #[serde(default = "default_encounter_detection_nothink")]
   163	    pub encounter_detection_nothink: bool,
   164	    // Presence sensor settings (mmWave encounter detection)
   165	    #[serde(default = "default_encounter_detection_mode")]
   166	    pub encounter_detection_mode: EncounterDetectionMode,
   167	    #[serde(default)]
   168	    pub presence_sensor_port: String,
   169	    #[serde(default = "default_presence_absence_threshold_secs")]
   170	    pub presence_absence_threshold_secs: u64,
   171	    #[serde(default = "default_presence_debounce_secs")]
   172	    pub presence_debounce_secs: u64,
   173	    #[serde(default = "default_presence_csv_log_enabled")]
   174	    pub presence_csv_log_enabled: bool,
   175	    // Shadow mode settings (dual detection comparison)
   176	    #[serde(default = "default_shadow_active_method")]
   177	    pub shadow_active_method: ShadowActiveMethod,
   178	    #[serde(default = "default_shadow_csv_log_enabled")]
   179	    pub shadow_csv_log_enabled: bool,
   180	    // Native STT shadow (Apple SFSpeechRecognizer comparison)
   181	    #[serde(default = "default_native_stt_shadow_enabled")]
   182	    pub native_stt_shadow_enabled: bool,
   183	    // Hybrid detection settings (sensor accelerates LLM confirmation)
   184	    #[serde(default = "default_hybrid_confirm_window_secs")]
   185	    pub hybrid_confirm_window_secs: u64,
   186	    #[serde(default = "default_hybrid_min_words_for_sensor_split")]
   187	    pub hybrid_min_words_for_sensor_split: usize,
   188	}
   189	
   190	fn default_native_stt_shadow_enabled() -> bool {
   191	    true // Enabled for continuous mode shadow transcript comparison
   192	}
   193	
   194	fn default_hybrid_confirm_window_secs() -> u64 {
   195	    180 // 3 min — allows recovery from brief sensor departures (hand wash, supplies, injections)
   196	}
   197	
   198	fn default_hybrid_min_words_for_sensor_split() -> usize {
   199	    500 // Minimum words for sensor timeout to force-split
   200	}
   201	
   202	fn default_shadow_active_method() -> ShadowActiveMethod {
   203	    ShadowActiveMethod::Sensor
   204	}
   205	
   206	fn default_shadow_csv_log_enabled() -> bool {
   207	    true
   208	}
   209	
   210	fn default_encounter_detection_mode() -> EncounterDetectionMode {
   211	    EncounterDetectionMode::Hybrid
   212	}
   213	
   214	fn default_presence_absence_threshold_secs() -> u64 {
   215	    180
   216	}
   217	
   218	fn default_presence_debounce_secs() -> u64 {
   219	    15 // 15s — prevents false splits from brief departures (patient shifting, doctor stepping to desk)
   220	}
   221	
   222	fn default_presence_csv_log_enabled() -> bool {
   223	    true
   224	}
   225	
   226	fn default_encounter_merge_enabled() -> bool {
   227	    true // Auto-merge split encounters by default
   228	}
   229	
   230	fn default_encounter_detection_model() -> String {
   231	    "fast-model".to_string() // ~7B model — 1.7B was insufficient (1.6% detection rate)
   232	}
   233	
   234	fn default_encounter_detection_nothink() -> bool {
   235	    false // Allow thinking for better reasoning with fast-model (~7B)
   236	}
   237	
   238	fn default_stt_alias() -> String {
   239	    "medical-streaming".to_string()
   240	}
   241	
   242	fn default_stt_postprocess() -> bool {
   243	    true
   244	}
   245	
   246	fn default_charting_mode() -> ChartingMode {
   247	    ChartingMode::Session
   248	}
   249	
   250	fn default_encounter_check_interval_secs() -> u32 {
   251	    120 // 2 minutes
   252	}
   253	
   254	fn default_encounter_silence_trigger_secs() -> u32 {
   255	    45 // 45 seconds — catches natural patient transitions; LLM detector validates completeness
   256	}
   257	
   258	fn default_screen_capture_interval_secs() -> u32 {
   259	    30 // 30 seconds default
   260	}
```

### Default settings initialization

```rust
   360	            output_format: "paragraphs".to_string(),
   361	            vad_threshold: 0.5,
   362	            silence_to_flush_ms: 500,
   363	            max_utterance_ms: 25000,
   364	            diarization_enabled: false,
   365	            max_speakers: 10,
   366	            llm_router_url: default_llm_router_url(),
   367	            llm_api_key: default_llm_api_key(),
   368	            llm_client_id: default_llm_client_id(),
   369	            soap_model: default_soap_model(),
   370	            soap_model_fast: default_soap_model_fast(),
   371	            fast_model: default_fast_model(),
   372	            medplum_server_url: default_medplum_url(),
   373	            medplum_client_id: default_medplum_client_id(),
   374	            medplum_auto_sync: default_medplum_auto_sync(),
   375	            whisper_mode: default_whisper_mode(),
   376	            whisper_server_url: default_whisper_server_url(),
   377	            whisper_server_model: default_whisper_server_model(),
   378	            stt_alias: default_stt_alias(),
   379	            stt_postprocess: default_stt_postprocess(),
   380	            soap_detail_level: default_soap_detail_level(),
   381	            soap_format: default_soap_format(),
   382	            soap_custom_instructions: String::new(),
   383	            auto_start_enabled: false,
   384	            greeting_sensitivity: default_greeting_sensitivity(),
   385	            min_speech_duration_ms: default_min_speech_duration_ms(),
   386	            auto_start_require_enrolled: false,
   387	            auto_start_required_role: None,
   388	            auto_end_enabled: default_auto_end_enabled(),
   389	            auto_end_silence_ms: default_auto_end_silence_ms(),
   390	            debug_storage_enabled: default_debug_storage_enabled(),
   391	            miis_enabled: false,
   392	            miis_server_url: default_miis_server_url(),
   393	            image_source: default_image_source(),
   394	            gemini_api_key: String::new(),
   395	            screen_capture_enabled: false,
   396	            screen_capture_interval_secs: default_screen_capture_interval_secs(),
   397	            charting_mode: default_charting_mode(),
   398	            continuous_auto_copy_soap: false,
   399	            encounter_check_interval_secs: default_encounter_check_interval_secs(),
   400	            encounter_silence_trigger_secs: default_encounter_silence_trigger_secs(),
   401	            encounter_merge_enabled: default_encounter_merge_enabled(),
   402	            encounter_detection_model: default_encounter_detection_model(),
   403	            encounter_detection_nothink: default_encounter_detection_nothink(),
   404	            encounter_detection_mode: default_encounter_detection_mode(),
   405	            presence_sensor_port: String::new(),
   406	            presence_absence_threshold_secs: default_presence_absence_threshold_secs(),
   407	            presence_debounce_secs: default_presence_debounce_secs(),
   408	            presence_csv_log_enabled: default_presence_csv_log_enabled(),
   409	            shadow_active_method: default_shadow_active_method(),
   410	            shadow_csv_log_enabled: default_shadow_csv_log_enabled(),
   411	            native_stt_shadow_enabled: default_native_stt_shadow_enabled(),
   412	            hybrid_confirm_window_secs: default_hybrid_confirm_window_secs(),
   413	            hybrid_min_words_for_sensor_split: default_hybrid_min_words_for_sensor_split(),
```

### Validation constraints

```rust
   552	        // Encounter check interval must be at least 30 seconds
   553	        if self.encounter_check_interval_secs > 0 && self.encounter_check_interval_secs < 30 {
   554	            errors.push(SettingsValidationError {
   555	                field: "encounter_check_interval_secs".to_string(),
   556	                message: format!(
   557	                    "Encounter check interval {}s is too frequent. Must be at least 30 seconds",
   558	                    self.encounter_check_interval_secs
   559	                ),
   560	            });
   561	        }
   562	
   563	        // Encounter silence trigger must be at least 10 seconds
   564	        if self.encounter_silence_trigger_secs > 0 && self.encounter_silence_trigger_secs < 10 {
   565	            errors.push(SettingsValidationError {
   566	                field: "encounter_silence_trigger_secs".to_string(),
   567	                message: format!(
   568	                    "Encounter silence trigger {}s is too short. Must be at least 10 seconds",
   569	                    self.encounter_silence_trigger_secs
   570	                ),
   571	            });
   572	        }
   573	
   574	        // Sensor-only mode requires a sensor port (shadow mode falls back to LLM gracefully)
   575	        if self.encounter_detection_mode == EncounterDetectionMode::Sensor && self.presence_sensor_port.is_empty() {
   576	            errors.push(SettingsValidationError {
   577	                field: "presence_sensor_port".to_string(),
   578	                message: "Sensor mode requires a presence sensor port to be configured".to_string(),
   579	            });
   580	        }
```

## 4. Presence Sensor Implementation

Source: `tauri-app/src-tauri/src/presence_sensor.rs`

```rust
     1	//! Presence Sensor Module
     2	//!
     3	//! Interfaces with a DFRobot SEN0395 24GHz mmWave presence sensor via USB-UART.
     4	//! The sensor outputs `$JYBSS,0` (absent) / `$JYBSS,1` (present) at ~1Hz.
     5	//!
     6	//! Architecture:
     7	//!   Serial Port (blocking read via spawn_blocking)
     8	//!       → Debounce FSM (10s default)
     9	//!       → watch channel (PresenceState)
    10	//!       → Absence Monitor (async task, 90s default threshold)
    11	//!       → Notify (fires when absence exceeds threshold)
    12	//!
    13	//! Optional CSV logging mirrors the format from `scripts/mmwave_logger.py`.
    14	
    15	use chrono::Utc;
    16	use std::io::BufRead;
    17	use std::path::PathBuf;
    18	use std::sync::atomic::{AtomicBool, Ordering};
    19	use std::sync::Arc;
    20	use std::time::{Duration, Instant};
    21	use tokio::sync::{watch, Notify};
    22	use tokio::task::JoinHandle;
    23	use tracing::{debug, info, warn};
    24	
    25	// ============================================================================
    26	// Types
    27	// ============================================================================
    28	
    29	/// Debounced presence state from the sensor
    30	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
    31	pub enum PresenceState {
    32	    Present,
    33	    Absent,
    34	    Unknown,
    35	}
    36	
    37	impl PresenceState {
    38	    pub fn as_str(&self) -> &'static str {
    39	        match self {
    40	            PresenceState::Present => "present",
    41	            PresenceState::Absent => "absent",
    42	            PresenceState::Unknown => "unknown",
    43	        }
    44	    }
    45	}
    46	
    47	/// Sensor connection health
    48	#[derive(Debug, Clone, PartialEq, Eq)]
    49	pub enum SensorStatus {
    50	    Connected,
    51	    Disconnected,
    52	    Error(String),
    53	}
    54	
    55	impl SensorStatus {
    56	    pub fn is_connected(&self) -> bool {
    57	        matches!(self, SensorStatus::Connected)
    58	    }
    59	}
    60	
    61	/// Configuration for the presence sensor
    62	#[derive(Debug, Clone)]
    63	pub struct SensorConfig {
    64	    pub port: String,
    65	    pub debounce_secs: u64,
    66	    pub absence_threshold_secs: u64,
    67	    pub csv_log_enabled: bool,
    68	}
    69	
    70	// ============================================================================
    71	// Auto-Detection
    72	// ============================================================================
    73	
    74	/// Auto-detect the presence sensor serial port.
    75	///
    76	/// Scans available serial ports for USB-serial devices (matching common patterns
    77	/// like `usbserial`, `usbmodem`, `USB`). If `configured_port` is non-empty and
    78	/// exists among available ports, it is returned as-is. Otherwise, returns the
    79	/// first matching USB-serial port found, or None.
    80	pub fn auto_detect_port(configured_port: &str) -> Option<String> {
    81	    let ports = match serialport::available_ports() {
    82	        Ok(p) => p,
    83	        Err(e) => {
    84	            warn!("Failed to enumerate serial ports: {}", e);
    85	            return if configured_port.is_empty() {
    86	                None
    87	            } else {
    88	                Some(configured_port.to_string())
    89	            };
    90	        }
    91	    };
    92	
    93	    let port_names: Vec<&str> = ports.iter().map(|p| p.port_name.as_str()).collect();
    94	    debug!("Available serial ports: {:?}", port_names);
    95	
    96	    // If configured port exists, use it
    97	    if !configured_port.is_empty() && ports.iter().any(|p| p.port_name == configured_port) {
    98	        return Some(configured_port.to_string());
    99	    }
   100	
   101	    // Auto-detect: look for USB serial ports (common patterns on macOS/Linux)
   102	    let usb_patterns = ["usbserial", "usbmodem", "USB"];
   103	    for port in &ports {
   104	        if usb_patterns.iter().any(|pat| port.port_name.contains(pat)) {
   105	            if !configured_port.is_empty() {
   106	                info!(
   107	                    "Configured sensor port '{}' not found. Auto-detected: {}",
   108	                    configured_port, port.port_name
   109	                );
   110	            } else {
   111	                info!("Auto-detected sensor port: {}", port.port_name);
   112	            }
   113	            return Some(port.port_name.clone());
   114	        }
   115	    }
   116	
   117	    if !configured_port.is_empty() {
   118	        warn!(
   119	            "Configured sensor port '{}' not found and no USB serial port detected",
   120	            configured_port
   121	        );
   122	    }
   123	    None
   124	}
   125	
   126	// ============================================================================
   127	// Sensor Handle
   128	// ============================================================================
   129	
   130	/// Handle to a running presence sensor. Call `stop()` to shut down.
   131	pub struct PresenceSensor {
   132	    state_tx: Arc<watch::Sender<PresenceState>>,
   133	    status_tx: Arc<watch::Sender<SensorStatus>>,
   134	    absence_trigger: Arc<Notify>,
   135	    stop: Arc<AtomicBool>,
   136	    reader_handle: Option<JoinHandle<()>>,
   137	    monitor_handle: Option<JoinHandle<()>>,
   138	}
   139	
   140	impl PresenceSensor {
   141	    /// Start the presence sensor with the given configuration.
   142	    ///
   143	    /// Returns a handle that provides:
   144	    /// - `subscribe_state()` — watch channel for debounced presence state
   145	    /// - `subscribe_status()` — watch channel for connection health
   146	    /// - `absence_notifier()` — fires when absence exceeds threshold
   147	    pub fn start(config: &SensorConfig) -> Result<Self, String> {
   148	        if config.port.is_empty() {
   149	            return Err("No serial port configured for presence sensor".to_string());
   150	        }
   151	
   152	        let (state_tx, _) = watch::channel(PresenceState::Unknown);
   153	        let state_tx = Arc::new(state_tx);
   154	
   155	        let (status_tx, _) = watch::channel(SensorStatus::Disconnected);
   156	        let status_tx = Arc::new(status_tx);
   157	
   158	        let absence_trigger = Arc::new(Notify::new());
   159	        let stop = Arc::new(AtomicBool::new(false));
   160	
   161	        // Start the serial reader task (blocking → tokio bridge)
   162	        let reader_handle = {
   163	            let state_tx = state_tx.clone();
   164	            let status_tx = status_tx.clone();
   165	            let stop = stop.clone();
   166	            let port_path = config.port.clone();
   167	            let debounce_secs = config.debounce_secs;
   168	            let csv_log_enabled = config.csv_log_enabled;
   169	
   170	            tokio::spawn(async move {
   171	                serial_reader_loop(
   172	                    &port_path,
   173	                    debounce_secs,
   174	                    csv_log_enabled,
   175	                    state_tx,
   176	                    status_tx,
   177	                    stop,
   178	                )
   179	                .await;
   180	            })
   181	        };
   182	
   183	        // Start the absence threshold monitor
   184	        let monitor_handle = {
   185	            let state_rx = state_tx.subscribe();
   186	            let absence_trigger = absence_trigger.clone();
   187	            let stop = stop.clone();
   188	            let threshold_secs = config.absence_threshold_secs;
   189	
   190	            tokio::spawn(async move {
   191	                absence_monitor(state_rx, absence_trigger, stop, threshold_secs).await;
   192	            })
   193	        };
   194	
   195	        info!(
   196	            "Presence sensor started: port={}, debounce={}s, absence_threshold={}s, csv={}",
   197	            config.port, config.debounce_secs, config.absence_threshold_secs, config.csv_log_enabled
   198	        );
   199	
   200	        Ok(Self {
   201	            state_tx,
   202	            status_tx,
   203	            absence_trigger,
   204	            stop,
   205	            reader_handle: Some(reader_handle),
   206	            monitor_handle: Some(monitor_handle),
   207	        })
   208	    }
   209	
   210	    /// Get a receiver for the debounced presence state
   211	    pub fn subscribe_state(&self) -> watch::Receiver<PresenceState> {
   212	        self.state_tx.subscribe()
   213	    }
   214	
   215	    /// Get a receiver for the sensor connection status
   216	    pub fn subscribe_status(&self) -> watch::Receiver<SensorStatus> {
   217	        self.status_tx.subscribe()
   218	    }
   219	
   220	    /// Get the absence trigger notifier (fires when absence exceeds threshold)
```

## 5. Frontend Continuous Mode Hook

Source: `tauri-app/src/hooks/useContinuousMode.ts`

```ts
     1	import { useState, useEffect, useCallback, useRef } from 'react';
     2	import { invoke } from '@tauri-apps/api/core';
     3	import { listen, UnlistenFn } from '@tauri-apps/api/event';
     4	import type { ContinuousModeStats, ContinuousModeEvent, TranscriptUpdate, AudioQualitySnapshot } from '../types';
     5	
     6	export interface UseContinuousModeResult {
     7	  /** Whether continuous mode is actively running */
     8	  isActive: boolean;
     9	  /** Whether a stop has been requested and we're waiting for cleanup (buffer flush + SOAP) */
    10	  isStopping: boolean;
    11	  /** Current stats from the backend */
    12	  stats: ContinuousModeStats;
    13	  /** Live transcript preview text (last ~500 chars) */
    14	  liveTranscript: string;
    15	  /** Audio quality snapshot from the pipeline */
    16	  audioQuality: AudioQualitySnapshot | null;
    17	  /** Per-encounter notes (passed to SOAP generation) */
    18	  encounterNotes: string;
    19	  /** Update encounter notes (debounced backend sync) */
    20	  setEncounterNotes: (notes: string) => void;
    21	  /** Start continuous mode */
    22	  start: () => Promise<void>;
    23	  /** Stop continuous mode */
    24	  stop: () => Promise<void>;
    25	  /** Manually trigger a new patient encounter split */
    26	  triggerNewPatient: () => Promise<void>;
    27	  /** Error message if any */
    28	  error: string | null;
    29	}
    30	
    31	const IDLE_STATS: ContinuousModeStats = {
    32	  state: 'idle',
    33	  recording_since: '',
    34	  encounters_detected: 0,
    35	  last_encounter_at: null,
    36	  last_encounter_words: null,
    37	  last_encounter_patient_name: null,
    38	  last_error: null,
    39	  buffer_word_count: 0,
    40	  buffer_started_at: null,
    41	};
    42	
    43	/**
    44	 * Hook for managing continuous charting mode.
    45	 *
    46	 * Listens to `continuous_mode_event` from the Rust backend and provides
    47	 * stats, live transcript preview, and start/stop controls.
    48	 */
    49	export function useContinuousMode(): UseContinuousModeResult {
    50	  const [isActive, setIsActive] = useState(false);
    51	  const [isStopping, setIsStopping] = useState(false);
    52	  const [stats, setStats] = useState<ContinuousModeStats>(IDLE_STATS);
    53	  const [liveTranscript, setLiveTranscript] = useState('');
    54	  const [audioQuality, setAudioQuality] = useState<AudioQualitySnapshot | null>(null);
    55	  const [encounterNotes, setEncounterNotesState] = useState('');
    56	  const [error, setError] = useState<string | null>(null);
    57	  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
    58	  const notesDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    59	
    60	  // Listen to continuous mode events from backend
    61	  useEffect(() => {
    62	    let unlisten: UnlistenFn | null = null;
    63	    let mounted = true;
    64	
    65	    listen<ContinuousModeEvent>('continuous_mode_event', (event) => {
    66	      if (!mounted) return;
    67	      const payload = event.payload;
    68	
    69	      switch (payload.type) {
    70	        case 'started':
    71	          setIsActive(true);
    72	          setError(null);
    73	          break;
    74	        case 'stopped':
    75	          setIsActive(false);
    76	          setIsStopping(false);
    77	          setStats(IDLE_STATS);
    78	          setLiveTranscript('');
    79	          setAudioQuality(null);
    80	          setEncounterNotesState('');
    81	          break;
    82	        case 'encounter_detected':
    83	          setEncounterNotesState('');
    84	          break;
    85	        case 'error':
    86	          setError(payload.error || 'Unknown error');
    87	          setIsActive(false);
    88	          setIsStopping(false);
    89	          break;
    90	      }
    91	    }).then((fn) => {
    92	      if (mounted) {
    93	        unlisten = fn;
    94	      } else {
    95	        fn(); // Component unmounted before listener resolved
    96	      }
    97	    });
    98	
    99	    return () => {
   100	      mounted = false;
   101	      if (unlisten) unlisten();
   102	    };
   103	  }, []);
   104	
   105	  // Listen to transcript updates for live preview
   106	  useEffect(() => {
   107	    if (!isActive) return;
   108	
   109	    let unlisten: UnlistenFn | null = null;
   110	    let mounted = true;
   111	
   112	    listen<TranscriptUpdate>('continuous_transcript_preview', (event) => {
   113	      if (mounted) setLiveTranscript(event.payload.finalized_text || '');
   114	    }).then((fn) => {
   115	      if (mounted) {
   116	        unlisten = fn;
   117	      } else {
   118	        fn();
   119	      }
   120	    });
   121	
   122	    return () => {
   123	      mounted = false;
   124	      if (unlisten) unlisten();
   125	    };
   126	  }, [isActive]);
   127	
   128	  // Poll for stats while active
   129	  useEffect(() => {
   130	    if (!isActive) {
   131	      if (pollRef.current) {
   132	        clearInterval(pollRef.current);
   133	        pollRef.current = null;
   134	      }
   135	      return;
   136	    }
   137	
   138	    const fetchStats = async () => {
   139	      try {
   140	        const result = await invoke<ContinuousModeStats>('get_continuous_mode_status');
   141	        setStats(result);
   142	      } catch (e) {
   143	        // Ignore poll errors
   144	      }
   145	    };
   146	
   147	    // Fetch immediately, then every 5 seconds
   148	    fetchStats();
   149	    pollRef.current = setInterval(fetchStats, 5000);
   150	
   151	    return () => {
   152	      if (pollRef.current) {
   153	        clearInterval(pollRef.current);
   154	        pollRef.current = null;
   155	      }
   156	    };
   157	  }, [isActive]);
   158	
   159	  // Listen to audio quality events from pipeline
   160	  useEffect(() => {
   161	    if (!isActive) return;
   162	
   163	    let unlisten: UnlistenFn | null = null;
   164	    let mounted = true;
   165	
   166	    listen<AudioQualitySnapshot>('audio_quality', (event) => {
   167	      if (mounted) setAudioQuality(event.payload);
   168	    }).then((fn) => {
   169	      if (mounted) {
   170	        unlisten = fn;
   171	      } else {
   172	        fn();
   173	      }
   174	    });
   175	
   176	    return () => {
   177	      mounted = false;
   178	      if (unlisten) unlisten();
   179	    };
   180	  }, [isActive]);
   181	
   182	  // Debounced encounter notes setter — syncs to backend after 500ms idle
   183	  const setEncounterNotes = useCallback((notes: string) => {
   184	    setEncounterNotesState(notes);
   185	
   186	    if (notesDebounceRef.current) {
   187	      clearTimeout(notesDebounceRef.current);
   188	    }
   189	    notesDebounceRef.current = setTimeout(() => {
   190	      invoke('set_continuous_encounter_notes', { notes }).catch((e) => {
   191	        console.error('Failed to sync encounter notes:', e);
   192	      });
   193	    }, 500);
   194	  }, []);
   195	
   196	  // Cleanup debounce timer on unmount
   197	  useEffect(() => {
   198	    return () => {
   199	      if (notesDebounceRef.current) {
   200	        clearTimeout(notesDebounceRef.current);
   201	      }
   202	    };
   203	  }, []);
   204	
   205	  const start = useCallback(async () => {
   206	    try {
   207	      setError(null);
   208	      await invoke('start_continuous_mode');
   209	      // isActive will be set to true when we receive the 'started' event
   210	    } catch (e) {
   211	      setError(String(e));
   212	    }
   213	  }, []);
   214	
   215	  const stop = useCallback(async () => {
   216	    try {
   217	      setIsStopping(true);
   218	      await invoke('stop_continuous_mode');
   219	      // isActive will be set to false when we receive the 'stopped' event
   220	    } catch (e) {
   221	      setError(String(e));
   222	      // Force reset if stop failed
   223	      setIsActive(false);
   224	      setIsStopping(false);
   225	    }
   226	  }, []);
   227	
   228	  const triggerNewPatient = useCallback(async () => {
   229	    try {
   230	      await invoke('trigger_new_patient');
```

## 6. Frontend Continuous Mode UI

Source: `tauri-app/src/components/modes/ContinuousMode.tsx`

```tsx
   231	  // Active — show monitoring dashboard
   232	  return (
   233	    <div className="mode-content continuous-mode">
   234	      {/* Status header with pulsing indicator */}
   235	      <div className="continuous-status-header">
   236	        <span className={`continuous-dot ${isStopping ? 'stopping' : stats.state === 'checking' ? 'checking' : 'listening'}`} />
   237	        <span className="continuous-status-text">
   238	          {isStopping
   239	            ? 'Ending... finalizing notes'
   240	            : stats.state === 'checking'
   241	              ? 'Checking for encounters...'
   242	              : 'Continuous mode active'}
   243	        </span>
   244	      </div>
   245	
   246	      {/* Sensor status indicator (only in sensor detection mode) */}
   247	      {stats.sensor_connected !== undefined && (
   248	        <div className="continuous-sensor-status" style={{ fontSize: 11, marginBottom: 4, display: 'flex', alignItems: 'center', gap: 4 }}>
   249	          <span style={{
   250	            display: 'inline-block',
   251	            width: 8,
   252	            height: 8,
   253	            borderRadius: '50%',
   254	            backgroundColor: !stats.sensor_connected
   255	              ? '#ef4444'
   256	              : stats.sensor_state === 'present'
   257	                ? '#22c55e'
   258	                : stats.sensor_state === 'absent'
   259	                  ? '#94a3b8'
   260	                  : '#a3a3a3',
   261	          }} />
   262	          <span style={{ opacity: 0.7 }}>
   263	            Sensor: {!stats.sensor_connected
   264	              ? 'Disconnected'
   265	              : stats.sensor_state === 'present'
   266	                ? 'Present'
   267	                : stats.sensor_state === 'absent'
   268	                  ? 'Absent'
   269	                  : 'Unknown'}
   270	          </span>
   271	        </div>
   272	      )}
   273	
   274	      {/* Shadow mode indicator (dual detection comparison) */}
   275	      {stats.shadow_mode_active && (
   276	        <div style={{
   277	          fontSize: 11,
   278	          marginBottom: 4,
   279	          display: 'flex',
   280	          alignItems: 'center',
   281	          gap: 4,
   282	          padding: '2px 6px',
   283	          borderRadius: 4,
   284	          backgroundColor: 'rgba(147, 51, 234, 0.1)',
   285	        }}>
   286	          <span style={{
   287	            display: 'inline-block',
   288	            width: 8,
   289	            height: 8,
   290	            borderRadius: '50%',
   291	            backgroundColor: stats.last_shadow_outcome === 'would_split' ? '#a855f7' : '#6b7280',
   292	          }} />
   293	          <span style={{ opacity: 0.7 }}>
   294	            Shadow ({stats.shadow_method?.toUpperCase()}): {
   295	              stats.last_shadow_outcome === 'would_split'
   296	                ? 'Would split'
   297	                : 'Observing...'
   298	            }
   299	          </span>
   300	        </div>
   301	      )}
   302	
   303	      {/* Session length timer */}
   304	      {elapsedTime && (
   305	        <div className="continuous-timer">
   306	          Session: {elapsedTime}
   307	        </div>
   308	      )}
   309	
   310	      {/* Audio quality indicator */}
   311	      <button
   312	        className={`quality-indicator ${qualityLevel}`}
   313	        onClick={handleDetailsClick}
   314	        aria-label="Audio quality - tap for details"
   315	      >
   316	        <span className="quality-dot" />
   317	        <span className="quality-label">
   318	          {qualityLevel === 'good' ? 'Good audio' : qualityLevel === 'fair' ? 'Fair audio' : 'Poor audio'}
   319	        </span>
   320	      </button>
   321	
   322	      {/* Audio quality details popover */}
   323	      {showDetails && audioQuality && (
   324	        <div className="recording-details-popover">
   325	          <div className="detail-row">
   326	            <span className="detail-label">Level</span>
   327	            <span className="detail-value">{audioQuality.rms_db.toFixed(0)} dB</span>
   328	          </div>
   329	          <div className="detail-row">
   330	            <span className="detail-label">SNR</span>
   331	            <span className="detail-value">{audioQuality.snr_db.toFixed(0)} dB</span>
   332	          </div>
   333	          {audioQuality.total_clipped > 0 && (
   334	            <div className="detail-row warning">
   335	              <span className="detail-label">Clips</span>
   336	              <span className="detail-value">{audioQuality.total_clipped}</span>
   337	            </div>
   338	          )}
   339	        </div>
   340	      )}
   341	
   342	      {/* New Patient button */}
   343	      <button
   344	        className="continuous-new-patient-btn"
   345	        onClick={handleNewPatient}
   346	        disabled={isStopping}
   347	      >
   348	        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
   349	          <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
   350	          <circle cx="9" cy="7" r="4" />
   351	          <line x1="19" y1="8" x2="19" y2="14" />
   352	          <line x1="22" y1="11" x2="16" y2="11" />
   353	        </svg>
   354	        New Patient
   355	      </button>
   356	
   357	      {/* Patient voice pulse indicator */}
   358	      <PatientPulse biomarkers={biomarkers} trends={biomarkerTrends} />
   359	
   360	      {/* Current encounter info */}
   361	      <div className="continuous-encounter-info">
   362	        {stats.buffer_started_at && stats.buffer_word_count > 0 ? (
   363	          <span>Current encounter: {encounterElapsed || '<1m'} &middot; {stats.buffer_word_count} words</span>
   364	        ) : (
   365	          <span className="continuous-encounter-waiting">Waiting for next patient...</span>
   366	        )}
   367	      </div>
   368	
   369	      {/* Predictive hint — "Pssst..." */}
   370	      {(predictiveHint || predictiveHintLoading) && (
   371	        <div className="predictive-hint-container">
   372	          <div className="predictive-hint-label">Pssst...</div>
   373	          <div className="predictive-hint-content">
   374	            {predictiveHintLoading ? (
   375	              <span className="predictive-hint-loading">Thinking...</span>
   376	            ) : (
   377	              <MarkdownContent content={predictiveHint} className="predictive-hint-markdown" />
   378	            )}
   379	          </div>
   380	        </div>
   381	      )}
   382	
   383	      {/* Image Suggestions (MIIS or AI) */}
   384	      {miisEnabled && (
   385	        <ImageSuggestions
   386	          suggestions={miisSuggestions}
   387	          isLoading={miisLoading}
   388	          error={miisError}
   389	          getImageUrl={miisGetImageUrl}
   390	          onImpression={onMiisImpression}
   391	          onClickImage={onMiisClick}
   392	          onDismiss={onMiisDismiss}
   393	          aiImages={aiImages}
   394	          aiLoading={aiLoading}
   395	          aiError={aiError}
   396	          onAiDismiss={onAiDismiss}
   397	          imageSource={imageSource}
   398	        />
   399	      )}
   400	
   401	      {/* Stats grid */}
   402	      <div className="continuous-stats">
   403	        <div className="continuous-stat">
   404	          <span className="continuous-stat-value">{stats.encounters_detected}</span>
   405	          <span className="continuous-stat-label">
   406	            encounter{stats.encounters_detected !== 1 ? 's' : ''} charted
   407	          </span>
   408	        </div>
   409	        <div className="continuous-stat">
   410	          <span className="continuous-stat-value">{stats.buffer_word_count}</span>
   411	          <span className="continuous-stat-label">words in buffer</span>
   412	        </div>
   413	      </div>
   414	
   415	      {/* Last encounter summary */}
   416	      {stats.last_encounter_at && (
   417	        <div className="continuous-last-encounter">
   418	          <span className="continuous-section-label">Last encounter</span>
   419	          <div className="continuous-last-encounter-info">
   420	            <span>{formatTime(stats.last_encounter_at)}</span>
   421	            {stats.last_encounter_words && (
   422	              <span> &middot; {stats.last_encounter_words} words</span>
   423	            )}
   424	            {stats.last_encounter_patient_name && (
   425	              <span> &mdash; {stats.last_encounter_patient_name}</span>
   426	            )}
   427	          </div>
   428	        </div>
   429	      )}
   430	
   431	      {/* Encounter Notes Toggle & Input */}
   432	      <button
   433	        className={`notes-toggle ${showNotes ? 'active' : ''} ${encounterNotes.trim() ? 'has-notes' : ''}`}
   434	        onClick={handleNotesToggle}
   435	        aria-label={showNotes ? 'Hide notes' : 'Add notes'}
   436	        aria-expanded={showNotes}
   437	      >
   438	        <span className="notes-icon">📝</span>
   439	        <span className="notes-label">{showNotes ? 'Hide Notes' : 'Add Notes'}</span>
   440	        <span className="notes-chevron">{showNotes ? '▲' : '▼'}</span>
   441	      </button>
   442	
   443	      {showNotes && (
   444	        <div className="session-notes-container">
   445	          <textarea
   446	            className="session-notes-input"
   447	            placeholder="Enter observations for this encounter..."
   448	            value={encounterNotes}
   449	            onChange={handleNotesChange}
   450	            rows={3}
   451	            aria-label="Encounter notes"
   452	          />
   453	        </div>
   454	      )}
   455	
   456	      {/* Transcript Toggle */}
   457	      <button
   458	        className={`transcript-toggle ${showTranscript ? 'active' : ''}`}
   459	        onClick={() => setShowTranscript(!showTranscript)}
   460	      >
```

## 7. Pipeline Logger

Source: `tauri-app/src-tauri/src/pipeline_log.rs`

```rust
     1	//! Pipeline replay logging for continuous mode.
     2	//!
     3	//! Writes one JSONL line per pipeline step (detection, clinical check, merge,
     4	//! SOAP, vision, hallucination filter) into each session's archive folder.
     5	//! Contains PHI — stored alongside existing PHI (transcript, SOAP) in the archive.
     6	
     7	use chrono::Utc;
     8	use serde::Serialize;
     9	use std::fs::OpenOptions;
    10	use std::io::Write;
    11	use std::path::{Path, PathBuf};
    12	use tracing::warn;
    13	
    14	const LOG_FILENAME: &str = "pipeline_log.jsonl";
    15	
    16	/// Appends structured JSONL events to a session's archive folder.
    17	/// Created per continuous-mode run; path updates when a new session_id is assigned.
    18	/// Buffers entries in memory when no path is set (before the session archive folder
    19	/// exists), then flushes them to disk when `set_session()` is called.
    20	pub struct PipelineLogger {
    21	    path: Option<PathBuf>,
    22	    /// Entries buffered while `path` is `None` (pre-split detection calls).
    23	    pending: Vec<String>,
    24	}
    25	
    26	/// A single pipeline log entry serialized as one JSONL line.
    27	#[derive(Debug, Serialize)]
    28	struct LogEntry {
    29	    ts: String,
    30	    step: String,
    31	    #[serde(skip_serializing_if = "Option::is_none")]
    32	    model: Option<String>,
    33	    #[serde(skip_serializing_if = "Option::is_none")]
    34	    prompt_system: Option<String>,
    35	    #[serde(skip_serializing_if = "Option::is_none")]
    36	    prompt_user: Option<String>,
    37	    #[serde(skip_serializing_if = "Option::is_none")]
    38	    response_raw: Option<String>,
    39	    #[serde(skip_serializing_if = "Option::is_none")]
    40	    latency_ms: Option<u64>,
    41	    #[serde(skip_serializing_if = "Option::is_none")]
    42	    success: Option<bool>,
    43	    #[serde(skip_serializing_if = "Option::is_none")]
    44	    error: Option<String>,
    45	    /// Step-specific context (word counts, flags, thresholds, parsed results, etc.)
    46	    #[serde(skip_serializing_if = "Option::is_none")]
    47	    context: Option<serde_json::Value>,
    48	}
    49	
    50	impl PipelineLogger {
    51	    /// Create a new logger with no path (call `set_session` before logging).
    52	    pub fn new() -> Self {
    53	        Self { path: None, pending: Vec::new() }
    54	    }
    55	
    56	    /// Set the archive directory for the current session.
    57	    /// Flushes any buffered entries that were logged before the path was known.
    58	    pub fn set_session(&mut self, session_dir: &Path) {
    59	        let path = session_dir.join(LOG_FILENAME);
    60	        // Flush pending entries accumulated before the session directory existed
    61	        if !self.pending.is_empty() {
    62	            if let Ok(mut f) = OpenOptions::new()
    63	                .create(true)
    64	                .append(true)
    65	                .open(&path)
    66	            {
    67	                for line in self.pending.drain(..) {
    68	                    if let Err(e) = writeln!(f, "{}", line) {
    69	                        warn!("Pipeline log flush failed: {}", e);
    70	                    }
    71	                }
    72	            } else {
    73	                warn!("Pipeline log: could not open {} to flush {} pending entries",
    74	                    path.display(), self.pending.len());
    75	            }
    76	        }
    77	        self.path = Some(path);
    78	    }
    79	
    80	    /// Clear the session path (between encounters).
    81	    /// Discards any unflushed pending entries.
    82	    pub fn clear_session(&mut self) {
    83	        self.path = None;
    84	        self.pending.clear();
    85	    }
    86	
    87	    /// Append a log entry. Buffers in memory if no path is set yet.
    88	    /// Never blocks the pipeline on I/O errors.
    89	    fn append(&mut self, entry: LogEntry) {
    90	        let line = match serde_json::to_string(&entry) {
    91	            Ok(l) => l,
    92	            Err(e) => {
    93	                warn!("Pipeline log serialization failed: {}", e);
    94	                return;
    95	            }
    96	        };
    97	        match &self.path {
    98	            Some(path) => {
    99	                if let Err(e) = OpenOptions::new()
   100	                    .create(true)
   101	                    .append(true)
   102	                    .open(path)
   103	                    .and_then(|mut f| writeln!(f, "{}", line))
   104	                {
   105	                    warn!("Pipeline log write failed: {}", e);
   106	                }
   107	            }
   108	            None => {
   109	                // Buffer for later flush when set_session() is called
   110	                self.pending.push(line);
   111	            }
   112	        }
   113	    }
   114	
   115	    /// Log an LLM call (detection, clinical check, merge, SOAP, vision).
   116	    pub fn log_llm_call(
   117	        &mut self,
   118	        step: &str,
   119	        model: &str,
   120	        system_prompt: &str,
```
