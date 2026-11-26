import Foundation

/// The existing /api/updates contract. Git revisions are identities, never sortable versions.
struct UpdateFleet: Decodable {
    var hubd: String?
    var machines: [UpdateMachine]
}

struct UpdateMachine: Decodable, Identifiable {
    var machine: String
    var local: Bool
    var online: Bool
    var hubd: String?
    var hubd_state: HubUpdateStatus?
    var hubd_note: String?
    var id: String { machine }
    var version: String? { hubd_state?.version ?? hubd }

    func behind(_ target: String?) -> Bool {
        // Match desktop: the local updater decides locally; remotes follow the shipping target.
        guard !local, let version, let target else { return false }
        return version != target
    }

    var working: Bool {
        ["checking", "building", "restarting"].contains(hubd_state?.state ?? "") ||
        hubd_note?.hasPrefix("building hubd ") == true
    }

    func label(_ target: String?) -> String {
        if !online { return "offline" }
        switch hubd_state?.state {
        case "checking": return "checking"
        case "building": return hubd_state?.waiting_until == nil ? "building" : "waiting to build"
        case "ready": return "ready to install"
        case "restarting": return "restarting"
        case "failed": return "failed"
        default: return working ? "building" : behind(target) ? "different from sending Hub" : "up to date"
        }
    }

    func canUpdate(_ target: String?) -> Bool {
        online && !local && !working && hubd_state?.state != "ready" && (hubd_state?.state == "failed" || behind(target))
    }
}

struct HubUpdateStatus: Decodable {
    var version: String
    var state: String
    var target: String?
    var step: String?
    var message: String?
    var waiting_until: Int64?
}
