import QtQuick

// A picture with rounded corners, cropped to fill, drawn by one shader straight from the image's texture: no
// offscreen layers, so a grid of them scrolls cheaply. It fades in once the picture is decoded, over a blurred
// stand-in when `placeholder` is set.
Item {
    id: rounded

    property alias source: picture.source
    property alias sourceSize: picture.sourceSize
    property alias status: picture.status
    property real radius: 16
    property bool fade: true
    property bool placeholder: false
    // The picture's colours, "#rrggbb", the most common first; without them the stand-in is a soft grey.
    property var swatches: []
    // A light band across the stand-in at this point (0 to 1 over the width), while `shining`; views share one clock.
    property bool shining: false
    property real band: 0

    Image {
        id: picture
        visible: false
        asynchronous: true
        smooth: true
    }

    // The same shader in its stand-in mode, kept until the picture has fully faded in over it.
    ShaderEffect {
        anchors.fill: parent
        visible: rounded.placeholder && (picture.status !== Image.Ready || appear.running)
        readonly property Image source: picture
        readonly property size size: Qt.size(width, height)
        readonly property real radius: Math.min(rounded.radius, width / 2, height / 2)
        readonly property vector4d uvRect: Qt.vector4d(0, 0, 1, 1)
        readonly property real blur: 1
        readonly property real shine: rounded.shining ? 1 : 0
        readonly property real band: rounded.band
        readonly property var tones: {
            const s = rounded.swatches || []
            if (!s.length)
                return [Qt.rgba(1, 1, 1, 0.075), Qt.rgba(1, 1, 1, 0.11), Qt.rgba(1, 1, 1, 0.045), Qt.rgba(1, 1, 1, 0.05), Qt.rgba(1, 1, 1, 0.1)]
            return [0, 1, 2, 3, 4].map(i => Qt.darker(s[i % s.length], 1.15))
        }
        readonly property color c0: tones[0]
        readonly property color c1: tones[1]
        readonly property color c2: tones[2]
        readonly property color c3: tones[3]
        readonly property color c4: tones[4]
        fragmentShader: "rounded.frag.qsb"
    }

    ShaderEffect {
        id: effect
        anchors.fill: parent
        visible: picture.status === Image.Ready
        opacity: rounded.fade ? 0 : 1
        readonly property Image source: picture
        readonly property size size: Qt.size(width, height)
        readonly property real radius: Math.min(rounded.radius, width / 2, height / 2)
        // Crop to fill: the part of the texture with the item's shape, centred.
        readonly property vector4d uvRect: {
            const iw = picture.implicitWidth, ih = picture.implicitHeight
            if (iw <= 0 || ih <= 0 || width <= 0 || height <= 0)
                return Qt.vector4d(0, 0, 1, 1)
            const scale = Math.max(width / iw, height / ih)
            const sw = width / (iw * scale), sh = height / (ih * scale)
            return Qt.vector4d((1 - sw) / 2, (1 - sh) / 2, sw, sh)
        }
        // The stand-in's uniforms, set so the shader never reads what another card left in them.
        readonly property real blur: 0
        readonly property real shine: 0
        readonly property real band: 0
        readonly property color c0: "transparent"
        readonly property color c1: "transparent"
        readonly property color c2: "transparent"
        readonly property color c3: "transparent"
        readonly property color c4: "transparent"
        fragmentShader: "rounded.frag.qsb"
        onVisibleChanged: if (visible && rounded.fade) appear.restart()
        OpacityAnimator {
            id: appear
            target: effect
            from: 0
            to: 1
            duration: 260
            easing.type: Easing.OutCubic
        }
    }
}
