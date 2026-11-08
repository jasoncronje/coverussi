import Foundation

/// What Wikipedia says about the artist on screen.
struct Artist: Equatable {
    var name: String
    var description: String?
    var summary: String
    var photo: URL?
}

enum Wikipedia {
    /// Words in a page's short description that mean it's about music, not a namesake.
    private static let musical = [
        "band", "singer", "musician", "rapper", "group", "duo", "trio", "composer", "songwriter",
        "dj", "producer", "orchestra", "guitarist", "pianist", "drummer", "musical", "record",
    ]

    /// The artist's page, trying the plain name first and Wikipedia's music titles after.
    /// "A & B" and "A feat. B" fall back to A when nothing matches the whole name.
    static func artist(_ name: String) async -> Artist? {
        var names = [name]
        for split in [" feat", " ft.", " & ", ", ", " x ", " and "] {
            if let r = name.range(of: split, options: .caseInsensitive) {
                names.append(String(name[..<r.lowerBound]))
                break
            }
        }
        for name in names {
            for title in [name, "\(name) (band)", "\(name) (musician)", "\(name) (singer)", "\(name) (rapper)"] {
                if Task.isCancelled { return nil }
                if let artist = await page(title) { return artist }
            }
        }
        return nil
    }

    private static func page(_ title: String) async -> Artist? {
        let path = title.replacingOccurrences(of: " ", with: "_")
            .addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(["/"])) ?? title
        guard let url = URL(string: "https://en.wikipedia.org/api/rest_v1/page/summary/\(path)") else { return nil }
        var req = URLRequest(url: url, timeoutInterval: 10)
        // Wikimedia asks every client to say who it is.
        req.setValue("Coverussi (Apple TV app)", forHTTPHeaderField: "User-Agent")
        guard let (data, response) = try? await URLSession.shared.data(for: req),
              (response as? HTTPURLResponse)?.statusCode == 200,
              let page = try? JSONDecoder().decode(Summary.self, from: data),
              page.type == "standard"
        else { return nil }
        let about = page.description?.lowercased() ?? ""
        guard musical.contains(where: { about.contains($0) }) else { return nil }
        return Artist(
            name: page.title, description: page.description, summary: page.extract,
            photo: page.thumbnail.flatMap { URL(string: $0.source) })
    }
}

private struct Summary: Decodable {
    var type: String
    var title: String
    var description: String?
    var extract: String
    var thumbnail: Image?

    struct Image: Decodable {
        var source: String
    }
}
