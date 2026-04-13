import XCTest
@testable import AMI_Assist

final class RecordingStoreTests: XCTestCase {
    private var tempDir: URL!
    private var store: RecordingStore!

    override func setUp() {
        super.setUp()
        tempDir = makeTempDirectory()
        store = RecordingStore(directory: tempDir)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: tempDir)
        super.tearDown()
    }

    // MARK: - Save

    func testSave_createsMetadataFile() {
        let rec = makeSampleRecording(id: "save-test")
        store.save(rec)

        let metaURL = tempDir.appendingPathComponent("save-test.json")
        XCTAssertTrue(FileManager.default.fileExists(atPath: metaURL.path))
    }

    func testSave_addsToRecordingsArray() {
        XCTAssertEqual(store.recordings.count, 0)

        let rec = makeSampleRecording()
        store.save(rec)

        XCTAssertEqual(store.recordings.count, 1)
        XCTAssertEqual(store.recordings[0].recordingId, rec.recordingId)
    }

    func testSave_updatesExistingRecording() {
        var rec = makeSampleRecording(id: "update-test")
        store.save(rec)

        rec.status = .processing
        rec.jobId = "job-123"
        store.save(rec)

        XCTAssertEqual(store.recordings.count, 1)
        XCTAssertEqual(store.recordings[0].status, .processing)
        XCTAssertEqual(store.recordings[0].jobId, "job-123")
    }

    func testSave_insertsAtFront() {
        let rec1 = makeSampleRecording(id: "first")
        let rec2 = makeSampleRecording(id: "second")

        store.save(rec1)
        store.save(rec2)

        XCTAssertEqual(store.recordings.count, 2)
        XCTAssertEqual(store.recordings[0].recordingId, "second")
        XCTAssertEqual(store.recordings[1].recordingId, "first")
    }

    // MARK: - Delete

    func testDelete_removesMetadataFile() {
        let rec = makeSampleRecording(id: "del-test")
        store.save(rec)

        // Create a fake audio file
        let audioURL = tempDir.appendingPathComponent("del-test.m4a")
        FileManager.default.createFile(atPath: audioURL.path, contents: Data("fake".utf8))

        store.delete(rec)

        XCTAssertFalse(FileManager.default.fileExists(atPath: tempDir.appendingPathComponent("del-test.json").path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: audioURL.path))
    }

    func testDelete_removesFromArray() {
        let rec = makeSampleRecording(id: "del-arr")
        store.save(rec)
        XCTAssertEqual(store.recordings.count, 1)

        store.delete(rec)
        XCTAssertEqual(store.recordings.count, 0)
    }

    func testDelete_onlyRemovesTargetRecording() {
        let rec1 = makeSampleRecording(id: "keep")
        let rec2 = makeSampleRecording(id: "delete")
        store.save(rec1)
        store.save(rec2)

        store.delete(rec2)

        XCTAssertEqual(store.recordings.count, 1)
        XCTAssertEqual(store.recordings[0].recordingId, "keep")
    }

    // MARK: - Lookup

    func testRecordingFor_findsExisting() {
        let rec = makeSampleRecording(id: "find-me")
        store.save(rec)

        let found = store.recording(for: "find-me")
        XCTAssertNotNil(found)
        XCTAssertEqual(found?.recordingId, "find-me")
    }

    func testRecordingFor_returnsNilForMissing() {
        let found = store.recording(for: "nonexistent")
        XCTAssertNil(found)
    }

    // MARK: - Filtering

    func testPendingUploads_filtersSavedOnly() {
        store.save(makeSampleRecording(id: "a", status: .saved))
        store.save(makeSampleRecording(id: "b", status: .processing))
        store.save(makeSampleRecording(id: "c", status: .saved))
        store.save(makeSampleRecording(id: "d", status: .complete))

        let pending = store.pendingUploads
        XCTAssertEqual(pending.count, 2)
        XCTAssertTrue(pending.allSatisfy { $0.status == .saved })
    }

    func testProcessingRecordings_filtersProcessingOnly() {
        store.save(makeSampleRecording(id: "a", status: .saved))
        store.save(makeSampleRecording(id: "b", status: .processing))
        store.save(makeSampleRecording(id: "c", status: .processing))

        let processing = store.processingRecordings
        XCTAssertEqual(processing.count, 2)
        XCTAssertTrue(processing.allSatisfy { $0.status == .processing })
    }

    // MARK: - Storage

    func testTotalStorageBytes_sumsFilesSizes() {
        // Write some data to the temp dir
        let file1 = tempDir.appendingPathComponent("test1.m4a")
        let file2 = tempDir.appendingPathComponent("test2.json")
        let data1 = Data(repeating: 0xAA, count: 1024)
        let data2 = Data(repeating: 0xBB, count: 512)

        try! data1.write(to: file1)
        try! data2.write(to: file2)

        let bytes = store.totalStorageBytes
        XCTAssertGreaterThanOrEqual(bytes, 1536) // At least 1024 + 512
    }

    func testTotalStorageBytes_emptyDir_returnsZero() {
        XCTAssertEqual(store.totalStorageBytes, 0)
    }

    // MARK: - Load persistence roundtrip

    func testLoadAll_reloadsFromDisk() {
        let rec = makeSampleRecording(id: "persist-test", durationMs: 90_000)
        store.save(rec)

        // Create a new store pointing at same directory
        let store2 = RecordingStore(directory: tempDir)

        XCTAssertEqual(store2.recordings.count, 1)
        XCTAssertEqual(store2.recordings[0].recordingId, "persist-test")
        XCTAssertEqual(store2.recordings[0].durationMs, 90_000)
    }

    func testLoadAll_sortsNewestFirst() {
        // Save recordings with different dates
        var older = makeSampleRecording(id: "older")
        // Manually create with a past date
        older = Recording(
            recordingId: "older",
            physicianId: "doc-1",
            physicianName: "Dr. Test",
            startedAt: Date(timeIntervalSince1970: 1000),
            durationMs: 60_000,
            status: .saved,
            jobId: nil,
            uploadedAt: nil,
            errorMessage: nil
        )
        var newer = makeSampleRecording(id: "newer")
        newer = Recording(
            recordingId: "newer",
            physicianId: "doc-1",
            physicianName: "Dr. Test",
            startedAt: Date(timeIntervalSince1970: 2000),
            durationMs: 60_000,
            status: .saved,
            jobId: nil,
            uploadedAt: nil,
            errorMessage: nil
        )

        store.save(older)
        store.save(newer)

        // Reload from disk
        let store2 = RecordingStore(directory: tempDir)
        XCTAssertEqual(store2.recordings.count, 2)
        XCTAssertEqual(store2.recordings[0].recordingId, "newer")
        XCTAssertEqual(store2.recordings[1].recordingId, "older")
    }

    func testLoadAll_ignoresNonJSONFiles() {
        // Save a recording so we have a valid JSON
        store.save(makeSampleRecording(id: "valid"))

        // Create a non-JSON file in the directory
        let txtFile = tempDir.appendingPathComponent("notes.txt")
        try! "some text".write(to: txtFile, atomically: true, encoding: .utf8)

        // Create a corrupt JSON file
        let badJSON = tempDir.appendingPathComponent("corrupt.json")
        try! "not valid json{{{".write(to: badJSON, atomically: true, encoding: .utf8)

        let store2 = RecordingStore(directory: tempDir)
        XCTAssertEqual(store2.recordings.count, 1)
        XCTAssertEqual(store2.recordings[0].recordingId, "valid")
    }
}
