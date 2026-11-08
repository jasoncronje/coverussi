import Foundation
import Observation
import Security

/// Where the display is and how to log in. The password lives in the keychain.
@MainActor @Observable
final class Config {
    var address: String {
        didSet { UserDefaults.standard.set(address, forKey: "address") }
    }
    var password: String {
        didSet { Keychain.set(password, for: "password") }
    }

    init() {
        address = UserDefaults.standard.string(forKey: "address") ?? ""
        password = Keychain.get("password") ?? ""
    }

    var isSet: Bool { boardURL != nil }

    var boardURL: URL? { Self.url(address) }

    /// A cover as the board names it: a full URL, or a path on Home Assistant (whose address the board gives).
    func artURL(_ art: String?, homeAssistant: String?) -> URL? {
        guard var art, !art.isEmpty else { return nil }
        if art.hasPrefix("/") {
            guard let ha = homeAssistant.flatMap(Self.url) else { return nil }
            return URL(string: art, relativeTo: ha)?.absoluteURL
        }
        // Apple's covers come in any size; the board asks for 600 px, a TV wants more.
        if art.contains("mzstatic.com") {
            art = art.replacingOccurrences(of: "/600x600", with: "/1200x1200")
        }
        return URL(string: art)
    }

    private static func url(_ text: String) -> URL? {
        var t = text.trimmingCharacters(in: .whitespaces)
        while t.hasSuffix("/") { t.removeLast() }
        guard !t.isEmpty else { return nil }
        if !t.contains("://") { t = "http://" + t }
        return URL(string: t)
    }
}

private enum Keychain {
    static func get(_ key: String) -> String? {
        var query = base(key)
        query[kSecReturnData as String] = true
        var out: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let data = out as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    static func set(_ value: String, for key: String) {
        SecItemDelete(base(key) as CFDictionary)
        var query = base(key)
        query[kSecValueData as String] = Data(value.utf8)
        SecItemAdd(query as CFDictionary, nil)
    }

    private static func base(_ key: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "coverussi", kSecAttrAccount as String: key]
    }
}
