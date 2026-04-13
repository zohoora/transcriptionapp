import Foundation
import Combine

/// Manages local recording metadata (JSON sidecars) and coordinates with APIClient.
class RecordingStore: ObservableObject {
    @Published var recordings: [Recording] = []

    let directory: URL

    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        return e
    }()
    private let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()

    init(directory: URL? = nil) {
        self.directory = directory ?? AudioRecorder.recordingsDirectory
        loadAll()
    }

    // MARK: - CRUD

    func save(_ recording: Recording) {
        let url = metadataURL(for: recording.recordingId)
        if let data = try? encoder.encode(recording) {
            try? data.write(to: url, options: .atomic)
        }
        if let idx = recordings.firstIndex(where: { $0.recordingId == recording.recordingId }) {
            recordings[idx] = recording
        } else {
            recordings.insert(recording, at: 0)
        }
    }

    func delete(_ recording: Recording) {
        let audioURL = directory.appendingPathComponent(recording.audioFileName)
        let metaURL = metadataURL(for: recording.recordingId)
        try? FileManager.default.removeItem(at: audioURL)
        try? FileManager.default.removeItem(at: metaURL)
        recordings.removeAll { $0.recordingId == recording.recordingId }
    }

    func recording(for id: String) -> Recording? {
        recordings.first { $0.recordingId == id }
    }

    /// Total size of local audio files in bytes.
    var totalStorageBytes: UInt64 {
        guard let files = try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.fileSizeKey]) else {
            return 0
        }
        return files.reduce(0) { total, url in
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
            return total + UInt64(size)
        }
    }

    /// Recordings that need uploading.
    var pendingUploads: [Recording] {
        recordings.filter { $0.status == .saved }
    }

    /// Recordings that are waiting for server processing.
    var processingRecordings: [Recording] {
        recordings.filter { $0.status == .processing }
    }

    // MARK: - Persistence

    func loadAll() {
        guard let files = try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) else {
            return
        }
        recordings = files
            .filter { $0.pathExtension == "json" }
            .compactMap { url in
                guard let data = try? Data(contentsOf: url) else { return nil }
                return try? decoder.decode(Recording.self, from: data)
            }
            .sorted { $0.startedAt > $1.startedAt }
    }

    // MARK: - Helpers

    private func metadataURL(for recordingId: String) -> URL {
        directory.appendingPathComponent("\(recordingId).json")
    }
}
