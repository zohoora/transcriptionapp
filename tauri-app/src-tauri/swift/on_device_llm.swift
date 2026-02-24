// On-Device LLM FFI Wrapper
//
// Exposes Apple Foundation Models (macOS 26+) via @_cdecl C-callable functions.
// Compiled by build.rs into a static object linked into the Rust binary.
// On older macOS, all functions gracefully return "not available".

import Foundation

// FoundationModels is only available on macOS 26+.
// We conditionally import it so compilation succeeds on older SDKs
// (the #available guard prevents runtime calls on older OS).
#if canImport(FoundationModels)
import FoundationModels
#endif

// MARK: - Availability Check

/// Returns 1 if on-device model is available, 0 if not, -1 on error.
@_cdecl("on_device_llm_check_availability")
public func checkAvailability() -> Int32 {
    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *) else {
        return 0
    }
    // LanguageModelSession is available — model can be used
    return 1
    #else
    return 0
    #endif
}

// MARK: - Text Generation

/// Generates text from a prompt using the on-device language model.
///
/// Returns 0 on success, 1 on error, 2 on timeout (60s).
/// On success, resultPtr is set to a C string (caller must free via on_device_llm_free_string).
/// On error, errorPtr is set to a C string describing the error.
@_cdecl("on_device_llm_generate")
public func generate(
    promptPtr: UnsafePointer<CChar>,
    resultPtr: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    errorPtr: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
    resultPtr.pointee = nil
    errorPtr.pointee = nil

    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *) else {
        errorPtr.pointee = strdup("macOS 26+ required for on-device LLM")
        return 1
    }

    let prompt = String(cString: promptPtr)

    // Bridge from synchronous C call to Swift async context.
    // The caller is on a Rust std::thread, so blocking via semaphore is safe.
    let semaphore = DispatchSemaphore(value: 0)
    var output: String?
    var errorMessage: String?

    Task {
        do {
            let session = LanguageModelSession()
            let response = try await session.respond(to: prompt)
            output = response.content
        } catch {
            errorMessage = "On-device LLM error: \(error.localizedDescription)"
        }
        semaphore.signal()
    }

    // Wait up to 60 seconds for generation
    let timeout = DispatchTime.now() + .seconds(60)
    let waitResult = semaphore.wait(timeout: timeout)

    if waitResult == .timedOut {
        errorPtr.pointee = strdup("On-device LLM generation timed out after 60s")
        return 2
    }

    if let error = errorMessage {
        errorPtr.pointee = strdup(error)
        return 1
    }

    if let text = output {
        resultPtr.pointee = strdup(text)
        return 0
    }

    errorPtr.pointee = strdup("On-device LLM returned nil")
    return 1
    #else
    errorPtr.pointee = strdup("FoundationModels not available (SDK too old)")
    return 1
    #endif
}

// MARK: - Memory Management

/// Free a string allocated by generate().
@_cdecl("on_device_llm_free_string")
public func freeString(_ ptr: UnsafeMutablePointer<CChar>?) {
    if let ptr = ptr {
        free(ptr)
    }
}
