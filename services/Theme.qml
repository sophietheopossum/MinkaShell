pragma Singleton
import Quickshell
import QtQuick
// Through the config-root symlink: Quickshell only honours qmldir
// singleton registration for paths inside the shell root.
import "../Proustite"

// Thin facade over the shared Proustite palette (successor to the Eternal
// Darkness tokens that used to live here) plus the shell's own metrics.
// Every module styles through these — no literal colors in widgets — so a
// palette edit in Proustite reaches every Minka app at once.
Singleton {
    // grounds
    readonly property color ground: Proustite.ground
    readonly property color surface: Proustite.surface
    readonly property color surfaceRaised: Proustite.surfaceRaised
    readonly property color line: Proustite.line

    // content
    readonly property color text: Proustite.text
    readonly property color textMuted: Proustite.textMuted
    readonly property color textFaint: Proustite.textFaint

    // accents
    readonly property color red: Proustite.red
    readonly property color redDim: Proustite.redDim
    readonly property color purple: Proustite.purple
    readonly property color purpleDim: Proustite.purpleDim
    readonly property color warnAmber: Proustite.warnAmber

    // bar
    readonly property color barBg: Qt.rgba(0.039, 0.027, 0.035, 0.92)
    readonly property int barHeight: 32

    readonly property string fontFamily: Proustite.fontFamily
    readonly property string monoFamily: Proustite.monoFamily
    readonly property int fontSize: Proustite.fontSize
}