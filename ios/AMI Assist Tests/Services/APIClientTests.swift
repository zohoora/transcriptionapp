import XCTest
@testable import AMI_Assist

final class APIClientTests: XCTestCase {

    override func setUp() {
        super.setUp()
        MockURLProtocol.reset()
    }

    override func tearDown() {
        MockURLProtocol.reset()
        super.tearDown()
    }

    // MARK: - Health check

    func testCheckHealth_callsHealthEndpoint() async throws {
        MockURLProtocol.requestHandler = { request in
            XCTAssertEqual(request.url?.path, "/health")
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, Data())
        }

        let client = makeClient()
        try await client.checkHealth()

        XCTAssertEqual(MockURLProtocol.capturedRequests.count, 1)
    }

    func testCheckHealth_throwsOnServerError() async {
        MockURLProtocol.requestHandler = { request in
            let response = HTTPURLResponse(url: request.url!, statusCode: 500, httpVersion: nil, headerFields: nil)!
            return (response, Data())
        }

        let client = makeClient()

        do {
            try await client.checkHealth()
            XCTFail("Expected error")
        } catch let error as APIError {
            if case .httpError(let code) = error {
                XCTAssertEqual(code, 500)
            } else {
                XCTFail("Expected httpError, got \(error)")
            }
        } catch {
            XCTFail("Expected APIError, got \(error)")
        }
    }

    // MARK: - Fetch physicians

    func testFetchPhysicians_decodesArray() async throws {
        let json = """
        [{"id": "doc-1", "name": "Dr. Smith"}, {"id": "doc-2", "name": "Dr. Jones"}]
        """.data(using: .utf8)!

        MockURLProtocol.requestHandler = { request in
            XCTAssertEqual(request.url?.path, "/physicians")
            XCTAssertEqual(request.httpMethod, "GET")
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, json)
        }

        let client = makeClient()
        let physicians = try await client.fetchPhysicians()

        XCTAssertEqual(physicians.count, 2)
        XCTAssertEqual(physicians[0].name, "Dr. Smith")
        XCTAssertEqual(physicians[1].id, "doc-2")
    }

    // MARK: - Fetch job

    func testFetchJob_constructsCorrectURL() async throws {
        let json = sampleJobJSON(jobId: "job-abc")

        MockURLProtocol.requestHandler = { request in
            XCTAssertEqual(request.url?.path, "/mobile/jobs/job-abc")
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, json)
        }

        let client = makeClient()
        let job = try await client.fetchJob(jobId: "job-abc")
        XCTAssertEqual(job.jobId, "job-abc")
    }

    // MARK: - Fetch jobs (batch)

    func testFetchJobs_withoutPhysicianId() async throws {
        let json = "[\(String(data: sampleJobJSON(jobId: "j1"), encoding: .utf8)!)]".data(using: .utf8)!

        MockURLProtocol.requestHandler = { request in
            XCTAssertEqual(request.url?.path, "/mobile/jobs")
            XCTAssertNil(request.url?.query)
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, json)
        }

        let client = makeClient()
        let jobs = try await client.fetchJobs()
        XCTAssertEqual(jobs.count, 1)
    }

    func testFetchJobs_withPhysicianId_addsQueryParam() async throws {
        let json = "[]".data(using: .utf8)!

        MockURLProtocol.requestHandler = { request in
            XCTAssertTrue(request.url?.absoluteString.contains("physician_id=doc-1") ?? false)
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, json)
        }

        let client = makeClient()
        let jobs = try await client.fetchJobs(physicianId: "doc-1")
        XCTAssertEqual(jobs.count, 0)
    }

    // MARK: - Delete job

    func testDeleteJob_usesDeleteMethod() async throws {
        MockURLProtocol.requestHandler = { request in
            XCTAssertEqual(request.httpMethod, "DELETE")
            XCTAssertEqual(request.url?.path, "/mobile/jobs/job-del")
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, Data())
        }

        let client = makeClient()
        try await client.deleteJob(jobId: "job-del")
    }

    // MARK: - Upload recording

    func testUploadRecording_sendsMultipartPOST() async throws {
        let responseJSON = sampleJobJSON(jobId: "new-job")

        MockURLProtocol.requestHandler = { request in
            XCTAssertEqual(request.httpMethod, "POST")
            XCTAssertEqual(request.url?.path, "/mobile/upload")
            let contentType = request.value(forHTTPHeaderField: "Content-Type") ?? ""
            XCTAssertTrue(contentType.contains("multipart/form-data"))

            // Verify body contains required fields
            if let body = request.httpBody, let bodyStr = String(data: body, encoding: .utf8) {
                XCTAssertTrue(bodyStr.contains("physician_id"), "Missing physician_id")
                XCTAssertTrue(bodyStr.contains("recording_id"), "Missing recording_id")
                XCTAssertTrue(bodyStr.contains("duration_ms"), "Missing duration_ms")
                XCTAssertTrue(bodyStr.contains("name=\"audio\""), "Missing audio field")
            }

            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, responseJSON)
        }

        // Create a temp audio file
        let tempDir = makeTempDirectory()
        defer { try? FileManager.default.removeItem(at: tempDir) }
        let audioURL = tempDir.appendingPathComponent("test.m4a")
        try Data("fake audio data".utf8).write(to: audioURL)

        let recording = makeSampleRecording(id: "upload-test", durationMs: 30_000)
        let client = makeClient()
        let job = try await client.uploadRecording(audioURL: audioURL, recording: recording)
        XCTAssertEqual(job.jobId, "new-job")
    }

    // MARK: - APIError

    func testAPIError_invalidResponse_description() {
        let error = APIError.invalidResponse
        XCTAssertEqual(error.errorDescription, "Invalid server response")
    }

    func testAPIError_httpError_description() {
        let error = APIError.httpError(statusCode: 404)
        XCTAssertEqual(error.errorDescription, "Server error (HTTP 404)")
    }

    // MARK: - Helpers

    private func makeClient() -> APIClient {
        // Create client with mock session
        let client = APIClient(baseURL: "http://test-server:8090")
        // Inject mock session via swizzling the shared session isn't ideal,
        // but APIClient uses URLSession.shared. For proper testing, we'd use DI.
        // Since APIClient uses URLSession.shared, we configure via URLProtocol registration.
        URLProtocol.registerClass(MockURLProtocol.self)
        return client
    }

    private func sampleJobJSON(jobId: String) -> Data {
        """
        {
            "job_id": "\(jobId)",
            "physician_id": "doc-1",
            "recording_id": "rec-1",
            "started_at": "2026-04-13T10:00:00Z",
            "duration_ms": 60000,
            "status": "queued",
            "error": null,
            "sessions_created": [],
            "created_at": "2026-04-13T10:01:00Z",
            "updated_at": "2026-04-13T10:01:00Z",
            "device_info": null
        }
        """.data(using: .utf8)!
    }
}
