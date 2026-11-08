import Foundation

/// What Wikipedia says about an artist.
struct Biography: Equatable {
    var name: String
    var description: String?
    var summary: String
    var photo: URL?
}

/// The names to look an artist up by: the whole credit, then the first artist of "A & B" or "A feat. B".
func artistNames(_ name: String) -> [String] {
    var names = [name]
    for split in [" feat", " ft.", " & ", ", ", " x ", " and "] {
        if let r = name.range(of: split, options: .caseInsensitive) {
            names.append(String(name[..<r.lowerBound]))
            break
        }
    }
    return names
}

enum Wikipedia {
    /// Words in a page's short description that mean it's about music, not a namesake.
    private static let musical = [
        "band", "singer", "musician", "rapper", "group", "duo", "trio", "composer", "songwriter",
        "dj", "producer", "orchestra", "guitarist", "pianist", "drummer", "musical", "record",
    ]

    /// The artist's page, trying the plain name first and Wikipedia's music titles after.
    static func biography(_ name: String) async -> Biography? {
        for name in artistNames(name) {
            for title in [name, "\(name) (band)", "\(name) (musician)", "\(name) (singer)", "\(name) (rapper)"] {
                if Task.isCancelled { return nil }
                if let artist = await page(title) { return artist }
            }
        }
        return nil
    }

    private static func page(_ title: String) async -> Biography? {
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
        return Biography(
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
