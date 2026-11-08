import Foundation

/// What the Artist panel shows, from Deezer (no key needed) and Wikipedia.
struct ArtistInfo {
    var name: String
    var fans: Int?
    var picture: URL?
    var biography: Biography?
}

/// What the Track panel shows. Anything Deezer doesn't know is left out.
struct TrackInfo {
    var title: String
    var artist: String
    var album: String?
    var cover: URL?
    /// A day from Deezer, or just the year when Wikipedia knows an earlier one.
    var released: String?
    var duration: Int?
    var position: Int?
    var tracks: Int?
    var label: String?
    var genres: [String] = []
    var bpm: Double?
    var about: Biography?
}

enum Deezer {
    static func artist(_ name: String) async -> ArtistInfo {
        async let biography = Wikipedia.biography(name)
        var found: DZArtist?
        for name in artistNames(name) where found == nil {
            found = await findArtist(name)
        }
        return ArtistInfo(
            name: found?.name ?? name,
            fans: found?.nb_fan,
            picture: (found?.picture_xl ?? found?.picture_big).flatMap(URL.init(string:)),
            biography: await biography)
    }

    /// The song as the board names it, filled in from Deezer's copy of it when there is one.
    static func track(_ title: String, by artist: String, album: String?, cover: URL?) async -> TrackInfo {
        async let about = Wikipedia.song(title, by: artist)
        var info = TrackInfo(title: title, artist: artist, album: album, cover: cover)
        if let hit = await findTrack(songTitle(title), by: artistNames(artist).last ?? artist) {
            async let track: DZTrack? = get("track/\(hit.id)")
            async let record: DZAlbum? = get("album/\(hit.album.id)")
            let (t, a) = await (track, record)
            info.album = a?.title ?? info.album
            info.cover = info.cover ?? (a?.cover_xl).flatMap(URL.init(string:))
            info.released = (t?.release_date ?? a?.release_date).flatMap(day)?.formatted(date: .long, time: .omitted)
            info.duration = t?.duration
            info.position = t?.track_position
            info.tracks = a?.nb_tracks
            info.label = a?.label
            info.genres = a?.genres?.data.map(\.name) ?? []
            info.bpm = t?.bpm.flatMap { $0 > 0 ? $0 : nil }
        }
        info.about = await about
        // Deezer dates the edition it carries, often a remaster decades on; Wikipedia's "1977 single" is the song's.
        if let year = info.about?.description?.firstMatch(of: /\b(19|20)\d\d\b/).map({ String($0.output.0) }),
           info.released.map({ !$0.contains(year) && ($0.firstMatch(of: /\d{4}/).map { String($0.output) } ?? "9999") > year }) ?? true {
            info.released = year
        }
        return info
    }

    /// The artist by that exact name, the most followed one if several share it.
    /// Search ranks tribute acts and karaoke versions close behind, so a near match isn't taken.
    private static func findArtist(_ name: String) async -> DZArtist? {
        guard let found: DZList<DZArtist> = await get("search/artist?q=\(query(name))&limit=10") else { return nil }
        return found.data.filter { same($0.name, name) }.max { ($0.nb_fan ?? 0) < ($1.nb_fan ?? 0) }
    }

    private static func findTrack(_ title: String, by artist: String) async -> DZSearchTrack? {
        // Deezer's artist:"" track:"" search finds nothing for plenty of songs; a plain one does.
        let q = query("\(artist) \(title)")
        guard let found: DZList<DZSearchTrack> = await get("search/track?q=\(q)&limit=10") else { return nil }
        return found.data.first { same($0.artist.name, artist) && same(songTitle($0.title), title) }
            ?? found.data.first { same($0.artist.name, artist) }
    }

    private static func same(_ a: String, _ b: String) -> Bool {
        a.compare(b, options: [.caseInsensitive, .diacriticInsensitive]) == .orderedSame
    }

    private static func query(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? text
    }

    private static func day(_ text: String) -> Date? {
        try? Date(text, strategy: .iso8601.year().month().day())
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

private struct DZSearchTrack: Decodable, Sendable {
    var id: Int
    var title: String
    var artist: Name
    var album: ID

    struct Name: Decodable, Sendable { var name: String }
    struct ID: Decodable, Sendable { var id: Int }
}

private struct DZTrack: Decodable, Sendable {
    var duration: Int?
    var track_position: Int?
    var release_date: String?
    var bpm: Double?
}

private struct DZAlbum: Decodable, Sendable {
    var title: String
    var release_date: String?
    var label: String?
    var nb_tracks: Int?
    var cover_xl: String?
    var genres: Genres?

    struct Genres: Decodable, Sendable { var data: [Genre] }
    struct Genre: Decodable, Sendable { var name: String }
}
