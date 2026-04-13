import SwiftUI

@main
struct AMIAssistApp: App {
    @StateObject private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            if appState.isSetupComplete {
                MainTabView()
                    .environmentObject(appState)
            } else {
                SetupView()
                    .environmentObject(appState)
            }
        }
    }
}
