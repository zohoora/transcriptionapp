import Foundation
import Combine

/// Manages upload queue and status polling.
/// Watches for network connectivity and automatically uploads pending recordings.
class UploadService: ObservableObject {
    @Published var isUploading = false

    private let store: RecordingStore
    private let networkMonitor: NetworkMonitor
    private var apiClient: APIClient?
    private var pollTimer: Timer?
    private var cancellables = Set<AnyCancellable>()
    private var uploadTask: Task<Void, Never>?

    init(store: RecordingStore, networkMonitor: NetworkMonitor) {
        self.store = store
        self.networkMonitor = networkMonitor

        // Auto-upload when network becomes available
        networkMonitor.$isConnected
            .removeDuplicates()
            .filter { $0 }
            .sink { [weak self] _ in self?.uploadPending() }
            .store(in: &cancellables)
    }

    func configure(serverURL: String) {
        apiClient = APIClient(baseURL: serverURL)
    }

    // MARK: - Upload

    func uploadPending() {
        guard !isUploading, let client = apiClient else { return }
        let pending = store.pendingUploads
        guard !pending.isEmpty else { return }

        isUploading = true
        uploadTask = Task { [weak self] in
            for recording in pending {
                guard let self, !Task.isCancelled else { break }
                await self.uploadOne(recording, client: client)
            }
            if let self {
                await MainActor.run { self.isUploading = false }
            }
        }
    }

    private func uploadOne(_ recording: Recording, client: APIClient) async {
        var rec = recording
        rec.status = .uploading
        let uploading = rec
        await MainActor.run { store.save(uploading) }

        let audioURL = AudioRecorder.recordingsDirectory.appendingPathComponent(rec.audioFileName)
        guard FileManager.default.fileExists(atPath: audioURL.path) else {
            rec.status = .failed
            rec.errorMessage = "Audio file not found"
            let failed = rec
            await MainActor.run { store.save(failed) }
            return
        }

        do {
            let job = try await client.uploadRecording(audioURL: audioURL, recording: rec)
            rec.status = .processing
            rec.jobId = job.jobId
            rec.uploadedAt = Date()
            rec.errorMessage = nil
            let success = rec
            await MainActor.run { store.save(success) }
        } catch {
            rec.status = .saved // Reset to saved so it retries later
            rec.errorMessage = error.localizedDescription
            let retry = rec
            await MainActor.run { store.save(retry) }
        }
    }

    // MARK: - Status Polling

    func startPolling() {
        stopPolling()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
            self?.pollStatuses()
        }
        pollStatuses() // Immediate first check
    }

    func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    private func pollStatuses() {
        guard let client = apiClient else { return }
        let processing = store.processingRecordings
        guard !processing.isEmpty else { return }

        Task {
            do {
                let jobs = try await client.fetchJobs()
                let jobMap = Dictionary(uniqueKeysWithValues: jobs.map { ($0.jobId, $0) })

                for recording in processing {
                    guard let jobId = recording.jobId,
                          let job = jobMap[jobId] else { continue }
                    var rec = recording
                    switch job.status {
                    case "complete":
                        rec.status = .complete
                        rec.errorMessage = nil
                    case "failed":
                        rec.status = .failed
                        rec.errorMessage = job.error
                    default:
                        continue
                    }
                    let updated = rec
                    await MainActor.run { store.save(updated) }
                }
            } catch {
                // Polling failure is transient — don't change recording status
            }
        }
    }
}
