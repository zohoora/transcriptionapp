import SwiftUI

struct MainTabView: View {
    @EnvironmentObject var appState: AppState
    @StateObject private var audioRecorder = AudioRecorder()
    @StateObject private var recordingStore = RecordingStore()
    @StateObject private var networkMonitor = NetworkMonitor()
    @StateObject private var uploadService: UploadService

    init() {
        // Can't reference other @StateObject in init, so create shared instances
        let store = RecordingStore()
        let monitor = NetworkMonitor()
        _recordingStore = StateObject(wrappedValue: store)
        _networkMonitor = StateObject(wrappedValue: monitor)
        _uploadService = StateObject(wrappedValue: UploadService(store: store, networkMonitor: monitor))
    }

    var body: some View {
        TabView {
            RecordingView()
                .tabItem {
                    Label("Record", systemImage: "mic.fill")
                }

            SessionsListView()
                .tabItem {
                    Label("Sessions", systemImage: "list.bullet")
                }

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gear")
                }
        }
        .environmentObject(audioRecorder)
        .environmentObject(recordingStore)
        .environmentObject(networkMonitor)
        .environmentObject(uploadService)
        .onAppear {
            uploadService.configure(serverURL: appState.serverURL)
            uploadService.startPolling()
            uploadService.uploadPending()
        }
        .onChange(of: appState.serverURL) { _, newURL in
            uploadService.configure(serverURL: newURL)
        }
    }
}
