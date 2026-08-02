import Quickshell
import "services"
import "modules/bar"
import "modules/wallpaper"
import "modules/dock"
import "modules/startmenu"
import "modules/notifications"
import "modules/calendar"
import "modules/status"
import "modules/screenpad"

// Minka shell entry point. One surface set per output; ShellLayout decides
// which outputs show persistent surfaces (Duo mode pins them to the
// ScreenPad), MenuState routes menu open/close across outputs.
ShellRoot {
    Variants {
        model: Quickshell.screens

        Scope {
            id: scope

            required property var modelData

            Wallpaper {
                modelData: scope.modelData
            }

            Bar {
                modelData: scope.modelData
            }

            MenuBackdrop {
                modelData: scope.modelData
            }

            Dock {
                modelData: scope.modelData
            }

            // Duo-mode only; hides itself in the general layout.
            SidePanel {
                modelData: scope.modelData
            }

            StartMenu {
                modelData: scope.modelData
            }

            NotifPopup {
                modelData: scope.modelData
            }

            CalendarMenu {
                modelData: scope.modelData
            }

            StatusMenu {
                modelData: scope.modelData
            }

            BatteryMenu {
                modelData: scope.modelData
            }
        }
    }
}
