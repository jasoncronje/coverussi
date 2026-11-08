import Foundation

/// Everything the artist page shows, from Deezer (no key needed) and Wikipedia.
struct ArtistProfile {
    var id: Int
    var name: String
    var fans: Int
    var picture: URL?
    var popular: [Track]
    var albums: [Album]
    var related: [RelatedArtist]
    var biography: Biography?

    struct Track: Identifiable {
        var id: Int
        var title: String
        var album: String
        var cover: URL?
        var duration: Int
    }

    struct Album: Identifiable {
        var id: Int
        var title: String
        var year: String
        var cover: URL?
    }

    struct RelatedArtist: Identifiable {
        var id: Int
        var name: String
        var picture: URL?
    }
}

enum Deezer {
    static func profile(named name: String) async -> ArtistProfile? {
        for name in artistNames(name) {
            if Task.isCancelled { return nil }
            if let artist = await find(name) { return await profile(artist) }
        }
        return nil
    }

    static func profile(id: Int) async -> ArtistProfile? {
        guard let artist: DZArtist = await get("artist/\(id)") else { return nil }
        return await profile(artist)
    }

    /// The artist by that exact name, the most followed one if several share it.
    /// Search ranks tribute acts and karaoke versions close behind, so a near match isn't taken.
    private static func find(_ name: String) async -> DZArtist? {
        let query = name.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? name
        guard let found: DZList<DZArtist> = await get("search/artist?q=\(query)&limit=10") else { return nil }
        return found.data
            .filter { $0.name.compare(name, options: [.caseInsensitive, .diacriticInsensitive]) == .orderedSame }
            .max { ($0.nb_fan ?? 0) < ($1.nb_fan ?? 0) }
    }

    private static func profile(_ artist: DZArtist) async -> ArtistProfile {
        async let top: DZList<DZTrack>? = get("artist/\(artist.id)/top?limit=10")
        async let albums: DZList<DZAlbum>? = get("artist/\(artist.id)/albums?limit=100")
        async let related: DZList<DZArtist>? = get("artist/\(artist.id)/related?limit=12")
        async let biography = Wikipedia.biography(artist.name)

        // Studio albums only, newest first; reissues under the same title count once.
        var seen = Set<String>()
        let studio = (await albums)?.data
            .filter { $0.record_type == "album" }
            .sorted { ($0.release_date ?? "") > ($1.release_date ?? "") }
            .filter { seen.insert($0.title.lowercased()).inserted } ?? []

        return ArtistProfile(
            id: artist.id,
            name: artist.name,
            fans: artist.nb_fan ?? 0,
            picture: (artist.picture_xl ?? artist.picture_big).flatMap(URL.init(string:)),
            popular: (await top)?.data.map {
                .init(id: $0.id, title: $0.title, album: $0.album.title,
                      cover: $0.album.cover_medium.flatMap(URL.init(string:)), duration: $0.duration)
            } ?? [],
            albums: studio.map {
                .init(id: $0.id, title: $0.title, year: String(($0.release_date ?? "").prefix(4)),
                      cover: ($0.cover_xl ?? $0.cover_big).flatMap(URL.init(string:)))
            },
            related: (await related)?.data.map {
                .init(id: $0.id, name: $0.name, picture: ($0.picture_big ?? $0.picture_xl).flatMap(URL.init(string:)))
            } ?? [],
            biography: await biography)
    }

    private static func get<T: Decodable & Sendable>(_ path: String) async -> T? {
        guard let url = URL(string: "https://api.deezer.com/" + path),
              let (data, _) = try? await URLSession.shared.data(for: URLRequest(url: url, timeoutInterval: 10))
        else { return nil }
        // Deezer answers errors with 200 and an "error" object, which simply won't decode.
        return try? JSONDecoder().decode(T.self, from: data)
    }
}

private struct DZList<T: Decodable & Sendable>: Decodable, Sendable {
    var data: [T]
}

private struct DZArtist: Decodable, Sendable {
    var id: Int
    var name: String
    var nb_fan: Int?
    var picture_big: String?
    var picture_xl: String?
}

private struct DZTrack: Decodable, Sendable {
    var id: Int
    var title: String
    var duration: Int
    var album: Album

    struct Album: Decodable, Sendable {
        var title: String
        var cover_medium: String?
    }
}

private struct DZAlbum: Decodable, Sendable {
    var id: Int
    var title: String
    var release_date: String?
    var record_type: String?
    var cover_big: String?
    var cover_xl: String?
}
