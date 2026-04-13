import SwiftUI

struct SetupView: View {
    @EnvironmentObject var appState: AppState
    @State private var serverURL = ""
    @State private var physicians: [Physician] = []
    @State private var isLoading = false
    @State private var isConnected = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                Spacer()

                Image(systemName: "waveform.circle.fill")
                    .font(.system(size: 60))
                    .foregroundStyle(.blue)

                Text("AMI Assist")
                    .font(.largeTitle.bold())

                Text("Connect to your clinic server to get started.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal)

                // Server URL input
                VStack(alignment: .leading, spacing: 8) {
                    Text("Server URL")
                        .font(.headline)

                    TextField("http://100.119.83.76:8090", text: $serverURL)
                        .textFieldStyle(.roundedBorder)
                        .keyboardType(.URL)
                        .textContentType(.URL)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)

                    if let error = errorMessage {
                        Text(error)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }
                .padding(.horizontal)

                // Connect button
                Button {
                    connect()
                } label: {
                    if isLoading {
                        ProgressView()
                            .frame(maxWidth: .infinity)
                    } else {
                        Text(isConnected ? "Connected — Select Physician" : "Connect")
                            .frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(serverURL.isEmpty || isLoading)
                .padding(.horizontal)

                // Physician list (shown after successful connection)
                if isConnected && !physicians.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Select Physician")
                            .font(.headline)
                            .padding(.horizontal)

                        List(physicians) { physician in
                            Button {
                                selectPhysician(physician)
                            } label: {
                                HStack {
                                    Text(physician.name)
                                        .foregroundStyle(.primary)
                                    Spacer()
                                    Image(systemName: "chevron.right")
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                        .listStyle(.insetGrouped)
                        .frame(maxHeight: 300)
                    }
                }

                Spacer()
            }
            .navigationTitle("")
        }
    }

    private func connect() {
        guard !serverURL.isEmpty else { return }
        isLoading = true
        errorMessage = nil
        isConnected = false

        // Normalize URL
        var url = serverURL.trimmingCharacters(in: .whitespacesAndNewlines)
        if !url.hasPrefix("http") {
            url = "http://\(url)"
        }
        serverURL = url

        let client = APIClient(baseURL: url)
        Task {
            do {
                try await client.checkHealth()
                let list = try await client.fetchPhysicians()
                await MainActor.run {
                    physicians = list
                    isConnected = true
                    isLoading = false
                    appState.serverURL = url
                }
            } catch {
                await MainActor.run {
                    errorMessage = "Could not connect: \(error.localizedDescription)"
                    isLoading = false
                }
            }
        }
    }

    private func selectPhysician(_ physician: Physician) {
        appState.physicianId = physician.id
        appState.physicianName = physician.name
    }
}
