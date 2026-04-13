import Foundation
@testable import AMI_Assist

/// Creates a temporary directory for test isolation. Returns the URL.
/// Caller is responsible for cleanup via `removeItem(at:)`.
func makeTempDirectory() -> URL {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent("AMIAssistTests-\(UUID().uuidString)")
    try! FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
}

/// Creates a sample Recording for use in tests.
func makeSampleRecording(
    id: String = UUID().uuidString,
    physicianId: String = "doc-1",
    physicianName: String = "Dr. Test",
    durationMs: UInt64 = 60_000,
    status: RecordingStatus = .saved,
    jobId: String? = nil
) -> Recording {
    Recording(
        recordingId: id,
        physicianId: physicianId,
        physicianName: physicianName,
        startedAt: Date(),
        durationMs: durationMs,
        status: status,
        jobId: jobId,
        uploadedAt: nil,
        errorMessage: nil
    )
}

/// Creates a URLSession configured to use MockURLProtocol.
func makeMockSession() -> URLSession {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [MockURLProtocol.self]
    return URLSession(configuration: config)
}
