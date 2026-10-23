import QtQuick
import QtQuick.Effects
import QtMultimedia
import Qt.labs.folderlistmodel
import "darwan"

// Rei: the video rests blurred behind a big clock; the first key or click clears the blur and brings
// in the login. Two layouts (theme option `layout`): "rise" lifts the clock to the top-left with the
// login on the left; "corners" sinks it to the bottom-left with the login at the bottom-right.
Item {
    id: root
    width: 1920
    height: 1080

    Custom { id: kit }
    Ambient { id: saver; duration: kit.dur(700) }

    readonly property real s: Screen.height / 1080
    readonly property string clockFmt: config.clockFormat === "12h" ? (config.clockShowAmPm === "true" ? "h:mm AP" : "h:mm") : "HH:mm"
    // Under SDDM hostName is set; the lockscreen has no session to pick.
    readonly property bool isSddm: typeof sddm !== "undefined" && sddm.hostName !== undefined
    readonly property bool corners: config.layout === "corners"
    readonly property int blurMax: Math.max(0, Math.min(64, parseInt(config.blur)))

    readonly property color textColor: kit.color("text", "#c3c6fb")
    readonly property color mutedColor: kit.color("colorMuted", "#8c90c2")
    readonly property color accent: kit.color("accent", "#e4e6ff")

    FolderListModel { id: fontFolder; showDirs: false; folder: Qt.resolvedUrl("font"); nameFilters: ["*.ttf", "*.otf"] }
    FontLoader { id: mainFont; source: fontFolder.count > 0 ? "font/" + fontFolder.get(0, "fileName") : "" }
    readonly property string textFont: kit.font("text", mainFont.status === FontLoader.Ready ? mainFont.name : "sans-serif")
    readonly property string clockFont: kit.font("clock", mainFont.status === FontLoader.Ready ? mainFont.name : "sans-serif")

    // ── The two stages ──
    // Resting until the first key or click; back to resting on Esc, after a quiet spell with an empty
    // field, or whenever the screensaver goes ambient (it reveals on input, which wakes us too).
    property bool awake: false
    // 0 = resting (blurred, clock only), 1 = awake (sharp, login). Everything that moves follows it.
    property real stage: awake && !saver.active ? 1 : 0
    Behavior on stage { NumberAnimation { duration: kit.dur(800); easing.type: kit.ease(Easing.OutCubic) } }
    // The login fades in over the second half of the move, so the clock leads.
    readonly property real loginShown: Math.max(0, stage * 2 - 1)

    property real intro: 0
    NumberAnimation on intro { from: 0; to: 1; duration: kit.dur(900); easing.type: Easing.OutCubic }

    function mix(a, b, t) { return a + (b - a) * t }

    function wake() {
        awake = true
        idle.restart()
        passInput.forceActiveFocus()
    }

    Connections {
        target: saver
        function onActiveChanged() { root.awake = !saver.active }
    }

    // The lock host focuses the theme itself when the password field isn't focused yet at load (its window
    // isn't active then), and keys sent to this item would wake nothing. Pass focus straight on to the field.
    onActiveFocusChanged: if (activeFocus) passInput.forceActiveFocus()
    // In case a key still lands here: wake, and keep a typed character for the field.
    Keys.onPressed: event => {
        root.wake()
        if (event.text.length > 0 && event.text.charCodeAt(0) >= 32 && event.key !== Qt.Key_Escape)
            passInput.insert(passInput.cursorPosition, event.text)
        event.accepted = true
    }

    // SDDM and the plain lockscreen have no screensaver to settle back; this does it for them.
    Timer {
        id: idle
        interval: 30000
        running: root.awake && passInput.text === ""
        onTriggered: root.awake = false
    }

    // ── Users and sessions (SDDM) ──
    property int userIndex: (typeof userModel !== "undefined" && userModel.lastIndex >= 0) ? userModel.lastIndex : 0
    property int sessionIndex: (typeof sessionModel !== "undefined" && sessionModel.lastIndex >= 0) ? sessionModel.lastIndex : 0
    ListView { id: sessionHelper; model: typeof sessionModel !== "undefined" ? sessionModel : null; currentIndex: root.sessionIndex; opacity: 0; width: 1; height: 1; z: -100; delegate: Item { property string sName: model.name || "" } }
    ListView { id: userHelper; model: typeof userModel !== "undefined" ? userModel : null; currentIndex: root.userIndex; opacity: 0; width: 1; height: 1; z: -100; delegate: Item { property string uName: model.realName || model.name || ""; property string uLogin: model.name || "" } }

    function login() {
        const user = userHelper.currentItem && userHelper.currentItem.uLogin !== "" ? userHelper.currentItem.uLogin : (typeof userModel !== "undefined" ? userModel.lastUser : "")
        if (typeof sddm !== "undefined")
            sddm.login(user, passInput.text, root.sessionIndex)
    }

    Connections {
        target: typeof sddm !== "undefined" ? sddm : null
        function onLoginFailed() {
            errText.text = "WRONG PASSWORD"
            passInput.text = ""
            shake.start()
            root.wake()
        }
    }

    // ── Background ──
    Rectangle { anchors.fill: parent; color: "#05050d"; z: -2000 }

    // The video and the user's own background, drawn sharp. The blur below samples this and covers it
    // while resting; the source must stay visible, since a hidden video pauses.
    Item {
        id: backdrop
        anchors.fill: parent
        z: -1000
        Loader {
            anchors.fill: parent
            active: !userBg.active
            sourceComponent: Item {
                MediaPlayer { id: player; source: "bg.mp4"; videoOutput: bgVideo; loops: MediaPlayer.Infinite; Component.onCompleted: player.play() }
                VideoOutput { id: bgVideo; anchors.fill: parent; fillMode: VideoOutput.PreserveAspectCrop }
            }
        }
        Background { id: userBg; anchors.fill: parent }
    }

    // `blurMax` is fixed by the setting (changing it rebuilds the shader); the stage animates `blur`.
    // Once fully awake the effect is hidden, so a sharp screen costs nothing extra.
    MultiEffect {
        anchors.fill: parent
        z: -900
        source: backdrop
        visible: root.stage < 0.999 && root.blurMax > 0
        autoPaddingEnabled: false
        blurEnabled: true
        blurMax: root.blurMax
        blur: 1 - root.stage
        brightness: -0.18 * (1 - root.stage)
    }

    // Keeps the text readable over the sharp video, on the side the login is.
    Rectangle {
        anchors.fill: parent
        z: -800
        opacity: 0.35 + 0.65 * root.stage
        gradient: Gradient {
            orientation: root.corners ? Gradient.Vertical : Gradient.Horizontal
            GradientStop { position: 0; color: root.corners ? "transparent" : Qt.rgba(0.01, 0.01, 0.05, 0.8) }
            GradientStop { position: root.corners ? 0.55 : 0.35; color: Qt.rgba(0.01, 0.01, 0.05, 0.3) }
            GradientStop { position: root.corners ? 1 : 0.6; color: root.corners ? Qt.rgba(0.01, 0.01, 0.05, 0.85) : "transparent" }
        }
    }

    // A click anywhere wakes it (and keeps the password field focused).
    MouseArea {
        anchors.fill: parent
        z: -10
        cursorShape: Qt.ArrowCursor
        onPressed: root.wake()
    }

    readonly property real pad: 0.07 * width

    // ── Clock ──
    // Both layouts rest the same way: big, in the middle of the left side. Awake it shrinks into its
    // corner; it scales from its top-left so the left edge stays put.
    Column {
        id: clock
        readonly property real awakeScale: 0.55
        transformOrigin: Item.TopLeft
        x: root.pad
        y: root.mix((root.height - height) / 2,
                    root.corners ? root.height * 0.92 - height * awakeScale : root.height * 0.12,
                    root.stage)
        scale: root.mix(1, awakeScale, root.stage)
        opacity: root.intro
        spacing: 4 * root.s

        Text {
            id: clockText
            text: Qt.formatTime(new Date(), root.clockFmt)
            font.family: root.clockFont
            font.pixelSize: 170 * root.s
            color: root.textColor
            layer.enabled: true
            layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "#99000000"; shadowBlur: 0.6; shadowVerticalOffset: 4 }
        }
        Text {
            id: dateText
            text: Qt.formatDate(new Date(), config.dateFormat || "dddd / MMMM d").toUpperCase()
            font.family: root.textFont
            font.pixelSize: 22 * root.s
            font.letterSpacing: 7 * root.s
            color: root.mutedColor
            leftPadding: 6 * root.s
        }
        // Once a second is plenty for a minute clock, and keeps it on the minute.
        Timer {
            interval: 1000
            running: true
            repeat: true
            onTriggered: {
                const now = new Date()
                clockText.text = Qt.formatTime(now, root.clockFmt)
                dateText.text = Qt.formatDate(now, config.dateFormat || "dddd / MMMM d").toUpperCase()
            }
        }
    }

    // Under the resting clock, where nothing else ever goes; gone early in the wake.
    Text {
        x: root.pad + 6 * root.s
        y: (root.height + clock.height) / 2 + 36 * root.s
        text: root.isSddm ? "PRESS ANY KEY TO LOG IN" : "TYPE YOUR PASSWORD TO UNLOCK"
        font.family: root.textFont
        font.pixelSize: 13 * root.s
        font.letterSpacing: 4 * root.s
        color: root.mutedColor
        opacity: 0.6 * root.intro * Math.max(0, 1 - root.stage * 3)
    }

    // ── Login ──
    Item {
        id: login
        width: 440 * root.s
        height: loginColumn.implicitHeight
        readonly property real baseX: root.corners ? root.width - root.pad - width : root.pad
        x: baseX
        y: (root.corners ? root.height * 0.92 - height : root.height * 0.46) + (1 - root.loginShown) * 24 * root.s
        opacity: root.loginShown
        readonly property int align: root.corners ? Text.AlignRight : Text.AlignLeft

        Column {
            id: loginColumn
            width: parent.width
            spacing: 6 * root.s

            Text {
                width: parent.width
                horizontalAlignment: login.align
                text: "WELCOME BACK"
                font.family: root.textFont
                font.pixelSize: 12 * root.s
                font.letterSpacing: 4 * root.s
                color: root.mutedColor
            }
            Text {
                width: parent.width
                horizontalAlignment: login.align
                text: ((userHelper.currentItem && userHelper.currentItem.uName) ? userHelper.currentItem.uName : (typeof userModel !== "undefined" && userModel.lastUser ? userModel.lastUser : "")).toUpperCase()
                font.family: root.textFont
                font.pixelSize: 40 * root.s
                font.letterSpacing: 1 * root.s
                color: userMa.containsMouse && root.isSddm ? root.accent : root.textColor
                Behavior on color { ColorAnimation { duration: kit.dur(200) } }
                // Under SDDM a click picks the next user.
                MouseArea {
                    id: userMa
                    anchors.fill: parent
                    enabled: root.isSddm && typeof userModel !== "undefined" && userModel.rowCount() > 1
                    hoverEnabled: true
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: root.userIndex = (root.userIndex + 1) % userModel.rowCount()
                }
            }

            Item { width: 1; height: 14 * root.s }

            Item {
                width: 340 * root.s
                height: 54 * root.s
                x: root.corners ? parent.width - width : 0

                Rectangle {
                    anchors.fill: parent
                    radius: 12 * root.s
                    color: Qt.rgba(0.02, 0.02, 0.08, 0.35)
                    border.width: 2 * root.s
                    border.color: passInput.activeFocus ? root.textColor : root.mutedColor
                }
                TextInput {
                    id: passInput
                    anchors.fill: parent
                    anchors.leftMargin: 18 * root.s
                    anchors.rightMargin: 54 * root.s
                    verticalAlignment: TextInput.AlignVCenter
                    echoMode: TextInput.Password
                    passwordCharacter: "●"
                    font.family: root.textFont
                    font.pixelSize: 18 * root.s
                    font.letterSpacing: 6 * root.s
                    color: root.textColor
                    clip: true
                    focus: true
                    // Typing works in both stages: while resting the first key wakes it and lands here.
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Escape) {
                            passInput.text = ""
                            root.awake = false
                            event.accepted = true
                            return
                        }
                        if (!root.awake)
                            root.wake()
                        else
                            idle.restart()
                        event.accepted = false
                    }
                    onTextEdited: errText.text = ""
                    onAccepted: {
                        if (text !== "")
                            root.login()
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: passInput.text === ""
                        text: "PASSWORD"
                        font.family: root.textFont
                        font.pixelSize: 13 * root.s
                        font.letterSpacing: 4 * root.s
                        color: root.mutedColor
                        opacity: 0.7
                    }
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: 18 * root.s
                    anchors.verticalCenter: parent.verticalCenter
                    text: "→"
                    font.family: root.textFont
                    font.pixelSize: 22 * root.s
                    color: goMa.containsMouse ? root.accent : root.textColor
                    opacity: passInput.text !== "" ? 1 : 0.35
                    Behavior on opacity { NumberAnimation { duration: kit.dur(200) } }
                    MouseArea {
                        id: goMa
                        anchors.fill: parent
                        anchors.margins: -10 * root.s
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (passInput.text !== "") root.login()
                    }
                }
            }

            Text {
                id: errText
                width: parent.width
                horizontalAlignment: login.align
                height: 20 * root.s
                verticalAlignment: Text.AlignBottom
                font.family: root.textFont
                font.pixelSize: 12 * root.s
                font.letterSpacing: 3 * root.s
                color: "#ff9db0"
            }
        }

        SequentialAnimation {
            id: shake
            NumberAnimation { target: login; property: "x"; to: login.baseX + 14 * root.s; duration: kit.dur(45) }
            NumberAnimation { target: login; property: "x"; to: login.baseX - 14 * root.s; duration: kit.dur(45) }
            NumberAnimation { target: login; property: "x"; to: login.baseX + 8 * root.s; duration: kit.dur(45) }
            NumberAnimation { target: login; property: "x"; to: login.baseX - 8 * root.s; duration: kit.dur(45) }
            NumberAnimation { target: login; property: "x"; to: login.baseX; duration: kit.dur(45) }
            // The animation broke the binding; put it back so a layout change still moves the login.
            onStopped: login.x = Qt.binding(() => login.baseX)
        }
    }

    // ── Power and session ──
    // Power and reboot on both screens; the session picker only under SDDM.
    Row {
        id: actions
        spacing: 48 * root.s
        x: root.corners ? root.width - root.pad - width : root.pad
        y: root.corners ? root.height * 0.07 : root.height * 0.91 - height
        opacity: root.loginShown

        Repeater {
            model: [
                { label: "SHUTDOWN", reboot: false },
                { label: "REBOOT", reboot: true }
            ]
            delegate: Text {
                required property var modelData
                visible: typeof sddm !== "undefined" && (modelData.reboot ? sddm.canReboot : sddm.canPowerOff)
                text: modelData.label
                font.family: root.textFont
                font.pixelSize: 14 * root.s
                font.letterSpacing: 3 * root.s
                color: actMa.containsMouse ? root.accent : root.mutedColor
                Behavior on color { ColorAnimation { duration: kit.dur(200) } }
                MouseArea {
                    id: actMa
                    anchors.fill: parent
                    anchors.margins: -8 * root.s
                    enabled: root.loginShown > 0.5
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: modelData.reboot ? sddm.reboot() : sddm.powerOff()
                }
            }
        }

        Text {
            visible: root.isSddm
            text: (sessionHelper.currentItem && sessionHelper.currentItem.sName ? sessionHelper.currentItem.sName : "SESSION").toUpperCase()
            font.family: root.textFont
            font.pixelSize: 14 * root.s
            font.letterSpacing: 3 * root.s
            color: sessMa.containsMouse ? root.accent : root.textColor
            Behavior on color { ColorAnimation { duration: kit.dur(200) } }
            MouseArea {
                id: sessMa
                anchors.fill: parent
                anchors.margins: -8 * root.s
                enabled: root.loginShown > 0.5
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: if (sessionModel.rowCount() > 0) root.sessionIndex = (root.sessionIndex + 1) % sessionModel.rowCount()
            }
        }
    }
}
