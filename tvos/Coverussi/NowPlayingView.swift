import SwiftUI

struct NowPlayingView: View {
    let board: Board
    let onSettings: () -> Void

    var body: some View {
        ZStack {
            Backdrop(art: board.showing?.art)
            if let s = board.showing {
                Playing(showing: s)
                    .transition(.opacity)
            } else {
                Idle(problem: board.problem, connected: board.connected, onSettings: onSettings)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.8), value: board.showing)
        .ignoresSafeArea()
        // Play/Pause on the remote opens the settings while something plays.
        .onPlayPauseCommand(perform: onSettings)
    }
}

/// The cover, blurred and darkened, filling the screen.
private struct Backdrop: View {
    let art: URL?

    var body: some View {
        Color.black
            .overlay {
                if let art {
                    AsyncImage(url: art) { image in
                        image.resizable().scaledToFill()
                    } placeholder: {
                        Color.black
                    }
                    .id(art)
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

    var body: some View {
        HStack(spacing: 90) {
            Cover(art: showing.art)
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
    }
}

private struct Cover: View {
    let art: URL?

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 24).fill(.white.opacity(0.08))
            if let art {
                AsyncImage(url: art) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    case .failure: note
                    default: ProgressView()
                    }
                }
                .id(art)
                .transition(.opacity)
            } else {
                note
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 24))
        .shadow(color: .black.opacity(0.5), radius: 40, y: 20)
    }

    private var note: some View {
        Image(systemName: "music.note").font(.system(size: 160)).foregroundStyle(.secondary)
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
            if !connected {
                Button("Settings", action: onSettings)
            }
        }
        .foregroundStyle(.white)
    }
}
