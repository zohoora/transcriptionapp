import SwiftUI

struct SessionsListView: View {
    @EnvironmentObject var recordingStore: RecordingStore
    @EnvironmentObject var uploadService: UploadService
    @State private var showDeleteConfirm = false
    @State private var recordingToDelete: Recording?

    var body: some View {
        NavigationStack {
            Group {
                if recordingStore.recordings.isEmpty {
                    ContentUnavailableView(
                        "No Recordings",
                        systemImage: "mic.slash",
                        description: Text("Record an appointment to see it here.")
                    )
                } else {
                    List {
                        ForEach(recordingStore.recordings) { recording in
                            RecordingRow(recording: recording)
                                .swipeActions(edge: .trailing) {
                                    Button(role: .destructive) {
                                        recordingToDelete = recording
                                        showDeleteConfirm = true
                                    } label: {
                                        Label("Delete", systemImage: "trash")
                                    }
                                }
                        }
                    }
                    .refreshable {
                        recordingStore.loadAll()
                        uploadService.uploadPending()
                    }
                }
            }
            .navigationTitle("Sessions")
            .alert("Delete Recording?", isPresented: $showDeleteConfirm) {
                Button("Cancel", role: .cancel) {}
                Button("Delete", role: .destructive) {
                    if let rec = recordingToDelete {
                        recordingStore.delete(rec)
                    }
                }
            } message: {
                Text("This will permanently delete the recording and its data.")
            }
        }
    }
}

// MARK: - Recording Row

struct RecordingRow: View {
    let recording: Recording

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(recording.startedAt, style: .date)
                    .font(.headline)
                Text("\(recording.startedAt, style: .time) · \(recording.formattedDuration)")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                if let error = recording.errorMessage {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(1)
                }
            }

            Spacer()

            StatusBadge(status: recording.status)
        }
        .padding(.vertical, 2)
    }
}

// MARK: - Status Badge

struct StatusBadge: View {
    let status: RecordingStatus

    var body: some View {
        Text(status.label)
            .font(.caption.bold())
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(status.color.opacity(0.15))
            .foregroundStyle(status.color)
            .clipShape(Capsule())
    }
}

extension RecordingStatus {
    var label: String {
        switch self {
        case .saved: return "Saved"
        case .uploading: return "Uploading"
        case .processing: return "Processing"
        case .complete: return "Complete"
        case .failed: return "Failed"
        }
    }

    var color: Color {
        switch self {
        case .saved: return .gray
        case .uploading: return .blue
        case .processing: return .orange
        case .complete: return .green
        case .failed: return .red
        }
    }
}
