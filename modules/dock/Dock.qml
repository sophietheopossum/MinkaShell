pragma ComponentBehavior: Bound
import Quickshell
import QtQuick
import "../../services"
// Quickshell does not watch imported .js files: after editing DockReorder.js,
// save this file (or restart the shell) before expecting the change live.
import "DockReorder.js" as DockReorder

// Persistent taskbar dock (Sophie's spec, 8/7/2026): always visible — the
// dock.proximity auto-hide is gone — with each window's title next to its
// icon and a right-click menu (close). Reserves an exclusive zone so
// maximized windows stop above it. Follows the bar's Duo policy: in duo
// mode only the ScreenPad carries it, and that instance lists every
// monitor's windows (the main display has no dock of its own); otherwise
// each output lists its own windows.
//
// Drag to reorder (Sophie's request, 15/9/2026): left-drag a chip and drop
// it. The order is ShojiWM's own window order (IPC windows.reorder), so it
// is also the tile sequence on a tiled workspace and the Alt+Tab ring, and
// it survives shell reloads with nothing stored here. A chip moves only
// within its own workspace: seams mark the groups, and the other groups dim
// while dragging. Pulling well above or below the strip before releasing,
// or pressing another button, puts the chip back.
PanelWindow {
    id: root

    required property var modelData

    // Compositor order as window ids, plus lookups. Rebuilt on every view
    // change; chips are keyed by id (chipRepeater), so a title, focus or rect
    // broadcast updates chips in place instead of recreating them.
    readonly property var dockModel: {
        const view = ShojiIpc.view;
        const ids = [];
        const byId = {};
        const groupOf = {};
        if (!view)
            return { ids: ids, byId: byId, groupOf: groupOf };
        for (const monitor of view.monitors) {
            if (!ShellLayout.duoMode && monitor.name !== root.modelData.name)
                continue;
            for (const ws of monitor.workspaces) {
                for (const win of ws.windows) {
                    ids.push(win.id);
                    byId[win.id] = win;
                    // One workspace = one group: windows.reorder never crosses
                    // workspaces, so neither does a drag.
                    groupOf[win.id] = monitor.name + ":" + ws.index;
                }
            }
        }
        return { ids: ids, byId: byId, groupOf: groupOf };
    }

    // While a drop waits for ShojiWM the chips stay in their pre-drop order,
    // drawn in the new one through `shifts`, so the confirming rebuild lands
    // every chip exactly where it is already drawn.
    property var heldIds: null
    property var heldGroups: null
    // An equal list of id strings leaves the Repeater's delegates alone
    // (QQuickRepeater::setModel compares the converted list), so chips are
    // only recreated on open, close or reorder.
    readonly property var shownIds: root.heldIds ?? root.dockModel.ids

    // Drag state, keyed by window id and kept here rather than in a chip:
    // an open or close still recreates chips mid-gesture.
    property string pressId: ""        // chip under the current press, any button
    property string dragId: ""         // left-press candidate, then the dragged window
    property bool dragActive: false
    property bool suppressClick: false
    property bool armWhenSettled: false
    // Pointer x in the strip's viewport (content x minus contentX): a still
    // pointer keeps its value while the strip scrolls, so scrolling can
    // neither start a drag nor slide the chip away from the pointer.
    property real pressViewX: 0
    property real pointerViewX: 0
    property real pointerY: 0           // stripArea coordinates
    property real grabOffset: 0         // press x within the drawn chip
    property var dragPlan: null         // {from, to, left, width, landingX}
    property bool outlineArmed: false
    property var shifts: ({})           // window id -> Translate x
    property var pendingMove: null      // {id, beforeId}
    // Liveness of the windows.reorder call in flight. A plain object the
    // callback closes over, so a reply that outlives the drop, or this Dock
    // (a screen unplugged mid-drop), returns without touching root.
    property var commitTicket: null
    property bool reloadHintShown: false
    // Bumped when Quickshell rescans desktop entries: chips outlive
    // broadcasts now, so their icon lookup has to hear about new entries.
    property int entriesRevision: 0

    readonly property int cancelBand: 48
    readonly property int edgeZone: 40
    readonly property real maxScrollSpeed: 900 // px/s at the very edge
    readonly property string dragGroup: root.dragActive
        ? (root.dockModel.groupOf[root.dragId] ?? "")
        : ""
    readonly property bool dragCancelArmed: root.dragActive
        && (root.pointerY < -root.cancelBand
            || root.pointerY > stripArea.height + root.cancelBand)
    // Edge auto-scroll while dragging an overflowing strip: quadratic across
    // the edge zone, full speed over the arrows or past the strip (the
    // compositor's implicit grab keeps motion coming off-surface). Derived
    // from pointer and width only, never from contentX: no loop.
    readonly property real autoScrollVelocity: {
        if (!root.dragActive || root.dragCancelArmed || !dockBody.overflowing)
            return 0;
        const x = root.pointerViewX;
        const far = chipFlick.width - root.edgeZone;
        if (x < root.edgeZone)
            return -root.maxScrollSpeed * Math.pow(Math.min(1, (root.edgeZone - x) / root.edgeZone), 2);
        if (x > far)
            return root.maxScrollSpeed * Math.pow(Math.min(1, (x - far) / root.edgeZone), 2);
        return 0;
    }

    screen: modelData
    visible: ShellLayout.showBarOn(modelData) && root.dockModel.ids.length > 0
    // Duo mode puts the dock across the top of the ScreenPad, with the
    // MinkaMon zone and the side column below it; the general layout keeps
    // it at the bottom. DockMenu flips its gravity to match.
    anchors.top: ShellLayout.duoMode
    anchors.bottom: !ShellLayout.duoMode
    // Duo mode spans the full width of the ScreenPad; the general layout
    // stays a centred pill sized to its chips (implicitWidth below).
    anchors.left: ShellLayout.duoMode
    anchors.right: ShellLayout.duoMode
    // Sourced from chipRow, not dockBody: in duo mode dockBody's width binds
    // to this window's width, so going through it would be a binding loop.
    // Capped at the output width so a long window list overflows into the
    // scroll arrows rather than growing the dock off the side of the screen.
    implicitWidth: Math.min(chipRow.width + 32, root.modelData.width)
    // Exactly the dock body: no padding on any edge, so the dock sits flush
    // against the bottom of the screen and maximized windows come right up to
    // its top edge. Keep this in step with dockBody's height.
    implicitHeight: 44
    // Forbidden zone for maximized windows; released when the dock hides.
    exclusiveZone: implicitHeight
    color: "transparent"

    onVisibleChanged: {
        if (!visible)
            root.abandonDrag();
    }
    onDockModelChanged: root.viewChanged()
    Component.onDestruction: {
        if (root.commitTicket)
            root.commitTicket.live = false;
    }

    Connections {
        target: DesktopEntries

        function onApplicationsChanged() {
            root.entriesRevision++;
        }
    }

    Connections {
        target: ShellLayout

        function onDuoModeChanged() {
            root.abandonDrag();
        }
    }

    // Covers the 140 ms slide of the dropped chip into its slot.
    Timer {
        id: settleTimer

        interval: 160
        onTriggered: root.tryRelease()
    }

    // Gives up on a drop ShojiWM never confirmed (2 s, or 400 ms once it has
    // answered ok): the chips then show the live order.
    Timer {
        id: pendingTimeout

        interval: 2000
        onTriggered: root.releaseHold()
    }

    // Overflow scroll button. Both arrows stay mapped once the chips stop
    // fitting and dim at the ends instead of appearing and disappearing:
    // visibility that depended on scroll position would feed back into the
    // width it is derived from, which is a binding loop.
    component DockArrow: Rectangle {
        id: arrow

        required property int direction // -1 = left, +1 = right
        property bool canScroll: false
        // Lit while a drag is auto-scrolling toward this end.
        property bool autoScrolling: false

        signal activated

        width: 22
        height: 32
        radius: 6
        color: (arrowArea.containsMouse && arrow.canScroll) || arrow.autoScrolling
             ? Theme.surfaceRaised
             : "transparent"
        opacity: arrow.canScroll ? 1.0 : 0.3

        Behavior on opacity {
            NumberAnimation { duration: 120 }
        }

        Text {
            anchors.centerIn: parent
            text: arrow.direction < 0 ? "‹" : "›"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize + 6
            color: (arrowArea.containsMouse && arrow.canScroll) || arrow.autoScrolling
                 ? Theme.red
                 : Theme.textMuted
        }

        MouseArea {
            id: arrowArea

            anchors.fill: parent
            hoverEnabled: true
            enabled: arrow.canScroll
            onClicked: arrow.activated()
        }
    }

    DockMenu {
        id: dockMenu
    }

    function openMenuFor(item, win) {
        if (dockMenu.visible && dockMenu.windowId === win.id) {
            dockMenu.dismiss();
            return;
        }
        const pos = item.mapToItem(null, item.width / 2, 0);
        dockMenu.openAt(root, pos.x, win);
    }

    // A chip's window id. childAt and itemAt hand back plain Items, which do
    // not declare the delegate's modelData, so it is read by name.
    function chipId(item) {
        return item ? item["modelData"] : undefined;
    }

    function chipAt(x, y) {
        // Row positioning waits for the next polish: straight after a rebuild
        // every new chip still sits at x 0. Settle it before hit-testing.
        chipRow.forceLayout();
        const item = chipRow.childAt(x - chipRow.x, y - chipRow.y);
        return typeof root.chipId(item) === "string" ? item : null;
    }

    // Row slots in shown order, or null while the Repeater has not caught up
    // with shownIds (positioningComplete calls refreshShifts once it has).
    function chipSlots() {
        chipRow.forceLayout();
        const ids = root.shownIds;
        if (chipRepeater.count !== ids.length)
            return null;
        const slots = [];
        for (let i = 0; i < ids.length; i++) {
            const item = chipRepeater.itemAt(i);
            if (!item || root.chipId(item) !== ids[i])
                return null;
            slots.push({ x: item.x, w: item.width });
        }
        return slots;
    }

    // Recompute the live-drag or pending-drop displacement from current
    // geometry. Synchronous on purpose (no callLater): a rebuild, scroll or
    // width change never shows a frame of stale offsets.
    function refreshShifts() {
        if (!root.dragActive && root.pendingMove === null)
            return;
        const slots = root.chipSlots();
        if (!slots)
            return;
        const ids = root.shownIds;
        const plan = root.dragActive
            ? DockReorder.livePlan(ids, root.dockModel.groupOf, slots, chipRow.spacing,
                                   root.dragId, root.pointerViewX + chipFlick.contentX,
                                   root.grabOffset, chipFlick.contentX,
                                   chipFlick.contentX + chipFlick.width, root.dragCancelArmed)
            : DockReorder.pendingPlan(ids, root.dockModel.groupOf, slots, root.pendingMove);
        if (!plan) {
            if (root.dragActive)
                root.cancelDrag();
            else
                root.releaseHold();
            return;
        }
        root.dragPlan = plan;
        root.shifts = DockReorder.shiftsFor(ids, slots, chipRow.spacing, plan);
    }

    function startDrag() {
        if (dockMenu.visible)
            dockMenu.dismiss(); // anchored to where the chip used to be
        scrollAnim.stop();
        root.outlineArmed = false;
        root.dragActive = true; // the lifted chip's Behavior is off before it moves
        root.refreshShifts();
        root.outlineArmed = root.dragActive; // outline appears in place, then glides
    }

    function finishDrag() {
        root.refreshShifts(); // against the release position
        const plan = root.dragPlan;
        if (!root.dragActive || !plan || root.dragCancelArmed || plan.to === plan.from) {
            root.cancelDrag();
            return;
        }
        const id = root.dragId;
        const ids = root.shownIds;
        const beforeId = DockReorder.beforeIdFor(ids, root.dockModel.groupOf, plan.from, plan.to);
        // Hold the pre-drop order (an equal list, so no rebuild) before the
        // drag ends, so the displacement never passes through zero.
        root.heldIds = ids.slice();
        root.heldGroups = root.heldIds.map(i => root.dockModel.groupOf[i]);
        root.pendingMove = { id: id, beforeId: beforeId };
        root.dragActive = false; // Behavior back on before the dropped chip's shift changes
        root.outlineArmed = false;
        root.dragId = "";
        root.refreshShifts(); // pending plan: the chip eases from the pointer into its slot
        settleTimer.restart();
        pendingTimeout.interval = 2000;
        pendingTimeout.restart();
        root.commitMove(id, beforeId);
    }

    function cancelDrag() {
        const wasActive = root.dragActive;
        root.dragActive = false; // Behavior on first, so the chip glides home
        root.outlineArmed = false;
        root.dragId = "";
        if (wasActive) {
            root.dragPlan = null;
            root.suppressClick = true;
            root.shifts = ({});
        }
    }

    // Dock hidden, layout mode flipped: drop everything, commit nothing new.
    function abandonDrag() {
        root.armWhenSettled = false;
        root.cancelDrag();
        if (root.pendingMove !== null)
            root.releaseHold();
    }

    // The one place a drop leaves the dock. A dock-only order would replace
    // this body with a shell-side store and apply that store in dockModel.ids;
    // the drag code stays as it is.
    function commitMove(id, beforeId) {
        const ticket = { live: true };
        root.commitTicket = ticket;
        ShojiIpc.reorderWindow(id, beforeId, (result, error) => {
            if (!ticket.live)
                return;
            if (error || !result || !result.ok) {
                if (String(error ?? "").startsWith("unknown method"))
                    root.reportReloadNeeded();
                root.releaseHold(); // chips glide back to the live order
                return;
            }
            pendingTimeout.interval = 400;
            pendingTimeout.restart();
            root.tryRelease();
        });
    }

    function tryRelease() {
        if (root.pendingMove === null || settleTimer.running)
            return;
        if (!DockReorder.moveSatisfied(root.dockModel.ids, root.dockModel.groupOf, root.pendingMove))
            return;
        root.releaseHold();
    }

    function releaseHold() {
        settleTimer.stop();
        pendingTimeout.stop();
        if (root.commitTicket) {
            root.commitTicket.live = false;
            root.commitTicket = null;
        }
        // Offsets first: the rebuild below then creates chips with none, and
        // when the order did not change the chips glide back instead.
        root.shifts = ({});
        root.dragPlan = null;
        root.pendingMove = null;
        root.heldGroups = null;
        root.heldIds = null;
        if (root.armWhenSettled) {
            root.armWhenSettled = false;
            if ((stripArea.pressedButtons & Qt.LeftButton) && root.dragId !== ""
                    && root.dockModel.byId[root.dragId])
                root.startDrag();
        }
    }

    function viewChanged() {
        if (root.heldIds !== null
                && !DockReorder.sameMembers(root.heldIds, root.heldGroups,
                                            root.dockModel.ids, root.dockModel.groupOf))
            root.releaseHold(); // never show a stale set or grouping
        if (root.dragId !== "" && !root.dockModel.byId[root.dragId]) {
            root.armWhenSettled = false;
            root.cancelDrag(); // the pressed or dragged window is gone
        }
        root.tryRelease();
        root.refreshShifts();
    }

    // The strip moved under a pressed, not yet dragging pointer: a title or a
    // window changed the row width, which re-centres the strip (duo) or the
    // whole surface (general layout). The next motion would read that shift
    // as travel and lift the chip, so the press stays a click instead.
    function layoutShifted() {
        if (!root.dragActive && !root.armWhenSettled)
            root.dragId = "";
    }

    // Only reachable if the ShojiWM config was not reloaded after
    // windows.reorder landed; one notification per shell generation.
    function reportReloadNeeded() {
        console.warn("dock: ShojiWM has no windows.reorder yet; reload its config with Super+Shift+R");
        if (root.reloadHintShown)
            return;
        root.reloadHintShown = true;
        Quickshell.execDetached(["notify-send", "--app-name=MinkaShell",
            "Dock order needs a ShojiWM reload",
            "Press Super+Shift+R to load windows.reorder, then drag again."]);
    }

    Rectangle {
        id: dockBody

        // Space the chips can use once the body's own padding is removed.
        readonly property real trackWidth: width - 16
        // Derived only from chipRow (its children) and this body's width, so
        // it can never depend on the scroll position or on arrow visibility.
        readonly property bool overflowing: chipRow.width > trackWidth
        // Arrow width plus the row spacing beside it.
        readonly property real arrowSlot: 26

        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 0
        // Duo mode: a full-width strip, squared off with a single seam along
        // its inner edge, matching the bar it replaces. General layout: a
        // rounded pill hugging its chips, capped at the output width so it
        // overflows into the arrows instead of running off the screen.
        width: ShellLayout.duoMode
             ? parent.width
             : Math.min(chipRow.width + 16, root.modelData.width - 40)
        height: 44
        radius: ShellLayout.duoMode ? 0 : 10
        color: Theme.barBg
        border.width: ShellLayout.duoMode ? 0 : 1
        border.color: Theme.line

        // Seam below the strip in duo mode (the dock sits at the top there).
        Rectangle {
            visible: ShellLayout.duoMode
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: Theme.line
        }

        Row {
            id: chipArea

            anchors.centerIn: parent
            spacing: 4
            onXChanged: root.layoutShifted()

            DockArrow {
                anchors.verticalCenter: parent.verticalCenter
                direction: -1
                visible: dockBody.overflowing
                canScroll: chipFlick.contentX > 0.5
                autoScrolling: autoScroll.running && root.autoScrollVelocity < 0
                onActivated: chipFlick.scrollBy(-1)
            }

            Flickable {
                id: chipFlick

                // One arrow click moves most of a page, like Firefox's tab
                // strip; horizontal wheel and touchpad scrolling still move it
                // freely.
                function scrollBy(dir) {
                    const limit = Math.max(0, contentWidth - width);
                    scrollAnim.to = Math.max(
                        0,
                        Math.min(limit, contentX + dir * width * 0.8)
                    );
                    scrollAnim.restart();
                }

                anchors.verticalCenter: parent.verticalCenter
                // Never derived from the scroll position, so the arrows can
                // read contentX without feeding back into this width.
                width: dockBody.overflowing
                     ? dockBody.trackWidth - 2 * dockBody.arrowSlot
                     : chipRow.width
                // 4 px above and below the 32 px chips so a lifted chip is not
                // clipped. chipRow sits at y 4 and chipArea still centres, so
                // chips and arrows keep their on-screen position.
                height: 40
                contentWidth: chipRow.width
                contentHeight: height
                clip: true
                flickableDirection: Flickable.HorizontalFlick
                boundsBehavior: Flickable.StopAtBounds
                // Left-drag on a chip reorders; the strip never drag-scrolls.
                // Wheel events ignore acceptedButtons, so the wheel, touchpad
                // and arrows still scroll. Not interactive: false, which would
                // also kill the wheel.
                acceptedButtons: Qt.NoButton
                // The dragged chip rides the pointer, which is fixed in the
                // viewport, so any scroll re-derives its slot.
                onContentXChanged: root.refreshShifts()

                NumberAnimation {
                    id: scrollAnim

                    target: chipFlick
                    property: "contentX"
                    duration: 160
                    easing.type: Easing.OutCubic
                }

                FrameAnimation {
                    id: autoScroll

                    // Stops once the viewport shows the end of the dragged chip's
                    // group: scrolling on would carry the chip, clamped to its
                    // group, out of view.
                    running: root.autoScrollVelocity < 0
                        ? chipFlick.contentX > Math.max(0.5, (root.dragPlan ? root.dragPlan.groupLo : 0) + 0.5)
                        : root.autoScrollVelocity > 0
                        ? chipFlick.contentX + chipFlick.width
                            < Math.min(chipFlick.contentWidth, root.dragPlan ? root.dragPlan.groupEnd : chipFlick.contentWidth) - 0.5
                        : false
                    onTriggered: {
                        const limit = Math.max(0, chipFlick.contentWidth - chipFlick.width);
                        // Capped step so a stalled frame (>50 ms) cannot lurch.
                        const step = root.autoScrollVelocity * Math.min(autoScroll.frameTime, 0.05);
                        const plan = root.dragPlan;
                        let next = chipFlick.contentX + step;
                        // One-sided: the group bound can stop the scroll, never
                        // pull contentX past where it already is.
                        if (plan)
                            next = step > 0
                                ? Math.min(next, Math.max(chipFlick.contentX, plan.groupEnd - chipFlick.width))
                                : Math.max(next, Math.min(chipFlick.contentX, plan.groupLo));
                        chipFlick.contentX = Math.max(0, Math.min(limit, next));
                    }
                }

                // Beneath chipRow. Chips only track hover (their areas accept
                // no buttons), so every press lands here, and this area outlives
                // any chip recreated mid-gesture.
                MouseArea {
                    id: stripArea

                    width: chipRow.width
                    height: chipFlick.height
                    acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton

                    onPressed: mouse => {
                        scrollAnim.stop();
                        chipFlick.cancelFlick();
                        // Another button during a drag, or while one waits for the
                        // last drop to settle, puts the chip back.
                        if (root.dragActive || root.armWhenSettled) {
                            root.armWhenSettled = false;
                            root.suppressClick = true;
                            root.cancelDrag();
                            return;
                        }
                        if (mouse.buttons !== mouse.button)
                            return;
                        root.suppressClick = false;
                        root.armWhenSettled = false;
                        root.dragId = "";
                        const item = root.chipAt(mouse.x, mouse.y);
                        root.pressId = item ? item.modelData : "";
                        if (!item || mouse.button !== Qt.LeftButton)
                            return;
                        root.dragId = item.modelData;
                        root.pressViewX = mouse.x - chipFlick.contentX;
                        root.pointerViewX = root.pressViewX;
                        root.pointerY = mouse.y;
                        root.grabOffset = Math.max(0, Math.min(item.width, mouse.x - item.drawnX));
                    }

                    onPositionChanged: mouse => {
                        if (!(mouse.buttons & Qt.LeftButton) || root.dragId === "")
                            return;
                        root.pointerViewX = mouse.x - chipFlick.contentX;
                        root.pointerY = mouse.y;
                        if (!root.dragActive) {
                            // Horizontal travel in viewport terms only: a scroll
                            // under a still pointer, or ShojiWM's zero-delta
                            // motion on layer commits, never counts.
                            if (Math.abs(root.pointerViewX - root.pressViewX)
                                    < Application.styleHints.startDragDistance)
                                return;
                            root.suppressClick = true;
                            if (!ShojiIpc.ready) {
                                root.dragId = "";
                                return;
                            }
                            if (root.pendingMove !== null) {
                                root.armWhenSettled = true; // arms once the last drop settles
                                return;
                            }
                            root.startDrag();
                            return;
                        }
                        root.refreshShifts();
                    }

                    onReleased: mouse => {
                        if (mouse.button !== Qt.LeftButton)
                            return;
                        if (root.dragActive) {
                            root.pointerViewX = mouse.x - chipFlick.contentX;
                            root.pointerY = mouse.y;
                            root.finishDrag();
                        } else {
                            root.dragId = "";
                            root.armWhenSettled = false;
                        }
                    }

                    onCanceled: {
                        root.armWhenSettled = false;
                        root.cancelDrag();
                    }

                    // Emitted after released, and only while the pointer is still
                    // over the strip: the same three actions as before, on the chip
                    // drawn under the pointer, if it is the one that was pressed.
                    onClicked: mouse => {
                        if (root.suppressClick)
                            return;
                        const item = root.chipAt(mouse.x, mouse.y);
                        if (!item || item.modelData !== root.pressId || !item.win)
                            return;
                        if (mouse.button === Qt.RightButton) {
                            root.openMenuFor(item, item.win);
                        } else if (mouse.button === Qt.MiddleButton) {
                            ShojiIpc.closeWindow(item.modelData);
                        } else {
                            ShojiIpc.activateWindow(item.modelData);
                        }
                    }
                }

                // Where the dragged chip will land; goes back to the origin slot
                // while the drop is armed to cancel.
                Rectangle {
                    id: landingOutline

                    visible: root.dragActive && root.dragPlan !== null
                    x: root.dragPlan ? root.dragPlan.landingX : 0
                    y: chipRow.y
                    width: root.dragPlan ? root.dragPlan.width : 0
                    height: 32
                    radius: 8
                    color: "transparent"
                    border.width: 1
                    border.color: root.dragCancelArmed ? Theme.textFaint : Theme.line

                    Behavior on x {
                        enabled: root.outlineArmed
                        NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
                    }
                }

                Row {
                    id: chipRow

                    y: 4
                    spacing: 6
                    onPositioningComplete: root.refreshShifts()
                    onWidthChanged: root.layoutShifted()

                    Repeater {
                        id: chipRepeater

                        model: root.shownIds

                        delegate: Rectangle {
                            id: dockItem

                            required property string modelData
                            required property int index

                            readonly property var win: root.dockModel.byId[dockItem.modelData] ?? null
                            // A string, so the desktop-entry lookup reruns only
                            // when the app id changes or entries are rescanned,
                            // not on every broadcast.
                            readonly property string appId: dockItem.win
                                ? (dockItem.win.appId || "")
                                : ""
                            readonly property var entry: {
                                root.entriesRevision;
                                return dockItem.appId ? DesktopEntries.heuristicLookup(dockItem.appId) : null;
                            }
                            readonly property bool isFocused: dockItem.win !== null
                                && dockItem.win.focused === true
                            readonly property bool lifted: root.dragActive
                                && root.dragId === dockItem.modelData
                            readonly property bool groupStart: dockItem.index > 0
                                && root.dockModel.groupOf[dockItem.modelData]
                                    !== root.dockModel.groupOf[root.shownIds[dockItem.index - 1]]
                            // Where the chip is drawn: Row slot plus reorder offset.
                            readonly property real drawnX: dockItem.x + slide.x

                            width: chip.width + 16
                            height: 32
                            radius: 8
                            z: dockItem.lifted ? 2
                             : root.pendingMove !== null && root.pendingMove.id === dockItem.modelData ? 1
                             : 0
                            color: dockItem.lifted || dockItem.isFocused ? Theme.surfaceRaised
                                 : itemArea.containsMouse && !root.dragActive ? Theme.surface
                                 : "transparent"
                            border.width: 1
                            border.color: dockItem.lifted
                                        ? (root.dragCancelArmed ? Theme.textFaint : Theme.red)
                                        : dockItem.isFocused ? Theme.redDim
                                        : "transparent"
                            opacity: root.dragActive && root.dockModel.groupOf[dockItem.modelData] !== root.dragGroup ? 0.35
                                   : dockItem.lifted && root.dragCancelArmed ? 0.5
                                   : 1

                            Behavior on opacity {
                                NumberAnimation { duration: 120 }
                            }

                            // Offsets, not x: chipRow owns x, and a transform never
                            // touches layout, so chipRow.width, the surface width,
                            // overflowing and the arrows never follow a drag.
                            transform: Translate {
                                id: slide

                                x: root.shifts[dockItem.modelData] ?? 0
                                y: dockItem.lifted ? -2 : 0

                                Behavior on x {
                                    enabled: !dockItem.lifted
                                    NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
                                }
                                Behavior on y {
                                    NumberAnimation { duration: 120; easing.type: Easing.OutCubic }
                                }
                            }

                            // Workspace (and, in duo mode, monitor) boundary, in the
                            // 6 px gap. Hidden while chips are displaced.
                            Rectangle {
                                visible: dockItem.groupStart
                                x: -4
                                anchors.verticalCenter: parent.verticalCenter
                                width: 1
                                height: 16
                                color: Theme.line
                                opacity: root.dragActive || root.pendingMove !== null ? 0 : 1
                            }

                            Row {
                                id: chip

                                anchors.centerIn: parent
                                spacing: 7

                                Image {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 20
                                    height: 20
                                    sourceSize.width: 20
                                    sourceSize.height: 20
                                    fillMode: Image.PreserveAspectFit
                                    source: dockItem.entry && dockItem.entry.icon
                                        ? Quickshell.iconPath(dockItem.entry.icon, "application-x-executable")
                                        : Quickshell.iconPath("application-x-executable")
                                }

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: dockItem.win
                                        ? (dockItem.win.title || dockItem.win.appId || "?")
                                        : "?"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize - 1
                                    color: dockItem.lifted || dockItem.isFocused ? Theme.text : Theme.textMuted
                                    elide: Text.ElideRight
                                    width: Math.min(implicitWidth, 150)
                                }
                            }

                            // Hover only: presses fall through to stripArea (Qt skips
                            // NoButton items as press targets), which outlives chips.
                            MouseArea {
                                id: itemArea

                                anchors.fill: parent
                                hoverEnabled: true
                                acceptedButtons: Qt.NoButton
                            }
                        }
                    }
                }
            }

            DockArrow {
                anchors.verticalCenter: parent.verticalCenter
                direction: 1
                visible: dockBody.overflowing
                canScroll: chipFlick.contentX
                    < chipFlick.contentWidth - chipFlick.width - 0.5
                autoScrolling: autoScroll.running && root.autoScrollVelocity > 0
                onActivated: chipFlick.scrollBy(1)
            }
        }
    }
}
