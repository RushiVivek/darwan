import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Mpris

// What is playing, and its lyrics, for themes as `darwan.nowPlaying` (docs/theme-contract.md, "Now playing").
// Lyrics: a .lrc beside the track or its tags, else the player's own text, else lrclib.net (all through
// `darwan lyrics`, which caches what it finds online).
QtObject {
    id: np

    // The player to follow: the one playing; else, of those paused, a music player's track before a browser tab's
    // ("Firefox is playing media", no artist): a local file first, then any track with an artist, then the rest.
    function rank(p) {
        const url = p.metadata ? String(p.metadata["xesam:url"] || "") : ""
        return (p.isPlaying ? 8 : 0) + (url.startsWith("file://") ? 4 : 0) + (p.trackArtist !== "" ? 2 : 0) + (p.trackTitle !== "" ? 1 : 0)
    }
    readonly property var player: {
        let best = null
        for (const p of Mpris.players.values)
            if (rank(p) > 0 && (best === null || rank(p) > rank(best)))
                best = p
        return best
    }
    readonly property bool active: player !== null && player.trackTitle !== ""
    readonly property bool playing: player !== null && player.isPlaying
    readonly property string title: player ? player.trackTitle : ""
    readonly property string artist: player ? player.trackArtist : ""
    readonly property string album: player ? player.trackAlbum : ""
    readonly property string url: player && player.metadata ? String(player.metadata["xesam:url"] || "") : ""
    // Seconds; 0 when the player doesn't say.
    readonly property real length: player && player.lengthSupported ? player.length : 0
    readonly property string playerText: player && player.metadata ? String(player.metadata["xesam:asText"] || "") : ""

    // [{ t: ms or null, text }]; `synced` when the lines carry times.
    property var lines: []
    property bool synced: false
    // Seconds, read from the player while it plays (MPRIS doesn't announce the position as it moves).
    property real position: 0
    // 0 to 1 through the track; 0 when its length is unknown.
    readonly property real progress: length > 0 ? Math.min(1, Math.max(0, position / length)) : 0
    // The line being sung, -1 before the first or without times.
    readonly property int index: {
        if (!synced)
            return -1
        const ms = position * 1000
        let i = -1
        for (let k = 0; k < lines.length && lines[k].t <= ms; k++)
            i = k
        return i
    }
    readonly property string current: index >= 0 ? lines[index].text : ""
    readonly property string next: synced && index + 1 < lines.length ? lines[index + 1].text : ""

    function plainFromPlayer() {
        lines = playerText === "" ? [] : playerText.split("\n").map(l => ({ t: null, text: l.trim() }))
        synced = false
    }

    function readPosition() {
        if (!player)
            return
        player.positionChanged()
        position = player.position
    }

    // The same track reported twice (title, then url a moment later) asks once.
    property string asked: ""
    function load() {
        const key = [title, artist, album, url].join("\u0001")
        if (key === asked)
            return
        asked = key
        lines = []
        synced = false
        readPosition()
        if (!active)
            return
        fetch.running = false
        fetch.command = [Quickshell.env("DARWAN_BIN") || "darwan", "lyrics", url,
                         "--title", title, "--artist", artist, "--album", album, "--length", String(length)]
                        .concat(playerText !== "" ? ["--player-has-lyrics"] : [])
        fetch.running = true
    }

    onUrlChanged: Qt.callLater(load)
    onTitleChanged: Qt.callLater(load)
    Component.onCompleted: load()

    property Process fetch: Process {
        stdout: StdioCollector {
            onStreamFinished: {
                let got = null
                try { got = JSON.parse(text) } catch (e) {}
                if (got && got.lines && got.lines.length > 0) {
                    np.lines = got.lines
                    np.synced = got.synced === true
                } else {
                    np.plainFromPlayer()
                }
                // Paused, the tick doesn't run: start from where the track stands.
                np.readPosition()
            }
        }
    }

    // Twice a second is close enough for a line of lyrics and a progress bar, and only while playing.
    property Timer tick: Timer {
        interval: 500
        repeat: true
        running: np.playing
        triggeredOnStart: true
        onTriggered: np.readPosition()
    }
}
