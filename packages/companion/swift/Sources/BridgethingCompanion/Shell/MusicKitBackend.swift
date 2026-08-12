#if canImport(MusicKit)

    import BridgethingCompanionCore
    import Foundation
    import MusicKit

    #if os(iOS)
        import AVFAudio
        import Combine
    #endif

    enum AmPlayerError: Error, CustomStringConvertible {
        case unavailable
        case itemNotFound(String)

        var description: String {
            switch self {
            case .unavailable: return "apple music playback is unavailable on this platform"
            case let .itemNotFound(id): return "apple music item not found: \(id)"
            }
        }
    }

    enum AmUri {
        static func kindName(_ kind: AmKind) -> String {
            switch kind {
            case .song: return "song"
            case .album: return "album"
            case .playlist: return "playlist"
            case .artist: return "artist"
            case .station: return "station"
            }
        }

        static func kind(named name: String) -> AmKind? {
            switch name {
            case "song": return .song
            case "album": return .album
            case "playlist": return .playlist
            case "artist": return .artist
            case "station": return .station
            default: return nil
            }
        }

        static func make(_ kind: AmKind, id: String) -> String {
            "applemusic:\(kindName(kind)):\(id)"
        }

        static func parse(_ uri: String) -> (kind: AmKind, id: String)? {
            let prefix = "applemusic:"
            guard uri.hasPrefix(prefix) else { return nil }
            let rest = uri.dropFirst(prefix.count)
            guard let colon = rest.firstIndex(of: ":"), let kind = kind(named: String(rest[..<colon])) else { return nil }
            let id = String(rest[rest.index(after: colon)...])
            return id.isEmpty ? nil : (kind, id)
        }
    }

    func isLibraryId(_ id: String) -> Bool {
        let head = Array(id.prefix(3))
        return head.count == 3 && head[0].isLetter && head[1] == "."
    }

    // MARK: - artwork url templating

    private let artSentinel = 999_999_999

    func artworkTemplate(_ artwork: Artwork?) -> String? {
        guard let artwork else { return nil }
        guard let url = artwork.url(width: artSentinel, height: artSentinel) else { return nil }
        let sentinel = "\(artSentinel)x\(artSentinel)"
        let s = url.absoluteString
        guard s.contains(sentinel) else { return s }
        return s.replacingOccurrences(of: sentinel, with: "{w}x{h}")
    }

    // MARK: - shared uri -> MusicKit item resolution

    private enum MusicKitItems {
        static func resolve<T>(
            _ id: String,
            catalogFilter: KeyPath<T.FilterType, MusicItemID>,
            libraryFilter: KeyPath<T.LibraryFilter, MusicItemID>
        ) async throws -> T where T: FilterableMusicItem & MusicLibraryRequestable & Decodable {
            if isLibraryId(id) {
                var req = MusicLibraryRequest<T>()
                req.filter(matching: libraryFilter, equalTo: MusicItemID(id))
                guard let item = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
                return item
            }
            let req = MusicCatalogResourceRequest<T>(matching: catalogFilter, equalTo: MusicItemID(id))
            guard let item = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return item
        }

        static func song(_ id: String) async throws -> Song {
            try await resolve(id, catalogFilter: \.id, libraryFilter: \.id)
        }

        static func album(_ id: String) async throws -> Album {
            try await resolve(id, catalogFilter: \.id, libraryFilter: \.id)
        }

        static func playlist(_ id: String) async throws -> Playlist {
            try await resolve(id, catalogFilter: \.id, libraryFilter: \.id)
        }

        static func artist(_ id: String) async throws -> Artist {
            try await resolve(id, catalogFilter: \.id, libraryFilter: \.id)
        }

        static func station(_ id: String) async throws -> Station {
            let req = MusicCatalogResourceRequest<Station>(matching: \.id, equalTo: MusicItemID(id))
            guard let station = try await req.response().items.first else { throw AmPlayerError.itemNotFound(id) }
            return station
        }
    }

    // MARK: - item -> AmItem mapping

    private func amItem(_ song: Song) -> AmItem {
        AmItem(
            uri: AmUri.make(.song, id: song.id.rawValue), kind: .song, title: song.title,
            subtitle: nil,
            artistName: song.artistName, artistUri: nil,
            albumName: song.albumTitle, albumUri: nil,
            artworkUrl: artworkTemplate(song.artwork),
            durationMs: song.duration.map { UInt32(($0 * 1000).rounded()) },
            trackCount: nil
        )
    }

    private func amItem(_ album: Album) -> AmItem {
        AmItem(
            uri: AmUri.make(.album, id: album.id.rawValue), kind: .album, title: album.title,
            subtitle: album.artistName,
            artistName: album.artistName, artistUri: nil,
            albumName: nil, albumUri: nil,
            artworkUrl: artworkTemplate(album.artwork),
            durationMs: nil,
            trackCount: UInt32(album.trackCount)
        )
    }

    private func amItem(_ artist: Artist) -> AmItem {
        AmItem(
            uri: AmUri.make(.artist, id: artist.id.rawValue), kind: .artist, title: artist.name,
            subtitle: nil, artistName: nil, artistUri: nil, albumName: nil, albumUri: nil,
            artworkUrl: artworkTemplate(artist.artwork), durationMs: nil, trackCount: nil
        )
    }

    private func amItem(_ playlist: Playlist) -> AmItem {
        AmItem(
            uri: AmUri.make(.playlist, id: playlist.id.rawValue), kind: .playlist, title: playlist.name,
            subtitle: playlist.curatorName, artistName: nil, artistUri: nil, albumName: nil, albumUri: nil,
            artworkUrl: artworkTemplate(playlist.artwork), durationMs: nil, trackCount: nil
        )
    }

    private func amItem(_ station: Station) -> AmItem {
        AmItem(
            uri: AmUri.make(.station, id: station.id.rawValue), kind: .station, title: station.name,
            subtitle: nil, artistName: nil, artistUri: nil, albumName: nil, albumUri: nil,
            artworkUrl: artworkTemplate(station.artwork), durationMs: nil, trackCount: nil
        )
    }

    public final class MusicKitBackend: AppleMusicBackend, @unchecked Sendable {
        private let lock = NSLock()
        private var observeTask: Task<Void, Never>?

        public init() {}

        // MARK: - player observation

        public func start(inbox: AmPlayerInbox) {
            stop()
            #if os(iOS)
                let (stream, cont) = AsyncStream<Void>.makeStream()
                Self.observePlayer(into: cont)
                let task = Task {
                    await withTaskCancellationHandler {
                        for await _ in stream {
                            if Task.isCancelled { break }
                            inbox.onChanged()
                        }
                    } onCancel: {
                        cont.finish()
                    }
                }
                lock.lock()
                observeTask = task
                lock.unlock()
            #endif
        }

        public func stop() {
            lock.lock()
            let held = observeTask
            observeTask = nil
            lock.unlock()
            held?.cancel()
        }

        #if os(iOS)
            private static func observePlayer(into cont: AsyncStream<Void>.Continuation) {
                let player = SystemMusicPlayer.shared
                let stateSub = player.state.objectWillChange.sink { _ in
                    Task {
                        try? await Task.sleep(for: .milliseconds(80))
                        cont.yield(())
                    }
                }
                let queueSub = player.queue.objectWillChange.sink { _ in
                    Task {
                        try? await Task.sleep(for: .milliseconds(80))
                        cont.yield(())
                    }
                }
                nonisolated(unsafe) let box = [stateSub, queueSub]
                cont.onTermination = { _ in _ = box }
            }
        #endif

        // MARK: - snapshot

        public func snapshot(sink: AmSnapshotSink) {
            #if os(iOS)
                Task { @MainActor in
                    let player = SystemMusicPlayer.shared
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
                    sink.complete(value: AmPlayerSnapshot(
                        entry: amEntry,
                        playing: state.playbackStatus == .playing,
                        positionMs: UInt32(max(0, player.playbackTime * 1000).rounded()),
                        shuffle: state.shuffleMode == .songs,
                        repeat: repeatMode,
                        canSeek: true
                    ))
                }
            #else
                sink.complete(value: AmPlayerSnapshot(
                    entry: nil, playing: false, positionMs: 0, shuffle: false, repeat: .off, canSeek: true
                ))
            #endif
        }

        // MARK: - auth

        private static func mapAuth(_ status: MusicAuthorization.Status) -> AmAuthStatus {
            switch status {
            case .notDetermined: return .notDetermined
            case .authorized: return .authorized
            case .denied: return .denied
            case .restricted: return .restricted
            @unknown default: return .denied
            }
        }

        public func authStatus(sink: AmAuthSink) {
            sink.complete(value: Self.mapAuth(MusicAuthorization.currentStatus))
        }

        public func requestAuthorization(sink: AmAuthSink) {
            Task {
                sink.complete(value: Self.mapAuth(await MusicAuthorization.request()))
            }
        }

        public func canPlayCatalogContent(sink: AmCatalogSink) {
            Task {
                guard let sub = try? await MusicSubscription.current else {
                    sink.complete(value: nil)
                    return
                }
                sink.complete(value: sub.canPlayCatalogContent)
            }
        }

        public func isOtherAudioPlaying(sink: AmFlagSink) {
            #if os(iOS)
                sink.complete(value: AVAudioSession.sharedInstance().isOtherAudioPlaying)
            #else
                sink.complete(value: false)
            #endif
        }

        // MARK: - playback

        public func playContext(contextUri: String, startAtUri: String?, sink: AmActionSink) {
            #if os(iOS)
                Task {
                    do {
                        try await Self.playContext(contextUri: contextUri, startAtUri: startAtUri)
                        sink.ok()
                    } catch {
                        sink.fail(reason: String(describing: error))
                    }
                }
            #else
                sink.fail(reason: AmPlayerError.unavailable.description)
            #endif
        }

        public func queueInsert(uri: String, next: Bool, sink: AmActionSink) {
            #if os(iOS)
                Task {
                    do {
                        guard let parsed = AmUri.parse(uri), parsed.kind == .song else {
                            throw AmPlayerError.itemNotFound(uri)
                        }
                        let song = try await MusicKitItems.song(parsed.id)
                        try await SystemMusicPlayer.shared.queue.insert(song, position: next ? .afterCurrentEntry : .tail)
                        sink.ok()
                    } catch {
                        sink.fail(reason: String(describing: error))
                    }
                }
            #else
                sink.fail(reason: AmPlayerError.unavailable.description)
            #endif
        }

        public func command(cmd: AmPlayerCommand, sink: AmActionSink) {
            #if os(iOS)
                Task {
                    do {
                        let player = SystemMusicPlayer.shared
                        switch cmd {
                        case .play: try await player.play()
                        case .pause: player.pause()
                        case .skipNext: try await player.skipToNextEntry()
                        case .skipPrev: try await player.skipToPreviousEntry()
                        case let .seekTo(positionMs): player.playbackTime = TimeInterval(positionMs) / 1000
                        case let .setShuffle(on): player.state.shuffleMode = on ? .songs : .off
                        case let .setRepeat(mode):
                            player.state.repeatMode = switch mode {
                            case .off: MusicPlayer.RepeatMode.none
                            case .all: .all
                            case .one: .one
                            }
                        }
                        sink.ok()
                    } catch {
                        sink.fail(reason: String(describing: error))
                    }
                }
            #else
                sink.fail(reason: AmPlayerError.unavailable.description)
            #endif
        }

        #if os(iOS)
            private static func playContext(contextUri: String, startAtUri: String?) async throws {
                guard let parsed = AmUri.parse(contextUri) else { throw AmPlayerError.itemNotFound(contextUri) }
                let startId = startAtUri.flatMap { AmUri.parse($0)?.id }
                let player = SystemMusicPlayer.shared
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
                    if isLibraryId(parsed.id) {
                        guard let albums = try await artist.with([.albums]).albums, !albums.isEmpty else {
                            throw AmPlayerError.itemNotFound(contextUri)
                        }
                        player.queue = SystemMusicPlayer.Queue(for: albums, startingAt: nil)
                    } else {
                        guard let top = try await artist.with([.topSongs]).topSongs, !top.isEmpty else {
                            throw AmPlayerError.itemNotFound(contextUri)
                        }
                        player.queue = SystemMusicPlayer.Queue(for: top, startingAt: nil)
                    }
                case .station:
                    let station = try await MusicKitItems.station(parsed.id)
                    player.queue = [station]
                }
                try await player.play()
            }
        #endif

        // MARK: - library

        public func library(scope: AmLibraryScope, limit: UInt32, offset: UInt32, sink: AmPageSink) {
            Task {
                do {
                    let page: AmPage
                    switch scope {
                    case .playlists:
                        var req = MusicLibraryRequest<Playlist>()
                        req.limit = Int(limit)
                        req.offset = Int(offset)
                        #if os(iOS)
                            req.sort(by: \.lastPlayedDate, ascending: false)
                        #endif
                        let res = try await req.response()
                        page = Self.page(res.items.map { amItem($0) }, limit: limit)
                    case .albums:
                        var req = MusicLibraryRequest<Album>()
                        req.limit = Int(limit)
                        req.offset = Int(offset)
                        #if os(iOS)
                            req.sort(by: \.libraryAddedDate, ascending: false)
                        #endif
                        let res = try await req.response()
                        page = Self.page(res.items.map { amItem($0) }, limit: limit)
                    case .artists:
                        var req = MusicLibraryRequest<Artist>()
                        req.limit = Int(limit)
                        req.offset = Int(offset)
                        let res = try await req.response()
                        page = Self.page(res.items.map { amItem($0) }, limit: limit)
                    case .songs:
                        var req = MusicLibraryRequest<Song>()
                        req.limit = Int(limit)
                        req.offset = Int(offset)
                        #if os(iOS)
                            req.sort(by: \.libraryAddedDate, ascending: false)
                        #endif
                        let res = try await req.response()
                        page = Self.page(res.items.map { amItem($0) }, limit: limit)
                    case .recentlyPlayed:
                        var req = MusicRecentlyPlayedContainerRequest()
                        req.limit = Int(limit)
                        req.offset = Int(offset)
                        let res = try await req.response()
                        let items: [AmItem] = res.items.compactMap { item in
                            switch item {
                            case let .album(album): return amItem(album)
                            case let .playlist(playlist): return amItem(playlist)
                            case let .station(station): return amItem(station)
                            @unknown default: return nil
                            }
                        }
                        page = Self.page(items, limit: limit)
                    case let .children(uri):
                        page = try await Self.children(of: uri, limit: limit, offset: offset)
                    }
                    sink.complete(value: page)
                } catch {
                    sink.fail(reason: String(describing: error))
                }
            }
        }

        private static func children(of uri: String, limit: UInt32, offset: UInt32) async throws -> AmPage {
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
                if isLibraryId(parsed.id) {
                    let artist = try await MusicKitItems.artist(parsed.id).with([.albums])
                    return slice((artist.albums ?? []).map { amItem($0) }, limit: limit, offset: offset)
                }
                let artist = try await MusicKitItems.artist(parsed.id).with([.topSongs, .albums])
                let top = (artist.topSongs ?? []).map { amItem($0) }
                let albums = (artist.albums ?? []).map { amItem($0) }
                return slice(top + albums, limit: limit, offset: offset)
            case .song, .station:
                return AmPage(items: [], total: 0, hasMore: false)
            }
        }

        private static func amTrackItem(_ track: MusicKit.Track) -> AmItem? {
            guard case let .song(song) = track else { return nil }
            return amItem(song)
        }

        public func recommendations(sink: AmShelvesSink) {
            Task {
                do {
                    let res = try await MusicPersonalRecommendationsRequest().response()
                    let shelves = res.recommendations.map { rec in
                        let items: [AmItem] = rec.items.compactMap { item in
                            switch item {
                            case let .album(album): return amItem(album)
                            case let .playlist(playlist): return amItem(playlist)
                            case let .station(station): return amItem(station)
                            @unknown default: return nil
                            }
                        }
                        return AmShelf(
                            id: rec.id.rawValue,
                            title: rec.title ?? "For You",
                            items: items,
                            total: UInt32(items.count)
                        )
                    }
                    sink.complete(value: shelves)
                } catch {
                    sink.fail(reason: String(describing: error))
                }
            }
        }

        public func resolve(uri: String, sink: AmItemSink) {
            Task {
                do {
                    guard let parsed = AmUri.parse(uri) else { throw AmPlayerError.itemNotFound(uri) }
                    let item: AmItem
                    switch parsed.kind {
                    case .song: item = try await amItem(MusicKitItems.song(parsed.id))
                    case .album: item = try await amItem(MusicKitItems.album(parsed.id))
                    case .playlist: item = try await amItem(MusicKitItems.playlist(parsed.id))
                    case .artist: item = try await amItem(MusicKitItems.artist(parsed.id))
                    case .station: item = try await amItem(MusicKitItems.station(parsed.id))
                    }
                    sink.complete(value: item)
                } catch {
                    sink.fail(reason: String(describing: error))
                }
            }
        }

        public func search(query: String, limit: UInt32, sink: AmSearchSink) {
            Task {
                do {
                    var req = MusicCatalogSearchRequest(
                        term: query, types: [Song.self, Album.self, Artist.self, Playlist.self]
                    )
                    req.limit = Int(limit)
                    let res = try await req.response()
                    sink.complete(value: AmSearchResults(
                        songs: res.songs.map { amItem($0) },
                        albums: res.albums.map { amItem($0) },
                        artists: res.artists.map { amItem($0) },
                        playlists: res.playlists.map { amItem($0) }
                    ))
                } catch {
                    sink.fail(reason: String(describing: error))
                }
            }
        }

        // MARK: - favorites

        private let storefrontLock = NSLock()
        private var cachedStorefront: String?

        private func storefront() async throws -> String {
            if let cached = storefrontLock.withLock({ cachedStorefront }) { return cached }
            let code = try await MusicDataRequest.currentCountryCode
            storefrontLock.withLock { cachedStorefront = code }
            return code
        }

        public func isFavorite(uris: [String], sink: AmFavoritesSink) {
            Task {
                let catalogIds = uris.map { uri -> String? in
                    guard let parsed = AmUri.parse(uri), parsed.kind == .song, !isLibraryId(parsed.id) else { return nil }
                    return parsed.id
                }
                let wanted = Array(Set(catalogIds.compactMap { $0 }))
                var byId: [String: Bool] = [:]
                for chunk in stride(from: 0, to: wanted.count, by: Self.favoritesBatchSize) {
                    let ids = Array(wanted[chunk ..< min(chunk + Self.favoritesBatchSize, wanted.count)])
                    for (id, favorite) in await favoriteFlags(ids: ids) { byId[id] = favorite }
                }
                sink.complete(value: catalogIds.map { id in id.flatMap { byId[$0] } ?? false })
            }
        }

        private func favoriteFlags(ids: [String]) async -> [String: Bool] {
            do {
                let sf = try await storefront()
                var comps = URLComponents(string: "https://api.music.apple.com/v1/catalog/\(sf)/songs")!
                comps.queryItems = [
                    URLQueryItem(name: "ids", value: ids.joined(separator: ",")),
                    URLQueryItem(name: "extend", value: "inFavorites"),
                ]
                let response = try await MusicDataRequest(urlRequest: URLRequest(url: comps.url!)).response()
                let decoded = try JSONDecoder().decode(InFavoritesResponse.self, from: response.data)
                return decoded.data.reduce(into: [:]) { acc, entry in
                    acc[entry.id] = entry.attributes?.inFavorites ?? false
                }
            } catch {
                return [:]
            }
        }

        public func addFavorite(uri: String, sink: AmActionSink) {
            Task {
                do {
                    guard let parsed = AmUri.parse(uri), parsed.kind == .song else {
                        throw AmPlayerError.itemNotFound(uri)
                    }
                    var comps = URLComponents(string: "https://api.music.apple.com/v1/me/favorites")!
                    comps.queryItems = [URLQueryItem(name: "ids[songs]", value: parsed.id)]
                    var urlRequest = URLRequest(url: comps.url!)
                    urlRequest.httpMethod = "POST"
                    _ = try await MusicDataRequest(urlRequest: urlRequest).response()
                    sink.ok()
                } catch {
                    sink.fail(reason: String(describing: error))
                }
            }
        }

        private static let favoritesBatchSize = 300

        private struct InFavoritesResponse: Decodable {
            struct Entry: Decodable {
                struct Attributes: Decodable { let inFavorites: Bool? }
                let id: String
                let attributes: Attributes?
            }

            let data: [Entry]
        }

        // MARK: - paging helpers

        private static func page(_ items: [AmItem], limit: UInt32) -> AmPage {
            AmPage(items: items, total: nil, hasMore: items.count >= Int(limit))
        }

        private static func slice(_ items: [AmItem], limit: UInt32, offset: UInt32) -> AmPage {
            let window = items.dropFirst(Int(offset)).prefix(Int(limit))
            return AmPage(
                items: Array(window),
                total: UInt32(items.count),
                hasMore: Int(offset) + window.count < items.count
            )
        }
    }

#endif
