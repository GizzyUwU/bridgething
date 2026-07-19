import Foundation
import os

#if canImport(MusicKit) && os(iOS)
    import AVFAudio
    import Combine
    import MusicKit
#endif

private let adapterLog = Logger(subsystem: "com.bridgething.applemusic", category: "musickit")

extension AppleMusicGlue {
    static func defaultAuth() -> any AppleMusicAuthProviding {
        #if canImport(MusicKit) && os(iOS)
            MusicKitAuth()
        #else
            UnavailableSeam()
        #endif
    }

    static func defaultPlayer() -> any AppleMusicPlayerProviding {
        #if canImport(MusicKit) && os(iOS)
            MusicKitPlayer()
        #else
            UnavailableSeam()
        #endif
    }

    static func defaultLibrary() -> any AppleMusicLibraryProviding {
        #if canImport(MusicKit) && os(iOS)
            MusicKitLibrary()
        #else
            UnavailableSeam()
        #endif
    }
}

#if canImport(MusicKit) && os(iOS)

    // MARK: - shared uri -> MusicKit item resolution

    private func isLibraryId(_ id: String) -> Bool { id.hasPrefix("i.") }

    private let artSentinel = 999_999_999

    func artworkTemplate(_ artwork: Artwork?) -> String? {
        guard let artwork else { return nil }
        guard let url = artwork.url(width: artSentinel, height: artSentinel) else { return nil }
        let sentinel = "\(artSentinel)x\(artSentinel)"
        let s = url.absoluteString
        guard s.contains(sentinel) else { return s }
        return s.replacingOccurrences(of: sentinel, with: "{w}x{h}")
    }

    private enum MusicKitItems {
        static func song(_ id: String) async throws -> Song {
            if isLibraryId(id) {
                var req = MusicLibraryRequest<Song>()
                req.filter(matching: \.id, equalTo: MusicItemID(id))
                guard let song = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
                return song
            }
            let req = MusicCatalogResourceRequest<Song>(matching: \.id, equalTo: MusicItemID(id))
            guard let song = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return song
        }

        static func album(_ id: String) async throws -> Album {
            if isLibraryId(id) {
                var req = MusicLibraryRequest<Album>()
                req.filter(matching: \.id, equalTo: MusicItemID(id))
                guard let album = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
                return album
            }
            let req = MusicCatalogResourceRequest<Album>(matching: \.id, equalTo: MusicItemID(id))
            guard let album = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return album
        }

        static func playlist(_ id: String) async throws -> Playlist {
            if isLibraryId(id) {
                var req = MusicLibraryRequest<Playlist>()
                req.filter(matching: \.id, equalTo: MusicItemID(id))
                guard let playlist = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
                return playlist
            }
            let req = MusicCatalogResourceRequest<Playlist>(matching: \.id, equalTo: MusicItemID(id))
            guard let playlist = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return playlist
        }

        static func artist(_ id: String) async throws -> Artist {
            if isLibraryId(id) {
                var req = MusicLibraryRequest<Artist>()
                req.filter(matching: \.id, equalTo: MusicItemID(id))
                guard let artist = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
                return artist
            }
            let req = MusicCatalogResourceRequest<Artist>(matching: \.id, equalTo: MusicItemID(id))
            guard let artist = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return artist
        }

        static func station(_ id: String) async throws -> Station {
            let req = MusicCatalogResourceRequest<Station>(matching: \.id, equalTo: MusicItemID(id))
            guard let station = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return station
        }
    }

    // MARK: - item -> AmItem mapping

    func amItem(_ song: Song) -> AmItem {
        AmItem(
            uri: AmUri.make(.song, id: song.id.rawValue), kind: .song, title: song.title,
            artistName: song.artistName,
            albumName: song.albumTitle,
            artworkUrl: artworkTemplate(song.artwork),
            durationMs: song.duration.map { UInt32(($0 * 1000).rounded()) }
        )
    }

    func amItem(_ album: Album) -> AmItem {
        AmItem(
            uri: AmUri.make(.album, id: album.id.rawValue), kind: .album, title: album.title,
            subtitle: album.artistName,
            artistName: album.artistName,
            artworkUrl: artworkTemplate(album.artwork),
            trackCount: UInt32(album.trackCount)
        )
    }

    func amItem(_ artist: Artist) -> AmItem {
        AmItem(
            uri: AmUri.make(.artist, id: artist.id.rawValue), kind: .artist, title: artist.name,
            artworkUrl: artworkTemplate(artist.artwork)
        )
    }

    func amItem(_ playlist: Playlist) -> AmItem {
        AmItem(
            uri: AmUri.make(.playlist, id: playlist.id.rawValue), kind: .playlist, title: playlist.name,
            subtitle: playlist.curatorName,
            artworkUrl: artworkTemplate(playlist.artwork)
        )
    }

    func amItem(_ station: Station) -> AmItem {
        AmItem(
            uri: AmUri.make(.station, id: station.id.rawValue), kind: .station, title: station.name,
            artworkUrl: artworkTemplate(station.artwork)
        )
    }

    // MARK: - auth

    final class MusicKitAuth: AppleMusicAuthProviding {
        private func map(_ status: MusicAuthorization.Status) -> AmAuthStatus {
            switch status {
            case .notDetermined: .notDetermined
            case .authorized: .authorized
            case .denied: .denied
            case .restricted: .restricted
            @unknown default: .denied
            }
        }

        func currentStatus() async -> AmAuthStatus { map(MusicAuthorization.currentStatus) }
        func requestAuthorization() async -> AmAuthStatus { map(await MusicAuthorization.request()) }

        func canPlayCatalogContent() async -> Bool? {
            guard let sub = try? await MusicSubscription.current else { return nil }
            return sub.canPlayCatalogContent
        }
    }

    // MARK: - player

    final class MusicKitPlayer: AppleMusicPlayerProviding, @unchecked Sendable {
        private let player = SystemMusicPlayer.shared
        private let lock = NSLock()
        private var cancellables: [AnyCancellable] = []

        func changes() -> AsyncStream<Void> {
            AsyncStream { cont in
                let stateSub = self.player.state.objectWillChange.sink { _ in
                    Task {
                        try? await Task.sleep(for: .milliseconds(80))
                        cont.yield(())
                    }
                }
                let queueSub = self.player.queue.objectWillChange.sink { _ in
                    Task {
                        try? await Task.sleep(for: .milliseconds(80))
                        cont.yield(())
                    }
                }
                self.lock.withLock { self.cancellables = [stateSub, queueSub] }
                cont.onTermination = { [weak self] _ in
                    self?.lock.withLock { self?.cancellables.removeAll() }
                }
            }
        }

        func currentSnapshot() async -> AmPlayerSnapshot {
            let entry = player.queue.currentEntry
            let amEntry: AmEntry? = entry.map { e in
                var uri: String?
                var durationMs: UInt32?
                switch e.item {
                case let .song(song):
                    uri = AmUri.make(.song, id: song.id.rawValue)
                    durationMs = song.duration.map { UInt32(($0 * 1000).rounded()) }
                case let .musicVideo(video):
                    uri = nil
                    durationMs = video.duration.map { UInt32(($0 * 1000).rounded()) }
                default:
                    uri = nil
                }
                return AmEntry(
                    uri: uri,
                    title: e.title,
                    artistName: e.subtitle,
                    albumName: nil,
                    artworkUrl: artworkTemplate(e.artwork),
                    durationMs: durationMs
                )
            }
            let state = player.state
            let repeatMode: AmRepeatMode = switch state.repeatMode {
            case .one: .one
            case .all: .all
            default: .off
            }
            return AmPlayerSnapshot(
                entry: amEntry,
                playing: state.playbackStatus == .playing,
                positionMs: UInt32(max(0, player.playbackTime * 1000).rounded()),
                shuffle: state.shuffleMode == .songs,
                repeatMode: repeatMode
            )
        }

        func play(contextUri: String, startAtUri: String?) async throws {
            guard let parsed = AmUri.parse(contextUri) else { throw AmPlayerError.itemNotFound(contextUri) }
            let startId = startAtUri.flatMap { AmUri.parse($0)?.id }
            switch parsed.kind {
            case .song:
                let song = try await MusicKitItems.song(parsed.id)
                player.queue = [song]
            case .album:
                let album = try await MusicKitItems.album(parsed.id)
                if let startId, let tracks = try await album.with([.tracks]).tracks,
                   let start = tracks.first(where: { $0.id.rawValue == startId }) {
                    player.queue = SystemMusicPlayer.Queue(for: tracks, startingAt: start)
                } else {
                    player.queue = [album]
                }
            case .playlist:
                let playlist = try await MusicKitItems.playlist(parsed.id)
                if let startId, let tracks = try await playlist.with([.tracks]).tracks,
                   let start = tracks.first(where: { $0.id.rawValue == startId }) {
                    player.queue = SystemMusicPlayer.Queue(for: tracks, startingAt: start)
                } else {
                    player.queue = [playlist]
                }
            case .artist:
                let artist = try await MusicKitItems.artist(parsed.id)
                guard let top = try await artist.with([.topSongs]).topSongs, !top.isEmpty else {
                    throw AmPlayerError.itemNotFound(contextUri)
                }
                player.queue = SystemMusicPlayer.Queue(for: top, startingAt: nil)
            case .station:
                let station = try await MusicKitItems.station(parsed.id)
                player.queue = [station]
            }
            try await player.play()
        }

        func queueInsert(uri: String, next: Bool) async throws {
            guard let parsed = AmUri.parse(uri), parsed.kind == .song else { throw AmPlayerError.itemNotFound(uri) }
            let song = try await MusicKitItems.song(parsed.id)
            try await player.queue.insert(song, position: next ? .afterCurrentEntry : .tail)
        }

        func play() async throws { try await player.play() }
        func pause() async throws { player.pause() }
        func skipNext() async throws { try await player.skipToNextEntry() }
        func skipPrev() async throws { try await player.skipToPreviousEntry() }
        func seek(toMs ms: UInt32) async throws { player.playbackTime = TimeInterval(ms) / 1000 }
        func setShuffle(_ on: Bool) async throws { player.state.shuffleMode = on ? .songs : .off }

        func setRepeat(_ mode: AmRepeatMode) async throws {
            player.state.repeatMode = switch mode {
            case .off: MusicPlayer.RepeatMode.none
            case .all: .all
            case .one: .one
            }
        }

        func isOtherAudioPlaying() async -> Bool {
            AVAudioSession.sharedInstance().isOtherAudioPlaying
        }
    }

    // MARK: - library

    final class MusicKitLibrary: AppleMusicLibraryProviding, @unchecked Sendable {
        func libraryPlaylists(limit: UInt32, offset: UInt32) async throws -> AmPage {
            var req = MusicLibraryRequest<Playlist>()
            req.limit = Int(limit)
            req.offset = Int(offset)
            req.sort(by: \.lastPlayedDate, ascending: false)
            let res = try await req.response()
            return page(res.items.map { amItem($0) }, limit: limit)
        }

        func libraryAlbums(limit: UInt32, offset: UInt32) async throws -> AmPage {
            var req = MusicLibraryRequest<Album>()
            req.limit = Int(limit)
            req.offset = Int(offset)
            req.sort(by: \.libraryAddedDate, ascending: false)
            let res = try await req.response()
            return page(res.items.map { amItem($0) }, limit: limit)
        }

        func libraryArtists(limit: UInt32, offset: UInt32) async throws -> AmPage {
            var req = MusicLibraryRequest<Artist>()
            req.limit = Int(limit)
            req.offset = Int(offset)
            let res = try await req.response()
            return page(res.items.map { amItem($0) }, limit: limit)
        }

        func recentlyPlayed(limit: UInt32, offset: UInt32) async throws -> AmPage {
            var req = MusicRecentlyPlayedContainerRequest()
            req.limit = Int(limit)
            req.offset = Int(offset)
            let res = try await req.response()
            let items: [AmItem] = res.items.compactMap { item in
                switch item {
                case let .album(album): amItem(album)
                case let .playlist(playlist): amItem(playlist)
                case let .station(station): amItem(station)
                @unknown default: nil
                }
            }
            return page(items, limit: limit)
        }

        func recommendations() async throws -> [AmShelf] {
            let res = try await MusicPersonalRecommendationsRequest().response()
            return res.recommendations.map { rec in
                let items: [AmItem] = rec.items.compactMap { item in
                    switch item {
                    case let .album(album): amItem(album)
                    case let .playlist(playlist): amItem(playlist)
                    case let .station(station): amItem(station)
                    @unknown default: nil
                    }
                }
                return AmShelf(
                    id: rec.id.rawValue,
                    title: rec.title ?? "For You",
                    items: items,
                    total: UInt32(items.count)
                )
            }
        }

        func children(of uri: String, limit: UInt32, offset: UInt32) async throws -> AmPage {
            guard let parsed = AmUri.parse(uri) else { throw AmPlayerError.itemNotFound(uri) }
            switch parsed.kind {
            case .album:
                let album = try await MusicKitItems.album(parsed.id).with([.tracks])
                let tracks = (album.tracks ?? []).compactMap(amTrackItem)
                return slice(tracks, limit: limit, offset: offset)
            case .playlist:
                let playlist = try await MusicKitItems.playlist(parsed.id).with([.tracks])
                let tracks = (playlist.tracks ?? []).compactMap(amTrackItem)
                return slice(tracks, limit: limit, offset: offset)
            case .artist:
                let artist = try await MusicKitItems.artist(parsed.id).with([.topSongs, .albums])
                let top = (artist.topSongs ?? []).map { amItem($0) }
                let albums = (artist.albums ?? []).map { amItem($0) }
                return slice(top + albums, limit: limit, offset: offset)
            case .song, .station:
                return AmPage(items: [], total: 0, hasMore: false)
            }
        }

        private func amTrackItem(_ track: MusicKit.Track) -> AmItem? {
            guard case let .song(song) = track else { return nil }
            return amItem(song)
        }

        func resolve(uri: String) async throws -> AmItem {
            guard let parsed = AmUri.parse(uri) else { throw AmPlayerError.itemNotFound(uri) }
            switch parsed.kind {
            case .song: return try await amItem(MusicKitItems.song(parsed.id))
            case .album: return try await amItem(MusicKitItems.album(parsed.id))
            case .playlist: return try await amItem(MusicKitItems.playlist(parsed.id))
            case .artist: return try await amItem(MusicKitItems.artist(parsed.id))
            case .station: return try await amItem(MusicKitItems.station(parsed.id))
            }
        }

        func search(query: String, limit: UInt32) async throws -> AmSearchResults {
            var req = MusicCatalogSearchRequest(
                term: query, types: [Song.self, Album.self, Artist.self, Playlist.self]
            )
            req.limit = Int(limit)
            let res = try await req.response()
            return AmSearchResults(
                songs: res.songs.map { amItem($0) },
                albums: res.albums.map { amItem($0) },
                artists: res.artists.map { amItem($0) },
                playlists: res.playlists.map { amItem($0) }
            )
        }

        // MARK: favorites (Apple Music API via MusicDataRequest; MusicKit has no typed surface.

        private let storefrontLock = NSLock()
        private var cachedStorefront: String?

        func librarySongs(limit: UInt32, offset: UInt32) async throws -> AmPage {
            var req = MusicLibraryRequest<Song>()
            req.limit = Int(limit)
            req.offset = Int(offset)
            req.sort(by: \.libraryAddedDate, ascending: false)
            let res = try await req.response()
            return page(res.items.map { amItem($0) }, limit: limit)
        }

        private func storefront() async throws -> String {
            if let cached = storefrontLock.withLock({ cachedStorefront }) { return cached }
            let code = try await MusicDataRequest.currentCountryCode
            storefrontLock.withLock { cachedStorefront = code }
            return code
        }

        func isFavorite(uris: [String]) async throws -> [Bool] {
            var result: [Bool] = []
            for uri in uris {
                guard let parsed = AmUri.parse(uri), parsed.kind == .song, !isLibraryId(parsed.id) else {
                    result.append(false)
                    continue
                }
                do {
                    let sf = try await storefront()
                    var comps = URLComponents(string: "https://api.music.apple.com/v1/catalog/\(sf)/songs/\(parsed.id)")!
                    comps.queryItems = [URLQueryItem(name: "extend", value: "inFavorites")]
                    let response = try await MusicDataRequest(urlRequest: URLRequest(url: comps.url!)).response()
                    let decoded = try JSONDecoder().decode(InFavoritesResponse.self, from: response.data)
                    result.append(decoded.data.first?.attributes?.inFavorites ?? false)
                } catch {
                    result.append(false)
                }
            }
            return result
        }

        func addFavorite(uri: String) async throws {
            guard let parsed = AmUri.parse(uri), parsed.kind == .song else { throw AmPlayerError.itemNotFound(uri) }
            var comps = URLComponents(string: "https://api.music.apple.com/v1/me/favorites")!
            comps.queryItems = [URLQueryItem(name: "ids[songs]", value: parsed.id)]
            var urlRequest = URLRequest(url: comps.url!)
            urlRequest.httpMethod = "POST"
            _ = try await MusicDataRequest(urlRequest: urlRequest).response()
        }

        private struct InFavoritesResponse: Decodable {
            struct Entry: Decodable {
                struct Attributes: Decodable { let inFavorites: Bool? }
                let attributes: Attributes?
            }

            let data: [Entry]
        }

        private func page(_ items: [AmItem], limit: UInt32) -> AmPage {
            AmPage(items: items, total: nil, hasMore: items.count >= Int(limit))
        }

        private func slice(_ items: [AmItem], limit: UInt32, offset: UInt32) -> AmPage {
            let window = items.dropFirst(Int(offset)).prefix(Int(limit))
            return AmPage(
                items: Array(window),
                total: UInt32(items.count),
                hasMore: Int(offset) + window.count < items.count
            )
        }
    }

#else

    final class UnavailableSeam: AppleMusicAuthProviding, AppleMusicPlayerProviding, AppleMusicLibraryProviding {
        func currentStatus() async -> AmAuthStatus { .restricted }
        func requestAuthorization() async -> AmAuthStatus { .restricted }
        func canPlayCatalogContent() async -> Bool? { false }

        func changes() -> AsyncStream<Void> { AsyncStream { $0.finish() } }
        func currentSnapshot() async -> AmPlayerSnapshot {
            AmPlayerSnapshot(entry: nil, playing: false, positionMs: 0, shuffle: false, repeatMode: .off)
        }

        func play(contextUri _: String, startAtUri _: String?) async throws { throw AmPlayerError.unavailable }
        func queueInsert(uri _: String, next _: Bool) async throws { throw AmPlayerError.unavailable }
        func play() async throws { throw AmPlayerError.unavailable }
        func pause() async throws { throw AmPlayerError.unavailable }
        func skipNext() async throws { throw AmPlayerError.unavailable }
        func skipPrev() async throws { throw AmPlayerError.unavailable }
        func seek(toMs _: UInt32) async throws { throw AmPlayerError.unavailable }
        func setShuffle(_: Bool) async throws { throw AmPlayerError.unavailable }
        func setRepeat(_: AmRepeatMode) async throws { throw AmPlayerError.unavailable }
        func isOtherAudioPlaying() async -> Bool { false }

        func libraryPlaylists(limit _: UInt32, offset _: UInt32) async throws -> AmPage { throw AmPlayerError.unavailable }
        func libraryAlbums(limit _: UInt32, offset _: UInt32) async throws -> AmPage { throw AmPlayerError.unavailable }
        func libraryArtists(limit _: UInt32, offset _: UInt32) async throws -> AmPage { throw AmPlayerError.unavailable }
        func recentlyPlayed(limit _: UInt32, offset _: UInt32) async throws -> AmPage { throw AmPlayerError.unavailable }
        func recommendations() async throws -> [AmShelf] { throw AmPlayerError.unavailable }
        func children(of _: String, limit _: UInt32, offset _: UInt32) async throws -> AmPage { throw AmPlayerError.unavailable }
        func resolve(uri _: String) async throws -> AmItem { throw AmPlayerError.unavailable }
        func search(query _: String, limit _: UInt32) async throws -> AmSearchResults { throw AmPlayerError.unavailable }
        func librarySongs(limit _: UInt32, offset _: UInt32) async throws -> AmPage { throw AmPlayerError.unavailable }
        func isFavorite(uris: [String]) async throws -> [Bool] { uris.map { _ in false } }
        func addFavorite(uri _: String) async throws { throw AmPlayerError.unavailable }
    }

#endif
