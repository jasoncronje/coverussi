import SwiftUI

@main
struct CoverussiApp: App {
    @State private var config: Config
    @State private var board: Board

    init() {
        let config = Config()
        _config = State(initialValue: config)
        _board = State(initialValue: Board(config: config))
    }

    var body: some Scene {
        WindowGroup {
            RootView(config: config, board: board)
        }
    }
}

struct RootView: View {
    @Bindable var config: Config
    let board: Board
    @State private var setup = false
    @Environment(\.scenePhase) private var phase

    var body: some View {
        NowPlayingView(board: board, onSettings: { setup = true })
            // Connect or Menu, the board is followed at whatever address the sheet was left with.
            .sheet(isPresented: $setup, onDismiss: { if config.isSet { board.start() } }) {
                SetupView(config: config) { setup = false }
            }
            .onAppear {
                if config.isSet { board.start() } else { setup = true }
            }
            .onChange(of: phase) { _, phase in
                if phase == .active, config.isSet { board.start() }
                if phase == .background { board.stop() }
            }
            // A now-playing screen that falls asleep isn't one.
            .onChange(of: board.showing != nil, initial: true) { _, playing in
                UIApplication.shared.isIdleTimerDisabled = playing
            }
    }
}
