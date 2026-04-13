import Foundation
import UIKit

/// HTTP client for the profile service.
class APIClient {
    private let session = URLSession.shared
    private let baseURL: String

    init(baseURL: String) {
        self.baseURL = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    // MARK: - Physicians

    func fetchPhysicians() async throws -> [Physician] {
        let url = URL(string: "\(baseURL)/physicians")!
        let (data, response) = try await session.data(from: url)
        try checkResponse(response)
        return try JSONDecoder().decode([Physician].self, from: data)
    }

    // MARK: - Upload

    /// Upload a recording to the profile service. Returns the created Job.
    func uploadRecording(audioURL: URL, recording: Recording) async throws -> Job {
        let url = URL(string: "\(baseURL)/mobile/upload")!
        var request = URLRequest(url: url)
        request.httpMethod = "POST"

        let boundary = UUID().uuidString
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()

        // Text fields
        let fields: [(String, String)] = [
            ("physician_id", recording.physicianId),
            ("started_at", ISO8601DateFormatter().string(from: recording.startedAt)),
            ("duration_ms", String(recording.durationMs)),
            ("recording_id", recording.recordingId),
            ("device_info", deviceInfo()),
        ]

        for (name, value) in fields {
            body.appendMultipartField(name: name, value: value, boundary: boundary)
        }

        // Audio file
        let audioData = try Data(contentsOf: audioURL)
        body.appendMultipartFile(
            name: "audio",
            fileName: recording.audioFileName,
            mimeType: "audio/mp4",
            data: audioData,
            boundary: boundary
        )

        body.append("--\(boundary)--\r\n".data(using: .utf8)!)
        request.httpBody = body

        // Longer timeout for large uploads
        request.timeoutInterval = 300

        let (data, response) = try await session.data(for: request)
        try checkResponse(response)
        return try JSONDecoder().decode(Job.self, from: data)
    }

    // MARK: - Jobs

    func fetchJob(jobId: String) async throws -> Job {
        let url = URL(string: "\(baseURL)/mobile/jobs/\(jobId)")!
        let (data, response) = try await session.data(from: url)
        try checkResponse(response)
        return try JSONDecoder().decode(Job.self, from: data)
    }

    func fetchJobs(physicianId: String? = nil) async throws -> [Job] {
        var urlString = "\(baseURL)/mobile/jobs"
        if let pid = physicianId {
            urlString += "?physician_id=\(pid)"
        }
        let url = URL(string: urlString)!
        let (data, response) = try await session.data(from: url)
        try checkResponse(response)
        return try JSONDecoder().decode([Job].self, from: data)
    }

    func deleteJob(jobId: String) async throws {
        let url = URL(string: "\(baseURL)/mobile/jobs/\(jobId)")!
        var request = URLRequest(url: url)
        request.httpMethod = "DELETE"
        let (_, response) = try await session.data(for: request)
        try checkResponse(response)
    }

    // MARK: - Health

    func checkHealth() async throws {
        let url = URL(string: "\(baseURL)/health")!
        let (_, response) = try await session.data(from: url)
        try checkResponse(response)
    }

    // MARK: - Helpers

    private func checkResponse(_ response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse else {
            throw APIError.invalidResponse
        }
        guard (200...299).contains(http.statusCode) else {
            throw APIError.httpError(statusCode: http.statusCode)
        }
    }

    private func deviceInfo() -> String {
        let device = UIDevice.current
        return "\(device.model), iOS \(device.systemVersion)"
    }
}

enum APIError: LocalizedError {
    case invalidResponse
    case httpError(statusCode: Int)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            return "Invalid server response"
        case .httpError(let code):
            return "Server error (HTTP \(code))"
        }
    }
}

// MARK: - Multipart helpers

private extension Data {
    mutating func appendMultipartField(name: String, value: String, boundary: String) {
        append("--\(boundary)\r\n".data(using: .utf8)!)
        append("Content-Disposition: form-data; name=\"\(name)\"\r\n\r\n".data(using: .utf8)!)
        append("\(value)\r\n".data(using: .utf8)!)
    }

    mutating func appendMultipartFile(name: String, fileName: String, mimeType: String, data: Data, boundary: String) {
        append("--\(boundary)\r\n".data(using: .utf8)!)
        append("Content-Disposition: form-data; name=\"\(name)\"; filename=\"\(fileName)\"\r\n".data(using: .utf8)!)
        append("Content-Type: \(mimeType)\r\n\r\n".data(using: .utf8)!)
        append(data)
        append("\r\n".data(using: .utf8)!)
    }
}
