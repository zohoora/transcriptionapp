import SwiftUI

struct RecordingView: View {
    @EnvironmentObject var appState: AppState
    @EnvironmentObject var audioRecorder: AudioRecorder
    @EnvironmentObject var recordingStore: RecordingStore
    @EnvironmentObject var uploadService: UploadService

    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 32) {
                Spacer()

                // Physician name
                if let name = appState.physicianName {
                    Text(name)
                        .font(.title2)
                        .foregroundStyle(.secondary)
                }

                // Timer
                Text(formatTime(audioRecorder.elapsedSeconds))
                    .font(.system(size: 48, weight: .light, design: .monospaced))
                    .foregroundStyle(audioRecorder.isRecording ? .primary : .secondary)

                // Audio level meter
                if audioRecorder.isRecording {
                    AudioLevelBar(level: audioRecorder.audioLevel)
                        .frame(height: 6)
                        .padding(.horizontal, 60)
                }

                // Record/Stop button
                Button {
                    if audioRecorder.isRecording {
                        stopRecording()
                    } else {
                        startRecording()
                    }
                } label: {
                    ZStack {
                        Circle()
                            .fill(audioRecorder.isRecording ? .red : .blue)
                            .frame(width: 88, height: 88)

                        if audioRecorder.isRecording {
                            RoundedRectangle(cornerRadius: 6)
                                .fill(.white)
                                .frame(width: 30, height: 30)
                        } else {
                            Circle()
                                .fill(.white)
                                .frame(width: 32, height: 32)
                        }
                    }
                }

                // Status label
                Text(audioRecorder.isRecording ? "Recording..." : "Ready")
                    .font(.headline)
                    .foregroundStyle(audioRecorder.isRecording ? .red : .secondary)

                if let error = errorMessage {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .padding(.horizontal)
                }

                Spacer()
            }
            .navigationTitle("AMI Assist")
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    private func startRecording() {
        errorMessage = nil
        do {
            _ = try audioRecorder.startRecording()
        } catch {
            errorMessage = "Failed to start: \(error.localizedDescription)"
        }
    }

    private func stopRecording() {
        guard let recordingId = audioRecorder.currentRecordingId else { return }
        let duration = audioRecorder.stopRecording()

        let recording = Recording(
            recordingId: recordingId,
            physicianId: appState.physicianId ?? "",
            physicianName: appState.physicianName ?? "",
            startedAt: Date().addingTimeInterval(-duration),
            durationMs: UInt64(duration * 1000),
            status: .saved,
            jobId: nil,
            uploadedAt: nil,
            errorMessage: nil
        )
        recordingStore.save(recording)
        uploadService.uploadPending()
    }

    private func formatTime(_ seconds: TimeInterval) -> String {
        let h = Int(seconds) / 3600
        let m = (Int(seconds) % 3600) / 60
        let s = Int(seconds) % 60
        if h > 0 {
            return String(format: "%d:%02d:%02d", h, m, s)
        }
        return String(format: "%02d:%02d", m, s)
    }
}

// MARK: - Audio Level Bar

struct AudioLevelBar: View {
    let level: Float

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.gray.opacity(0.2))

                Capsule()
                    .fill(barColor)
                    .frame(width: geo.size.width * CGFloat(level))
                    .animation(.linear(duration: 0.15), value: level)
            }
        }
    }

    private var barColor: Color {
        if level > 0.8 { return .red }
        if level > 0.5 { return .green }
        return .green.opacity(0.7)
    }
}
