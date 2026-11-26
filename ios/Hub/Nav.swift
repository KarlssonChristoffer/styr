import SwiftUI

/// Which tab is showing and where each tab's stack is, so the tab bar can step aside inside a
/// thread as in the sketches.
@MainActor
@Observable
final class Nav {
    init() { if UserDefaults.standard.bool(forKey: "chatworld") { Theme.world = .chat } }
    var tab = UserDefaults.standard.integer(forKey: "tab")
    /// The project Hub is focused on (`FocusMode.swift`): the project button goes straight in, the inbox shows only it.
    var focus: ProjectRef? = Nav.loadFocus()
    var inbox: [Route] = []
    var projects: [Route] = []
    var machines: [Route] = []
    var settings: [Route] = []
    /// Bumped when the current tab's icon is tapped again; the page scrolls to its top.
    var topTick = [0, 0, 0, 0]
    /// The inbox is searching or picking many: the pill gives way to a close button or the action bar.
    var searching = false
    var picking = false
    /// The menu a hold on the pen opens, and the Talk an idea screen it leads to.
    var penMenu = false
    /// Hub's own menu and confirmation, drawn once at the top of the app (`HubMenu.swift`, `HubConfirm.swift`).
    var menu: HubMenuRequest?
    var confirm: HubConfirmRequest?
    /// A hold on the pen goes on: where the finger is (global), and a count of let-gos so the menu
    /// can pick the row under the finger. `penHeld` stops the tap that would follow the hold.
    var penFinger: CGPoint?
    var penLetGo = 0
    var penHeld = false
    /// The last places opened in each tab of each world (newest first), what a hold on a pill icon lists,
    /// and the hold's finger (global) and let-go count, as for the pen (`Places.swift`).
    var recent: [String: [Route]] = [:]
    var placesTab: Int?
    var placesFinger: CGPoint?
    var placesLetGo = 0
    /// The inbox shows only quick threads (those in a scratch folder).
    var quick = false
    /// Which world is on screen, what the other one had open, and the state of a change between them
    /// (`Modes.swift`).
    var world: World = UserDefaults.standard.bool(forKey: "chatworld") ? .chat : .dev
    var snapshot: UIImage?
    var wAnim: SwitchAnimation = .slide
    var wOrigin = CGPoint.zero
    var wForward = true
    var wProgress: CGFloat = 1
    var wToken = 0
    private var savedWorlds: [World: (tab: Int, a: [Route], b: [Route], c: [Route], d: [Route])] = [:]

    func saveWorld() { savedWorlds[world] = (tab, inbox, projects, machines, settings) }

    func loadWorld() {
        let s = savedWorlds[world]
        tab = s?.tab ?? 0; inbox = s?.a ?? []; projects = s?.b ?? []; machines = s?.c ?? []; settings = s?.d ?? []
    }
    var talking = false
    /// The next new-thread page starts on the project picker (the pen menu's "New thread in…").
    var pickProjectFirst = false

    /// Swaps the top of the current stack, so Back from a thread just started goes to where you
    /// were, not to the empty new-thread page.
    func replaceTop(_ r: Route) {
        switch tab {
        case 0: if !inbox.isEmpty { inbox.removeLast() }; inbox.append(r)
        case 1: if !projects.isEmpty { projects.removeLast() }; projects.append(r)
        case 2: if !machines.isEmpty { machines.removeLast() }; machines.append(r)
        default: if !settings.isEmpty { settings.removeLast() }; settings.append(r)
        }
    }

    func push(_ r: Route) {
        switch tab { case 0: inbox.append(r); case 1: projects.append(r); case 2: machines.append(r); default: settings.append(r) }
    }

    var deep: Bool {
        switch tab {
        case 0: return !inbox.isEmpty
        case 1: return !projects.isEmpty
        case 2: return !machines.isEmpty
        default: return !settings.isEmpty
        }
    }
}
