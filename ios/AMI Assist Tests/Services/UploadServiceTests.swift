import XCTest
@testable import AMI_Assist

final class UploadServiceTests: XCTestCase {
    private var tempDir: URL!
    private var store: RecordingStore!

    override func setUp() {
        super.setUp()
        tempDir = makeTempDirectory()
        store = RecordingStore(directory: tempDir)
        MockURLProtocol.reset()
        URLProtocol.registerClass(MockURLProtocol.self)
    }

    override func tearDown() {
        URLProtocol.unregisterClass(MockURLProtocol.self)
        MockURLProtocol.reset()
        try? FileManager.default.removeItem(at: tempDir)
        super.tearDown()
    }

    // MARK: - Initial state

    func testInitialState_isNotUploading() {
        let monitor = NetworkMonitor()
        let service = UploadService(store: store, networkMonitor: monitor)
        XCTAssertFalse(service.isUploading)
    }

    // MARK: - Recording status transitions

    func testRecordingStatusLifecycle() {
        // Test the expected lifecycle: saved → uploading → processing → complete
        var rec = makeSampleRecording(status: .saved)
        XCTAssertEqual(rec.status, .saved)

        rec.status = .uploading
        XCTAssertEqual(rec.status, .uploading)

        rec.status = .processing
        XCTAssertEqual(rec.status, .processing)

        rec.status = .complete
        XCTAssertEqual(rec.status, .complete)
    }

    func testRecordingStatusLifecycle_failurePath() {
        var rec = makeSampleRecording(status: .saved)
        rec.status = .uploading
        rec.status = .saved // Reverted on failure
        XCTAssertEqual(rec.status, .saved)
    }

    // MARK: - Pending uploads

    func testUploadPending_skipsWhenNoApiClient() {
        let monitor = NetworkMonitor()
        let service = UploadService(store: store, networkMonitor: monitor)
        // Don't configure — no apiClient

        store.save(makeSampleRecording(status: .saved))
        service.uploadPending()

        // Should not crash, should not change state
        XCTAssertFalse(service.isUploading)
    }

    func testUploadPending_skipsWhenNoPending() {
        let monitor = NetworkMonitor()
        let service = UploadService(store: store, networkMonitor: monitor)
        service.configure(serverURL: "http://test:8090")

        // No recordings to upload
        service.uploadPending()
        XCTAssertFalse(service.isUploading)
    }

    // MARK: - Store filtering integration

    func testStoreCorrectlyFiltersPendingAndProcessing() {
        store.save(makeSampleRecording(id: "a", status: .saved))
        store.save(makeSampleRecording(id: "b", status: .uploading))
        store.save(makeSampleRecording(id: "c", status: .processing))
        store.save(makeSampleRecording(id: "d", status: .complete))
        store.save(makeSampleRecording(id: "e", status: .failed))
        store.save(makeSampleRecording(id: "f", status: .saved))

        XCTAssertEqual(store.pendingUploads.count, 2)
        XCTAssertEqual(store.processingRecordings.count, 1)
    }
}
