import Foundation
import Observation
import os

/// What the board says it shows: the `telemetry` message of `/api/live`.
struct Telemetry: Decodable, Equatable {
    var mode: String?
    var source: String?
    var source_name: String?
    var app: String?
    var title: String?
    var artist: String?
    var album: String?
    var art: String?
    var art_dimmed: Bool?
}

/// A record the board identified by listening: the `now_playing` message.
struct Record: Decodable, Equatable {
    var title: String
    var artist: String
    var album: String?
    var art_url: String?
    var duration_ms: Double?
    var matched_at: Double
    var offset_ms: Double?
}

/// The song to put on screen, from whichever source the board follows.
struct Showing: Equatable {
    var title: String
    var artist: String
    var album: String
    var source: String
    var art: URL?
    /// Records only: where the song was when it was matched, and its length.
    var startedAt: Date?
    var duration: TimeInterval?
    var dimmed: Bool
}

/// The board's live feed: logs in with the web app's password, then follows `/api/live`.
@MainActor @Observable
final class Board {
    private(set) var connected = false
    private(set) var problem: String?
    private(set) var telemetry: Telemetry?
    private(set) var record: Record?
    /// Home Assistant's address, as the board knows it: streamed covers are paths on it.
    private(set) var homeAssistant: String?

    private let config: Config
    private var task: Task<Void, Never>?
    private let session = URLSession(configuration: .default)

    init(config: Config) {
        self.config = config
    }

    var showing: Showing? {
        guard let t = telemetry else { return nil }
        if t.mode == "analog", let r = record {
            let started = r.offset_ms.map { Date(timeIntervalSince1970: r.matched_at - $0 / 1000) }
            return Showing(
                title: r.title, artist: r.artist, album: r.album ?? "",
                source: t.source_name ?? "Record", art: config.artURL(r.art_url, homeAssistant: homeAssistant),
                startedAt: started, duration: r.duration_ms.map { $0 / 1000 }, dimmed: t.art_dimmed ?? false)
        }
        guard let title = t.title, !title.isEmpty else { return nil }
        let player = t.source_name ?? t.source?.replacingOccurrences(of: "media_player.", with: "")
            .replacingOccurrences(of: "_", with: " ") ?? ""
        return Showing(
            title: title, artist: t.artist ?? "", album: t.album ?? "",
            source: [player, t.app].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · "),
            art: config.artURL(t.art, homeAssistant: homeAssistant), startedAt: nil, duration: nil, dimmed: false)
    }

    func start() {
        task?.cancel()
        task = Task { await run() }
    }

    func stop() {
        task?.cancel()
        task = nil
        connected = false
    }

    private func run() async {
        var retry: Duration = .seconds(1)
        while !Task.isCancelled {
            do {
                try await login()
                problem = nil
                try await follow()
            } catch is CancellationError {
                return
            } catch {
                problem = (error as? BoardError)?.message ?? error.localizedDescription
            }
            if connected {
                // A connection that worked starts the backoff over.
                retry = .seconds(1)
            } else {
                // The board can't be reached, so what it last said is no longer true.
                telemetry = nil
                record = nil
            }
            connected = false
            try? await Task.sleep(for: retry)
            retry = min(retry * 2, .seconds(15))
        }
    }

    private func login() async throws {
        guard let base = config.boardURL else { throw BoardError("Set the display's address") }
        // A board that's gone shouldn't keep the old song up for a minute while a request waits.
        let (data, _) = try await session.data(for: URLRequest(url: base.appending(path: "api/session"), timeoutInterval: 5))
        let s = try JSONDecoder().decode(SessionState.self, from: data)
        // Without a password the board lets everyone in.
        if s.logged_in { return }
        var req = URLRequest(url: base.appending(path: "api/login"), timeoutInterval: 5)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(["password": config.password])
        let (_, response) = try await session.data(for: req)
        if (response as? HTTPURLResponse)?.statusCode == 401 { throw BoardError("Wrong password") }
    }

    private func follow() async throws {
        guard let base = config.boardURL, var parts = URLComponents(url: base, resolvingAgainstBaseURL: false)
        else { return }
        parts.scheme = parts.scheme == "https" ? "wss" : "ws"
        guard let url = parts.url?.appending(path: "api/live") else { return }
        let socket = session.webSocketTask(with: url)
        socket.resume()
        let pinger = Task { await keepAlive(socket) }
        defer {
            pinger.cancel()
            socket.cancel(with: .goingAway, reason: nil)
        }
        while !Task.isCancelled {
            // Binary messages are the panel's frames; only the JSON ones matter here.
            guard case .string(let text) = try await socket.receive() else { continue }
            connected = true
            handle(Data(text.utf8))
        }
    }

    /// A board that drops off the network never closes the socket, so ask it to answer now and then.
    /// Closing the socket ends the wait in `follow`, and `run` reconnects.
    private func keepAlive(_ socket: URLSessionWebSocketTask) async {
        while !Task.isCancelled {
            do { try await Task.sleep(for: .seconds(20)) } catch { return }
            let pong = OSAllocatedUnfairLock(initialState: false)
            socket.sendPing { error in
                if error == nil { pong.withLock { $0 = true } }
            }
            do { try await Task.sleep(for: .seconds(10)) } catch { return }
            if !pong.withLock({ $0 }) {
                socket.cancel(with: .goingAway, reason: nil)
                return
            }
        }
    }

    private func handle(_ data: Data) {
        let decoder = JSONDecoder()
        guard let kind = try? decoder.decode(Kind.self, from: data) else { return }
        switch kind.type {
        case "snapshot":
            if let m = try? decoder.decode(Snapshot.self, from: data) {
                telemetry = m.telemetry
                record = m.now_playing
                homeAssistant = m.ha_url
            }
        case "telemetry":
            if let m = try? decoder.decode(Message<Telemetry>.self, from: data) { telemetry = m.data }
        case "now_playing":
            record = (try? decoder.decode(Message<Record>.self, from: data))?.data
        case "event":
            if kind.name == "analog_stopped" { record = nil }
        default:
            break
        }
    }
}

private struct SessionState: Decodable {
    var logged_in: Bool
    var password_set: Bool
}

private struct Kind: Decodable {
    var type: String
    var name: String?
}

private struct Snapshot: Decodable {
    var telemetry: Telemetry?
    var now_playing: Record?
    var ha_url: String?
}

private struct Message<T: Decodable>: Decodable {
    var data: T?
}

struct BoardError: Error {
    var message: String
    init(_ message: String) { self.message = message }
}
