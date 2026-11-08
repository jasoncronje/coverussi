import SwiftUI

/// The song to show details for, as it was when Select was pressed.
struct InfoRequest: Identifiable {
    let id = UUID()
    var title: String
    var artist: String
    var album: String
    var cover: URL?
}

/// Details on the song and its artist, a tab each. Menu closes it.
struct InfoView: View {
    let request: InfoRequest
    @State private var track: TrackInfo?
    @State private var artist: ArtistInfo?

    var body: some View {
        TabView {
            Group {
                if let track { TrackTab(track: track) } else { ProgressView() }
            }
            .tabItem { Text("Track") }
            Group {
                if let artist { ArtistTab(artist: artist) } else { ProgressView() }
            }
            .tabItem { Text("Artist") }
        }
        .task {
            async let t = Deezer.track(
                request.title, by: request.artist, album: request.album.isEmpty ? nil : request.album,
                cover: request.cover)
            async let a = Deezer.artist(request.artist)
            track = await t
            artist = await a
        }
    }
}

private struct TrackTab: View {
    let track: TrackInfo

    var body: some View {
        Panel(kind: "Track", title: track.title, subtitle: track.artist, about: track.about) {
            Picture(url: track.cover, shape: RoundedRectangle(cornerRadius: 24))
        } facts: {
            if let album = track.album { Fact("Album", album) }
            if let position = track.position {
                Fact("Track", track.tracks.map { "\(position) of \($0)" } ?? "\(position)")
            }
            if let released = track.released { Fact("Released", released) }
            if let duration = track.duration {
                Fact("Length", String(format: "%d:%02d", duration / 60, duration % 60))
            }
            if !track.genres.isEmpty { Fact("Genre", track.genres.joined(separator: ", ")) }
            if let label = track.label { Fact("Label", label) }
            if let bpm = track.bpm { Fact("Tempo", "\(Int(bpm.rounded())) BPM") }
        }
    }
}

private struct ArtistTab: View {
    let artist: ArtistInfo

    var body: some View {
        Panel(kind: "Artist", title: artist.name, subtitle: artist.biography?.description, about: artist.biography) {
            Picture(url: artist.picture ?? artist.biography?.photo, shape: Circle())
        } facts: {
            if let fans = artist.fans, fans > 0 { Fact("Fans", "\(fans.formatted()) on Deezer") }
        }
    }
}

/// The shared layout: picture on the left; name, facts and Wikipedia's summary on the right.
private struct Panel<Art: View, Facts: View>: View {
    let kind: String
    let title: String
    let subtitle: String?
    let about: Biography?
    @ViewBuilder let art: Art
    @ViewBuilder let facts: Facts

    var body: some View {
        HStack(alignment: .top, spacing: 80) {
            art.frame(width: 460, height: 460)
            VStack(alignment: .leading, spacing: 28) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(kind.uppercased())
                        .font(.caption.weight(.semibold))
                        .tracking(3)
                        .foregroundStyle(.secondary)
                    Text(title)
                        .font(.system(size: 64, weight: .bold))
                        .lineLimit(2)
                        .minimumScaleFactor(0.7)
                    if let subtitle {
                        Text(subtitle)
                            .font(.system(size: 34, weight: .medium))
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
                VStack(alignment: .leading, spacing: 12) {
                    facts
                }
                if let about {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(about.summary)
                            .font(.system(size: 29))
                            .foregroundStyle(.white.opacity(0.85))
                            .lineSpacing(6)
                            .lineLimit(9)
                        Text("From Wikipedia")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 120)
        .padding(.top, 40)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .foregroundStyle(.white)
    }
}

private struct Fact: View {
    let name: String
    let value: String

    init(_ name: String, _ value: String) {
        self.name = name
        self.value = value
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text(name).foregroundStyle(.secondary).frame(width: 190, alignment: .leading)
            Text(value).lineLimit(1)
        }
        .font(.system(size: 30))
    }
}

private struct Picture<S: Shape>: View {
    let url: URL?
    let shape: S

    var body: some View {
        ZStack {
            shape.fill(.white.opacity(0.08))
            if let url {
                AsyncImage(url: url) { image in
                    image.resizable().scaledToFill()
                } placeholder: {
                    ProgressView()
                }
            } else {
                Image(systemName: "music.note").font(.system(size: 120)).foregroundStyle(.secondary)
            }
        }
        .clipShape(shape)
        .shadow(color: .black.opacity(0.5), radius: 30, y: 15)
    }
}
