import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var appState: AppState
    @EnvironmentObject var recordingStore: RecordingStore
    @EnvironmentObject var uploadService: UploadService
    @State private var showChangePhysician = false

    var body: some View {
        NavigationStack {
            List {
                // Server
                Section("Server") {
                    LabeledContent("URL", value: appState.serverURL)
                }

                // Physician
                Section("Physician") {
                    LabeledContent("Name", value: appState.physicianName ?? "—")
                    Button("Change Physician") {
                        showChangePhysician = true
                    }
                }

                // Storage
                Section("Storage") {
                    LabeledContent("Local recordings", value: "\(recordingStore.recordings.count)")
                    LabeledContent("Storage used", value: formatBytes(recordingStore.totalStorageBytes))

                    Button("Upload All Pending") {
                        uploadService.uploadPending()
                    }
                    .disabled(recordingStore.pendingUploads.isEmpty)

                    Button("Delete Uploaded Recordings", role: .destructive) {
                        deleteUploaded()
                    }
                    .disabled(uploadedRecordings.isEmpty)
                }

                // About
                Section("About") {
                    LabeledContent("App", value: "AMI Assist Mobile")
                    LabeledContent("Version", value: appVersion)
                }
            }
            .navigationTitle("Settings")
            .sheet(isPresented: $showChangePhysician) {
                SetupView()
                    .environmentObject(appState)
            }
        }
    }

    private var uploadedRecordings: [Recording] {
        recordingStore.recordings.filter { $0.status == .complete }
    }

    private func deleteUploaded() {
        for recording in uploadedRecordings {
            recordingStore.delete(recording)
        }
    }

    private var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(bytes))
    }
}
