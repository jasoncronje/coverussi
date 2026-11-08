import SwiftUI

/// An artist to open a page for: the one playing, by name, or one Deezer already knows.
struct ArtistRequest: Identifiable, Hashable {
    var name: String
    var deezerID: Int?
    var id: String { deezerID.map(String.init) ?? name }
}

/// The artist's page, laid out like a streaming app's: photo, popular songs, albums, about, and more like them.
struct ArtistView: View {
    let request: ArtistRequest
    /// The song on screen, marked in the popular list when it's there.
    let nowPlaying: String?
    @State private var profile: ArtistProfile?
    @State private var missing = false

    var body: some View {
        Group {
            if let profile {
                Page(profile: profile, nowPlaying: nowPlaying)
            } else if missing {
                VStack(spacing: 30) {
                    Image(systemName: "person.crop.circle.badge.questionmark").font(.system(size: 120))
                    Text("Couldn't find \(request.name)").font(.title2)
                }
                .foregroundStyle(.secondary)
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.black)
        .task(id: request) {
            let found = if let id = request.deezerID {
                await Deezer.profile(id: id)
            } else {
                await Deezer.profile(named: request.name)
            }
            profile = found
            missing = found == nil
        }
    }
}

/// The web app's amber, kept for what's live: here, the song that's playing.
private let accent = Color(red: 0xF2 / 255, green: 0xB3 / 255, blue: 0x3D / 255)
private let gutter: CGFloat = 90

private struct Page: View {
    let profile: ArtistProfile
    let nowPlaying: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 70) {
                Hero(profile: profile)
                if !profile.popular.isEmpty {
                    Shelf("Popular") { Popular(tracks: profile.popular, nowPlaying: nowPlaying) }
                }
                if !profile.albums.isEmpty {
                    Shelf("Albums") { Albums(albums: profile.albums) }
                }
                if let biography = profile.biography {
                    Shelf("About") { About(biography: biography) }
                }
                if !profile.related.isEmpty {
                    Shelf("Fans also like") { Related(artists: profile.related) }
                }
            }
            .padding(.bottom, gutter)
        }
        .ignoresSafeArea(edges: .top)
    }
}

/// The photo on the right, fading into the page, with the name over it.
private struct Hero: View {
    let profile: ArtistProfile

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            HStack {
                Spacer()
                AsyncImage(url: profile.picture) { image in
                    image.resizable().scaledToFill()
                } placeholder: {
                    Color.clear
                }
                .frame(width: 1100, height: 640)
                .clipped()
                .mask(LinearGradient(colors: [.clear, .black, .black], startPoint: .leading, endPoint: .trailing))
                .mask(LinearGradient(colors: [.black, .black, .clear], startPoint: .top, endPoint: .bottom))
            }
            VStack(alignment: .leading, spacing: 14) {
                Text("ARTIST")
                    .font(.caption.weight(.semibold))
                    .tracking(3)
                    .foregroundStyle(.secondary)
                Text(profile.name)
                    .font(.system(size: 120, weight: .heavy))
                    .lineLimit(2)
                    .minimumScaleFactor(0.5)
                if profile.fans > 0 {
                    Text("\(profile.fans.formatted()) fans on Deezer")
                        .font(.title3)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: 1000, alignment: .leading)
            .padding(.horizontal, gutter)
            .padding(.bottom, 20)
        }
        .frame(height: 640)
        .foregroundStyle(.white)
    }
}

private struct Shelf<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text(title)
                .font(.system(size: 44, weight: .bold))
                .padding(.horizontal, gutter)
            content
        }
    }
}

/// The top ten, in two columns of five.
private struct Popular: View {
    let tracks: [ArtistProfile.Track]
    let nowPlaying: String?

    var body: some View {
        let rows = Array(tracks.prefix(10).enumerated())
        HStack(alignment: .top, spacing: 60) {
            column(rows.prefix(5))
            column(rows.dropFirst(5))
        }
        .padding(.horizontal, gutter)
    }

    private func column(_ rows: ArraySlice<(offset: Int, element: ArtistProfile.Track)>) -> some View {
        VStack(spacing: 12) {
            ForEach(rows, id: \.element.id) { row in
                // A button only so the remote can move through the list.
                Button {} label: {
                    TrackRow(number: row.offset + 1, track: row.element, playing: same(row.element.title, nowPlaying))
                }
                .buttonStyle(.card)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// "Peg" and "Peg (2023 Remaster)" are the same song.
    private func same(_ a: String, _ b: String?) -> Bool {
        guard let b else { return false }
        func base(_ s: String) -> String {
            var s = s.lowercased()
            for cut in [" (", " - ", " ["] {
                if let r = s.range(of: cut) { s = String(s[..<r.lowerBound]) }
            }
            return s.trimmingCharacters(in: .whitespaces)
        }
        return base(a) == base(b)
    }
}

private struct TrackRow: View {
    let number: Int
    let track: ArtistProfile.Track
    let playing: Bool

    var body: some View {
        HStack(spacing: 24) {
            Group {
                if playing {
                    Image(systemName: "speaker.wave.2.fill").foregroundStyle(accent)
                } else {
                    Text("\(number)").foregroundStyle(.secondary)
                }
            }
            .font(.title3.monospacedDigit())
            .frame(width: 44)
            AsyncImage(url: track.cover) { image in
                image.resizable().scaledToFill()
            } placeholder: {
                Color.white.opacity(0.08)
            }
            .frame(width: 80, height: 80)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 4) {
                Text(track.title)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(playing ? accent : .white)
                    .lineLimit(1)
                Text(track.album)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 20)
            Text(String(format: "%d:%02d", track.duration / 60, track.duration % 60))
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
    }
}

private struct Albums: View {
    let albums: [ArtistProfile.Album]

    var body: some View {
        ScrollView(.horizontal) {
            LazyHStack(alignment: .top, spacing: 48) {
                ForEach(albums) { album in
                    VStack(alignment: .leading, spacing: 18) {
                        Button {} label: {
                            AsyncImage(url: album.cover) { image in
                                image.resizable().scaledToFill()
                            } placeholder: {
                                Color.white.opacity(0.08)
                            }
                            .frame(width: 320, height: 320)
                        }
                        .buttonStyle(.card)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(album.title)
                                .font(.callout.weight(.semibold))
                                .lineLimit(2)
                            Text(album.year)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .frame(width: 320, alignment: .leading)
                    }
                }
            }
            .padding(.horizontal, gutter)
            .padding(.vertical, 30)
        }
        .scrollClipDisabled()
    }
}

private struct About: View {
    let biography: Biography

    var body: some View {
        Button {} label: {
            HStack(alignment: .top, spacing: 44) {
                if let photo = biography.photo {
                    AsyncImage(url: photo) { image in
                        image.resizable().scaledToFill()
                    } placeholder: {
                        Color.white.opacity(0.08)
                    }
                    .frame(width: 260, height: 260)
                    .clipShape(RoundedRectangle(cornerRadius: 16))
                }
                VStack(alignment: .leading, spacing: 16) {
                    if let description = biography.description {
                        Text(description)
                            .font(.title3.weight(.semibold))
                    }
                    Text(biography.summary)
                        .font(.body)
                        .foregroundStyle(.white.opacity(0.85))
                        .lineSpacing(6)
                    Text("From Wikipedia")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(44)
        }
        .buttonStyle(.card)
        .padding(.horizontal, gutter)
    }
}

/// Similar artists; choosing one opens their page.
private struct Related: View {
    let artists: [ArtistProfile.RelatedArtist]

    var body: some View {
        ScrollView(.horizontal) {
            LazyHStack(alignment: .top, spacing: 56) {
                ForEach(artists) { artist in
                    NavigationLink(value: ArtistRequest(name: artist.name, deezerID: artist.id)) {
                        VStack(spacing: 20) {
                            AsyncImage(url: artist.picture) { image in
                                image.resizable().scaledToFill()
                            } placeholder: {
                                Color.white.opacity(0.08)
                            }
                            .frame(width: 240, height: 240)
                            .clipShape(Circle())
                            Text(artist.name)
                                .font(.callout.weight(.semibold))
                                .lineLimit(1)
                                .frame(width: 240)
                        }
                    }
                    .buttonStyle(.borderless)
                }
            }
            .padding(.horizontal, gutter)
            .padding(.vertical, 30)
        }
        .scrollClipDisabled()
    }
}
