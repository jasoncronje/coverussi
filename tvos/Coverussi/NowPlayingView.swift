import SwiftUI

struct NowPlayingView: View {
    let board: Board
    let onSettings: () -> Void
    /// The cover on screen. A new one replaces it only once it has loaded, so tracks change without a flash.
    @State private var cover: UIImage?
    @State private var loading = false
    /// The artist page, while it's open.
    @State private var artistPage: ArtistRequest?

    var body: some View {
        ZStack {
            Backdrop(cover: cover)
            if let s = board.showing {
                Playing(showing: s, cover: cover, loading: loading)
                    .transition(.opacity)
                    // Select on the remote opens the artist's page.
                    .focusable()
                    .onTapGesture {
                        if !s.artist.isEmpty { artistPage = ArtistRequest(name: s.artist) }
                    }
            } else {
                Idle(problem: board.problem, connected: board.connected, onSettings: onSettings)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.8), value: board.showing)
        .ignoresSafeArea()
        // Play/Pause on the remote opens the settings while something plays.
        .onPlayPauseCommand(perform: onSettings)
        // Menu goes back a page, and from the artist's own page back to the song.
        .fullScreenCover(item: $artistPage) { request in
            NavigationStack {
                ArtistView(request: request, nowPlaying: board.showing?.title)
                    .navigationDestination(for: ArtistRequest.self) {
                        ArtistView(request: $0, nowPlaying: board.showing?.title)
                    }
            }
        }
        .task(id: board.showing?.art) { await load(board.showing?.art) }
    }

    private func load(_ art: URL?) async {
        guard let art else {
            loading = false
            withAnimation(.easeInOut(duration: 0.8)) { cover = nil }
            return
        }
        loading = true
        let image = try? await URLSession.shared.data(from: art).0
        guard !Task.isCancelled else { return }
        withAnimation(.easeInOut(duration: 0.8)) {
            cover = image.flatMap(UIImage.init(data:))
            loading = false
        }
    }
}

/// The cover, blurred and darkened, filling the screen.
private struct Backdrop: View {
    let cover: UIImage?

    var body: some View {
        Color.black
            .overlay {
                if let cover {
                    Image(uiImage: cover)
                        .resizable()
                        .scaledToFill()
                        .id(ObjectIdentifier(cover))
                        .blur(radius: 80)
                        .opacity(0.55)
                        .transition(.opacity)
                }
            }
            .clipped()
    }
}

private struct Playing: View {
    let showing: Showing
    let cover: UIImage?
    let loading: Bool
    @State private var hint = true

    var body: some View {
        HStack(spacing: 90) {
            Cover(cover: cover, loading: loading)
                .frame(width: 720, height: 720)
                .opacity(showing.dimmed ? 0.5 : 1)
            VStack(alignment: .leading, spacing: 18) {
                if !showing.source.isEmpty {
                    Text(showing.source.uppercased())
                        .font(.caption.weight(.semibold))
                        .tracking(3)
                        .foregroundStyle(.secondary)
                }
                Text(showing.title)
                    .font(.system(size: 76, weight: .bold))
                    .lineLimit(3)
                Text(showing.artist)
                    .font(.system(size: 44, weight: .medium))
                    .foregroundStyle(.white.opacity(0.85))
                    .lineLimit(2)
                if !showing.album.isEmpty {
                    Text(showing.album)
                        .font(.system(size: 36))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                if let started = showing.startedAt, let duration = showing.duration, duration > 0 {
                    Progress(started: started, duration: duration)
                        .padding(.top, 30)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 140)
        .foregroundStyle(.white)
        // Nothing on screen says how to reach the settings, so say it for a moment.
        .overlay(alignment: .bottom) {
            if hint {
                Text(showing.artist.isEmpty ? "Play/Pause for settings" : "Select for the artist  ·  Play/Pause for settings")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.bottom, 60)
                    .transition(.opacity)
            }
        }
        .task {
            try? await Task.sleep(for: .seconds(5))
            withAnimation(.easeInOut(duration: 0.8)) { hint = false }
        }
    }
}

private struct Cover: View {
    let cover: UIImage?
    let loading: Bool

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 24).fill(.white.opacity(0.08))
            if let cover {
                Image(uiImage: cover)
                    .resizable()
                    .scaledToFill()
                    .id(ObjectIdentifier(cover))
                    .transition(.opacity)
            } else if loading {
                ProgressView()
            } else {
                Image(systemName: "music.note").font(.system(size: 160)).foregroundStyle(.secondary)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 24))
        .shadow(color: .black.opacity(0.5), radius: 40, y: 20)
    }
}

/// A record's place in the song, counted from when the board matched it.
private struct Progress: View {
    let started: Date
    let duration: TimeInterval

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let elapsed = min(max(context.date.timeIntervalSince(started), 0), duration)
            VStack(spacing: 10) {
                GeometryReader { g in
                    ZStack(alignment: .leading) {
                        Capsule().fill(.white.opacity(0.2))
                        Capsule().fill(.white.opacity(0.85)).frame(width: g.size.width * elapsed / duration)
                    }
                }
                .frame(height: 8)
                HStack {
                    Text(clock(elapsed))
                    Spacer()
                    Text("-" + clock(duration - elapsed))
                }
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: 700)
        }
    }

    private func clock(_ s: TimeInterval) -> String {
        let s = Int(s.rounded())
        return String(format: "%d:%02d", s / 60, s % 60)
    }
}

private struct Idle: View {
    let problem: String?
    let connected: Bool
    let onSettings: () -> Void

    var body: some View {
        VStack(spacing: 30) {
            Image(systemName: "opticaldisc").font(.system(size: 120)).foregroundStyle(.secondary)
            Text(connected ? "Nothing playing" : problem ?? "Connecting…")
                .font(.title2)
                .foregroundStyle(.white.opacity(0.8))
            Button("Settings", action: onSettings)
        }
        .foregroundStyle(.white)
    }
}
