import AVFoundation
import Combine

/// Wraps AVAudioRecorder for AAC recording at 16kHz mono.
class AudioRecorder: NSObject, ObservableObject {
    @Published var isRecording = false
    @Published var elapsedSeconds: TimeInterval = 0
    @Published var audioLevel: Float = 0 // 0.0–1.0 normalized

    private var recorder: AVAudioRecorder?
    private var timer: Timer?
    private var startTime: Date?

    /// Current recording's UUID. Set when recording starts.
    private(set) var currentRecordingId: String?

    /// Directory where recordings are stored.
    static var recordingsDirectory: URL {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let dir = docs.appendingPathComponent("recordings")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    func startRecording() throws -> (id: String, url: URL) {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .default)
        try session.setActive(true)

        let id = UUID().uuidString
        let url = Self.recordingsDirectory.appendingPathComponent("\(id).m4a")

        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
            AVSampleRateKey: 16000.0,
            AVNumberOfChannelsKey: 1,
            AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
        ]

        recorder = try AVAudioRecorder(url: url, settings: settings)
        recorder?.isMeteringEnabled = true
        recorder?.record()

        currentRecordingId = id
        startTime = Date()
        isRecording = true
        elapsedSeconds = 0

        // Update elapsed time and audio levels every 0.25s
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            guard let self, let start = self.startTime else { return }
            self.elapsedSeconds = Date().timeIntervalSince(start)
            self.recorder?.updateMeters()
            // Normalize from dB range (-60..0) to 0..1
            let db = self.recorder?.averagePower(forChannel: 0) ?? -60
            self.audioLevel = max(0, min(1, (db + 60) / 60))
        }

        return (id, url)
    }

    func stopRecording() -> TimeInterval {
        timer?.invalidate()
        timer = nil
        recorder?.stop()
        recorder = nil
        isRecording = false

        let duration = elapsedSeconds
        elapsedSeconds = 0
        audioLevel = 0
        currentRecordingId = nil

        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        return duration
    }
}
