pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import org.darwan

// Wallpapers in big cards, three or four across, asking for more near the end. The model changes in place, so the
// grid keeps its place while pictures arrive.
GridView {
    id: grid

    required property WallFeed feed
    property string mode: "online"
    readonly property bool loading: feed.info.loading === true
    readonly property bool more: feed.info.more === true
    readonly property int columns: Math.max(2, Math.floor((width + 20) / (400 + 20)))

    signal open(var item, int index)
    signal wantMore()

    cellWidth: width / columns
    cellHeight: cellWidth * 9 / 16 + 20
    model: feed.model
    boundsBehavior: Flickable.StopAtBounds
    // Rows made a screen ahead, so their pictures are decoding before they scroll in.
    cacheBuffer: height
    reuseItems: true
    clip: true
    SmoothWheel { view: grid; step: grid.cellHeight }
    // While more are coming, stand-ins hold the slots they will fill: the rest of the last row and one more, or the
    // screen when there is nothing yet. Real cards take those slots as they arrive, so nothing moves.
    readonly property int waiting: !loading ? 0
        : count === 0 ? columns * Math.max(2, Math.ceil(height / cellHeight))
        : (columns - count % columns) % columns + columns
    // One clock for every stand-in's light band, in widths of the grid: it crosses, then rests.
    property real sweep: 0
    NumberAnimation on sweep {
        running: grid.waiting > 0 && grid.visible
        loops: Animation.Infinite
        from: -0.3
        to: 1.9
        duration: 2400
    }
    // Only while scrolling, as wallspace shows it.
    ScrollIndicator.vertical: ScrollIndicator {}
    // Near the end, the next 25.
    onContentYChanged: if (more && !loading && contentHeight > 0 && contentY + height > contentHeight - cellHeight * 2) wantMore()

    delegate: Item {
        id: cell
        required property int index
        required property string json
        width: grid.cellWidth
        height: grid.cellHeight
        WallCard {
            x: 10
            width: cell.width - 20
            height: width * 9 / 16
            item: JSON.parse(cell.json)
            mode: grid.mode
            backend: grid.feed.backend
            onOpen: grid.open(grid.feed.items[cell.index], cell.index)
        }
    }
    footer: Item {
        width: grid.width
        // Rows of stand-ins below the last real one; the first may share that row.
        height: (Math.ceil((grid.count + grid.waiting) / grid.columns) - Math.ceil(grid.count / grid.columns)) * grid.cellHeight + 80
        Repeater {
            model: grid.waiting
            delegate: RoundedImage {
                required property int index
                readonly property int slot: grid.count + index
                x: slot % grid.columns * grid.cellWidth + 10
                y: (Math.floor(slot / grid.columns) - Math.ceil(grid.count / grid.columns)) * grid.cellHeight
                width: grid.cellWidth - 20
                height: width * 9 / 16
                placeholder: true
                shining: true
                band: (grid.sweep * grid.width - x - 0.35 * y) / width
            }
        }
    }
}
